//! Saying the steps can no longer be undone needs their journal to be gone: a
//! later process loads what is left, and it may hold overwritten files' contents.
//!
//! One test in its own binary: it sets `IRONSHELL_SESSION_DIR` for the process,
//! which also keeps it away from the real journal.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::safety;

#[test]
fn a_journal_that_cannot_be_removed_is_an_error_and_a_removable_one_is_not() {
    let dir = std::env::temp_dir().join(format!("ish_journal_clear_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::env::set_var("IRONSHELL_SESSION_DIR", &dir);
    let mut env = Env::new();

    // A file where the journal directory goes: removing a directory there fails.
    let blocker = dir.join("journal");
    std::fs::write(&blocker, "not a directory").expect("blocker");
    for builtin in ["journal_clear", "session_end"] {
        let err = builtins::call(builtin, vec![], &mut env)
            .expect_err("the journal directory was not removed");
        assert_eq!(safety::code_of(&err).as_str(), "E_IO", "{builtin}: {err:#}");
        assert!(
            format!("{err:#}").contains("could not be removed"),
            "{builtin}: {err:#}"
        );
    }

    // No journal at all is what clearing it asks for.
    std::fs::remove_file(&blocker).expect("remove the blocker");
    builtins::call("journal_clear", vec![], &mut env).expect("an absent journal is clear");

    std::env::remove_var("IRONSHELL_SESSION_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}
