//! `diff` in `ish -b`, run through the real binary: the two files are read in
//! process, and the output and status are the ones GNU `diff` gives on the
//! same inputs. The unit tests in `ironshell-posix` cover the algorithm over
//! a fake host; these cover the shell end to end, in agent mode, where a
//! command that is not in process would have to pass the gate.

use std::path::PathBuf;
use std::process::{Command, Output};

struct Dirs {
    root: PathBuf,
    workspace: PathBuf,
    session: PathBuf,
}

impl Dirs {
    fn new(tag: &str) -> Dirs {
        let root = std::env::temp_dir().join(format!("ae-posix-diff-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let d = Dirs {
            workspace: root.join("ws"),
            session: root.join("session"),
            root,
        };
        std::fs::create_dir_all(&d.workspace).unwrap();
        std::fs::create_dir_all(&d.session).unwrap();
        d
    }

    /// `ish --bash -c CMD` from the workspace, in agent mode.
    fn run(&self, cmd: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ish"))
            .args(["--bash", "-c", cmd])
            .current_dir(&self.workspace)
            .env("NO_COLOR", "1")
            .env("IRONSHELL_MODE", "agent")
            .env("IRONSHELL_WORKSPACE", &self.workspace)
            .env("IRONSHELL_SESSION_DIR", &self.session)
            .env("IRONSHELL_ALLOW_SH", "")
            .output()
            .expect("run ish")
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn diff_compares_files_in_process_with_gnu_output_and_status() {
    let d = Dirs::new("compare");
    std::fs::write(d.workspace.join("a.txt"), "1\n2\n3\n").unwrap();
    std::fs::write(d.workspace.join("b.txt"), "1\nTWO\n3\n").unwrap();
    std::fs::write(d.workspace.join("same.txt"), "1\n2\n3\n").unwrap();

    // GNU: diff a.txt b.txt => "2c2 / < 2 / --- / > TWO", status 1.
    let o = d.run("diff a.txt b.txt");
    assert_eq!(stdout(&o), "2c2\n< 2\n---\n> TWO\n", "{o:?}");
    assert_eq!(o.status.code(), Some(1), "{o:?}");

    // GNU: diff -u --label A --label B a.txt b.txt, status 1.
    let o = d.run("diff -u --label A --label B a.txt b.txt");
    assert_eq!(
        stdout(&o),
        "--- A\n+++ B\n@@ -1,3 +1,3 @@\n 1\n-2\n+TWO\n 3\n",
        "{o:?}"
    );
    assert_eq!(o.status.code(), Some(1), "{o:?}");

    // GNU: diff -q a.txt b.txt => "Files a.txt and b.txt differ", status 1.
    let o = d.run("diff -q a.txt b.txt");
    assert_eq!(stdout(&o), "Files a.txt and b.txt differ\n", "{o:?}");
    assert_eq!(o.status.code(), Some(1), "{o:?}");

    // GNU: identical files print nothing and exit 0.
    let o = d.run("diff a.txt same.txt");
    assert_eq!(stdout(&o), "", "{o:?}");
    assert_eq!(o.status.code(), Some(0), "{o:?}");

    // GNU: standard input as the first file, against b.txt.
    let o = d.run("printf '1\\n2\\n3\\n' | diff - b.txt");
    assert_eq!(stdout(&o), "2c2\n< 2\n---\n> TWO\n", "{o:?}");
    assert_eq!(o.status.code(), Some(1), "{o:?}");

    // GNU: a missing file is named on standard error, and the status is 2.
    let o = d.run("diff a.txt nosuch.txt");
    assert_eq!(stdout(&o), "", "{o:?}");
    assert_eq!(
        stderr(&o),
        "diff: nosuch.txt: No such file or directory\n",
        "{o:?}"
    );
    assert_eq!(o.status.code(), Some(2), "{o:?}");
}
