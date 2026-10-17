//! Bounded move of one top-level Rust item between two supplied source texts.
//!
//! Pure: both sources arrive as strings and both results are returned as
//! strings. Nothing is read from or written to the filesystem, and nothing is
//! compiled. The caller must add any `use` or visibility change the moved item
//! needs before the sources build together.
use crate::{safety, value::Value};
use anyhow::Result;
use quote::ToTokens;
use std::collections::BTreeMap;
use syn::visit::Visit;

const SOURCE_BYTES: usize = 262_144;
const SOURCE_PUNCTUATION: usize = 4096;
const ITEM_BYTES: usize = 256;
const ITEM_PUNCTUATION: usize = 32;
const OUTPUT_BYTES: usize = 2_097_152;
/// Attributes that survive a move unchanged in meaning. Anything else, such as
/// `no_mangle`, `macro_export`, `path` or `cfg_attr`, depends on where the item
/// sits and is refused.
const ALLOWED_ATTRIBUTES: [&str; 9] = [
    "doc", "derive", "allow", "warn", "deny", "inline", "must_use", "cfg", "repr",
];

fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}

fn budget(name: &str) -> anyhow::Error {
    safety::budget_exceeded(
        name,
        "input/output exceeds move limits".into(),
        "supply each source <=256 KiB/4096 punctuation, item name <=256 bytes/32 punctuation, output <=2 MiB",
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

fn parse(name: &str, source: &str) -> Result<syn::File> {
    let file = syn::parse_str::<syn::File>(source)
        .map_err(|_| bad(name, "expected Rust source file text"))?;
    if file.shebang.is_some() {
        return Err(bad(name, "shebang lines are unsupported"));
    }
    Ok(file)
}

/// The name a top-level item defines in the type, value or module namespace.
/// Macros, impls, uses and foreign blocks define no movable name.
fn defined(item: &syn::Item) -> Option<String> {
    let ident = match item {
        syn::Item::Fn(i) => &i.sig.ident,
        syn::Item::Struct(i) => &i.ident,
        syn::Item::Enum(i) => &i.ident,
        syn::Item::Union(i) => &i.ident,
        syn::Item::Const(i) => &i.ident,
        syn::Item::Static(i) => &i.ident,
        syn::Item::Type(i) => &i.ident,
        syn::Item::Trait(i) => &i.ident,
        syn::Item::Mod(i) => &i.ident,
        _ => return None,
    };
    Some(normalized(ident))
}

/// Whether an impl block's self type is the plain single-segment path `name`.
fn impls(item: &syn::Item, name: &str) -> bool {
    matches!(item, syn::Item::Impl(i)
        if matches!(&*i.self_ty, syn::Type::Path(p)
            if p.qself.is_none()
                && p.path.segments.len() == 1
                && normalized(&p.path.segments[0].ident) == name))
}

/// Whether a use tree binds `name`, either as its last segment or as its
/// rename. Globs are not counted; they bind nothing by name.
fn imports(tree: &syn::UseTree, name: &str) -> bool {
    match tree {
        syn::UseTree::Path(p) => imports(&p.tree, name),
        syn::UseTree::Name(n) => normalized(&n.ident) == name,
        syn::UseTree::Rename(r) => normalized(&r.ident) == name || normalized(&r.rename) == name,
        syn::UseTree::Group(g) => g.items.iter().any(|tree| imports(tree, name)),
        syn::UseTree::Glob(_) => false,
    }
}

fn attributes(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

fn allowed(attribute: &syn::Attribute) -> bool {
    attribute
        .path()
        .get_ident()
        .is_some_and(|ident| ALLOWED_ATTRIBUTES.contains(&normalized(ident).as_str()))
}

/// Macro definitions and disallowed attributes anywhere inside the moved item,
/// including its nested items.
#[derive(Default)]
struct Opaque {
    macro_rules: bool,
    attribute: bool,
}

impl<'ast> Visit<'ast> for Opaque {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        self.attribute |= !attributes(item).iter().all(allowed);
        syn::visit::visit_item(self, item);
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        self.macro_rules |= item.mac.path.is_ident("macro_rules");
        syn::visit::visit_item_macro(self, item);
    }
}

pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.len() != 3 || input.is_some() {
        return Err(bad(name, "invalid argument count or pipeline input"));
    }
    let source_text = text(name, &args[0], SOURCE_BYTES, SOURCE_PUNCTUATION)?;
    let item_text = text(name, &args[1], ITEM_BYTES, ITEM_PUNCTUATION)?;
    let destination_text = text(name, &args[2], SOURCE_BYTES, SOURCE_PUNCTUATION)?;
    let mut source = parse(name, source_text)?;
    let mut destination = parse(name, destination_text)?;
    let identifier = syn::parse_str::<syn::Ident>(item_text)
        .map_err(|_| bad(name, "expected Rust item identifier"))?;
    let item_name = normalized(&identifier);

    // Moving a type would orphan its impls in the source, so refuse outright.
    if source.items.iter().any(|item| impls(item, &item_name)) {
        return Err(bad(
            name,
            "source has an impl block for the item's type; moving it would orphan the impl",
        ));
    }
    let positions: Vec<usize> = source
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| defined(item).as_deref() == Some(item_name.as_str()))
        .map(|(position, _)| position)
        .collect();
    let [position] = positions[..] else {
        return Err(bad(
            name,
            "source must define the item exactly once at top level",
        ));
    };
    let mut opaque = Opaque::default();
    let moved = &source.items[position];
    if matches!(moved, syn::Item::Mod(m) if m.content.is_none()) {
        return Err(bad(
            name,
            "out-of-line modules cannot be moved; supply the inline module",
        ));
    }
    opaque.visit_item(moved);
    if opaque.macro_rules {
        return Err(bad(
            name,
            "moved item defines macro_rules; macros cannot be moved",
        ));
    }
    if opaque.attribute {
        return Err(bad(
            name,
            "moved item carries an attribute outside doc, derive, allow, warn, deny, inline, must_use, cfg or repr",
        ));
    }
    if destination.items.iter().any(|item| {
        defined(item).as_deref() == Some(item_name.as_str())
            || matches!(item, syn::Item::Use(u) if imports(&u.tree, &item_name))
    }) {
        return Err(bad(
            name,
            "destination already defines or imports the item name",
        ));
    }

    let moved = source.items.remove(position);
    destination.items.push(moved);
    let source_output = source.to_token_stream().to_string();
    let destination_output = destination.to_token_stream().to_string();
    if source_output.len() > OUTPUT_BYTES || destination_output.len() > OUTPUT_BYTES {
        return Err(budget(name));
    }
    parse(name, &source_output).map_err(|_| bad(name, "output source is not a Rust file"))?;
    parse(name, &destination_output)
        .map_err(|_| bad(name, "output destination is not a Rust file"))?;
    let mut record = BTreeMap::new();
    record.insert("source".to_string(), Value::Str(source_output));
    record.insert("destination".to_string(), Value::Str(destination_output));
    Ok(Value::Record(record))
}
