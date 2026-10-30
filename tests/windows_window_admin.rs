//! Contracts for the Windows window builtins: `gui_focus_window`,
//! `gui_minimize_window`, `gui_maximize_window`, `gui_move_window`,
//! `gui_resize_window` and `gui_close_window`.
//!
//! These move, resize, minimise and close real windows, so each test is
//! `#[ignore]`d and also returns early unless AE_DISPOSABLE_HOST=1, which only
//! the disposable Windows runner sets. Run them with `cargo test -- --ignored`
//! on that runner only. The one window the lifecycle test touches is a WinForms
//! form it starts itself, with a unique title, and kills in a Drop guard that
//! runs even when the test panics.
//!
//! The builtins are checked against user32 called directly from this test
//! process, not against their own output: FindWindowW finds the window by its
//! title, GetWindowRect reads its outer rectangle, and IsIconic, IsZoomed,
//! IsWindow and GetForegroundWindow read its state.
//!
//! Coordinates: the test process calls SetProcessDPIAware, so GetWindowRect
//! reports physical pixels, as the form's own script does. The builtins run
//! their PowerShell in a child process that is not declared DPI-aware, so on a
//! display scaled above 100% its coordinates may be scaled. The runner uses 100%,
//! where the two agree exactly. The +/-2 pixel tolerance covers rounding only.

#![cfg(windows)]

use iron_shell::env::Env;
use iron_shell::value::Value;
use std::ffi::c_void;
use std::process::{Child, Command, Stdio};
use std::sync::Once;
use std::time::{Duration, Instant};

/// An HWND, as user32 hands it out. The test only compares and passes it.
type Hwnd = *mut c_void;

/// The RECT layout user32 writes into GetWindowRect.
#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> Hwnd;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn IsWindow(hwnd: Hwnd) -> i32;
    fn IsIconic(hwnd: Hwnd) -> i32;
    fn IsZoomed(hwnd: Hwnd) -> i32;
    fn GetForegroundWindow() -> Hwnd;
    fn SetProcessDPIAware() -> i32;
}

/// True only on the disposable runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// Calls a builtin with the session journal isolated from the repository's.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    static ISOLATED: Once = Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_win_admin_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
    iron_shell::builtins::call(name, args, &mut Env::new())
}

/// A NUL-terminated UTF-16 copy of `text`, for the W-suffixed user32 calls.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The top-level window whose title is exactly `title`, or null. The class
/// name is a real null pointer, so any class matches.
fn find(title: &str) -> Hwnd {
    let title = wide(title);
    // SAFETY: a null class is documented as "any class". The title buffer is
    // NUL-terminated and lives across the call.
    unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }
}

/// The outer rectangle of `hwnd`, in physical pixels (see the module docs).
fn outer_rect(hwnd: Hwnd) -> Rect {
    let mut rect = Rect::default();
    // SAFETY: `rect` is a live out-parameter of the documented layout.
    let ok = unsafe { GetWindowRect(hwnd, &mut rect) };
    assert_ne!(ok, 0, "GetWindowRect failed for {hwnd:?}");
    rect
}

fn is_window(hwnd: Hwnd) -> bool {
    // SAFETY: IsWindow takes any handle value and only reads it.
    unsafe { IsWindow(hwnd) != 0 }
}

fn is_iconic(hwnd: Hwnd) -> bool {
    // SAFETY: IsIconic takes any handle value and only reads it.
    unsafe { IsIconic(hwnd) != 0 }
}

fn is_zoomed(hwnd: Hwnd) -> bool {
    // SAFETY: IsZoomed takes any handle value and only reads it.
    unsafe { IsZoomed(hwnd) != 0 }
}

fn foreground() -> Hwnd {
    // SAFETY: GetForegroundWindow takes no arguments.
    unsafe { GetForegroundWindow() }
}

/// Polls `done` until it holds, failing with `what` after `within`.
fn wait_until(what: &str, within: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !done() {
        assert!(
            Instant::now() < deadline,
            "{what} did not hold within {within:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A WinForms form of 400 by 300 pixels with the title in AE_TEST_TITLE. It
/// declares itself DPI-aware, so its size is in physical pixels.
const FORM_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System.Runtime.InteropServices;
public class FormDpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
"@
[void][FormDpi]::SetProcessDPIAware()
$form = New-Object System.Windows.Forms.Form
$form.Text = [Environment]::GetEnvironmentVariable('AE_TEST_TITLE')
$form.Size = New-Object System.Drawing.Size(400, 300)
$form.StartPosition = 'CenterScreen'
[System.Windows.Forms.Application]::Run($form)
"#;

/// Kills the process it owns, even while a panic unwinds.
struct Owned(Child);

impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_form(title: &str) -> Owned {
    let child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-Command",
            FORM_SCRIPT,
        ])
        .env("AE_TEST_TITLE", title)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("powershell.exe runs on the disposable runner");
    Owned(child)
}

/// One form, driven through every window builtin in turn. The order matters:
/// a minimised window reports its rectangle at -32000, so the move and resize
/// are checked before it is minimised.
#[test]
#[ignore = "moves and closes real windows: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn a_form_is_moved_resized_minimised_maximised_focused_and_closed() {
    if !disposable() {
        return;
    }
    // SAFETY: SetProcessDPIAware takes no arguments; a false answer only means
    // the process was already declared DPI-aware, and is not checked.
    unsafe {
        SetProcessDPIAware();
    }

    let title = format!("ae winadmin {}", std::process::id());
    let _form = start_form(&title);
    wait_until("the form has a window", Duration::from_secs(20), || {
        !find(&title).is_null()
    });
    let hwnd = find(&title);

    // Move: the top-left corner of the outer rectangle lands at (100, 120).
    let answer = call(
        "gui_move_window",
        vec![s(&title), Value::Int(100), Value::Int(120)],
    )
    .expect("gui_move_window moves the form");
    assert_eq!(answer, Value::Bool(true));
    let r = outer_rect(hwnd);
    assert!(
        r.left.abs_diff(100) <= 2 && r.top.abs_diff(120) <= 2,
        "moved to (100,120) but the rectangle starts at ({},{})",
        r.left,
        r.top
    );

    // Resize: the outer rectangle is 500 by 400, and its corner stays put.
    let answer = call(
        "gui_resize_window",
        vec![s(&title), Value::Int(500), Value::Int(400)],
    )
    .expect("gui_resize_window resizes the form");
    assert_eq!(answer, Value::Bool(true));
    let r = outer_rect(hwnd);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    assert!(
        (w - 500).abs() <= 2 && (h - 400).abs() <= 2,
        "resized to 500x400 but the rectangle is {w}x{h}"
    );
    assert!(
        r.left.abs_diff(100) <= 2 && r.top.abs_diff(120) <= 2,
        "a resize moved the window to ({},{})",
        r.left,
        r.top
    );

    // Minimise: the window is iconic (SW_MINIMIZE is asynchronous, so poll).
    let answer = call("gui_minimize_window", vec![s(&title)])
        .expect("gui_minimize_window minimises the form");
    assert_eq!(answer, Value::Bool(true));
    wait_until("the form is minimised", Duration::from_secs(5), || {
        is_iconic(hwnd)
    });

    // Maximise: restores from iconic and zooms the window.
    let answer = call("gui_maximize_window", vec![s(&title)])
        .expect("gui_maximize_window maximises the form");
    assert_eq!(answer, Value::Bool(true));
    wait_until("the form is maximised", Duration::from_secs(5), || {
        is_zoomed(hwnd) && !is_iconic(hwnd)
    });

    // Focus: the builtin must answer true. Windows' foreground lock can refuse
    // SetForegroundWindow from a process that was not last given input, so the
    // oracle result is reported on stderr (run with --nocapture) and not
    // asserted. Only the builtin's own answer is asserted.
    let answer = call("gui_focus_window", vec![s(&title)])
        .expect("gui_focus_window brings the form to the front");
    assert_eq!(answer, Value::Bool(true));
    if foreground() == hwnd {
        eprintln!("focus oracle: GetForegroundWindow is the form");
    } else {
        eprintln!(
            "focus oracle: GetForegroundWindow is {:?}, not the form {hwnd:?} (foreground lock)",
            foreground()
        );
    }

    // Close: WM_CLOSE ends the form's message loop, so the window goes away.
    let answer =
        call("gui_close_window", vec![s(&title)]).expect("gui_close_window sends the close");
    assert_eq!(answer, Value::Bool(true));
    wait_until("the form is closed", Duration::from_secs(5), || {
        !is_window(hwnd)
    });
}

/// Every window builtin, given a title no window has, answers E_NOT_FOUND.
#[test]
#[ignore = "moves and closes real windows: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn each_window_builtin_says_not_found_for_a_title_no_window_has() {
    if !disposable() {
        return;
    }
    let title = format!("no window has this title {}", std::process::id());
    assert!(find(&title).is_null(), "a window already has {title}");

    let cases = [
        ("gui_focus_window", vec![s(&title)]),
        ("gui_minimize_window", vec![s(&title)]),
        ("gui_maximize_window", vec![s(&title)]),
        (
            "gui_move_window",
            vec![s(&title), Value::Int(10), Value::Int(10)],
        ),
        (
            "gui_resize_window",
            vec![s(&title), Value::Int(100), Value::Int(100)],
        ),
        ("gui_close_window", vec![s(&title)]),
    ];
    for (name, args) in cases {
        match call(name, args) {
            Ok(v) => panic!("{name} answered {v:?} for a title that names no window"),
            Err(e) => assert!(e.to_string().contains("E_NOT_FOUND"), "{name}: {e}"),
        }
    }
}
