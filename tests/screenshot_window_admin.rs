//! Contracts for `gui_screenshot_window` on Windows and macOS.
//!
//! Every test here captures the real desktop, so each is `#[ignore]`d and also
//! returns early unless AE_DISPOSABLE_HOST=1, which only the disposable CI
//! runner sets. Run them with `cargo test -- --ignored` on that runner only.
//! Each starts its own window with a unique title and closes it through a guard
//! that runs even when the test panics.

#![cfg(any(windows, target_os = "macos"))]

use aethershell::env::Env;
use aethershell::value::Value;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::process::{Child, Command, Stdio};

#[cfg(target_os = "macos")]
use std::process::Command;

/// True only on the disposable CI runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// Calls a builtin with the session journal isolated from the repository's.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    static ISOLATED: Once = Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_shot_admin_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    aethershell::builtins::call(name, args, &mut Env::new())
}

/// A scratch directory for this test process.
fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_shot_admin_dir_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The width and height from a PNG's IHDR chunk, after checking the signature.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(
        bytes.len() >= 24,
        "a {}-byte file is not a PNG",
        bytes.len()
    );
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
    assert_eq!(&bytes[12..16], b"IHDR", "the PNG has no IHDR chunk");
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (w, h)
}

/// Captures `title` to `path`, retrying while no window has that title yet.
/// Any other refusal fails at once, so a permission problem is not waited on.
fn capture_when_shown(title: &str, path: &str, within: Duration) -> Value {
    let deadline = Instant::now() + within;
    loop {
        match call("gui_screenshot_window", vec![s(title), s(path)]) {
            Ok(v) => return v,
            Err(e) if e.to_string().contains("E_NOT_FOUND") && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => panic!("{title}: {e}"),
        }
    }
}

/// A title that names no window: E_NOT_FOUND, and nothing is written.
fn assert_missing_window_is_not_found(title: &str, path: &Path) {
    let msg = match call(
        "gui_screenshot_window",
        vec![s(title), s(&path.to_string_lossy())],
    ) {
        Ok(v) => panic!("answered {v:?} for a title that names no window"),
        Err(e) => e.to_string(),
    };
    assert!(msg.contains("E_NOT_FOUND"), "missing window: {msg}");
    assert!(
        !path.exists(),
        "nothing may be written for a missing window"
    );
}

// ------------------------------------------------------------------- Windows

/// A WinForms form of 400 by 300 pixels with the given title. Its process
/// declares itself DPI-aware, so the form is 400 by 300 physical pixels and the
/// capture (which is DPI-aware too) measures the same size.
#[cfg(windows)]
const FORM_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System.Runtime.InteropServices;
public class FormDpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
"@
[void][FormDpi]::SetProcessDPIAware()
$form = New-Object System.Windows.Forms.Form
$form.Text = [Environment]::GetEnvironmentVariable('AE_TEST_TITLE')
$form.Size = New-Object System.Drawing.Size(400, 300)
$form.StartPosition = 'CenterScreen'
[System.Windows.Forms.Application]::Run($form)
"#;

/// Kills the process it owns, even while a panic unwinds.
#[cfg(windows)]
struct Owned(Child);

#[cfg(windows)]
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(windows)]
fn start_form(title: &str) -> Owned {
    let child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-Command",
            FORM_SCRIPT,
        ])
        .env("AE_TEST_TITLE", title)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("powershell.exe runs on the disposable runner");
    Owned(child)
}

/// The titles `gui_list_windows` names right now.
#[cfg(windows)]
fn listed_titles() -> Vec<String> {
    match call("gui_list_windows", vec![]).expect("gui_list_windows answers on Windows") {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::Record(map) => match map.get("title") {
                    Some(Value::Str(title)) => Some(title.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        other => panic!("gui_list_windows answered {other:?}, not an Array"),
    }
}

#[cfg(windows)]
#[test]
#[ignore = "captures the real desktop: disposable runner only (AE_DISPOSABLE_HOST=1)"]
fn a_window_is_listed_then_captured_by_its_exact_title() {
    if !disposable() {
        return;
    }
    let title = format!("ae shot {}", std::process::id());
    let _form = start_form(&title);

    // The listing builtin names the window by its title; that is the id the
    // capture takes.
    let listed_by = Instant::now() + Duration::from_secs(20);
    while !listed_titles().contains(&title) {
        assert!(
            Instant::now() < listed_by,
            "{title} was not listed within 20 s"
        );
        std::thread::sleep(Duration::from_millis(200));
    }

    let dir = scratch_dir();
    let path = dir.join("form.png");
    let path_text = path.to_string_lossy().into_owned();
    let answer = capture_when_shown(&title, &path_text, Duration::from_secs(10));
    assert_eq!(answer, s(&path_text));

    let bytes = std::fs::read(&path).expect("the PNG was written");
    let (w, h) = png_size(&bytes);
    assert!(
        w.abs_diff(400) <= 2 && h.abs_diff(300) <= 2,
        "the form is 400x300 but the PNG is {w}x{h}"
    );

    assert_missing_window_is_not_found(
        &format!("no window has this title {}", std::process::id()),
        &dir.join("none.png"),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --------------------------------------------------------------------- macOS

/// Closes the TextEdit window this test opened, saving nothing.
#[cfg(target_os = "macos")]
struct QuitTextEdit;

#[cfg(target_os = "macos")]
impl Drop for QuitTextEdit {
    fn drop(&mut self) {
        let _ = Command::new("osascript")
            .args(["-e", "tell application \"TextEdit\" to quit saving no"])
            .status();
    }
}

/// Opens `document` in TextEdit. Its window title is the file name.
#[cfg(target_os = "macos")]
fn start_document(document: &Path) -> QuitTextEdit {
    let status = Command::new("open")
        .args(["-a", "TextEdit"])
        .arg(document)
        .status()
        .expect("open runs on the disposable runner");
    assert!(status.success(), "open -a TextEdit failed");
    QuitTextEdit
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "captures the real desktop: disposable runner only (AE_DISPOSABLE_HOST=1)"]
fn a_textedit_window_is_captured_by_its_exact_title() {
    if !disposable() {
        return;
    }
    let dir = scratch_dir();
    let document = dir.join(format!("ae-shot-{}.txt", std::process::id()));
    std::fs::write(&document, "hello from the test\n").expect("document");
    // The title TextEdit shows for the window: the file's name.
    let title = document
        .file_name()
        .expect("a file name")
        .to_string_lossy()
        .into_owned();
    let _window = start_document(&document);

    let path = dir.join("window.png");
    let path_text = path.to_string_lossy().into_owned();
    let answer = capture_when_shown(&title, &path_text, Duration::from_secs(20));
    assert_eq!(answer, s(&path_text));

    let bytes = std::fs::read(&path).expect("the PNG was written");
    let (w, h) = png_size(&bytes);
    // The window's size in pixels is not read back here, so this checks only
    // that the image has a size.
    assert!(w > 0 && h > 0, "the PNG is {w}x{h}");

    assert_missing_window_is_not_found(
        &format!("no window has this title {}", std::process::id()),
        &dir.join("none.png"),
    );
    let _ = std::fs::remove_dir_all(&dir);
}
