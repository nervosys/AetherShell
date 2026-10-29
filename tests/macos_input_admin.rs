//! Contracts for the macOS input builtins: `gui_mouse_move`, `gui_mouse_click`,
//! `gui_mouse_drag`, `gui_mouse_scroll`, `gui_mouse_position`, `gui_key_press`,
//! `gui_key_combo`, `gui_type_text`, `gui_get_active_window` and
//! `gui_screen_size`.
//!
//! These post real mouse and keyboard events, so each test is `#[ignore]`d and
//! also returns early unless AE_DISPOSABLE_HOST=1, which only the disposable
//! macOS runner sets. Run them with `cargo test -- --ignored` on that runner
//! only. The pointer moves to points the test chooses, and the typing test
//! types into an NSWindow that an osascript process starts itself, which is
//! killed in a Drop guard that runs even when the test panics.
//!
//! Each builtin is checked against an oracle that is not the builtin: a
//! separate osascript that reads CGEventGetLocation, NSScreen and
//! AXIsProcessTrusted directly. Positions are compared within one point,
//! which covers rounding only.
//!
//! Accessibility: posting events needs Accessibility for the terminal or
//! runner that runs the test. Without it the builtins refuse with E_OS_DENIED
//! and send nothing. A test accepts exactly that outcome, prints why, and
//! never passes silently: any other error fails it, and E_OS_DENIED is only
//! accepted when the oracle says trust is missing.

#![cfg(target_os = "macos")]

use aethershell::env::Env;
use aethershell::value::Value;
use std::process::{Child, Command};
use std::sync::Once;
use std::time::{Duration, Instant};

const OSASCRIPT: &str = "/usr/bin/osascript";

/// True only on the disposable runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

/// Calls a builtin with the session journal isolated from the repository's.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    static ISOLATED: Once = Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_mac_input_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    aethershell::builtins::call(name, args, &mut Env::new())
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

/// The oracle. It does not call any builtin. `argv[0]` picks the question:
/// `trusted` (AXIsProcessTrusted), `pos` (the pointer as CGEventGetLocation
/// reports it, "x y"), or `screen` (NSScreen.mainScreen.frame's size, "w h").
const ORACLE_SCRIPT: &str = r#"ObjC.import('AppKit');
ObjC.import('CoreGraphics');
ObjC.import('ApplicationServices');
function run(argv) {
    var q = argv[0];
    if (q === 'trusted') {
        return $.AXIsProcessTrusted() ? 'true' : 'false';
    }
    if (q === 'pos') {
        var p = $.CGEventGetLocation($.CGEventCreate(null));
        return p.x + ' ' + p.y;
    }
    if (q === 'screen') {
        var frame = ($.NSScreen.mainScreen || $.NSScreen.screens.objectAtIndex(0)).frame;
        return frame.size.width + ' ' + frame.size.height;
    }
    throw new Error('unknown question ' + q);
}
"#;

/// Runs the oracle and returns its trimmed stdout. A failure fails the test
/// with the oracle's own stderr.
fn oracle(question: &str) -> String {
    let out = Command::new(OSASCRIPT)
        .args(["-l", "JavaScript", "-e", ORACLE_SCRIPT, question])
        .output()
        .expect("/usr/bin/osascript runs on the disposable runner");
    assert!(
        out.status.success(),
        "the oracle `{question}` failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The oracle's pointer position, rounded to whole points.
fn oracle_pos() -> (i64, i64) {
    let text = oracle("pos");
    let mut parts = text.split_whitespace().map(|p| {
        p.parse::<f64>()
            .unwrap_or_else(|_| panic!("the oracle's position `{text}` is not two numbers"))
    });
    let x = parts.next().expect("the oracle gives an x");
    let y = parts.next().expect("the oracle gives a y");
    (x.round() as i64, y.round() as i64)
}

/// The `x` and `y` Ints of a Record answer, or a failure naming the answer.
fn pair_of(v: &Value, a: &str, b: &str) -> (i64, i64) {
    match v {
        Value::Record(r) => match (r.get(a), r.get(b)) {
            (Some(Value::Int(x)), Some(Value::Int(y))) => (*x, *y),
            _ => panic!("the Record {v:?} has no Int {a} and {b}"),
        },
        other => panic!("expected a Record with {a} and {b}, got {other:?}"),
    }
}

/// Runs an input builtin that posts events. Success is returned, after the
/// oracle has confirmed the process is trusted (a sent event without trust
/// would mean the builtin's check failed open). An error is acceptable only as
/// the Accessibility refusal E_OS_DENIED: that is printed with its reason and
/// the caller ends its test there, with nothing sent. Any other error fails.
fn sent_or_denied(name: &str, args: Vec<Value>) -> Option<Value> {
    match call(name, args) {
        Ok(v) => {
            assert_eq!(
                oracle("trusted"),
                "true",
                "{name} sent events, but the oracle says this process is not trusted for Accessibility"
            );
            Some(v)
        }
        Err(e) => {
            assert_eq!(
                code_of(&e),
                "E_OS_DENIED",
                "{name} failed with something other than the Accessibility refusal: {e}"
            );
            eprintln!(
                "{name}: Accessibility is not granted to this runner, so the outcome is E_OS_DENIED and nothing was sent: {e}"
            );
            None
        }
    }
}

fn wait_until(what: &str, within: Duration, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < within,
            "timed out after {within:?} waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// An osascript helper that owns an NSWindow with a focused NSTextField and a
/// local key monitor. The monitor rewrites `record` with every key's characters
/// typed so far. The process is killed and its file removed when dropped.
struct Helper {
    child: Child,
    record: std::path::PathBuf,
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.record);
    }
}

/// The helper window: NSApp.run keeps it alive, and the process terminates
/// itself after 90 s as a backstop. `argv` is the record path, then the title.
const FIELD_SCRIPT: &str = r#"ObjC.import('AppKit');
function run(argv) {
    var record = argv[0];
    var title = argv[1];
    var app = $.NSApplication.sharedApplication;
    app.setActivationPolicy(0);
    var win = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer($.NSMakeRect(200, 200, 420, 120), 15, 2, false);
    win.setTitle(title);
    var field = $.NSTextField.alloc.initWithFrame($.NSMakeRect(20, 40, 380, 24));
    win.contentView.addSubview(field);
    win.makeKeyAndOrderFront(null);
    win.makeFirstResponder(field);
    app.activateIgnoringOtherApps(true);
    var typed = '';
    $.NSEvent.addLocalMonitorForEventsMatchingMaskHandler($.NSEventMaskKeyDown, function (ev) {
        var c = ObjC.unwrap(ev.characters);
        if (c) typed += c;
        $(typed).writeToFileAtomicallyEncodingError(record, true, $.NSUTF8StringEncoding, null);
        return ev;
    });
    app.performSelectorWithObjectAfterDelay('terminate:', null, 90);
    app.run();
}
"#;

fn start_field(title: &str) -> Helper {
    let record = std::env::temp_dir().join(format!("ae_mac_input_{}.txt", std::process::id()));
    let child = Command::new(OSASCRIPT)
        .args(["-l", "JavaScript", "-e", FIELD_SCRIPT])
        .arg(record.to_str().expect("the temp path is UTF-8"))
        .arg(title)
        .spawn()
        .expect("/usr/bin/osascript runs on the disposable runner");
    Helper { child, record }
}

#[test]
#[ignore = "moves the real mouse pointer: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn the_pointer_moves_and_reads_back_at_the_same_point() {
    if !disposable() {
        return;
    }
    let (tx, ty) = (300, 300);
    let Some(moved) = sent_or_denied("gui_mouse_move", vec![Value::Int(tx), Value::Int(ty)]) else {
        return;
    };
    assert!(
        matches!(moved, Value::Bool(true)),
        "gui_mouse_move answered {moved:?}; expected true"
    );

    let (bx, by) = pair_of(
        &call("gui_mouse_position", vec![]).expect("gui_mouse_position reads the pointer"),
        "x",
        "y",
    );
    let (ox, oy) = oracle_pos();
    for (who, (x, y)) in [("the builtin", (bx, by)), ("the oracle", (ox, oy))] {
        assert!(
            (x - tx).abs() <= 1 && (y - ty).abs() <= 1,
            "{who} reads the pointer at {x},{y}; expected {tx},{ty} within 1 px"
        );
    }
    assert!(
        (bx - ox).abs() <= 1 && (by - oy).abs() <= 1,
        "the builtin says {bx},{by} and the oracle says {ox},{oy}"
    );
}

#[test]
#[ignore = "reads the screen: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn the_screen_size_matches_the_main_screen_frame() {
    if !disposable() {
        return;
    }
    let (w, h) = pair_of(
        &call("gui_screen_size", vec![]).expect("gui_screen_size reads the screen"),
        "width",
        "height",
    );
    let text = oracle("screen");
    let mut parts = text
        .split_whitespace()
        .map(|p| p.parse::<f64>().expect("a number"));
    let ow = parts.next().expect("the oracle gives a width");
    let oh = parts.next().expect("the oracle gives a height");
    // The builtin rounds the frame's size to whole points.
    assert!(
        (w as f64 - ow).abs() <= 0.5 && (h as f64 - oh).abs() <= 0.5,
        "gui_screen_size says {w}x{h}; NSScreen.mainScreen.frame says {ow}x{oh}"
    );
}

#[test]
#[ignore = "types into a window: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn typed_text_reaches_the_focused_window_unchanged() {
    if !disposable() {
        return;
    }
    let title = format!("ae input test {}", std::process::id());
    let helper = start_field(&title);
    let pid = i64::from(helper.child.id());
    // The text is sent to whatever window has focus, so the helper must be
    // the frontmost app first. gui_get_active_window reports its pid.
    wait_until(
        "the helper to be the frontmost app",
        Duration::from_secs(20),
        || match call("gui_get_active_window", vec![]) {
            Ok(Value::Record(r)) => matches!(r.get("pid"), Some(Value::Int(p)) if *p == pid),
            Ok(_) => false,
            Err(_) => false,
        },
    );

    // A single key first: whether synthetic keys reach the helper at all,
    // separately from how text is set on an event.
    let Some(_) = sent_or_denied("gui_key_press", vec![Value::Str("x".to_string())]) else {
        return;
    };
    let reached = (0..25).any(|_| {
        std::thread::sleep(Duration::from_millis(200));
        std::fs::read_to_string(&helper.record).is_ok_and(|got| got == "x")
    });
    eprintln!(
        "a key press reached the helper: {reached}; it recorded {:?}",
        std::fs::read_to_string(&helper.record).ok()
    );
    let _ = std::fs::write(&helper.record, "");

    // Non-ASCII text, and a chunk longer than the 20-unit event limit, so
    // both the Unicode path and the chunking are exercised.
    let text = "ae typed 42 \u{fc}ber the limit of twenty units";
    let Some(sent) = sent_or_denied("gui_type_text", vec![Value::Str(text.to_string())]) else {
        return;
    };
    assert!(
        matches!(sent, Value::Bool(true)),
        "gui_type_text answered {sent:?}; expected true"
    );
    let arrived = (0..50).any(|_| {
        std::thread::sleep(Duration::from_millis(200));
        std::fs::read_to_string(&helper.record).is_ok_and(|got| got.ends_with(text))
    });
    let got = std::fs::read_to_string(&helper.record).unwrap_or_default();
    assert!(
        arrived,
        "the helper recorded {got:?} where {text:?} was typed"
    );
}

#[test]
#[ignore = "sends key and mouse input: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn an_unknown_key_or_an_out_of_range_coordinate_is_refused_with_no_side_effects() {
    if !disposable() {
        return;
    }
    let before = pair_of(
        &call("gui_mouse_position", vec![]).expect("gui_mouse_position reads the pointer"),
        "x",
        "y",
    );
    let cases: [(&str, Vec<Value>); 7] = [
        ("gui_key_press", vec![Value::Str("nope".into())]),
        (
            "gui_key_combo",
            vec![Value::Str("ctrl".into()), Value::Str("nope".into())],
        ),
        ("gui_mouse_move", vec![Value::Int(100_001), Value::Int(0)]),
        (
            "gui_mouse_drag",
            vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(100_001),
                Value::Int(4),
            ],
        ),
        ("gui_mouse_click", vec![Value::Str("up".into())]),
        ("gui_mouse_scroll", vec![Value::Int(101)]),
        ("gui_type_text", vec![Value::Str(String::new())]),
    ];
    for (name, args) in cases {
        match call(name, args) {
            Ok(v) => panic!("{name} answered {v:?}; expected E_BAD_ARG"),
            Err(e) => assert_eq!(code_of(&e), "E_BAD_ARG", "{name}: {e}"),
        }
    }
    let after = pair_of(
        &call("gui_mouse_position", vec![]).expect("gui_mouse_position reads the pointer"),
        "x",
        "y",
    );
    assert_eq!(before, after, "a refused call moved the pointer");
}

#[test]
#[ignore = "moves to the current point: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn accessibility_refusal_is_e_os_denied_and_says_why() {
    if !disposable() {
        return;
    }
    // Moving the pointer to the point it already occupies has no visible effect,
    // so this test can run whether or not Accessibility is granted.
    let (x, y) = oracle_pos();
    let outcome = call("gui_mouse_move", vec![Value::Int(x), Value::Int(y)]);
    if oracle("trusted") == "true" {
        assert!(
            matches!(outcome, Ok(Value::Bool(true))),
            "with Accessibility granted gui_mouse_move answered {outcome:?}; expected true"
        );
        eprintln!("Accessibility is granted, so gui_mouse_move sent its event");
    } else {
        let e = outcome.expect_err("without Accessibility the move must be refused");
        assert_eq!(code_of(&e), "E_OS_DENIED", "{e}");
        eprintln!(
            "Accessibility is not granted to this runner (AXIsProcessTrusted is false), so gui_mouse_move refused with E_OS_DENIED: {e}"
        );
    }
}
