//! Contracts for `clipboard_get_image` and `clipboard_set_image`.
//!
//! The refusal tests run everywhere and touch no clipboard: every call is
//! refused before the clipboard is asked. The real-clipboard test is Linux
//! only, `#[ignore]`d, and runs only in the disposable distro: it starts its own
//! Xvfb on display :96 and never touches the user's desktop or the Windows
//! clipboard.

use aethershell::builtins::call_with_input;
use aethershell::env::Env;
use aethershell::value::Value;

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

/// A 67-byte 1x1 RGBA PNG.
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

/// An environment whose undo journal is this test process's own.
fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_clipboard_image_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    Env::new()
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// The error text of a call that must be refused.
fn refusal(name: &str, args: Vec<Value>, input: Option<Value>) -> String {
    match call_with_input(name, args, input, &mut env()) {
        Ok(v) => panic!("{name} answered {v:?} where it must refuse"),
        Err(e) => e.to_string(),
    }
}

/// A file in a fresh directory under the system temp dir.
fn scratch_file(stem: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ae_clipboard_image_{stem}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join(stem);
    std::fs::write(&path, bytes).expect("write scratch file");
    path
}

#[test]
fn get_image_refuses_arguments_and_piped_input() {
    let msg = refusal("clipboard_get_image", vec![s("x")], None);
    assert!(msg.contains("E_BAD_ARG"), "argument: {msg}");
    let msg = refusal("clipboard_get_image", vec![], Some(s("x")));
    assert!(msg.contains("E_BAD_ARG"), "piped input: {msg}");
}

#[test]
fn set_image_checks_its_argument_and_file_before_the_clipboard() {
    for (args, want) in [
        (vec![], "E_BAD_ARG"),
        (vec![Value::Int(1)], "E_BAD_ARG"),
        (vec![s("a.png"), s("b.png")], "E_BAD_ARG"),
    ] {
        let msg = refusal("clipboard_set_image", args.clone(), None);
        assert!(msg.contains(want), "{args:?}: {msg}");
    }
    let msg = refusal("clipboard_set_image", vec![s("x.png")], Some(s("piped")));
    assert!(msg.contains("E_BAD_ARG"), "piped input: {msg}");

    let missing = std::env::temp_dir().join(format!(
        "ae_clipboard_image_missing_{}.png",
        std::process::id()
    ));
    let msg = refusal(
        "clipboard_set_image",
        vec![s(&missing.to_string_lossy())],
        None,
    );
    assert!(msg.contains("E_NOT_FOUND"), "missing file: {msg}");

    let text = scratch_file("notes.txt", b"not an image");
    let msg = refusal(
        "clipboard_set_image",
        vec![s(&text.to_string_lossy())],
        None,
    );
    assert!(msg.contains("E_BAD_ARG"), "a text file: {msg}");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn both_refuse_off_linux_after_the_checks() {
    let msg = refusal("clipboard_get_image", vec![], None);
    assert!(msg.contains("E_UNIMPLEMENTED"), "get: {msg}");
    let png = scratch_file("one.png", PNG_1X1);
    let msg = refusal("clipboard_set_image", vec![s(&png.to_string_lossy())], None);
    assert!(msg.contains("E_UNIMPLEMENTED"), "set: {msg}");
}

// ----------------------------------------------------------------- Linux only

/// The display this test's own Xvfb serves. Nothing else uses it.
#[cfg(target_os = "linux")]
const DISPLAY: &str = ":96";

/// The socket Xvfb opens for [`DISPLAY`].
#[cfg(target_os = "linux")]
const SOCKET: &str = "/tmp/.X11-unix/X96";

/// Owns the Xvfb this test started. Dropping it, including while a panic
/// unwinds, kills every xclip left holding the clipboard on [`DISPLAY`], then
/// Xvfb itself.
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

/// Kills the xclip holders started on `display`, found by their command name
/// and the DISPLAY they inherited. Nothing else on the machine matches both.
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
        if comm.trim() != "xclip" {
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

/// Whether the X server accepts a connection on the abstract socket named
/// like [`SOCKET`]. Under WSL the socket file may never appear.
#[cfg(target_os = "linux")]
fn abstract_socket_accepts() -> bool {
    use std::os::linux::net::SocketAddrExt;
    std::os::unix::net::SocketAddr::from_abstract_name(SOCKET.as_bytes())
        .and_then(|addr| std::os::unix::net::UnixStream::connect_addr(&addr))
        .is_ok()
}

/// `clipboard_get_image`, asked until it answers, for up to 10 s: the holder
/// takes the selection a moment after the setter returns.
#[cfg(target_os = "linux")]
fn settled_image() -> std::collections::BTreeMap<String, Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match call_with_input("clipboard_get_image", vec![], None, &mut env()) {
            Ok(Value::Record(r)) => return r,
            Ok(other) => panic!("clipboard_get_image answered {other:?}, not a Record"),
            Err(e) if std::time::Instant::now() >= deadline => {
                panic!("clipboard_get_image did not find the image within 10 s: {e}")
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

#[cfg(target_os = "linux")]
fn decoded(rec: &std::collections::BTreeMap<String, Value>) -> Vec<u8> {
    use base64::Engine;
    match rec.get("base64") {
        Some(Value::Str(b)) => base64::engine::general_purpose::STANDARD
            .decode(b)
            .expect("base64 decodes"),
        other => panic!("base64 is {other:?}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn images_round_trip_through_a_real_x11_clipboard() {
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
    while !std::path::Path::new(SOCKET).exists() && !abstract_socket_accepts() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "Xvfb did not open {SOCKET} within 10 s"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // This test alone owns the process environment's display variables.
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::set_var("DISPLAY", DISPLAY);

    // Nothing on the clipboard: no image, and that is E_NOT_FOUND, not Null.
    let msg = refusal("clipboard_get_image", vec![], None);
    assert!(msg.contains("E_NOT_FOUND"), "empty clipboard: {msg}");

    // Set, then read back exactly the same bytes.
    let png = scratch_file("one.png", PNG_1X1);
    let png_arg = s(&png.to_string_lossy());
    match call_with_input(
        "clipboard_set_image",
        vec![png_arg.clone()],
        None,
        &mut env(),
    ) {
        Ok(Value::Bool(true)) => {}
        other => panic!("clipboard_set_image answered {other:?}"),
    }
    let rec = settled_image();
    assert_eq!(rec.get("mime"), Some(&s("image/png")));
    assert_eq!(rec.get("bytes"), Some(&Value::Int(PNG_1X1.len() as i64)));
    assert_eq!(decoded(&rec), PNG_1X1);
    let types =
        call_with_input("clipboard_types", vec![], None, &mut env()).expect("clipboard_types");
    assert!(
        matches!(&types, Value::Array(items) if items.contains(&s("image"))),
        "clipboard_types: {types:?}"
    );

    // The holder outlives the call: the image is still there 2 s later.
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(decoded(&settled_image()), PNG_1X1);

    // A refused file leaves the clipboard as it was.
    let text = scratch_file("notes.txt", b"not an image");
    let msg = refusal(
        "clipboard_set_image",
        vec![s(&text.to_string_lossy())],
        None,
    );
    assert!(msg.contains("E_BAD_ARG"), "a text file: {msg}");
    assert_eq!(decoded(&settled_image()), PNG_1X1);

    // No display: both refuse with E_NO_UI.
    std::env::remove_var("DISPLAY");
    let msg = refusal("clipboard_get_image", vec![], None);
    assert!(msg.contains("E_NO_UI"), "get without a display: {msg}");
    let msg = refusal("clipboard_set_image", vec![png_arg], None);
    assert!(msg.contains("E_NO_UI"), "set without a display: {msg}");
}
