//! `a2ui_notify`'s `sent` means queued. `ui_attached` is what says whether anyone
//! is reading the queue, so an agent is not left believing a user saw the message.
//!
//! One test in its own binary: attachment is a one-way flag on a global channel.

use iron_shell::ai::a2ui::A2UI_CHANNEL;
use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;

fn notify() -> Value {
    let mut env = Env::new();
    builtins::call("a2ui_notify", vec![Value::Str("probe".into())], &mut env).expect("a2ui_notify")
}

fn field(v: &Value, name: &str) -> Value {
    match v {
        Value::Record(r) => r.get(name).cloned().expect(name),
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn notify_says_queued_and_whether_a_ui_has_read_the_queue() {
    let before = notify();
    assert_eq!(field(&before, "sent"), Value::Bool(true));
    assert_eq!(
        field(&before, "ui_attached"),
        Value::Bool(false),
        "no UI has read this channel yet"
    );

    A2UI_CHANNEL.receive_all().expect("a UI drains the channel");

    let after = notify();
    assert_eq!(field(&after, "sent"), Value::Bool(true));
    assert_eq!(field(&after, "ui_attached"), Value::Bool(true));
}
