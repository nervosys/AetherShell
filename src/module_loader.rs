//! `import` and `export`: file modules.
//!
//! Forms (as `parser.rs` parses them):
//!
//! ```text
//! import "lib/math.ae"                  # every export bound into the importer
//! import "lib/math.ae" as math          # a Record of the exports, bound as `math`
//! import { gcd, lcm as l } from "lib/math.ae"
//! export { a, b as c }                  # marks names in this file as exported
//! export { a } from "lib/math.ae"       # re-export one of another module's exports
//! pub let x = 1                         # `pub` also marks a name as exported
//! ```
//!
//! A module is evaluated in its own fresh environment, at most once per
//! process per canonical path. Only its exported names reach the importer.
//! Relative paths resolve against the directory of the file doing the import
//! (the script's directory, or the directory of the module that imports), or
//! against the current directory (or the workspace, when jailed) at the top
//! level of the REPL and `-c`. Every read goes through the same guard as
//! `file.read`, so agent mode cannot import from outside the workspace.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};

use crate::ast::{ExportItem, ImportItem};
use crate::env::Env;
use crate::safety::{self, Effect, ErrorCode, GuardCtx, SafetyError};
use crate::value::Value;

/// Deepest chain of modules that may be loading at once.
pub const MAX_IMPORT_DEPTH: usize = 32;
/// Largest module source accepted, in bytes.
pub const MAX_MODULE_BYTES: u64 = 4 * 1024 * 1024;

/// The exported names of a module, with their values.
type Exports = BTreeMap<String, Value>;

/// Modules evaluated so far in this process, by canonical path.
static CACHE: OnceLock<Mutex<HashMap<PathBuf, Exports>>> = OnceLock::new();

thread_local! {
    /// Directory of each file being evaluated, innermost last. The entry
    /// script's directory is pushed by `enter_entry_file`; each module pushes its
    /// own while it runs. Empty at the top level of the REPL and `-c`.
    static BASE_DIRS: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    /// Canonical paths of the modules being loaded, outermost first.
    static LOADING: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Keeps the entry file's directory in scope for the import resolver.
pub struct EntryFileGuard;

impl Drop for EntryFileGuard {
    fn drop(&mut self) {
        BASE_DIRS.with(|b| {
            b.borrow_mut().pop();
        });
    }
}

/// Make `file`'s directory the base for the imports it makes. Hold the guard
/// for as long as the file's statements run.
pub fn enter_entry_file(file: &Path) -> EntryFileGuard {
    let abs = if file.is_absolute() {
        file.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|c| c.join(file))
            .unwrap_or_else(|_| file.to_path_buf())
    };
    let dir = abs
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    BASE_DIRS.with(|b| b.borrow_mut().push(dir));
    EntryFileGuard
}

/// Pops the module's stacks on every exit path, including a panic.
struct LoadFrame;

impl Drop for LoadFrame {
    fn drop(&mut self) {
        LOADING.with(|l| {
            l.borrow_mut().pop();
        });
        BASE_DIRS.with(|b| {
            b.borrow_mut().pop();
        });
    }
}

/// `import` statement.
pub fn import(
    items: &[ImportItem],
    source: &str,
    alias: Option<&str>,
    env: &mut Env,
) -> Result<()> {
    let (path, module) = load(source)?;
    if !items.is_empty() {
        for item in items {
            let value = module.get(&item.name).ok_or_else(|| {
                safety::not_found(
                    "import",
                    &format!("export in module '{}'", display(&path)),
                    &item.name,
                )
            })?;
            let bound = item.alias.as_deref().unwrap_or(&item.name);
            env.set_var_unchecked(bound.to_string(), value.clone());
        }
    } else if let Some(name) = alias {
        env.set_var_unchecked(name.to_string(), Value::Record(module));
    } else {
        for (name, value) in module {
            env.set_var_unchecked(name, value);
        }
    }
    Ok(())
}

/// `export { ... }` in the file being run: each name must be defined here and
/// is recorded as exported under its alias, if it has one.
pub fn export_local(items: &[ExportItem], env: &mut Env) -> Result<()> {
    for item in items {
        let value = env
            .get_var(&item.name)
            .cloned()
            .ok_or_else(|| safety::not_found("export", "definition", &item.name))?;
        let name = item.alias.as_deref().unwrap_or(&item.name);
        if name != item.name {
            env.set_var_unchecked(name.to_string(), value);
        }
        env.add_export(name);
    }
    Ok(())
}

/// `export { ... } from "path"`: bind and export names taken from another module.
pub fn reexport(items: &[ExportItem], source: &str, env: &mut Env) -> Result<()> {
    let (path, module) = load(source)?;
    for item in items {
        let value = module.get(&item.name).ok_or_else(|| {
            safety::not_found(
                "export",
                &format!("export in module '{}'", display(&path)),
                &item.name,
            )
        })?;
        let name = item.alias.as_deref().unwrap_or(&item.name);
        env.set_var_unchecked(name.to_string(), value.clone());
        env.add_export(name);
    }
    Ok(())
}

/// Load a module's exports, evaluating it on first use.
fn load(source: &str) -> Result<(PathBuf, Exports)> {
    if source.is_empty() || source.contains('\0') {
        return Err(safety::bad_arg(
            "import",
            "a module path",
            if source.is_empty() {
                "an empty string"
            } else {
                "a path with a NUL byte"
            },
        ));
    }
    if source.contains("://") {
        return Err(safety::bad_arg(
            "import",
            "a local .ae file path",
            "a URL; modules are read from the filesystem only",
        ));
    }
    if source.starts_with("pkg:") {
        return Err(safety::unimplemented(
            "import",
            "package imports (`pkg:`) are not wired to the module loader",
            "import the module file by its path",
        ));
    }

    let path = resolve(source)?;
    let key = path.clone();

    if let Some(exports) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Ok((path, exports.clone()));
    }

    let loading_now = LOADING.with(|l| l.borrow().clone());
    if let Some(start) = loading_now.iter().position(|p| *p == path) {
        let mut chain: Vec<String> = loading_now[start..].iter().map(|p| display(p)).collect();
        chain.push(display(&path));
        return Err(safety::bad_arg(
            "import",
            "an acyclic import graph",
            &format!("a cycle: {}", chain.join(" -> ")),
        ));
    }
    if loading_now.len() >= MAX_IMPORT_DEPTH {
        return Err(safety::budget_exceeded(
            "import",
            format!(
                "import depth {} reached loading '{}'; the limit is {MAX_IMPORT_DEPTH}",
                loading_now.len(),
                display(&path)
            ),
            "flatten the module graph, or import fewer modules in a chain",
        ));
    }

    let source_text = read_module(&path)?;
    let stmts = crate::parser::parse_program(&source_text)
        .map_err(|e| in_module(&path, anyhow!("{e:#}")))?;

    let exports = evaluate(&path, &stmts).map_err(|e| in_module(&path, e))?;

    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, exports.clone());
    Ok((path, exports))
}

fn evaluate(path: &Path, stmts: &[crate::ast::Stmt]) -> Result<Exports> {
    let dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    LOADING.with(|l| l.borrow_mut().push(path.to_path_buf()));
    BASE_DIRS.with(|b| b.borrow_mut().push(dir));
    let _frame = LoadFrame;

    let mut module_env = fresh_module_env();
    crate::eval::eval_program(stmts, &mut module_env)?;
    Ok(module_env.exported_vars())
}

#[cfg(feature = "native")]
fn fresh_module_env() -> Env {
    crate::modules::env_with_modules()
}

#[cfg(not(feature = "native"))]
fn fresh_module_env() -> Env {
    Env::new()
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Exports>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The path a source string names before the file system is consulted.
fn module_path(source: &str) -> PathBuf {
    let p = Path::new(source);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match BASE_DIRS.with(|b| b.borrow().last().cloned()) {
        Some(dir) => dir.join(p),
        None => PathBuf::from(safety::resolve_path_str(source)),
    }
}

/// Resolve a source string to the canonical file it names, guarded as a read.
fn resolve(source: &str) -> Result<PathBuf> {
    let mut candidate = module_path(source);
    if !candidate.exists() && candidate.extension().is_none() {
        candidate = PathBuf::from(format!("{}.ae", candidate.display()));
    }
    // Canonicalize when the file exists so a symlink cannot lead the jail
    // check somewhere the lexical path does not show.
    let target = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.clone());
    let shown = display(&target);

    safety::guard(GuardCtx {
        builtin: "import",
        effect: Effect::ReadLocal,
        what: "read",
        targets: vec![shown.clone()],
        blast_radius: serde_json::json!({ "path": shown }),
        reversible: true,
        fs_paths: true,
    })
    .map_err(anyhow::Error::new)?;

    // `guard` jails only writes: reads are unjailed by policy (see the note in
    // builtins.rs beside `web_upload_file`). A module is code the importer runs,
    // so an import is held to the workspace whenever the jail is on: agent mode,
    // or an explicit AETHER_WORKSPACE.
    let jailed = safety::current_mode() == safety::Mode::Agent
        || std::env::var_os("AETHER_WORKSPACE").is_some();
    if jailed && !safety::within_workspace(&shown) {
        return Err(anyhow::Error::new(SafetyError {
            code: ErrorCode::OutsideWorkspace,
            message: format!("import: '{shown}' is outside the workspace root"),
            builtin: "import".to_string(),
            hint: format!(
                "import modules under {} or set AETHER_WORKSPACE",
                safety::workspace_root().display()
            ),
            approval: None,
            did_you_mean: Vec::new(),
            expected: String::new(),
            got: String::new(),
        }));
    }

    if !target.exists() {
        return Err(safety::not_found("import", "module", source));
    }
    if target.is_dir() {
        return Err(safety::bad_arg(
            "import",
            "a .ae file",
            &format!("the directory '{shown}'"),
        ));
    }
    Ok(target)
}

/// Read a module's source, refusing anything over `MAX_MODULE_BYTES`.
fn read_module(path: &Path) -> Result<String> {
    let shown = display(path);
    let file = File::open(path).map_err(|e| {
        safety::bad_state(
            "import",
            &format!("cannot open module '{shown}': {e}"),
            "check that the file exists and is readable",
        )
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_MODULE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            safety::bad_state(
                "import",
                &format!("cannot read module '{shown}': {e}"),
                "check that the file is readable",
            )
        })?;
    if bytes.len() as u64 > MAX_MODULE_BYTES {
        return Err(safety::budget_exceeded(
            "import",
            format!("module '{shown}' is over the {MAX_MODULE_BYTES}-byte limit"),
            "split the module into smaller files",
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        safety::bad_arg(
            "import",
            "a UTF-8 source file",
            &format!("the non-UTF-8 file '{shown}'"),
        )
    })
}

/// Attach the module path to an error raised while loading or evaluating it.
fn in_module(path: &Path, err: anyhow::Error) -> anyhow::Error {
    let shown = display(path);
    match err.downcast::<SafetyError>() {
        Ok(mut coded) => {
            coded.message = format!("{} (in module '{shown}')", coded.message);
            anyhow::Error::new(coded)
        }
        Err(other) => anyhow!("in module '{shown}': {other:#}"),
    }
}

/// A path for messages: the `\\?\` prefix Windows canonicalization adds is noise.
fn display(path: &Path) -> String {
    let s = path.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}
