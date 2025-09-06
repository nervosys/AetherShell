use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::value::Value;

/// Process/runtime environment for the Aether evaluator & builtins.
///
/// - Stores variables (including the pipeline slot `__pipe_input__`)
/// - Optionally tracks a current working directory (cwd)
#[derive(Debug, Default, Clone)]
pub struct Env {
    vars: BTreeMap<String, Value>,
    cwd: Option<PathBuf>,
    pipe_input: Option<Value>,
}

impl Env {
    /// Create a fresh environment.
    pub fn new() -> Self {
        Self {
            vars: BTreeMap::new(),
            cwd: None,
            pipe_input: None,
        }
    }

    pub fn input(&self) -> Option<&Value> {
        self.pipe_input.as_ref()
    }
    pub fn set_input(&mut self, v: Option<Value>) {
        self.pipe_input = v;
    }
    pub fn take_input(&mut self) -> Option<Value> {
        self.pipe_input.take()
    }
    /// Clears the current pipeline input value
    pub fn clear_input(&mut self) {
        self.pipe_input = None;
    }

    // -------------------------
    // Variable accessors
    // -------------------------

    /// Immutable lookup. Works on `&Env` (what your error needed).
    pub fn get_var(&self, name: &str) -> Option<&Value> {
        self.vars.get(name)
    }

    /// Mutable set/replace.
    pub fn set_var<S: Into<String>>(&mut self, name: S, value: Value) {
        self.vars.insert(name.into(), value);
    }

    /// Delete a variable if present.
    pub fn del_var(&mut self, name: &str) {
        self.vars.remove(name);
    }

    /// Mutable lookup (if you ever need to mutate a stored value in place).
    pub fn get_var_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.vars.get_mut(name)
    }

    /// Take (remove and return) a variable. Handy for temporary slots.
    pub fn take_var(&mut self, name: &str) -> Option<Value> {
        self.vars.remove(name)
    }

    /// Expose the whole map if some builtin needs to iterate (read-only).
    pub fn vars(&self) -> &BTreeMap<String, Value> {
        &self.vars
    }

    /// Expose mutable map (use sparingly).
    pub fn vars_mut(&mut self) -> &mut BTreeMap<String, Value> {
        &mut self.vars
    }

    // -------------------------
    // Working directory helpers
    // -------------------------

    /// Get current working directory tracked by the shell (if any).
    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// Set current working directory tracked by the shell.
    pub fn set_cwd<P: Into<PathBuf>>(&mut self, p: P) {
        self.cwd = Some(p.into());
    }
}
