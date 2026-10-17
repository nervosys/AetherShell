//! Bounded extraction of one Rust function's top-level tail expression, and
//! its inverse: inlining one immutable local into its single following use.
use crate::{safety, value::Value};
use anyhow::Result;
use quote::{quote, ToTokens};
use syn::visit::Visit;
fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn budget(name: &str) -> anyhow::Error {
    safety::budget_exceeded(name, "input/output exceeds syntax-edit limits".into(), "supply source <=64 KiB/256 punctuation, name <=256 bytes/32 punctuation, type <=16 KiB/128 punctuation")
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
fn normalized(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_string()
}
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
pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if name == "refactor_inline" {
        return inline(name, args, input);
    }
    let constant = name == "refactor_extract_constant";
    if args.len() != if constant { 3 } else { 2 } || input.is_some() {
        return Err(bad(name, "invalid argument count or pipeline input"));
    }
    let source = text(name, &args[0], 65536, 256)?;
    let mut function = syn::parse_str::<syn::ItemFn>(source)
        .map_err(|_| bad(name, "expected exactly one Rust free function with body"))?;
    if function.sig.variadic.is_some()
        || function
            .sig
            .inputs
            .iter()
            .any(|arg| matches!(arg, syn::FnArg::Receiver(_)))
    {
        return Err(bad(name, "receivers and variadics are unsupported"));
    }
    let binding = syn::parse_str::<syn::Ident>(text(name, &args[1], 256, 32)?)
        .map_err(|_| bad(name, "expected Rust binding identifier"))?;
    let binding_name = normalized(&binding);
    let mut guard = Guard {
        name: &binding_name,
        count: 0,
        opaque: false,
    };
    guard.visit_item_fn(&function);
    let ty = if constant {
        let ty = syn::parse_str::<syn::Type>(text(name, &args[2], 16384, 128)?)
            .map_err(|_| bad(name, "invalid explicit constant type"))?;
        guard.visit_type(&ty);
        Some(ty)
    } else {
        None
    };
    if guard.count > 0 || guard.opaque {
        return Err(bad(
            name,
            "binding identifier already occurs or syntax has opaque macros/attributes/nested items",
        ));
    }
    let Some(syn::Stmt::Expr(_, None)) = function.block.stmts.last() else {
        return Err(bad(
            name,
            "function requires a top-level tail expression without semicolon",
        ));
    };
    let syn::Stmt::Expr(expression, _) = function.block.stmts.pop().unwrap() else {
        unreachable!()
    };
    let declaration = if let Some(ty) = ty {
        quote!(const #binding: #ty = #expression;)
    } else {
        quote!(let #binding = #expression;)
    };
    let declaration = syn::parse2::<syn::Stmt>(declaration)
        .map_err(|_| bad(name, "cannot construct binding declaration"))?;
    function.block.stmts.push(declaration);
    function.block.stmts.push(syn::Stmt::Expr(
        syn::parse2(quote!(#binding)).map_err(|_| bad(name, "cannot construct tail expression"))?,
        None,
    ));
    let output = function.to_token_stream().to_string();
    if output.len() > 1048576 {
        return Err(budget(name));
    }
    syn::parse_str::<syn::ItemFn>(&output)
        .map_err(|_| bad(name, "output is not a Rust function"))?;
    Ok(Value::Str(output))
}
/// Literals and place reads: evaluating one earlier cannot cause another
/// expression's effects, and cannot observe them unless that expression is
/// itself effectful.
fn inert(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Lit(_) | syn::Expr::Path(_) => true,
        syn::Expr::Paren(e) => inert(&e.expr),
        syn::Expr::Group(e) => inert(&e.expr),
        syn::Expr::Field(e) => inert(&e.base),
        syn::Expr::Reference(e) => e.mutability.is_none() && inert(&e.expr),
        _ => false,
    }
}
/// Syntax without calls, assignments, blocks, mutable borrows or control
/// flow; operator traits are assumed not to mutate unrelated locals.
fn effect_free(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Lit(_) | syn::Expr::Path(_) => true,
        syn::Expr::Paren(e) => effect_free(&e.expr),
        syn::Expr::Group(e) => effect_free(&e.expr),
        syn::Expr::Field(e) => effect_free(&e.base),
        syn::Expr::Reference(e) => e.mutability.is_none() && effect_free(&e.expr),
        syn::Expr::Unary(e) => !matches!(e.op, syn::UnOp::Deref(_)) && effect_free(&e.expr),
        syn::Expr::Binary(e) => !assignment(&e.op) && effect_free(&e.left) && effect_free(&e.right),
        syn::Expr::Cast(e) => effect_free(&e.expr),
        syn::Expr::Tuple(e) => e.elems.iter().all(effect_free),
        syn::Expr::Array(e) => e.elems.iter().all(effect_free),
        _ => false,
    }
}
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
/// Primary and postfix forms, which need no parentheses as an operand.
/// Block-like forms are excluded: one could begin an expression statement.
fn atomic(expr: &syn::Expr) -> bool {
    matches!(
        expr,
        syn::Expr::Lit(_)
            | syn::Expr::Path(_)
            | syn::Expr::Paren(_)
            | syn::Expr::Call(_)
            | syn::Expr::MethodCall(_)
            | syn::Expr::Field(_)
            | syn::Expr::Index(_)
            | syn::Expr::Tuple(_)
            | syn::Expr::Array(_)
            | syn::Expr::Try(_)
    )
}
/// Whether a pattern takes its whole initializer by value: `_`, `ref` and
/// destructuring leave the place alive until the end of its scope.
fn moves_whole(pat: &syn::Pat) -> bool {
    match pat {
        syn::Pat::Ident(p) => p.by_ref.is_none() && p.subpat.is_none(),
        syn::Pat::Type(p) => moves_whole(&p.pat),
        _ => false,
    }
}
/// Operators that consume their operands; comparisons borrow them.
fn by_value(op: &syn::BinOp) -> bool {
    use syn::BinOp::*;
    matches!(
        op,
        Add(_)
            | Sub(_)
            | Mul(_)
            | Div(_)
            | Rem(_)
            | BitXor(_)
            | BitAnd(_)
            | BitOr(_)
            | Shl(_)
            | Shr(_)
    )
}
struct Inline<'a> {
    name: &'a str,
    value: Option<syn::Expr>,
    skipped: bool,
}
impl Inline<'_> {
    fn is_use(&self, expr: &syn::Expr) -> bool {
        matches!(expr, syn::Expr::Path(p) if p.qself.is_none()
            && p.path.get_ident().is_some_and(|i| normalized(i) == self.name))
    }
    /// Operands in evaluation order: descend into the first one that is not
    /// inert, recording whether inert operands now evaluate before the value.
    fn operands<'e>(
        &mut self,
        operands: impl IntoIterator<Item = &'e mut syn::Expr>,
        operator: bool,
    ) -> bool {
        for operand in operands {
            if self.replace(operand, operator) {
                return true;
            }
            if !inert(operand) {
                return false;
            }
            self.skipped = true;
        }
        false
    }
    /// Replace the use only where it is evaluated first, exactly once,
    /// unconditionally and consumed by value, so that neither evaluation
    /// order nor drop timing changes. An operator's operand is parenthesized
    /// unless the value is atomic.
    fn replace(&mut self, expr: &mut syn::Expr, operator: bool) -> bool {
        if self.is_use(expr) {
            let value = self.value.take().unwrap();
            *expr = if operator && !atomic(&value) {
                syn::Expr::Paren(syn::ExprParen {
                    attrs: Vec::new(),
                    paren_token: Default::default(),
                    expr: Box::new(value),
                })
            } else {
                value
            };
            return true;
        }
        match expr {
            syn::Expr::Paren(e) => self.replace(&mut e.expr, false),
            syn::Expr::Group(e) => self.replace(&mut e.expr, operator),
            syn::Expr::Unary(e) if !matches!(e.op, syn::UnOp::Deref(_)) => {
                self.replace(&mut e.expr, true)
            }
            syn::Expr::Cast(e) => self.replace(&mut e.expr, true),
            syn::Expr::Try(e) => self.replace(&mut e.expr, true),
            syn::Expr::Return(e) => e.expr.as_mut().is_some_and(|e| self.replace(e, false)),
            syn::Expr::Break(e) => e.expr.as_mut().is_some_and(|e| self.replace(e, false)),
            syn::Expr::If(e) => {
                !matches!(*e.cond, syn::Expr::Let(_)) && self.replace(&mut e.cond, false)
            }
            // The right operand of && and || is conditional.
            syn::Expr::Binary(e) if matches!(e.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) => {
                self.replace(&mut e.left, true)
            }
            syn::Expr::Binary(e) if by_value(&e.op) => {
                self.operands([&mut *e.left, &mut *e.right], true)
            }
            syn::Expr::Index(e) if inert(&e.expr) && !self.is_use(&e.expr) => {
                self.skipped = true;
                self.replace(&mut e.index, false)
            }
            syn::Expr::Call(e) if inert(&e.func) && !self.is_use(&e.func) => {
                self.skipped = true;
                self.operands(e.args.iter_mut(), false)
            }
            syn::Expr::MethodCall(e) if inert(&e.receiver) && !self.is_use(&e.receiver) => {
                self.skipped = true;
                self.operands(e.args.iter_mut(), false)
            }
            syn::Expr::Tuple(e) => self.operands(e.elems.iter_mut(), false),
            syn::Expr::Array(e) => self.operands(e.elems.iter_mut(), false),
            syn::Expr::Struct(e) if e.rest.is_none() => {
                self.operands(e.fields.iter_mut().map(|field| &mut field.expr), false)
            }
            _ => false,
        }
    }
}
fn inline(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.len() != 2 || input.is_some() {
        return Err(bad(name, "invalid argument count or pipeline input"));
    }
    let source = text(name, &args[0], 65536, 256)?;
    let mut function = syn::parse_str::<syn::ItemFn>(source)
        .map_err(|_| bad(name, "expected exactly one Rust free function with body"))?;
    let binding = syn::parse_str::<syn::Ident>(text(name, &args[1], 256, 32)?)
        .map_err(|_| bad(name, "expected Rust binding identifier"))?;
    let binding_name = normalized(&binding);
    let mut guard = Guard {
        name: &binding_name,
        count: 0,
        opaque: false,
    };
    guard.visit_item_fn(&function);
    if guard.opaque {
        return Err(bad(
            name,
            "syntax has opaque macros/attributes/nested items",
        ));
    }
    if guard.count != 2 {
        return Err(bad(
            name,
            "identifier must occur exactly twice in the function: its binding and one use",
        ));
    }
    let position = function.block.stmts.iter().position(|stmt| {
        matches!(stmt, syn::Stmt::Local(local)
            if matches!(&local.pat, syn::Pat::Ident(p) if normalized(&p.ident) == binding_name))
    });
    let Some(position) = position else {
        return Err(bad(name, "no top-level let binding of the identifier"));
    };
    let syn::Stmt::Local(local) = function.block.stmts.remove(position) else {
        unreachable!()
    };
    let syn::Pat::Ident(pattern) = &local.pat else {
        unreachable!()
    };
    let simple = pattern.by_ref.is_none()
        && pattern.mutability.is_none()
        && pattern.subpat.is_none()
        && local.attrs.is_empty();
    let Some(init) = local.init.filter(|init| simple && init.diverge.is_none()) else {
        return Err(bad(
            name,
            "binding must be let NAME = EXPR; without mut, ref, subpattern, type, attributes or else",
        ));
    };
    let value = *init.expr;
    let effect_free = effect_free(&value);
    let mut inline = Inline {
        name: &binding_name,
        value: Some(value),
        skipped: false,
    };
    let replaced = match function.block.stmts.get_mut(position) {
        Some(syn::Stmt::Local(next)) => next.init.as_mut().is_some_and(|init| {
            (moves_whole(&next.pat) || !inline.is_use(&init.expr))
                && inline.replace(&mut init.expr, false)
        }),
        // A place statement `NAME;` does not move, so the value outlives it.
        Some(syn::Stmt::Expr(next, semicolon)) => {
            (semicolon.is_none() || !inline.is_use(next)) && inline.replace(next, false)
        }
        _ => false,
    };
    if !replaced {
        return Err(bad(
            name,
            "the use must be in the next statement, evaluated first, once, unconditionally and by value",
        ));
    }
    if inline.skipped && !effect_free {
        return Err(bad(
            name,
            "an effectful value cannot move after operands that are evaluated before its use",
        ));
    }
    let output = function.to_token_stream().to_string();
    if output.len() > 1048576 {
        return Err(budget(name));
    }
    syn::parse_str::<syn::ItemFn>(&output)
        .map_err(|_| bad(name, "output is not a Rust function"))?;
    Ok(Value::Str(output))
}
