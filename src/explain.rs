//! What a program would do, decided before it runs.
//!
//! E4's containment is a runtime property: the gate fires when the call is
//! made, so an agent learns a pipeline needs approval by being refused halfway
//! through it. But `safety::effect_of` is static, so the effect set of a whole
//! program is computable from its syntax. This computes it
//! (`docs/LANGUAGE_FIRST_PRINCIPLES.md` §6):
//!
//! ```text
//! $ ae --explain -c 'ls("src") | where(fn(f) => f.size > 1000) | map(fn(f) => rm(f.path))'
//! ```
//!
//! reports `ls` as `read_local` and `rm` as `destructive`, and that
//! agent mode would ask for approval before the delete.
//!
//! It is conservative in one direction only. A call it cannot resolve to a
//! builtin -- through a variable, or a computed callee -- is listed as
//! unresolved and raises the verdict to `unknown`, never lowers it. What it
//! cannot see is `eval` of a string built at run time, which it reports as the
//! `eval` builtin's own effect and no more; that limit is stated in the output.

use crate::ast::{Expr, Stmt};
use crate::safety::{decide, effect_of, net_allowlist, Decision, Effect, Mode};
use crate::value::Value;
use anyhow::Result;
use std::collections::{BTreeMap, HashSet};

struct Walk<'a> {
    env: &'a crate::env::Env,
    /// Names bound by `let`, which shadow builtins of the same name.
    locals: HashSet<String>,
    calls: Vec<(String, Option<String>)>,
}

impl Walk<'_> {
    /// The builtin a callee names, if it names one.
    fn resolve(&self, callee: &Expr) -> (String, Option<String>) {
        match callee {
            Expr::Ident(n) => {
                if self.locals.contains(n) {
                    (n.clone(), None)
                } else if crate::builtins::is_dispatched(n) {
                    (n.clone(), Some(n.clone()))
                } else {
                    (n.clone(), None)
                }
            }
            Expr::MemberAccess { object, field } => {
                if let Expr::Ident(m) = object.as_ref() {
                    let shown = format!("{m}.{field}");
                    if self.locals.contains(m) {
                        return (shown, None);
                    }
                    if let Some(Value::Record(r)) = self.env.get_var(m) {
                        if let Some(Value::Builtin(b)) = r.get(field) {
                            return (shown, Some(b.name.clone()));
                        }
                    }
                    return (shown, None);
                }
                ("<computed>".into(), None)
            }
            _ => ("<computed>".into(), None),
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { name, value, .. } => {
                self.expr(value);
                self.locals.insert(name.clone());
            }
            Stmt::Expr(e) => self.expr(e),
            Stmt::Cfg { body, .. } => self.stmt(body),
            Stmt::Import { .. } | Stmt::Export { .. } => {}
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Call {
                callee,
                args,
                named,
            } => {
                let r = self.resolve(callee);
                self.calls.push(r);
                if !matches!(callee.as_ref(), Expr::Ident(_) | Expr::MemberAccess { .. }) {
                    self.expr(callee);
                }
                for a in args {
                    self.expr(a);
                }
                for (_, a) in named {
                    self.expr(a);
                }
            }
            Expr::Pipe { left, right } => {
                self.expr(left);
                // A bare name on the right of a pipe is a call: `xs | sort`.
                match right.as_ref() {
                    Expr::Ident(_) | Expr::MemberAccess { .. } => {
                        let r = self.resolve(right);
                        self.calls.push(r);
                    }
                    other => self.expr(other),
                }
            }
            Expr::Array(xs) => xs.iter().for_each(|x| self.expr(x)),
            Expr::Record(fs) => fs.iter().for_each(|(_, x)| self.expr(x)),
            Expr::Lambda { body, .. } | Expr::AsyncLambda { body, .. } => self.expr(body),
            Expr::Await(x) | Expr::Throw(x) => self.expr(x),
            Expr::TryCatch {
                try_expr,
                catch_expr,
                ..
            } => {
                self.expr(try_expr);
                self.expr(catch_expr);
            }
            Expr::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Unary { expr, .. } => self.expr(expr),
            Expr::MemberAccess { object, .. } => self.expr(object),
            Expr::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.expr(&arm.body);
                }
            }
            Expr::LitInt(_)
            | Expr::LitFloat(_)
            | Expr::LitStr(_)
            | Expr::LitBool(_)
            | Expr::Null
            | Expr::Ident(_) => {}
        }
    }
}

/// The agent-mode decision for one effect, including the egress rule that
/// `safety::guard` adds on top of the policy table.
fn agent_decision(effect: Effect) -> &'static str {
    let d = decide(effect, Mode::Agent);
    if d == Decision::Allow && effect == Effect::Network && net_allowlist().is_none() {
        return "approve";
    }
    match d {
        Decision::Allow => "allow",
        Decision::Approve => "approve",
        Decision::Deny => "deny",
    }
}

fn rank(decision: &str) -> u8 {
    match decision {
        "allow" => 0,
        "unknown" => 1,
        "approve" => 2,
        _ => 3,
    }
}

/// Explain `code`: every call site, its effect and agent-mode decision, and
/// the strictest decision overall. Nothing is evaluated.
pub fn explain(code: &str) -> Result<Value> {
    let stmts = crate::parser::parse_program(code).map_err(|e| {
        crate::safety::bad_arg(
            "explain_effects",
            "AetherShell source that parses",
            &e.to_string(),
        )
    })?;
    let env = crate::modules::env_with_modules();
    let mut w = Walk {
        env: &env,
        locals: HashSet::new(),
        calls: Vec::new(),
    };
    for s in &stmts {
        w.stmt(s);
    }

    let mut calls = Vec::new();
    let mut effects: BTreeMap<u8, &'static str> = BTreeMap::new();
    let mut unresolved = Vec::new();
    let mut verdict = "allow";
    for (shown, builtin) in &w.calls {
        let mut r = BTreeMap::new();
        r.insert("call".to_string(), Value::Str(shown.clone()));
        match builtin {
            Some(b) => {
                let effect = effect_of(b);
                let decision = agent_decision(effect);
                effects.insert(effect as u8, effect.as_str());
                if rank(decision) > rank(verdict) {
                    verdict = decision;
                }
                r.insert("builtin".to_string(), Value::Str(b.clone()));
                r.insert(
                    "effect".to_string(),
                    Value::Str(effect.as_str().to_string()),
                );
                r.insert("decision".to_string(), Value::Str(decision.to_string()));
            }
            None => {
                if !unresolved.contains(shown) {
                    unresolved.push(shown.clone());
                }
                if rank("unknown") > rank(verdict) {
                    verdict = "unknown";
                }
                r.insert("decision".to_string(), Value::Str("unknown".to_string()));
            }
        }
        calls.push(Value::Record(r));
    }

    let mut out = BTreeMap::new();
    out.insert("calls".to_string(), Value::Array(calls));
    out.insert(
        "effects".to_string(),
        Value::Array(
            effects
                .values()
                .map(|e| Value::Str(e.to_string()))
                .collect(),
        ),
    );
    out.insert("decision".to_string(), Value::Str(verdict.to_string()));
    out.insert(
        "unresolved".to_string(),
        Value::Array(unresolved.into_iter().map(Value::Str).collect()),
    );
    out.insert(
        "note".to_string(),
        Value::Str(
            "static: decisions are agent mode's; a call through a variable is unresolved, \
             and code built at run time for eval is not seen"
                .to_string(),
        ),
    );
    Ok(Value::Record(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(v: &'a Value, k: &str) -> &'a Value {
        match v {
            Value::Record(r) => r.get(k).unwrap_or_else(|| panic!("no {k}")),
            _ => panic!("not a record"),
        }
    }

    #[test]
    fn a_pure_pipeline_is_allowed() {
        let v = explain("[1, 2, 3] | map(fn(x) => x * 2) | sum").unwrap();
        assert_eq!(field(&v, "decision"), &Value::Str("allow".into()));
    }

    #[test]
    fn a_delete_inside_a_lambda_is_found() {
        let v = explain(r#"ls("src") | map(fn(f) => file.move(f.path, "old"))"#).unwrap();
        assert_eq!(field(&v, "decision"), &Value::Str("approve".into()));
        let Value::Array(effects) = field(&v, "effects") else {
            panic!()
        };
        assert!(
            effects.contains(&Value::Str("read_local".into())),
            "{effects:?}"
        );
    }

    #[test]
    fn a_call_through_a_variable_is_unknown_not_allowed() {
        let v = explain("let f = fn(x) => x\nf(1)").unwrap();
        assert_eq!(field(&v, "decision"), &Value::Str("unknown".into()));
    }

    #[test]
    fn network_asks_without_an_allowlist() {
        let v = explain(r#"http_get("https://example.com")"#).unwrap();
        // Unless the environment running the test allowlists hosts.
        if net_allowlist().is_none() {
            assert_eq!(field(&v, "decision"), &Value::Str("approve".into()));
        }
    }
}
