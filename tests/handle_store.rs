//! The handle store's count limit must hold however the session directory is
//! written: by this process, many times over, and by another writer that adds
//! files behind its back. The timing test measures what filling a session
//! costs; it is ignored by default so that a normal run does not wait for it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use iron_shell::handles::{self, MAX_HANDLES};
use iron_shell::value::Value;

/// Serialises the tests: the handle store is process-global, and each test
/// points `IRONSHELL_SESSION_DIR` at its own directory.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the lock and a fresh, empty session directory for as long as it lives.
struct Session {
    _guard: MutexGuard<'static, ()>,
    dir: PathBuf,
}

impl Session {
    fn new(name: &str) -> Session {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir =
            std::env::temp_dir().join(format!("ish-handle-store-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("IRONSHELL_SESSION_DIR", &dir);
        handles::clear();
        std::fs::create_dir_all(&dir).expect("create session dir");
        Session { _guard: guard, dir }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        handles::clear();
        std::env::remove_var("IRONSHELL_SESSION_DIR");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The number a handle id carries: `h17` is 17.
fn id_number(id: &str) -> usize {
    id[1..].parse().expect("a handle id is h<number>")
}

#[test]
fn the_limit_holds_after_filling_a_session() {
    let _session = Session::new("fill");

    for i in 0..MAX_HANDLES {
        handles::put(Value::Int(i as i64), 9999)
            .unwrap_or_else(|e| panic!("put {i} is under the cap: {e}"));
    }
    let refused = handles::put(Value::Int(0), 9999).expect_err("the cap must hold");
    let message = format!("{refused}");
    assert!(message.contains("E_BUDGET_EXCEEDED"), "{message}");
}

#[test]
fn a_second_writer_is_counted_against_the_limit() {
    let session = Session::new("second-writer");

    let mut mine = Vec::new();
    for i in 0..10 {
        mine.push(handles::put(Value::Int(i), 9999).expect("under the cap"));
    }

    // Another process writing to the same directory, simulated by writing the
    // files directly. Its handles fill the session up to six below the cap.
    let template = std::fs::read(session.dir.join(format!("{}.json", mine[0].id)))
        .expect("read one of this process's handle files");
    let foreign_top = MAX_HANDLES - 6;
    for n in 11..=foreign_top {
        std::fs::write(session.dir.join(format!("h{n}.json")), &template).expect("foreign write");
    }

    // The session has six free slots. Our next ids must go past the foreign
    // ones, or a put would overwrite another writer's handle.
    for _ in 0..6 {
        let h = handles::put(Value::Int(1), 9999).expect("a free slot");
        assert!(
            id_number(&h.id) > foreign_top,
            "{} collides with the other writer's handles",
            h.id
        );
    }
    let refused = handles::put(Value::Int(1), 9999).expect_err("the seventh must be refused");
    assert!(format!("{refused}").contains("E_BUDGET_EXCEEDED"));

    // And what this process stored earlier is still there, unharmed.
    assert_eq!(handles::get(&mine[0].id), Some(Value::Int(0)));
}

#[test]
fn dropping_a_handle_frees_a_slot_at_the_limit() {
    let _session = Session::new("drop");

    let mut first = None;
    for i in 0..MAX_HANDLES {
        let h = handles::put(Value::Int(i as i64), 9999).expect("under the cap");
        first.get_or_insert(h);
    }
    assert!(handles::put(Value::Int(0), 9999).is_err());

    assert!(handles::drop_handle(&first.expect("a first handle").id));
    handles::put(Value::Int(0), 9999).expect("the freed slot is usable");
    assert!(
        handles::put(Value::Int(0), 9999).is_err(),
        "and only that one"
    );
}

/// Times filling a session with 4,000 puts, all under the real cap. Run with
/// `cargo test --test handle_store -- --ignored --nocapture`.
#[test]
#[ignore = "timing: run with --ignored --nocapture"]
fn timing_of_4000_puts() {
    let _session = Session::new("timing");

    let started = Instant::now();
    for i in 0..4000 {
        handles::put(Value::Int(i), 9999).expect("under the cap");
    }
    let elapsed = started.elapsed();
    println!(
        "TIMING 4000 puts: {:.1} ms total, {:.1} us per put",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1_000_000.0 / 4000.0
    );
}

#[test]
fn an_id_that_is_not_a_handle_never_reaches_the_file_system() {
    let session = Session::new("traversal");
    let outside = session
        .dir
        .parent()
        .unwrap()
        .join(format!("ish-handle-victim-{}.json", std::process::id()));
    std::fs::write(&outside, "{}").unwrap();
    let id = format!("../{}", outside.file_stem().unwrap().to_str().unwrap());

    let dropped = handles::drop_handle(&id);
    let still_there = outside.exists();
    let _ = std::fs::remove_file(&outside);

    assert!(!dropped, "a path was accepted as a handle id");
    assert!(
        still_there,
        "drop_handle deleted a file outside the session"
    );
    assert!(handles::get(&id).is_none());
}
