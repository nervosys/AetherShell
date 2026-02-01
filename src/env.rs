use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::value::Value;

/// Process/runtime environment for the Aether evaluator & builtins.
///
/// - Stores variables (including the pipeline slot `__pipe_input__`)
/// - Tracks mutability: variables are immutable by default
/// - Tracks visibility: variables are private by default, `pub` makes them exportable
/// - Optionally tracks a current working directory (cwd)
#[derive(Debug, Default, Clone)]
pub struct Env {
    vars: BTreeMap<String, Value>,
    /// Track which variables are mutable (created with `let mut`)
    mutable_vars: BTreeMap<String, bool>,
    /// Track which variables are public (can be imported by other modules)
    public_vars: HashSet<String>,
    /// Explicitly exported names (from `export { ... }` statements)
    exported_names: HashSet<String>,
    cwd: Option<PathBuf>,
    pipe_input: Option<Value>,
}

impl Env {
    /// Create a fresh environment.
    pub fn new() -> Self {
        Self {
            vars: BTreeMap::new(),
            mutable_vars: BTreeMap::new(),
            public_vars: HashSet::new(),
            exported_names: HashSet::new(),
            cwd: None,
            pipe_input: None,
        }
    }

    /// Register a module as an immutable global variable.
    /// Used by the module system to provide namespaced access to builtins.
    pub fn register_module(&mut self, name: &str, module: Value) {
        self.vars.insert(name.to_string(), module);
        self.mutable_vars.insert(name.to_string(), false);
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

    /// Set a new variable or reassign if mutable.
    /// Returns error if trying to reassign an immutable variable.
    pub fn set_var<S: Into<String>>(&mut self, name: S, value: Value) -> Result<(), String> {
        let name_str = name.into();

        // Check if variable already exists
        if self.vars.contains_key(&name_str) {
            // Check mutability
            if !self.is_mutable(&name_str) {
                return Err(format!(
                    "Cannot reassign immutable variable '{}'. Use 'let mut {}' to make it mutable.",
                    name_str, name_str
                ));
            }
        }

        self.vars.insert(name_str, value);
        Ok(())
    }

    /// Declare a new variable with specified mutability.
    /// This is used for initial `let` or `let mut` declarations.
    pub fn declare_var<S: Into<String>>(
        &mut self,
        name: S,
        value: Value,
        is_mut: bool,
    ) -> Result<(), String> {
        let name_str = name.into();

        // Check if variable already exists
        if self.vars.contains_key(&name_str) {
            // If existing variable is immutable, prevent shadowing/reassignment
            if !self.is_mutable(&name_str) {
                return Err(format!(
                    "Cannot reassign immutable variable '{}'. Use 'let mut {}' to make it mutable.",
                    name_str, name_str
                ));
            }
            // If mutable, allow update but preserve mutability
            self.vars.insert(name_str, value);
            return Ok(());
        }

        // New variable - set value and mutability
        self.vars.insert(name_str.clone(), value);
        self.mutable_vars.insert(name_str, is_mut);
        Ok(())
    }

    /// Internal use only: Set a variable without mutability checks.
    /// Used for pattern matching, lambda params, builtins, etc.
    pub(crate) fn set_var_unchecked<S: Into<String>>(&mut self, name: S, value: Value) {
        let name_str = name.into();
        self.vars.insert(name_str.clone(), value);
        // Mark as mutable for internal bindings
        self.mutable_vars.insert(name_str, true);
    }

    /// Check if a variable is mutable
    pub fn is_mutable(&self, name: &str) -> bool {
        self.mutable_vars.get(name).copied().unwrap_or(false)
    }

    /// Check if a variable is public (can be imported)
    pub fn is_public(&self, name: &str) -> bool {
        self.public_vars.contains(name)
    }

    /// Check if a variable is exported (either pub or explicitly exported)
    pub fn is_exported(&self, name: &str) -> bool {
        self.public_vars.contains(name) || self.exported_names.contains(name)
    }

    /// Mark a variable as public
    pub fn set_public(&mut self, name: &str) {
        self.public_vars.insert(name.to_string());
    }

    /// Add an explicit export
    pub fn add_export(&mut self, name: &str) {
        self.exported_names.insert(name.to_string());
    }

    /// Get all exported variable names (pub vars + explicit exports)
    pub fn exports(&self) -> impl Iterator<Item = &String> {
        self.public_vars.iter().chain(self.exported_names.iter())
    }

    /// Get exported variables as a map (for import "path" as mod)
    pub fn exported_vars(&self) -> BTreeMap<String, Value> {
        let mut exports = BTreeMap::new();
        for name in self.exports() {
            if let Some(value) = self.vars.get(name) {
                exports.insert(name.clone(), value.clone());
            }
        }
        exports
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
