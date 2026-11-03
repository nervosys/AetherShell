//! `config_reload` replaces the configuration the running shell uses, and a
//! config file that exists but does not parse is reported once at startup.
//!
//! The configuration lives in one process-wide slot, so the in-process checks
//! share one test function. The binary check runs `ish` as a child process.
//! Every check points `IRONSHELL_CONFIG` and the XDG homes at a temporary
//! directory, so the real user configuration is never read or written.

use std::path::PathBuf;
use std::process::Command;

use iron_shell::config::{get_config, reload_config};

const AE: &str = env!("CARGO_BIN_EXE_ish");

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("ish-config-reload-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create fixture directory");
        Fixture { root }
    }

    fn config(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    fn isolate<'a>(&self, cmd: &'a mut Command) -> &'a mut Command {
        cmd.env("IRONSHELL_CONFIG", self.config())
            .env("IRONSHELL_CONFIG_HOME", self.root.join("config-home"))
            .env("IRONSHELL_DATA_HOME", self.root.join("data-home"))
            .env("IRONSHELL_CACHE_HOME", self.root.join("cache-home"))
            .env("IRONSHELL_SESSION_DIR", self.root.join("session"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write_valid(path: &PathBuf, prompt: &str, theme: &str) {
    let text = format!("[prompt]\nformat = \"{prompt}\"\n\n[colors]\ntheme = \"{theme}\"\n");
    std::fs::write(path, text).expect("write valid config");
}

const BROKEN: &str = "[prompt\nformat = \n";

/// (a) a reload installs the new file's values; (b) a reload of a file that
/// does not parse returns an error and leaves the values in use alone.
#[test]
fn reload_replaces_the_settings_in_use_and_failure_keeps_them() {
    let fx = Fixture::new("unit");
    std::env::set_var("IRONSHELL_CONFIG", fx.config());

    write_valid(&fx.config(), "A-prompt", "dracula");
    assert_eq!(get_config().prompt.format, "A-prompt");
    assert_eq!(get_config().colors.theme, "dracula");

    // (b) broken file: error, and the settings in use are still A.
    std::fs::write(fx.config(), BROKEN).expect("write broken config");
    let err = reload_config().expect_err("a file that does not parse must not reload");
    assert!(
        err.to_string().contains("config.toml"),
        "error should name the file: {err}"
    );
    assert_eq!(get_config().prompt.format, "A-prompt");
    assert_eq!(get_config().colors.theme, "dracula");

    // (a) rewrite with B values: reload succeeds and get_config shows B.
    write_valid(&fx.config(), "B-prompt", "nord");
    let reloaded = reload_config().expect("a valid file must reload");
    assert_eq!(reloaded.prompt.format, "B-prompt");
    assert_eq!(get_config().prompt.format, "B-prompt");
    assert_eq!(get_config().colors.theme, "nord");
}

/// A broken file that exists is named on stderr once, at startup, with the
/// defaults in use.
#[test]
fn broken_config_is_named_once_on_stderr_at_startup() {
    let fx = Fixture::new("bin-broken");
    std::fs::write(fx.config(), BROKEN).expect("write broken config");

    let out = fx
        .isolate(&mut Command::new(AE))
        .args(["-c", "config_reload()"])
        .output()
        .expect("run ish");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let startup: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("ish: config "))
        .collect();
    assert_eq!(startup.len(), 1, "stderr was: {stderr}");
    let path = fx.config().display().to_string();
    assert!(
        startup[0].contains(&path) && startup[0].ends_with("; using defaults"),
        "startup line was: {}",
        startup[0]
    );
}

/// No config file means no startup line at all.
#[test]
fn missing_config_prints_nothing_on_stderr() {
    let fx = Fixture::new("bin-missing");

    let out = fx
        .isolate(&mut Command::new(AE))
        .args(["-c", "1 + 1"])
        .output()
        .expect("run ish");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("ish: config "), "stderr was: {stderr}");
}
