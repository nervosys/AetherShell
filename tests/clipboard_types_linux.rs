//! Contracts for `clipboard_types`.
//!
//! The refusal tests run everywhere and touch no clipboard: every call is
//! refused before the clipboard is asked. The real-clipboard test is Linux
//! only, `#[ignore]`d, and runs only in the disposable distro: it starts its own
//! Xvfb on display :97, puts text and an image on that display's clipboard with
//! xclip, and never touches the user's desktop or the Windows clipboard.

use iron_shell::builtins::call_with_input;
use iron_shell::env::Env;
use iron_shell::value::Value;

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

/// An environment whose undo journal is this test process's own, as the other
/// clipboard contract files use.
fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_clipboard_types_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
    Env::new()
}

/// The error text of a call that must be refused.
fn refusal(args: Vec<Value>, input: Option<Value>) -> String {
    match call_with_input("clipboard_types", args, input, &mut env()) {
        Ok(v) => panic!("clipboard_types answered {v:?} where it must refuse"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn clipboard_types_refuses_surplus_arguments() {
    let msg = refusal(vec![Value::Int(1)], None);
    assert!(msg.contains("E_BAD_ARG"), "surplus argument: {msg}");
}

#[test]
fn clipboard_types_refuses_piped_input() {
    let msg = refusal(vec![], Some(Value::Int(1)));
    assert!(msg.contains("E_BAD_ARG"), "piped input: {msg}");
}

/// macOS reads the pasteboard's types: an Array of the format names the other
/// platforms use. Reading changes nothing.
#[cfg(target_os = "macos")]
#[test]
fn clipboard_types_on_macos_lists_known_formats() {
    match call_with_input("clipboard_types", vec![], None, &mut env()) {
        Ok(Value::Array(types)) => {
            for t in types {
                match t {
                    Value::Str(t) => assert!(
                        ["text", "image", "files", "audio"].contains(&t.as_str()),
                        "unknown clipboard format {t}"
                    ),
                    other => panic!("a format should be a String, got {other:?}"),
                }
            }
        }
        other => panic!("clipboard_types should answer an Array, got {other:?}"),
    }
}

// ----------------------------------------------------------------- Linux only

/// The display this test's own Xvfb serves. Nothing else uses it.
#[cfg(target_os = "linux")]
const DISPLAY: &str = ":97";

/// The socket Xvfb opens for [`DISPLAY`].
#[cfg(target_os = "linux")]
const SOCKET: &str = "/tmp/.X11-unix/X97";

/// A 67-byte 1x1 RGBA PNG, fed to xclip as the image on the clipboard. xclip
/// does not decode it; the test only needs the bytes to be a PNG.
#[cfg(target_os = "linux")]
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR, 13 bytes
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // width 1, height 1
    0x08, 0x06, 0x00, 0x00, 0x00, // 8-bit RGBA, no interlace
    0x1F, 0x15, 0xC4, 0x89, // IHDR CRC
    0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, // IDAT, 10 bytes
    0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, // zlib data
    0x0D, 0x0A, 0x2D, 0xB4, // IDAT CRC
    0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND
];

/// Owns the Xvfb this test started. Dropping it, including while a panic
/// unwinds, kills every xclip or xsel left holding the clipboard on
/// [`DISPLAY`], then Xvfb itself.
#[cfg(target_os = "linux")]
struct X11Guard {
    xvfb: std::process::Child,
}

#[cfg(target_os = "linux")]
impl Drop for X11Guard {
    fn drop(&mut self) {
        kill_clipboard_holders(DISPLAY);
        let _ = self.xvfb.kill();
        let _ = self.xvfb.wait();
    }
}

/// Kills the clipboard holders started on `display`. xclip -i forks a process
/// that keeps the selection; it inherits DISPLAY from the xclip that started
/// it, so a holder is found by that variable and by its command name. Nothing
/// else on the machine matches both.
#[cfg(target_os = "linux")]
fn kill_clipboard_holders(display: &str) {
    let want = format!("DISPLAY={display}");
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str() else { continue };
        if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        if !matches!(comm.trim(), "xclip" | "xsel") {
            continue;
        }
        let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        if environ.split(|b| *b == 0).any(|var| var == want.as_bytes()) {
            if let Ok(pid) = pid.parse::<i32>() {
                // SAFETY: plain kill(2) on a pid read from /proc just above.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
}

/// Runs `xclip` with `bytes` on its stdin and waits for it to hand the
/// clipboard to its holder. The holder outlives this call; the guard ends it.
#[cfg(target_os = "linux")]
fn put(args: &[&str], bytes: &[u8]) {
    use std::io::Write;
    let mut child = std::process::Command::new("xclip")
        .args(args)
        .env("DISPLAY", DISPLAY)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("xclip must be installed in the disposable distro");
    child
        .stdin
        .take()
        .expect("xclip stdin")
        .write_all(bytes)
        .expect("write to xclip");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match child.try_wait().expect("poll xclip") {
            Some(status) => {
                assert!(status.success(), "xclip {args:?} exited {status}");
                return;
            }
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                panic!("xclip {args:?} did not hand over the clipboard within 10 s");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

/// One call to `clipboard_types`, with its error text on failure.
#[cfg(target_os = "linux")]
fn clipboard_types() -> Result<Vec<String>, String> {
    match call_with_input("clipboard_types", vec![], None, &mut env()) {
        Ok(Value::Array(items)) => Ok(items
            .into_iter()
            .map(|item| match item {
                Value::Str(s) => s,
                other => panic!("a clipboard format is a String, got {other:?}"),
            })
            .collect()),
        Ok(other) => panic!("clipboard_types answered {other:?}, not an Array"),
        Err(e) => Err(e.to_string()),
    }
}

/// Asks until the answer is `expected`, for up to 10 s: the holder takes the
/// selection a moment after xclip returns. Returns the answer it settled on.
#[cfg(target_os = "linux")]
fn settle(expected: &[&str]) -> Vec<String> {
    let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let got = clipboard_types();
        if let Ok(list) = &got {
            if *list == want {
                return list.clone();
            }
        }
        if std::time::Instant::now() >= deadline {
            panic!("clipboard_types did not settle on {want:?}; last answer {got:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Whether the X server accepts a connection on the abstract socket named
/// like [`SOCKET`].
#[cfg(target_os = "linux")]
fn abstract_socket_accepts() -> bool {
    use std::os::linux::net::SocketAddrExt;
    std::os::unix::net::SocketAddr::from_abstract_name(SOCKET.as_bytes())
        .and_then(|addr| std::os::unix::net::UnixStream::connect_addr(&addr))
        .is_ok()
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn clipboard_types_lists_the_formats_of_a_real_x11_clipboard() {
    disposable::require();
    let _guard = {
        let xvfb = std::process::Command::new("Xvfb")
            .args([DISPLAY, "-nolisten", "tcp"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Xvfb must be installed in the disposable distro");
        X11Guard { xvfb }
    };
    let started = std::time::Instant::now();
    // Xvfb also listens on the abstract socket of the same name. Under WSL,
    // /tmp/.X11-unix belongs to WSLg and the file may never appear, while
    // clients still connect through the abstract socket.
    while !std::path::Path::new(SOCKET).exists() && !abstract_socket_accepts() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "Xvfb did not open {SOCKET} within 10 s"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // This test alone owns the process environment: no other test reads or
    // sets DISPLAY or WAYLAND_DISPLAY, so changing them here is safe.
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::set_var("DISPLAY", DISPLAY);

    // An empty clipboard is the empty answer, not a failure.
    assert_eq!(
        clipboard_types().expect("an empty clipboard is answered, not refused"),
        Vec::<String>::new()
    );

    // Text: UTF8_STRING, STRING and TEXT all map to "text".
    put(&["-selection", "clipboard", "-i"], b"hello");
    assert_eq!(settle(&["text"]), vec!["text".to_string()]);

    // An image replaces the text: image/png maps to "image".
    put(
        &["-selection", "clipboard", "-t", "image/png", "-i"],
        PNG_1X1,
    );
    assert_eq!(settle(&["image"]), vec!["image".to_string()]);

    // With no display at all there is no clipboard to ask: E_NO_UI, and no tool
    // is started.
    std::env::remove_var("DISPLAY");
    std::env::remove_var("WAYLAND_DISPLAY");
    let msg = refusal(vec![], None);
    assert!(msg.contains("E_NO_UI"), "no display: {msg}");
}
