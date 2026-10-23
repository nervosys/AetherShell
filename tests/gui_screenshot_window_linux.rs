//! Contracts for `gui_screenshot_window`.
//!
//! The refusal tests run everywhere and capture nothing. The capture test is
//! Linux only, `#[ignore]`d, and runs only in the disposable distro: it starts
//! its own Xvfb on display :95 and an xmessage window on it, and never touches
//! the user's desktop.

use aethershell::builtins::call_with_input;
use aethershell::env::Env;
use aethershell::value::Value;

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_shot_window_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    Env::new()
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn refusal(args: Vec<Value>, input: Option<Value>) -> String {
    match call_with_input("gui_screenshot_window", args, input, &mut env()) {
        Ok(v) => panic!("gui_screenshot_window answered {v:?} where it must refuse"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn bad_calls_are_refused_before_anything_is_captured() {
    for args in [
        vec![],
        vec![Value::Int(1)],
        vec![s("")],
        vec![s("a"), Value::Int(2)],
        vec![s("a"), s("b.png"), s("c")],
    ] {
        let msg = refusal(args.clone(), None);
        assert!(msg.contains("E_BAD_ARG"), "{args:?}: {msg}");
    }
    let msg = refusal(vec![s("a")], Some(s("piped")));
    assert!(msg.contains("E_BAD_ARG"), "piped input: {msg}");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn it_refuses_off_linux_and_writes_nothing() {
    let path = std::env::temp_dir().join(format!("ae_shot_window_{}.png", std::process::id()));
    let msg = refusal(vec![s("Some Window"), s(&path.to_string_lossy())], None);
    assert!(msg.contains("E_UNIMPLEMENTED"), "{msg}");
    assert!(!path.exists(), "nothing may be written");
}

// ----------------------------------------------------------------- Linux only

#[cfg(target_os = "linux")]
const DISPLAY: &str = ":95";

#[cfg(target_os = "linux")]
const SOCKET: &str = "/tmp/.X11-unix/X95";

/// Owns this test's Xvfb and xmessage; dropping it, including while a panic
/// unwinds, kills both.
#[cfg(target_os = "linux")]
struct X11Guard {
    children: Vec<std::process::Child>,
}

#[cfg(target_os = "linux")]
impl Drop for X11Guard {
    fn drop(&mut self) {
        for child in self.children.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

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
fn a_window_is_captured_by_its_exact_title() {
    disposable::require();
    let mut guard = X11Guard {
        children: Vec::new(),
    };
    guard.children.push(
        std::process::Command::new("Xvfb")
            .args([DISPLAY, "-nolisten", "tcp"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Xvfb must be installed in the disposable distro"),
    );
    let started = std::time::Instant::now();
    while !std::path::Path::new(SOCKET).exists() && !abstract_socket_accepts() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "Xvfb did not open {SOCKET} within 10 s"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::set_var("DISPLAY", DISPLAY);

    let title = format!("ae shot {}", std::process::id());
    guard.children.push(
        std::process::Command::new("xmessage")
            .args(["-title", &title, "hello from the test"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("xmessage (x11-utils) must be installed in the disposable distro"),
    );

    let dir = std::env::temp_dir().join(format!("ae_shot_window_dir_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("window.png");
    let path_text = path.to_string_lossy().into_owned();

    // The window maps a moment after xmessage starts: ask for up to 10 s.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let answer = loop {
        match call_with_input(
            "gui_screenshot_window",
            vec![s(&title), s(&path_text)],
            None,
            &mut env(),
        ) {
            Ok(v) => break v,
            Err(e) if std::time::Instant::now() >= deadline => {
                panic!("the window was not captured within 10 s: {e}")
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
        }
    };
    assert_eq!(answer, s(&path_text));
    let bytes = std::fs::read(&path).expect("the PNG was written");
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
    assert!(
        bytes.len() > 100,
        "a {}-byte PNG is not a capture",
        bytes.len()
    );

    // A title that names no window: E_NOT_FOUND, and nothing is written.
    let other = dir.join("none.png");
    let msg = refusal(
        vec![s("no window has this title"), s(&other.to_string_lossy())],
        None,
    );
    assert!(msg.contains("E_NOT_FOUND"), "missing window: {msg}");
    assert!(
        !other.exists(),
        "nothing may be written for a missing window"
    );

    // No display at all: E_NO_UI.
    std::env::remove_var("DISPLAY");
    let msg = refusal(vec![s(&title), s(&other.to_string_lossy())], None);
    assert!(msg.contains("E_NO_UI"), "no display: {msg}");

    let _ = std::fs::remove_dir_all(&dir);
}
