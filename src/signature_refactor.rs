//! Bounded syntax edits of one supplied Rust free function.
use crate::{safety, value::Value};
use anyhow::Result;
use quote::ToTokens;
use std::collections::BTreeSet;
use syn::visit::Visit;

fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn budget(name: &str, message: &str) -> anyhow::Error {
    safety::budget_exceeded(
        name,
        message.into(),
        "supply a smaller function or signature",
    )
}
fn text<'a>(name: &str, value: &'a Value, bytes: usize, punctuation: usize) -> Result<&'a str> {
    let Value::Str(source) = value else {
        return Err(bad(name, "expected Rust source String"));
    };
    if source.len() > bytes || source.bytes().filter(u8::is_ascii_punctuation).count() > punctuation
    {
        return Err(budget(
            name,
            "source exceeds its byte or raw punctuation limit",
        ));
    }
    Ok(source)
}
fn normalized(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_string()
}
fn argument_name(name: &str, argument: &syn::FnArg) -> Result<String> {
    let syn::FnArg::Typed(argument) = argument else {
        return Err(bad(name, "receivers are unsupported"));
    };
    let syn::Pat::Ident(pattern) = argument.pat.as_ref() else {
        return Err(bad(name, "parameters require plain identifier patterns"));
    };
    if pattern.by_ref.is_some() || pattern.subpat.is_some() {
        return Err(bad(name, "ref and subpatterns are unsupported"));
    }
    Ok(normalized(&pattern.ident))
}
fn parameter_names(name: &str, signature: &syn::Signature) -> Result<BTreeSet<String>> {
    if signature.variadic.is_some() {
        return Err(bad(name, "variadic signatures are unsupported"));
    }
    if signature.inputs.len() > 64 {
        return Err(budget(name, "function exceeds 64 parameters"));
    }
    let mut names = BTreeSet::new();
    for argument in &signature.inputs {
        if !names.insert(argument_name(name, argument)?) {
            return Err(bad(name, "duplicate parameter binding"));
        }
    }
    Ok(names)
}
struct References<'a> {
    names: &'a BTreeSet<String>,
    used: bool,
    opaque: bool,
}
impl<'ast> Visit<'ast> for References<'_> {
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.used |= self.names.contains(&normalized(ident));
    }
    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.opaque = true;
    }
    fn visit_item(&mut self, _: &'ast syn::Item) {
        self.opaque = true;
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        // Literal documentation cannot capture a runtime parameter. Other
        // attributes may affect expansion and are not resolved by this editor.
        let literal_doc = attribute.path().is_ident("doc")
            && matches!(&attribute.meta,
            syn::Meta::NameValue(value) if matches!(&value.value,
                syn::Expr::Lit(value) if matches!(value.lit, syn::Lit::Str(_))));
        self.opaque |= !literal_doc;
    }
    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        if matches!(expression, syn::Expr::Verbatim(_)) {
            self.opaque = true;
        } else {
            syn::visit::visit_expr(self, expression);
        }
    }
    fn visit_pat(&mut self, pattern: &'ast syn::Pat) {
        if matches!(pattern, syn::Pat::Verbatim(_)) {
            self.opaque = true;
        } else {
            syn::visit::visit_pat(self, pattern);
        }
    }
    fn visit_type(&mut self, ty: &'ast syn::Type) {
        if matches!(ty, syn::Type::Verbatim(_)) {
            self.opaque = true;
        } else {
            syn::visit::visit_type(self, ty);
        }
    }
}
fn check_removal(name: &str, function: &syn::ItemFn, dropped: &BTreeSet<String>) -> Result<()> {
    if dropped.is_empty() {
        return Ok(());
    }
    let mut references = References {
        names: dropped,
        used: false,
        opaque: false,
    };
    for attribute in &function.attrs {
        references.visit_attribute(attribute);
    }
    for argument in &function.sig.inputs {
        if let syn::FnArg::Typed(argument) = argument {
            for attribute in &argument.attrs {
                references.visit_attribute(attribute);
            }
        }
    }
    references.visit_block(&function.block);
    if references.used || references.opaque {
        return Err(bad(name, "cannot drop parameters mentioned in the body or with opaque macros, attributes or nested items"));
    }
    Ok(())
}
pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let max = if name == "refactor_add_parameter" {
        3
    } else {
        2
    };
    if !(2..=max).contains(&args.len()) || input.is_some() {
        return Err(bad(
            name,
            "invalid argument count or unexpected pipeline input",
        ));
    }
    let source = text(name, &args[0], 65536, 256)?;
    let mut function = syn::parse_str::<syn::ItemFn>(source)
        .map_err(|_| bad(name, "expected exactly one Rust free function with a body"))?;
    let previous = parameter_names(name, &function.sig)?;
    match name {
        "refactor_change_signature" => {
            let replacement = text(name, &args[1], 16384, 128)?;
            let signature = syn::parse_str::<syn::Signature>(replacement)
                .map_err(|_| bad(name, "invalid Rust function signature"))?;
            if normalized(&signature.ident) != normalized(&function.sig.ident) {
                return Err(bad(name, "replacement must retain the function name"));
            }
            let next = parameter_names(name, &signature)?;
            if signature.to_token_stream().to_string() == function.sig.to_token_stream().to_string()
            {
                return Err(bad(name, "replacement signature is unchanged"));
            }
            let dropped = previous.difference(&next).cloned().collect();
            check_removal(name, &function, &dropped)?;
            function.sig = signature;
        }
        "refactor_add_parameter" => {
            let parameter = text(name, &args[1], 16384, 128)?;
            let parameter = syn::parse_str::<syn::FnArg>(parameter)
                .map_err(|_| bad(name, "invalid typed Rust parameter"))?;
            let parameter_name = argument_name(name, &parameter)?;
            if previous.contains(&parameter_name) {
                return Err(bad(name, "parameter binding already exists"));
            }
            if previous.len() == 64 {
                return Err(budget(name, "function exceeds 64 parameters"));
            }
            let position = match args.get(2) {
                None => function.sig.inputs.len(),
                Some(Value::Int(n)) if *n >= 0 && *n as u64 <= function.sig.inputs.len() as u64 => {
                    *n as usize
                }
                _ => {
                    return Err(bad(
                        name,
                        "position must be an Int from zero through the parameter count",
                    ))
                }
            };
            function.sig.inputs.insert(position, parameter);
        }
        "refactor_remove_parameter" => {
            let parameter = text(name, &args[1], 256, 32)?;
            let parameter = syn::parse_str::<syn::Ident>(parameter)
                .map_err(|_| bad(name, "expected a Rust parameter identifier"))?;
            let parameter = normalized(&parameter);
            if !previous.contains(&parameter) {
                return Err(bad(name, "parameter binding does not exist"));
            }
            check_removal(name, &function, &BTreeSet::from([parameter.clone()]))?;
            function.sig.inputs = function
                .sig
                .inputs
                .into_iter()
                .filter(|argument| argument_name(name, argument).is_ok_and(|n| n != parameter))
                .collect();
        }
        _ => unreachable!("internal signature refactoring operation"),
    }
    let output = function.to_token_stream().to_string();
    if output.len() > 1048576 {
        return Err(budget(name, "generated source exceeds 1 MiB"));
    }
    syn::parse_str::<syn::ItemFn>(&output)
        .map_err(|_| bad(name, "generated source is not a Rust function"))?;
    Ok(Value::Str(output))
}
