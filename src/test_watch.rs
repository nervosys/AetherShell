//! Placeholder for test_watch; replaced by its implementation.
use crate::value::Value;
use anyhow::Result;
pub(super) fn run(name: &str, _args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    Err(crate::safety::unimplemented(
        name,
        "watching and re-running tests is not implemented; NOTHING WAS DONE",
        "use `cargo watch -x test`",
    ))
}
