//! Placeholder for refactor_move_to_file; replaced by its implementation.
use crate::value::Value;
use anyhow::Result;
pub(super) fn run(name: &str, _args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    Err(crate::safety::unimplemented(
        name,
        "not implemented; NOTHING WAS DONE",
        "use an IDE refactoring",
    ))
}
