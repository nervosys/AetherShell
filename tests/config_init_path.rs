//! `config_init` creates the file the shell reads (`ShellConfig::config_file`),
//! and never overwrites it.
//!
//! Each check runs `ish` as a child process. HOME, USERPROFILE, the XDG and
//! IronShell directories and IRONSHELL_CONFIG are pointed at a temporary
//! directory, so the real user configuration is never read or written.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const AE: &str = env!("CARGO_BIN_EXE_ish");

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("ish-config-init-path-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).expect("create fixture home");
        Fixture { root }
    }

    fn config_home(&self) -> PathBuf {
        self.root.join("config-home")
    }

    /// Run `ish -c script` with IRONSHELL_CONFIG set to `custom`.
    fn run_with_custom(&self, custom: &Path, script: &str) -> Output {
        refuse_if_real_legacy_file();
        Command::new(AE)
            .env("IRONSHELL_CONFIG", custom)
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("xdg"))
            .env("APPDATA", self.root.join("appdata"))
            .env("IRONSHELL_CONFIG_HOME", self.config_home())
            .env("IRONSHELL_DATA_HOME", self.root.join("data-home"))
            .env("IRONSHELL_CACHE_HOME", self.root.join("cache-home"))
            .env("IRONSHELL_SESSION_DIR", self.root.join("session"))
            .args(["-c", script])
            .output()
            .expect("run ish")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// On Windows `dirs` finds the home directory from the user profile, not from
/// HOME, so a run can read the real `~/.ironshellrc` whenever it exists.
/// Refuse to run rather than touch it.
fn refuse_if_real_legacy_file() {
    if cfg!(windows) {
        if let Some(home) = dirs::home_dir() {
            assert!(
                !home.join(".ironshellrc").exists(),
                "refusing to run: the real ~/.ironshellrc exists and cannot be redirected on Windows"
            );
        }
    }
}

fn stdout_ok(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "ish failed: status {:?}, stdout {stdout}, stderr {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

/// Every regular file below `dir`, recursively (empty when `dir` is missing).
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// (a) With IRONSHELL_CONFIG set, config_init creates that file, with the
/// generated default text, and no config.toml in the config home.
#[test]
fn config_init_creates_the_file_the_shell_reads() {
    let fx = Fixture::new("create");
    let custom = fx.root.join("custom.toml");

    let out = stdout_ok(&fx.run_with_custom(&custom, "config_init()"));
    assert!(out.contains("Created config file"), "stdout: {out}");
    assert!(
        out.contains("custom.toml"),
        "stdout does not name the real path: {out}"
    );

    let written = std::fs::read_to_string(&custom).expect("custom.toml was created");
    assert!(
        written.contains("[shell]"),
        "custom.toml is not the generated config"
    );

    let stray: Vec<PathBuf> = files_under(&fx.config_home());
    assert!(
        stray.is_empty(),
        "config_init wrote files into the config home: {stray:?}"
    );
}

/// (b) A second config_init does not change an existing file, and says so.
#[test]
fn config_init_never_overwrites_an_existing_file() {
    let fx = Fixture::new("exists");
    let custom = fx.root.join("custom.toml");
    std::fs::create_dir_all(&fx.root).expect("create fixture root");
    std::fs::write(&custom, "# MARKER\n").expect("write marker");

    let out = stdout_ok(&fx.run_with_custom(&custom, "config_init()"));
    assert!(out.contains("already exists"), "stdout: {out}");
    assert!(
        out.contains("custom.toml"),
        "stdout does not name the real path: {out}"
    );

    let after = std::fs::read_to_string(&custom).expect("custom.toml still exists");
    assert_eq!(
        after, "# MARKER\n",
        "config_init overwrote the existing file"
    );
}
