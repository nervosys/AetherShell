//! `config_set` saves to the file the shell reads, and the running shell uses
//! the saved value at once.
//!
//! Each check runs `ish` as a child process. HOME, USERPROFILE, the XDG and
//! IronShell directories and IRONSHELL_CONFIG are pointed at a temporary
//! directory (or removed), so the real user configuration is never read or
//! written.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const AE: &str = env!("CARGO_BIN_EXE_ish");

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("ish-config-set-path-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).expect("create fixture home");
        Fixture { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn config_home(&self) -> PathBuf {
        self.root.join("config-home")
    }

    /// Run `ish -c script`. `custom` sets IRONSHELL_CONFIG; otherwise it is
    /// removed from the child's environment.
    fn run(&self, custom: Option<&Path>, script: &str) -> Output {
        if custom.is_none() {
            refuse_if_real_legacy_file();
        }
        let mut cmd = Command::new(AE);
        cmd.env_remove("IRONSHELL_CONFIG")
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("XDG_CONFIG_HOME", self.root.join("xdg"))
            .env("APPDATA", self.root.join("appdata"))
            .env("IRONSHELL_CONFIG_HOME", self.config_home())
            .env("IRONSHELL_DATA_HOME", self.root.join("data-home"))
            .env("IRONSHELL_CACHE_HOME", self.root.join("cache-home"))
            .env("IRONSHELL_SESSION_DIR", self.root.join("session"));
        if let Some(path) = custom {
            cmd.env("IRONSHELL_CONFIG", path);
        }
        cmd.args(["-c", script]).output().expect("run ish")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// On Windows `dirs` finds the home directory from the user profile, not from
/// HOME, so a child run without IRONSHELL_CONFIG reads and writes the real
/// `~/.ironshellrc` whenever it exists. Refuse such a run rather than touch it.
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

/// The stdout of a run that must succeed.
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

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{} should exist: {e}", path.display()))
}

/// (a) With IRONSHELL_CONFIG set, config_set writes that file and not the
/// config.toml in the config directory.
#[test]
fn with_ironshell_config_set_writes_that_file_only() {
    let fx = Fixture::new("custom");
    let custom = fx.root.join("custom.toml");

    let out = fx.run(
        Some(&custom),
        r#"config_set("ai.default_model", "model-a")"#,
    );
    assert!(stdout_ok(&out).contains("Config saved: ai.default_model = model-a"));

    assert!(
        read(&custom).contains("model-a"),
        "the custom file does not hold the value"
    );
    assert!(
        !fx.config_home().join("config.toml").exists(),
        "config_set wrote config.toml in the config directory instead of IRONSHELL_CONFIG"
    );
}

/// (b) With no IRONSHELL_CONFIG and no file, config_set creates
/// config.toml in the config directory.
#[test]
fn without_a_file_config_set_creates_config_home_config_toml() {
    let fx = Fixture::new("create");

    stdout_ok(&fx.run(None, r#"config_set("ai.default_model", "model-b")"#));

    let created = fx.config_home().join("config.toml");
    assert!(
        read(&created).contains("model-b"),
        "config.toml lacks the value"
    );
    assert!(
        !fx.home().join(".ironshellrc").exists(),
        "config_set created ~/.ironshellrc"
    );
}

/// (c) After config_set, config_get in the same process returns the new value.
#[test]
fn config_get_in_the_same_process_sees_the_saved_value() {
    let fx = Fixture::new("reload");
    let custom = fx.root.join("custom.toml");

    let out = fx.run(
        Some(&custom),
        "config_set(\"ai.default_model\", \"model-c\")\nconfig_get(\"ai.default_model\")",
    );
    let stdout = stdout_ok(&out);
    assert!(
        stdout.contains("model-c"),
        "config_get did not return the value config_set saved: {stdout}"
    );
}

/// (d) When the shell reads ~/.ironshellrc, config_set writes back to it and
/// does not create config.toml. Unix only: on Windows the home directory
/// cannot be redirected to a temporary directory (see `refuse_if_real_legacy_file`).
#[cfg(unix)]
#[test]
fn a_legacy_home_file_in_use_is_written_back() {
    let fx = Fixture::new("legacy");
    let legacy = fx.home().join(".ironshellrc");
    std::fs::write(&legacy, "[ai]\ndefault_model = \"legacy\"\n").expect("write legacy file");

    stdout_ok(&fx.run(None, r#"config_set("ai.default_model", "model-d")"#));

    assert!(
        read(&legacy).contains("model-d"),
        "~/.ironshellrc lacks the value"
    );
    assert!(
        !fx.config_home().join("config.toml").exists(),
        "config_set wrote config.toml although ~/.ironshellrc is the file in use"
    );
}
