//! Contracts for clipboard_set_image, clipboard_get_image and clipboard_types
//! on Windows and macOS, against the real clipboard.
//!
//! Every test here writes or reads the real clipboard, so every test is ignored
//! and also returns early unless AE_DISPOSABLE_HOST=1, which only the
//! disposable CI runner sets. Run them with `cargo test -- --ignored` on that
//! runner only. A developer's clipboard is never touched by a plain test run.

#![cfg(any(windows, target_os = "macos"))]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

// The clipboard and AETHER_WORKSPACE are process-global, so tests take this lock.
static LOCK: Mutex<()> = Mutex::new(());

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn call(name: &str, args: Vec<Value>, env: &mut Env) -> anyhow::Result<Value> {
    aethershell::builtins::call(name, args, env)
}

fn code(result: anyhow::Result<Value>, expected: &str) {
    let error = result.expect_err("the call should have been refused");
    assert!(error.to_string().contains(expected), "{error}");
}

fn fields(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Record(map) => map,
        other => panic!("expected a Record, got {other:?}"),
    }
}

/// True only on the disposable CI runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

/// A fresh directory under the temp dir, made the workspace for one test, and
/// removed afterwards even when the test panics.
struct Workspace(std::path::PathBuf);

impl Workspace {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "ae_clip_image_{tag}_{}_{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create workspace");
        std::env::set_var("AETHER_WORKSPACE", &dir);
        Workspace(dir)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        std::env::remove_var("AETHER_WORKSPACE");
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A 2x2 opaque PNG with four distinct colours, encoded in the test.
fn png_2x2() -> Vec<u8> {
    let mut img = image::RgbaImage::new(2, 2);
    img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    img.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    img.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
    img.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageOutputFormat::Png,
        )
        .expect("encode the test PNG");
    bytes
}

/// Dimensions and RGBA pixels of an image. The OS may re-encode the PNG, so
/// tests compare decoded pixels, never the raw bytes.
fn decoded(bytes: &[u8]) -> (u32, u32, Vec<[u8; 4]>) {
    let img = image::load_from_memory(bytes)
        .expect("the answer decodes as an image")
        .to_rgba8();
    let (width, height) = img.dimensions();
    (width, height, img.pixels().map(|p| p.0).collect())
}

/// Copies the test PNG to the clipboard through clipboard_set_image.
fn set_test_image(ws: &Workspace, env: &mut Env) {
    let path = ws.path("two-by-two.png");
    std::fs::write(&path, png_2x2()).expect("write the test PNG");
    let copied = call("clipboard_set_image", vec![s(&path)], env).expect("the image is copied");
    assert!(matches!(copied, Value::Bool(true)), "{copied:?}");
}

#[test]
#[ignore = "writes the real clipboard: disposable runner only (AE_DISPOSABLE_HOST=1)"]
fn set_then_get_round_trips_the_pixels() {
    if !disposable() {
        return;
    }
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Workspace::new("round_trip");
    let mut env = Env::default();
    set_test_image(&ws, &mut env);

    let rec = fields(call("clipboard_get_image", vec![], &mut env).expect("the image is read"));
    assert!(
        matches!(rec.get("mime"), Some(Value::Str(m)) if m == "image/png"),
        "{rec:?}"
    );
    let base64 = match rec.get("base64") {
        Some(Value::Str(b)) => b.clone(),
        other => panic!("base64: expected a String, got {other:?}"),
    };
    let bytes = match rec.get("bytes") {
        Some(Value::Int(n)) => *n,
        other => panic!("bytes: expected an Int, got {other:?}"),
    };
    let raw = base64_decode(&base64);
    assert_eq!(
        raw.len() as i64,
        bytes,
        "bytes must match the base64 payload"
    );

    let (width, height, pixels) = decoded(&raw);
    let (want_w, want_h, want_pixels) = decoded(&png_2x2());
    assert_eq!((width, height), (want_w, want_h));
    assert_eq!(pixels, want_pixels);
}

#[test]
#[ignore = "writes the real clipboard: disposable runner only (AE_DISPOSABLE_HOST=1)"]
fn types_reports_an_image_after_the_set() {
    if !disposable() {
        return;
    }
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Workspace::new("types_image");
    let mut env = Env::default();
    set_test_image(&ws, &mut env);

    let types = call("clipboard_types", vec![], &mut env).expect("the formats are listed");
    match types {
        Value::Array(items) => assert!(
            items
                .iter()
                .any(|v| matches!(v, Value::Str(t) if t == "image")),
            "{items:?}"
        ),
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
#[ignore = "writes the real clipboard: disposable runner only (AE_DISPOSABLE_HOST=1)"]
fn text_on_the_clipboard_is_not_found_as_an_image() {
    if !disposable() {
        return;
    }
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut env = Env::default();
    let copied =
        call("clipboard_set", vec![s("ae-no-image-here")], &mut env).expect("the text is copied");
    assert!(matches!(copied, Value::Bool(true)), "{copied:?}");

    code(call("clipboard_get_image", vec![], &mut env), "E_NOT_FOUND");
    let types = call("clipboard_types", vec![], &mut env).expect("the formats are listed");
    match types {
        Value::Array(items) => {
            assert!(
                items
                    .iter()
                    .any(|v| matches!(v, Value::Str(t) if t == "text")),
                "{items:?}"
            );
            assert!(
                !items
                    .iter()
                    .any(|v| matches!(v, Value::Str(t) if t == "image")),
                "{items:?}"
            );
        }
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
#[ignore = "writes the real clipboard: disposable runner only (AE_DISPOSABLE_HOST=1)"]
fn oversize_image_file_is_refused_before_the_clipboard() {
    if !disposable() {
        return;
    }
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Workspace::new("oversize");
    let mut env = Env::default();
    // A PNG signature, then zeros: one byte past the 32 MiB cap.
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(32 * 1024 * 1024 + 1, 0);
    let path = ws.path("too-big.png");
    std::fs::write(&path, &bytes).expect("write the oversize file");
    code(
        call("clipboard_set_image", vec![s(&path)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
}

/// The payload of a standard base64 string, decoded for the test.
fn base64_decode(text: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .expect("the payload is base64")
}
