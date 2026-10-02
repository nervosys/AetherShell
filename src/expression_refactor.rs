//! Bounded extraction of one Rust function's top-level tail expression.
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
    collision: bool,
    opaque: bool,
}
impl<'ast> Visit<'ast> for Guard<'_> {
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.collision |= normalized(ident) == self.name;
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
        collision: false,
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
    if guard.collision || guard.opaque {
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
