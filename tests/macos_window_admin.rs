//! Contracts for the macOS window builtins: `gui_list_windows`,
//! `gui_focus_window`, `gui_minimize_window`, `gui_maximize_window`,
//! `gui_move_window`, `gui_resize_window` and `gui_close_window`.
//!
//! These move, resize, minimise and close real windows, so each test is
//! `#[ignore]`d and also returns early unless AE_DISPOSABLE_HOST=1, which only
//! the disposable macOS runner sets. Run them with `cargo test -- --ignored`
//! on that runner only. The one window the lifecycle test touches is an
//! NSWindow that an osascript process starts itself, with a unique title, and
//! kills in a Drop guard that runs even when the test panics.
//!
//! The builtins are checked against oracles that are not the builtins: a
//! JavaScript for osascript that reads CGWindowList (kCGWindowBounds, by the
//! exact kCGWindowName) and System Events (AXMinimized) directly. The window
//! frame is the outer rectangle, in points, with a top-left origin on the
//! primary display; the +/-2 point tolerance covers rounding only.
//!
//! Permissions: System Events needs Accessibility for the terminal or runner
//! that runs the test, and CGWindowList withholds other processes' window
//! titles without Screen Recording. A missing permission fails the test with
//! the builtin's or the oracle's own message; nothing is skipped silently.

#![cfg(target_os = "macos")]

use aethershell::env::Env;
use aethershell::value::Value;
use std::process::{Child, Command, Stdio};
use std::sync::Once;
use std::time::{Duration, Instant};

const OSASCRIPT: &str = "/usr/bin/osascript";

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
        let dir = std::env::temp_dir().join(format!("ae_mac_admin_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    aethershell::builtins::call(name, args, &mut Env::new())
}

/// An NSWindow titled AE_TEST_TITLE, 300 by 200 points at (200, 200), shown
/// for 60 seconds by pumping the run loop. Style 15 is titled, closable,
/// miniaturizable and resizable; 2 is buffered backing; 0 is a regular app.
const WINDOW_SCRIPT: &str = r#"ObjC.import('AppKit');
function run() {
    var title = ObjC.unwrap($.NSProcessInfo.processInfo.environment.objectForKey('AE_TEST_TITLE'));
    var app = $.NSApplication.sharedApplication;
    app.setActivationPolicy(0);
    var win = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer($.NSMakeRect(200, 200, 300, 200), 15, 2, false);
    win.setTitle(title);
    win.makeKeyAndOrderFront(null);
    app.activateIgnoringOtherApps(true);
    for (var i = 0; i < 60; i++) {
        $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(1));
    }
}
"#;

/// The oracle. It does not call any builtin. `argv[0]` picks the question:
/// `screen` (the primary display's size in points), `bounds` (the frame of the
/// window named argv[1], or `none`), or `minimized` (AXMinimized of the first
/// System Events window named argv[1], or `none`). Exit 2 is an unknown
/// question.
const ORACLE_SCRIPT: &str = r#"ObjC.import('CoreGraphics');
ObjC.import('stdlib');
function run(argv) {
    var op = argv[0];
    if (op === 'screen') {
        var size = $.CGDisplayBounds($.CGMainDisplayID()).size;
        return size.width + ' ' + size.height;
    }
    var title = argv[1];
    if (op === 'bounds') {
        // Option 0 is kCGWindowListOptionAll, so a window that is minimised or
        // on another Space still counts as present.
        var windows = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(0, 0))) || [];
        for (var i = 0; i < windows.length; i++) {
            if (windows[i].kCGWindowName === title) {
                var b = windows[i].kCGWindowBounds;
                return [b.X, b.Y, b.Width, b.Height].join(' ');
            }
        }
        return 'none';
    }
    if (op === 'minimized') {
        var procs = Application('System Events').processes();
        for (var p = 0; p < procs.length; p++) {
            var wins = procs[p].windows();
            for (var w = 0; w < wins.length; w++) {
                if (wins[w].name() === title) {
                    return String(wins[w].attributes.byName('AXMinimized').value());
                }
            }
        }
        return 'none';
    }
    $.exit(2);
}
"#;

/// Runs the oracle and returns its trimmed stdout. A failure (for example no
/// Accessibility permission) fails the test with the oracle's own stderr.
fn oracle(question: &str, title: &str) -> String {
    let out = Command::new(OSASCRIPT)
        .args(["-l", "JavaScript", "-e", ORACLE_SCRIPT, question, title])
        .output()
        .expect("/usr/bin/osascript runs on the disposable runner");
    assert!(
        out.status.success(),
        "the oracle `{question}` failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The oracle's frame for `title` as [x, y, width, height], or None.
fn frame_of(title: &str) -> Option<[f64; 4]> {
    let text = oracle("bounds", title);
    if text == "none" {
        return None;
    }
    let values: Vec<f64> = text
        .split_whitespace()
        .map(|v| v.parse().expect("the oracle answers numbers"))
        .collect();
    Some([values[0], values[1], values[2], values[3]])
}

/// Polls until `done` holds, failing with `what` after `within`.
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

/// Polls the oracle's frame of `title` until it is `expected` within 2 points.
fn expect_frame(title: &str, expected: [f64; 4]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let got = frame_of(title);
        let ok = got.is_some_and(|g| {
            g.iter()
                .zip(expected.iter())
                .all(|(a, b)| (a - b).abs() <= 2.0)
        });
        if ok {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the frame of {title:?} is {got:?}, expected {expected:?} (+/-2 points)"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The (hwnd, pid) that gui_list_windows reports for the window titled `title`.
fn listed(title: &str) -> Option<(i64, i64)> {
    let list = call("gui_list_windows", vec![]).expect("gui_list_windows lists the windows");
    let Value::Array(items) = list else {
        panic!("gui_list_windows answered {list:?}, not an array");
    };
    items.iter().find_map(|item| match item {
        Value::Record(rec) if rec.get("title") == Some(&s(title)) => {
            match (rec.get("hwnd"), rec.get("pid")) {
                (Some(Value::Int(h)), Some(Value::Int(p))) => Some((*h, *p)),
                other => panic!("a listed window has fields {other:?}, not Int hwnd and pid"),
            }
        }
        _ => None,
    })
}

/// Kills the process it owns, even while a panic unwinds.
struct Owned(Child);

impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_window(title: &str) -> Owned {
    let child = Command::new(OSASCRIPT)
        .args(["-l", "JavaScript", "-e", WINDOW_SCRIPT])
        .env("AE_TEST_TITLE", title)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("/usr/bin/osascript runs on the disposable runner");
    Owned(child)
}

/// One window, driven through every window builtin in turn. The order matters:
/// a minimised window cannot be moved, so it is minimised last, before close.
#[test]
#[ignore = "moves, resizes and closes a real window: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn a_window_is_listed_moved_resized_focused_maximised_minimised_and_closed() {
    if !disposable() {
        return;
    }
    let title = format!("ae winadmin {}", std::process::id());
    let window = start_window(&title);
    let owner = window.0.id() as i64;

    wait_until(
        "the window is listed by gui_list_windows (Screen Recording withholds other processes' titles)",
        Duration::from_secs(10),
        || listed(&title).is_some(),
    );
    let (hwnd, pid) = listed(&title).expect("the window is listed");
    assert!(hwnd > 0, "gui_list_windows reported handle {hwnd}");
    assert_eq!(
        pid, owner,
        "the window is owned by the osascript child (pid {owner})"
    );

    // Move: the frame's origin lands at (100, 120), and its size is unchanged.
    let answer = call(
        "gui_move_window",
        vec![s(&title), Value::Int(100), Value::Int(120)],
    )
    .expect("gui_move_window moves the window");
    assert_eq!(answer, Value::Bool(true));
    expect_frame(&title, [100.0, 120.0, 300.0, 200.0]);

    // Resize: 400 by 300 points, and the origin stays at (100, 120).
    let answer = call(
        "gui_resize_window",
        vec![s(&title), Value::Int(400), Value::Int(300)],
    )
    .expect("gui_resize_window resizes the window");
    assert_eq!(answer, Value::Bool(true));
    expect_frame(&title, [100.0, 120.0, 400.0, 300.0]);

    // Focus: the builtin answers true. Whether the window is frontmost is not
    // asserted, since the foreground can be refused to a background process.
    let answer = call("gui_focus_window", vec![s(&title)]).expect("gui_focus_window raises it");
    assert_eq!(answer, Value::Bool(true));

    // Maximise: the window fills the primary display's width (the visible
    // frame is the full width unless a side Dock is shown) and stays on screen.
    let answer = call("gui_maximize_window", vec![s(&title)]).expect("gui_maximize_window works");
    assert_eq!(answer, Value::Bool(true));
    let screen: Vec<f64> = oracle("screen", "")
        .split_whitespace()
        .map(|v| v.parse().expect("the oracle answers numbers"))
        .collect();
    wait_until(
        "the window fills the screen width",
        Duration::from_secs(5),
        || {
            frame_of(&title)
                .is_some_and(|f| (f[2] - screen[0]).abs() <= 2.0 && f[1] + f[3] <= screen[1] + 2.0)
        },
    );

    // Minimise: System Events' AXMinimized reads true (checked by the oracle).
    let answer =
        call("gui_minimize_window", vec![s(&title)]).expect("gui_minimize_window minimises it");
    assert_eq!(answer, Value::Bool(true));
    wait_until("the window is minimised", Duration::from_secs(5), || {
        oracle("minimized", &title) == "true"
    });

    // Close: the close button is pressed, and the window server drops it.
    let answer =
        call("gui_close_window", vec![s(&title)]).expect("gui_close_window closes the window");
    assert_eq!(answer, Value::Bool(true));
    wait_until("the window is closed", Duration::from_secs(5), || {
        frame_of(&title).is_none()
    });
    drop(window);
}

/// Every window builtin, given a title no window has, answers E_NOT_FOUND.
#[test]
#[ignore = "drives window builtins: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn each_window_builtin_says_not_found_for_a_title_no_window_has() {
    if !disposable() {
        return;
    }
    let title = format!("no window has this title {}", std::process::id());
    assert_eq!(
        oracle("bounds", &title),
        "none",
        "a window already has {title}"
    );

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

/// Bad arguments are refused before any window is touched, and focusing by a
/// CGWindow number is E_UNIMPLEMENTED on macOS.
#[test]
#[ignore = "drives window builtins: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn bad_arguments_and_focus_by_handle_are_refused() {
    if !disposable() {
        return;
    }
    let title = format!("no window has this title {}", std::process::id());
    let cases: [(&str, Vec<Value>, &str); 5] = [
        ("gui_focus_window", vec![Value::Int(0)], "E_BAD_ARG"),
        (
            "gui_move_window",
            vec![s(&title), s("left"), Value::Int(10)],
            "E_BAD_ARG",
        ),
        (
            "gui_resize_window",
            vec![s(&title), Value::Int(0), Value::Int(10)],
            "E_BAD_ARG",
        ),
        (
            "gui_focus_window",
            vec![Value::Int(12345)],
            "E_UNIMPLEMENTED",
        ),
        ("gui_list_windows", vec![Value::Int(1)], "E_BAD_ARG"),
    ];
    for (name, args, code) in cases {
        match call(name, args) {
            Ok(v) => panic!("{name} answered {v:?}; expected {code}"),
            Err(e) => assert!(
                e.to_string().contains(code),
                "{name}: expected {code}, got {e}"
            ),
        }
    }
}
