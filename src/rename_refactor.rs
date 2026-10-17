//! Whole-file, syntactic renaming of one Rust identifier. Every identifier
//! token whose normalized text equals the old name becomes the new name;
//! lifetimes, strings and comments are not identifiers and are left alone.
use crate::{safety, value::Value};
use anyhow::Result;
use quote::ToTokens;
use syn::{visit::Visit, visit_mut::VisitMut};
const RESERVED: [&str; 5] = ["self", "Self", "super", "crate", "_"];
fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn budget(name: &str) -> anyhow::Error {
    safety::budget_exceeded(
        name,
        "input/output exceeds rename limits".into(),
        "supply source <=256 KiB/4096 punctuation, names <=256 bytes/32 punctuation, output <=2 MiB",
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
fn normalized(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_string()
}
/// Conservative textual scan: any identifier-like word in the tokens (raw
/// prefixes split off, string contents included) equal to `name` matches.
fn mentions(tokens: &str, name: &str) -> bool {
    tokens
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| word == name)
}
/// Doc comments are attributes of the form `#[doc = "..."]`; their text is
/// not renamed, so they are exempt from the attribute token scan.
fn doc(attribute: &syn::Attribute) -> bool {
    attribute.path().is_ident("doc")
        && matches!(&attribute.meta, syn::Meta::NameValue(value) if matches!(&value.value, syn::Expr::Lit(value) if matches!(value.lit, syn::Lit::Str(_))))
}
/// Identifier occurrences plus everything that makes a rename unsound to
/// perform mechanically: macros or attributes mentioning the old name,
/// verbatim syntax, and whether the new name is already taken.
struct Scan<'a> {
    old: &'a str,
    new: &'a str,
    old_count: usize,
    new_count: usize,
    opaque: bool,
}
impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        let word = normalized(ident);
        self.old_count += usize::from(word == self.old);
        self.new_count += usize::from(word == self.new);
    }
    fn visit_lifetime(&mut self, _: &'ast syn::Lifetime) {}
    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        let tokens = value.tokens.to_string();
        self.opaque |= mentions(&tokens, self.old) || mentions(&tokens, self.new);
        syn::visit::visit_macro(self, value);
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        self.opaque |= if doc(attribute) {
            attribute
                .path()
                .get_ident()
                .is_some_and(|ident| normalized(ident) == self.old)
        } else {
            mentions(&attribute.to_token_stream().to_string(), self.old)
        };
        syn::visit::visit_attribute(self, attribute);
    }
    fn visit_expr(&mut self, value: &'ast syn::Expr) {
        self.opaque |= matches!(value, syn::Expr::Verbatim(_));
        syn::visit::visit_expr(self, value);
    }
    fn visit_pat(&mut self, value: &'ast syn::Pat) {
        self.opaque |= matches!(value, syn::Pat::Verbatim(_));
        syn::visit::visit_pat(self, value);
    }
    fn visit_type(&mut self, value: &'ast syn::Type) {
        self.opaque |= matches!(value, syn::Type::Verbatim(_));
        syn::visit::visit_type(self, value);
    }
    fn visit_item(&mut self, value: &'ast syn::Item) {
        self.opaque |= matches!(value, syn::Item::Verbatim(_));
        syn::visit::visit_item(self, value);
    }
    fn visit_impl_item(&mut self, value: &'ast syn::ImplItem) {
        self.opaque |= matches!(value, syn::ImplItem::Verbatim(_));
        syn::visit::visit_impl_item(self, value);
    }
    fn visit_trait_item(&mut self, value: &'ast syn::TraitItem) {
        self.opaque |= matches!(value, syn::TraitItem::Verbatim(_));
        syn::visit::visit_trait_item(self, value);
    }
    fn visit_foreign_item(&mut self, value: &'ast syn::ForeignItem) {
        self.opaque |= matches!(value, syn::ForeignItem::Verbatim(_));
        syn::visit::visit_foreign_item(self, value);
    }
}
/// Renames every identifier whose normalized text is `old`. Lifetimes and
/// labels are not identifiers in the source, so they are skipped.
struct Rename<'a> {
    old: &'a str,
    new: &'a syn::Ident,
}
impl VisitMut for Rename<'_> {
    fn visit_ident_mut(&mut self, ident: &mut syn::Ident) {
        if normalized(ident) == self.old {
            *ident = self.new.clone();
        }
    }
    fn visit_lifetime_mut(&mut self, _: &mut syn::Lifetime) {}
}
fn identifier(name: &str, value: &str, what: &str) -> Result<syn::Ident> {
    let ident = syn::parse_str::<syn::Ident>(value)
        .map_err(|_| bad(name, &format!("expected Rust identifier for {what}")))?;
    if RESERVED.contains(&normalized(&ident).as_str()) {
        return Err(bad(
            name,
            &format!("{what} cannot be self, Self, super, crate or _"),
        ));
    }
    Ok(ident)
}
pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.len() != 3 || input.is_some() {
        return Err(bad(name, "invalid argument count or pipeline input"));
    }
    let source = text(name, &args[0], 262144, 4096)?;
    let old_text = text(name, &args[1], 256, 32)?;
    let new_text = text(name, &args[2], 256, 32)?;
    let old_ident = identifier(name, old_text, "old name")?;
    let new_ident = identifier(name, new_text, "new name")?;
    let old = normalized(&old_ident);
    let new = normalized(&new_ident);
    if old == new {
        return Err(bad(name, "old and new names are equal"));
    }
    let mut file =
        syn::parse_file(source).map_err(|_| bad(name, "expected one Rust source file"))?;
    let mut scan = Scan {
        old: &old,
        new: &new,
        old_count: 0,
        new_count: 0,
        opaque: false,
    };
    scan.visit_file(&file);
    if scan.new_count > 0 {
        return Err(bad(name, "new name already occurs as an identifier"));
    }
    if scan.old_count == 0 {
        return Err(bad(name, "old name does not occur as an identifier"));
    }
    if scan.opaque {
        return Err(bad(
            name,
            "old name occurs in a macro, non-doc attribute, or verbatim syntax",
        ));
    }
    Rename {
        old: &old,
        new: &new_ident,
    }
    .visit_file_mut(&mut file);
    let output = file.to_token_stream().to_string();
    if output.len() > 2_097_152 {
        return Err(budget(name));
    }
    let renamed =
        syn::parse_file(&output).map_err(|_| bad(name, "output is not a Rust source file"))?;
    let mut check = Scan {
        old: &old,
        new: &new,
        old_count: 0,
        new_count: 0,
        opaque: false,
    };
    check.visit_file(&renamed);
    if check.old_count != 0 || check.new_count != scan.old_count {
        return Err(bad(
            name,
            "rename did not change exactly the matching identifiers",
        ));
    }
    Ok(Value::Str(output))
}
