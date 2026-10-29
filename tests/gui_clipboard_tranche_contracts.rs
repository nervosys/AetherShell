//! Refusal contracts for the `gui_*` and `clipboard_*` builtins.
//!
//! Every call in this file is refused before any operating-system call is
//! made. Nothing here reads or writes the clipboard, takes a screenshot, lists,
//! moves, focuses or closes a window, moves or clicks the mouse, presses or
//! types a key, opens a dialog, shows a notification, runs OCR or reads the
//! screen size. A call that would be accepted is never made, so a regression
//! that stops refusing one of these would be caught by its assertion and not
//! by touching a real desktop.

use aethershell::builtins::{call_with_input, is_dispatched};
use aethershell::env::Env;
use aethershell::signature::signature_of;
use aethershell::value::Value;

/// An environment whose undo journal is this test process's own, as the
/// declaration tests use.
fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_gui_contracts_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    Env::new()
}

/// Every name this tranche declares, aliases included.
const NAMES: &[&str] = &[
    "clipboard",
    "clipboard_clear",
    "clipboard_get",
    "clipboard_get_image",
    "clipboard_has_text",
    "clipboard_history",
    "clipboard_set",
    "clipboard_set_image",
    "gui_close_window",
    "gui_color_picker",
    "gui_dialog_file_open",
    "gui_dialog_file_save",
    "gui_dialog_folder",
    "gui_dialog_input",
    "gui_dialog_message",
    "gui_focus_window",
    "gui_get_active_window",
    "gui_key_combo",
    "gui_key_press",
    "gui_list_windows",
    "gui_maximize_window",
    "gui_minimize_window",
    "gui_mouse_click",
    "gui_mouse_double_click",
    "gui_mouse_drag",
    "gui_mouse_move",
    "gui_mouse_position",
    "gui_mouse_scroll",
    "gui_move_window",
    "gui_notify",
    "gui_ocr",
    "gui_resize_window",
    "gui_screen_size",
    "gui_screenshot",
    "gui_screenshot_window",
    "gui_type_text",
    "gui_wait",
    "screenshot",
    "windows",
];

/// Runs one call and returns its error text. A call that succeeds fails the
/// test: every case here must be refused.
fn refusal(name: &str, args: Vec<Value>, input: Option<Value>) -> String {
    match call_with_input(name, args, input, &mut env()) {
        Ok(v) => panic!("{name} answered {v:?} where it must refuse"),
        Err(e) => e.to_string(),
    }
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

#[test]
fn every_name_in_the_tranche_is_declared_and_dispatched() {
    for name in NAMES {
        assert!(signature_of(name).is_some(), "{name} has no Signature");
        assert!(is_dispatched(name), "{name} is not dispatched");
    }
}

#[test]
fn every_alias_resolves_to_its_implementations_declaration() {
    let pairs = [
        ("windows", "gui_list_windows"),
        ("screenshot", "gui_screenshot"),
        ("clipboard", "clipboard_get"),
    ];
    for (alias, canonical) in pairs {
        let sig = signature_of(alias).unwrap_or_else(|| panic!("{alias} is not declared"));
        assert_eq!(
            sig.name, canonical,
            "{alias} resolves to the wrong declaration"
        );
    }
}

#[test]
fn zero_parameter_builtins_refuse_surplus_arguments_and_piped_input() {
    let zero = [
        "gui_list_windows",
        "gui_mouse_position",
        "gui_get_active_window",
        "gui_screen_size",
        "gui_color_picker",
        "clipboard_get",
        "clipboard_clear",
        "clipboard_has_text",
        "clipboard_get_image",
        "clipboard_history",
        "clipboard_types",
    ];
    for name in zero {
        let msg = refusal(name, vec![Value::Int(1)], None);
        assert!(msg.contains("E_BAD_ARG"), "{name} with an argument: {msg}");
        let msg = refusal(name, vec![], Some(Value::Int(1)));
        assert!(msg.contains("E_BAD_ARG"), "{name} with piped input: {msg}");
    }
}

#[test]
fn wrong_typed_and_surplus_arguments_are_refused_before_any_tool_runs() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("gui_focus_window", vec![Value::Float(1.5)]),
        ("gui_focus_window", vec![Value::Int(0)]),
        ("gui_focus_window", vec![Value::Int(-5)]),
        ("gui_focus_window", vec![s("a"), s("b")]),
        ("gui_minimize_window", vec![Value::Int(1)]),
        ("gui_minimize_window", vec![s("a"), s("b")]),
        ("gui_maximize_window", vec![Value::Int(1)]),
        ("gui_close_window", vec![s("a"), s("b")]),
        ("gui_move_window", vec![s("a"), s("x"), Value::Int(1)]),
        ("gui_move_window", vec![s("a"), Value::Int(1)]),
        (
            "gui_move_window",
            vec![s("a"), Value::Int(1_000_000), Value::Int(0)],
        ),
        (
            "gui_resize_window",
            vec![s("a"), Value::Int(0), Value::Int(10)],
        ),
        (
            "gui_resize_window",
            vec![s("a"), Value::Int(10), Value::Int(100_001)],
        ),
        ("gui_resize_window", vec![s("a"), Value::Int(1)]),
        ("gui_screenshot", vec![Value::Int(1)]),
        ("gui_screenshot", vec![s("a.png"), s("b.png")]),
        ("gui_screenshot", vec![s("")]),
        ("screenshot", vec![Value::Int(1)]),
        ("gui_screenshot_window", vec![Value::Int(1)]),
        ("gui_screenshot_window", vec![s("a"), Value::Int(2)]),
        ("gui_screenshot_window", vec![s("a"), s("b.png"), s("c")]),
        ("gui_mouse_move", vec![s("a"), Value::Int(1)]),
        (
            "gui_mouse_move",
            vec![Value::Int(1), Value::Int(2), Value::Int(3)],
        ),
        (
            "gui_mouse_move",
            vec![Value::Int(1_000_000_000), Value::Int(0)],
        ),
        ("gui_mouse_click", vec![s("up")]),
        ("gui_mouse_click", vec![Value::Int(4)]),
        ("gui_mouse_click", vec![s("left"), Value::Int(1)]),
        ("gui_mouse_double_click", vec![s("up")]),
        ("gui_mouse_double_click", vec![Value::Int(1), Value::Int(2)]),
        (
            "gui_mouse_drag",
            vec![Value::Int(1), Value::Int(2), Value::Int(3)],
        ),
        (
            "gui_mouse_drag",
            vec![Value::Int(1), Value::Int(2), Value::Int(3), s("4")],
        ),
        ("gui_mouse_scroll", vec![Value::Int(101)]),
        ("gui_mouse_scroll", vec![Value::Int(-101)]),
        ("gui_mouse_scroll", vec![s("up")]),
        ("gui_mouse_scroll", vec![Value::Int(1), Value::Int(2)]),
        ("gui_key_press", vec![s("")]),
        ("gui_key_press", vec![s("nope")]),
        ("gui_key_press", vec![s("ctrl+")]),
        ("gui_key_press", vec![s("+")]),
        ("gui_key_press", vec![s("ctrl")]),
        ("gui_key_press", vec![s("ctrl+enter+x")]),
        ("gui_key_press", vec![s("alt+ctrl")]),
        ("gui_key_press", vec![s(&"a".repeat(100))]),
        ("gui_key_press", vec![Value::Int(1)]),
        ("gui_key_combo", vec![]),
        ("gui_key_combo", vec![s("ctrl"), s("nope")]),
        ("gui_key_combo", vec![s("ctrl"), Value::Int(5), s("c")]),
        ("gui_key_combo", vec![s("ctrl")]),
        ("gui_type_text", vec![s("")]),
        ("gui_type_text", vec![Value::Int(1)]),
        ("gui_type_text", vec![s("a\u{7}b")]),
        ("gui_type_text", vec![s("a\u{0}b")]),
        ("gui_type_text", vec![s(&"x".repeat(10_001))]),
        ("gui_ocr", vec![Value::Int(1)]),
        ("gui_ocr", vec![s("")]),
        ("gui_ocr", vec![s("-x")]),
        ("gui_ocr", vec![s("a.png"), s("b.png")]),
        ("gui_notify", vec![Value::Int(1)]),
        ("gui_notify", vec![s("t"), Value::Int(2)]),
        ("gui_notify", vec![s("")]),
        ("gui_notify", vec![s("t"), s("m"), s("x")]),
        ("gui_dialog_message", vec![Value::Int(1)]),
        ("gui_dialog_message", vec![s("")]),
        ("gui_dialog_message", vec![s("m"), Value::Int(2)]),
        ("gui_dialog_message", vec![s("m"), s("")]),
        ("gui_dialog_message", vec![s("m"), s("t"), s("x")]),
        ("gui_dialog_input", vec![Value::Int(1)]),
        ("gui_dialog_input", vec![s("p"), Value::Int(2)]),
        ("gui_dialog_input", vec![s("p"), s("t"), s("x")]),
        ("gui_dialog_file_open", vec![Value::Int(1)]),
        ("gui_dialog_file_open", vec![s("a"), s("b")]),
        ("gui_dialog_file_save", vec![Value::Int(1)]),
        ("gui_dialog_file_save", vec![s("a"), s("b")]),
        ("gui_dialog_folder", vec![Value::Int(1)]),
        ("gui_dialog_folder", vec![s("a"), s("b")]),
        ("gui_wait", vec![s("x")]),
        ("gui_wait", vec![Value::Int(-1)]),
        ("gui_wait", vec![Value::Int(3_600_001)]),
        ("gui_wait", vec![Value::Int(1), Value::Int(2)]),
        ("clipboard_set", vec![Value::Int(1)]),
        ("clipboard_set", vec![s("a"), s("b")]),
        ("clipboard_set_image", vec![Value::Int(1)]),
        ("clipboard_set_image", vec![s("a"), s("b")]),
    ];
    for (name, args) in cases {
        let msg = refusal(name, args.clone(), None);
        assert!(
            msg.contains("E_BAD_ARG"),
            "{name} {args:?} was not refused as E_BAD_ARG: {msg}"
        );
    }
}

#[test]
fn piped_input_is_refused_where_no_pipeline_input_is_documented() {
    let cases: Vec<(&str, Vec<Value>, Value)> = vec![
        ("gui_focus_window", vec![s("a")], Value::Int(1)),
        ("gui_minimize_window", vec![s("a")], Value::Int(1)),
        ("gui_maximize_window", vec![s("a")], Value::Int(1)),
        ("gui_close_window", vec![s("a")], Value::Int(1)),
        (
            "gui_move_window",
            vec![s("a"), Value::Int(1), Value::Int(2)],
            Value::Int(1),
        ),
        (
            "gui_resize_window",
            vec![s("a"), Value::Int(1), Value::Int(2)],
            Value::Int(1),
        ),
        ("gui_screenshot", vec![s("a.png")], Value::Int(1)),
        ("gui_screenshot_window", vec![s("a")], Value::Int(1)),
        (
            "gui_mouse_move",
            vec![Value::Int(1), Value::Int(2)],
            Value::Int(1),
        ),
        ("gui_mouse_click", vec![s("left")], Value::Int(1)),
        ("gui_mouse_double_click", vec![], Value::Int(1)),
        (
            "gui_mouse_drag",
            vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)],
            Value::Int(1),
        ),
        ("gui_mouse_scroll", vec![Value::Int(1)], Value::Int(1)),
        ("gui_key_press", vec![s("enter")], Value::Int(1)),
        ("gui_key_combo", vec![s("ctrl"), s("c")], Value::Int(1)),
        ("gui_type_text", vec![s("a")], Value::Int(1)),
        ("gui_ocr", vec![s("a.png")], Value::Int(1)),
        ("gui_notify", vec![s("t")], Value::Int(1)),
        ("gui_dialog_message", vec![s("m")], Value::Int(1)),
        ("gui_dialog_input", vec![s("p")], Value::Int(1)),
        ("gui_dialog_file_open", vec![s("t")], Value::Int(1)),
        ("gui_dialog_file_save", vec![s("t")], Value::Int(1)),
        ("gui_dialog_folder", vec![s("t")], Value::Int(1)),
        ("gui_wait", vec![Value::Int(1)], Value::Int(1)),
        ("clipboard_set", vec![s("a")], s("b")),
        ("clipboard_set", vec![], Value::Int(5)),
        ("clipboard_set_image", vec![s("a")], Value::Int(1)),
    ];
    for (name, args, input) in cases {
        let msg = refusal(name, args, Some(input));
        assert!(
            msg.contains("E_BAD_ARG"),
            "{name} accepted piped input: {msg}"
        );
    }
}

#[test]
fn a_clipboard_set_with_neither_text_nor_a_string_is_refused() {
    // Neither form copies anything: the refusal is what this asserts.
    let msg = refusal("clipboard_set", vec![], None);
    assert!(msg.contains("E_BAD_ARG"), "no text: {msg}");
}

#[test]
fn a_capture_checks_its_argument_before_anything_else() {
    // gui_screenshot_window is implemented on Linux, Windows and macOS, and the
    // clipboard image calls on Linux. Their argument is checked first on every
    // OS, so a bad call says so.
    let msg = refusal("gui_screenshot_window", vec![Value::Int(1)], None);
    assert!(msg.contains("E_BAD_ARG"), "{msg}");
    let msg = refusal("clipboard_set_image", vec![Value::Int(1)], None);
    assert!(msg.contains("E_BAD_ARG"), "{msg}");
}
