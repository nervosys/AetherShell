//! Bounded extraction of one contiguous top-level statement range of a Rust
//! function into a private function, and the call that replaces it. Captured
//! variables are passed by value in first-use order with their declared
//! types. The analysis is deliberately conservative; see the signature doc.
use crate::{safety, value::Value};
use anyhow::Result;
use quote::quote;
use std::collections::HashSet;
use syn::visit::Visit;
fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn budget(name: &str) -> anyhow::Error {
    safety::budget_exceeded(
        name,
        "input/output exceeds syntax-edit limits".into(),
        "supply source <=64 KiB/256 punctuation, name <=256 bytes/32 punctuation",
    )
}
fn text<'a>(name: &str, value: &'a Value, bytes: usize, punctuation: usize) -> Result<&'a str> {
    let Value::Str(value) = value else {
        return Err(bad(name, "arguments must be Strings"));
    };
    if value.len() > bytes || value.bytes().filter(u8::is_ascii_punctuation).count() > punctuation {
        return Err(budget(name));
    }
    Ok(value)
}
fn index(name: &str, value: &Value) -> Result<usize> {
    let Value::Int(value) = value else {
        return Err(bad(name, "start and end must be Int"));
    };
    usize::try_from(*value).map_err(|_| bad(name, "start and end must be nonnegative"))
}
fn normalized(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_string()
}
fn unique(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect()
}
/// Counts every identifier, so that the new name cannot collide with anything
/// in the function. Macros, nested items, non-doc attributes and verbatim
/// syntax make the function opaque to the conservative analysis.
struct Guard<'a> {
    name: &'a str,
    count: usize,
    opaque: bool,
}
impl<'ast> Visit<'ast> for Guard<'_> {
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.count += usize::from(normalized(ident) == self.name);
    }
    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.opaque = true;
    }
    fn visit_item(&mut self, _: &'ast syn::Item) {
        self.opaque = true;
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        let doc = attribute.path().is_ident("doc")
            && matches!(&attribute.meta, syn::Meta::NameValue(value) if matches!(&value.value, syn::Expr::Lit(value) if matches!(value.lit, syn::Lit::Str(_))));
        self.opaque |= !doc;
    }
    fn visit_expr(&mut self, value: &'ast syn::Expr) {
        if matches!(value, syn::Expr::Verbatim(_)) {
            self.opaque = true;
        } else {
            syn::visit::visit_expr(self, value);
        }
    }
    fn visit_pat(&mut self, value: &'ast syn::Pat) {
        if matches!(value, syn::Pat::Verbatim(_)) {
            self.opaque = true;
        } else {
            syn::visit::visit_pat(self, value);
        }
    }
    fn visit_type(&mut self, value: &'ast syn::Type) {
        if matches!(value, syn::Type::Verbatim(_)) {
            self.opaque = true;
        } else {
            syn::visit::visit_type(self, value);
        }
    }
}
/// Every identifier bound by any pattern, in traversal order, duplicates kept.
#[derive(Default)]
struct Bindings(Vec<String>);
impl<'ast> Visit<'ast> for Bindings {
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.0.push(normalized(&pattern.ident));
        syn::visit::visit_pat_ident(self, pattern);
    }
}
/// Single-segment expression paths, in traversal (source) order.
#[derive(Default)]
struct PathNames(Vec<String>);
impl<'ast> Visit<'ast> for PathNames {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if path.qself.is_none() {
            if let Some(ident) = path.path.get_ident() {
                self.0.push(normalized(ident));
            }
        }
        syn::visit::visit_expr_path(self, path);
    }
}
fn bindings_in_stmts(stmts: &[syn::Stmt]) -> Vec<String> {
    let mut found = Bindings::default();
    for stmt in stmts {
        found.visit_stmt(stmt);
    }
    found.0
}
fn bindings_of(pattern: &syn::Pat) -> Vec<String> {
    let mut found = Bindings::default();
    found.visit_pat(pattern);
    found.0
}
fn paths_in_stmts(stmts: &[syn::Stmt]) -> Vec<String> {
    let mut found = PathNames::default();
    for stmt in stmts {
        found.visit_stmt(stmt);
    }
    found.0
}
fn paths_in_expr(expr: &syn::Expr) -> Vec<String> {
    let mut found = PathNames::default();
    found.visit_expr(expr);
    found.0
}
/// Operators that assign through their left operand.
fn assignment(op: &syn::BinOp) -> bool {
    use syn::BinOp::*;
    matches!(
        op,
        AddAssign(_)
            | SubAssign(_)
            | MulAssign(_)
            | DivAssign(_)
            | RemAssign(_)
            | BitXorAssign(_)
            | BitAndAssign(_)
            | BitOrAssign(_)
            | ShlAssign(_)
            | ShrAssign(_)
    )
}
/// Refuses, inside the extracted range, every construct whose meaning depends
/// on the original function: early exits, awaits, writes to or mutable
/// borrows of captured variables, and method receivers that might autoref.
struct RangeCheck<'a> {
    captured: &'a [String],
    loops: usize,
    refusal: Option<&'static str>,
}
impl RangeCheck<'_> {
    fn mentions(&self, expr: &syn::Expr) -> bool {
        paths_in_expr(expr)
            .iter()
            .any(|name| self.captured.contains(name))
    }
}
impl<'ast> Visit<'ast> for RangeCheck<'_> {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        let refusal = match expr {
            syn::Expr::Return(_) => Some("return inside the extracted range"),
            syn::Expr::Try(_) => Some("? inside the extracted range"),
            syn::Expr::Await(_) => Some(".await inside the extracted range"),
            syn::Expr::Break(_) | syn::Expr::Continue(_) if self.loops == 0 => {
                Some("break or continue outside a loop inside the extracted range")
            }
            syn::Expr::Assign(e) if self.mentions(&e.left) => {
                Some("assignment to a captured variable")
            }
            syn::Expr::Binary(e) if assignment(&e.op) && self.mentions(&e.left) => {
                Some("compound assignment to a captured variable")
            }
            syn::Expr::Reference(e) if e.mutability.is_some() && self.mentions(&e.expr) => {
                Some("&mut of a captured variable")
            }
            syn::Expr::MethodCall(e) if self.mentions(&e.receiver) => {
                Some("method call on a captured variable (may autoref &mut)")
            }
            _ => None,
        };
        self.refusal = self.refusal.or(refusal);
        let looping = matches!(
            expr,
            syn::Expr::While(_) | syn::Expr::Loop(_) | syn::Expr::ForLoop(_)
        );
        self.loops += usize::from(looping);
        syn::visit::visit_expr(self, expr);
        self.loops -= usize::from(looping);
    }
}
pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.len() != 4 || input.is_some() {
        return Err(bad(name, "invalid argument count or pipeline input"));
    }
    let source = text(name, &args[0], 65536, 256)?;
    let mut function = syn::parse_str::<syn::ItemFn>(source)
        .map_err(|_| bad(name, "expected exactly one Rust free function with body"))?;
    let ident = syn::parse_str::<syn::Ident>(text(name, &args[1], 256, 32)?)
        .map_err(|_| bad(name, "expected Rust identifier for the new function"))?;
    let new_name = normalized(&ident);
    let start = index(name, &args[2])?;
    let end = index(name, &args[3])?;
    let signature = &function.sig;
    if signature.variadic.is_some()
        || signature
            .inputs
            .iter()
            .any(|arg| matches!(arg, syn::FnArg::Receiver(_)))
    {
        return Err(bad(name, "receivers and variadics are unsupported"));
    }
    if !signature.generics.params.is_empty() || signature.generics.where_clause.is_some() {
        return Err(bad(
            name,
            "generic functions and where clauses are unsupported",
        ));
    }
    if signature.constness.is_some()
        || signature.asyncness.is_some()
        || signature.unsafety.is_some()
        || signature.abi.is_some()
    {
        return Err(bad(
            name,
            "const, async, unsafe and extern functions are unsupported",
        ));
    }
    let mut guard = Guard {
        name: &new_name,
        count: 0,
        opaque: false,
    };
    guard.visit_item_fn(&function);
    if guard.count > 0 || guard.opaque {
        return Err(bad(
            name,
            "name already occurs, or syntax has opaque macros/attributes/nested items",
        ));
    }
    let len = function.block.stmts.len();
    if start >= end || end > len {
        return Err(bad(
            name,
            "range must satisfy 0 <= start < end <= statement count",
        ));
    }
    // No shadowing: every identifier is bound at most once in the function.
    let mut all_bound = Vec::new();
    for arg in &function.sig.inputs {
        if let syn::FnArg::Typed(typed) = arg {
            all_bound.extend(bindings_of(&typed.pat));
        }
    }
    all_bound.extend(bindings_in_stmts(&function.block.stmts));
    let mut seen = HashSet::new();
    if all_bound.iter().any(|bound| !seen.insert(bound)) {
        return Err(bad(
            name,
            "a name is bound more than once; shadowing is unsupported",
        ));
    }
    // Captured candidates: plain by-value parameters and top-level lets before
    // the range, each with the type it must be passed with.
    let mut typed = std::collections::HashMap::<String, (syn::Ident, syn::Type)>::new();
    for arg in &function.sig.inputs {
        if let syn::FnArg::Typed(param) = arg {
            if let syn::Pat::Ident(p) = &*param.pat {
                if p.by_ref.is_none() && p.mutability.is_none() && p.subpat.is_none() {
                    typed.insert(normalized(&p.ident), (p.ident.clone(), (*param.ty).clone()));
                }
            }
        }
    }
    for stmt in &function.block.stmts[..start] {
        let syn::Stmt::Local(local) = stmt else {
            continue;
        };
        if let syn::Pat::Type(declared) = &local.pat {
            if let syn::Pat::Ident(p) = &*declared.pat {
                if p.by_ref.is_none()
                    && p.subpat.is_none()
                    && !matches!(&*declared.ty, syn::Type::Infer(_))
                {
                    typed.insert(
                        normalized(&p.ident),
                        (p.ident.clone(), (*declared.ty).clone()),
                    );
                }
            }
        }
    }
    let range = &function.block.stmts[start..end];
    let range_bound: HashSet<String> = bindings_in_stmts(range).into_iter().collect();
    let bound_names: HashSet<String> = all_bound.into_iter().collect();
    let mut captured = Vec::new();
    for used in unique(paths_in_stmts(range)) {
        // Items and prelude names are not locals; range-bound names are internal.
        if !bound_names.contains(&used) || range_bound.contains(&used) {
            continue;
        }
        let Some((ident, ty)) = typed.get(&used) else {
            return Err(bad(
                name,
                &format!("captured `{used}` needs a plain typed parameter or `let {used}: T`"),
            ));
        };
        captured.push((ident.clone(), ty.clone(), used));
    }
    let captured_names: Vec<String> = captured.iter().map(|(_, _, n)| n.clone()).collect();
    let mut check = RangeCheck {
        captured: &captured_names,
        loops: 0,
        refusal: None,
    };
    for stmt in range {
        check.visit_stmt(stmt);
    }
    if let Some(refusal) = check.refusal {
        return Err(bad(name, refusal));
    }
    if paths_in_stmts(&function.block.stmts[end..])
        .iter()
        .any(|used| range_bound.contains(used))
    {
        return Err(bad(
            name,
            "a name bound inside the extracted range is used after it",
        ));
    }
    let tail = end == len && matches!(function.block.stmts.last(), Some(syn::Stmt::Expr(_, None)));
    let parameters = captured.iter().map(|(ident, ty, _)| quote!(#ident: #ty));
    let arguments: Vec<&syn::Ident> = captured.iter().map(|(ident, _, _)| ident).collect();
    let return_type = if tail {
        function.sig.output.clone()
    } else {
        syn::ReturnType::Default
    };
    let body = &function.block.stmts[start..end];
    let new_function: syn::ItemFn = syn::parse2(quote!(
        fn #ident(#(#parameters),*) #return_type { #(#body)* }
    ))
    .map_err(|_| bad(name, "cannot construct the extracted function"))?;
    let replacement = if tail {
        syn::Stmt::Expr(
            syn::parse2(quote!(#ident(#(#arguments),*)))
                .map_err(|_| bad(name, "cannot construct the call"))?,
            None,
        )
    } else {
        syn::parse2::<syn::Stmt>(quote!(#ident(#(#arguments),*);))
            .map_err(|_| bad(name, "cannot construct the call"))?
    };
    let rest = function.block.stmts.split_off(end);
    function.block.stmts.truncate(start);
    function.block.stmts.push(replacement);
    function.block.stmts.extend(rest);
    let output = quote!(#function #new_function).to_string();
    if output.len() > 1048576 {
        return Err(budget(name));
    }
    syn::parse_file(&output).map_err(|_| bad(name, "output is not valid Rust"))?;
    Ok(Value::Str(output))
}
