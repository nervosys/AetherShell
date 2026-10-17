//! Whole-file, syntactic renaming of one Rust identifier. Every identifier
//! token whose normalized text equals the old name becomes the new name;
//! lifetimes, strings and comments are not identifiers and are left alone.
use crate::{safety, value::Value};
use anyhow::Result;
use quote::ToTokens;
use syn::visit::Visit;
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
fn word_end(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }
    end
}
/// `start` is the opening quote of a normal string or byte/C string.
fn string_end(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}
/// `start` is the first `#` or the opening quote of a raw string.
fn raw_string_end(chars: &[char], start: usize) -> usize {
    let hashes = chars[start..].iter().take_while(|c| **c == '#').count();
    let mut i = start + hashes + 1;
    while i < chars.len() {
        if chars[i] == '"'
            && i + hashes < chars.len()
            && chars[i + 1..=i + hashes].iter().all(|c| *c == '#')
        {
            return i + hashes + 1;
        }
        i += 1;
    }
    chars.len()
}
/// `start` is a quote: a char literal, or else a lifetime or label whose
/// identifier must not be treated as a name.
fn quote_end(chars: &[char], start: usize) -> usize {
    if chars.get(start + 1) == Some(&'\\') {
        let mut i = start + 2;
        while i < chars.len() && chars[i] != '\'' {
            i += 1;
        }
        return (i + 1).min(chars.len());
    }
    if chars.get(start + 2) == Some(&'\'') {
        return start + 3;
    }
    word_end(chars, start + 1)
}
/// Renames identifier tokens in token-formatted Rust text. Strings, raw
/// strings, chars, lifetimes and numbers are copied verbatim; raw identifiers
/// normalize before comparison; the caller reparses and verifies the result.
fn rename_tokens(text: &str, old: &str, new: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            let end = string_end(&chars, i);
            out.extend(&chars[i..end]);
            i = end;
        } else if c == '\'' {
            let end = quote_end(&chars, i);
            out.extend(&chars[i..end]);
            i = end;
        } else if c.is_ascii_digit() {
            let mut end = i;
            while end < chars.len()
                && (chars[end].is_alphanumeric()
                    || chars[end] == '_'
                    || (chars[end] == '.' && chars.get(end + 1).is_some_and(char::is_ascii_digit)))
            {
                end += 1;
            }
            out.extend(&chars[i..end]);
            i = end;
        } else if c.is_alphabetic() || c == '_' {
            let end = word_end(&chars, i);
            let word: String = chars[i..end].iter().collect();
            let hashes = chars[end..].iter().take_while(|c| **c == '#').count();
            if matches!(word.as_str(), "b" | "c") && chars.get(end) == Some(&'"') {
                let stop = string_end(&chars, end);
                out.extend(&chars[i..stop]);
                i = stop;
            } else if word == "b" && chars.get(end) == Some(&'\'') {
                let stop = quote_end(&chars, end);
                out.extend(&chars[i..stop]);
                i = stop;
            } else if matches!(word.as_str(), "r" | "br" | "cr")
                && chars.get(end + hashes) == Some(&'"')
            {
                let stop = raw_string_end(&chars, end);
                out.extend(&chars[i..stop]);
                i = stop;
            } else if word == "r"
                && chars.get(end) == Some(&'#')
                && chars
                    .get(end + 1)
                    .is_some_and(|c| c.is_alphabetic() || *c == '_')
            {
                let stop = word_end(&chars, end + 1);
                let name: String = chars[end + 1..stop].iter().collect();
                if name == old {
                    out.push_str(new);
                } else {
                    out.push_str("r#");
                    out.push_str(&name);
                }
                i = stop;
            } else {
                out.push_str(if word == old { new } else { &word });
                i = end;
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
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
    let file = syn::parse_file(source).map_err(|_| bad(name, "expected one Rust source file"))?;
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
    let output = rename_tokens(
        &file.to_token_stream().to_string(),
        &old,
        &new_ident.to_string(),
    );
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
