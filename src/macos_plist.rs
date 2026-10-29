//! A reader for the XML property lists that macOS tools print, such as
//! `diskutil info -plist`. It reads the top-level dictionary and keeps the
//! values of its keys: strings, integers and booleans as their Rust types, and
//! anything else (nested dictionaries, arrays, dates, data) as `Other`, with
//! its contents skipped. It is a small tokenizer, not a general XML parser: it
//! takes no DTD, no entities beyond the five the plist format uses, and no
//! namespaces. It compiles on every OS so its unit tests run everywhere; the
//! builtins that call it run only on macOS.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::BTreeMap;

/// Most top-level keys one property list may hold.
const MAX_KEYS: usize = 4096;

/// A value of one top-level key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlistValue {
    Str(String),
    Int(i64),
    Bool(bool),
    /// A dictionary, array, real, date or data value, which is not read.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Open(String),
    Close(String),
    Empty(String),
    Text(String),
}

/// The five entities the property list format uses. `&amp;` goes last, so an
/// escaped `&lt;` written as `&amp;lt;` decodes once, to `&lt;`.
fn decode(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Split the document into tags and the text between them. Declarations
/// (`<?xml ?>`, `<!DOCTYPE>`) and comments are dropped.
fn tokens(text: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let Some(lt) = rest.find('<') else {
            let tail = rest.trim();
            if !tail.is_empty() {
                out.push(Tok::Text(decode(tail)));
            }
            return Ok(out);
        };
        let before = rest[..lt].trim();
        if !before.is_empty() {
            out.push(Tok::Text(decode(before)));
        }
        let after = &rest[lt..];
        if after.starts_with("<!--") {
            match after.find("-->") {
                Some(end) => rest = &after[end + 3..],
                None => return Err("a comment is not closed".to_string()),
            }
            continue;
        }
        let Some(gt) = after.find('>') else {
            return Err("a tag is not closed".to_string());
        };
        let inner = &after[1..gt];
        rest = &after[gt + 1..];
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        if let Some(name) = inner.strip_prefix('/') {
            out.push(Tok::Close(name.trim().to_string()));
        } else if let Some(name) = inner.strip_suffix('/') {
            out.push(Tok::Empty(name.trim().to_string()));
        } else {
            out.push(Tok::Open(inner.trim().to_string()));
        }
    }
}

/// The text of an element whose content starts at `at` and whose closing tag
/// is `</name>`. Returns the text and the index after the closing tag.
fn read_text(toks: &[Tok], at: usize, name: &str) -> Result<(String, usize), String> {
    let mut text = String::new();
    let mut i = at;
    loop {
        match toks.get(i) {
            Some(Tok::Text(t)) => {
                text.push_str(t);
                i += 1;
            }
            Some(Tok::Close(n)) if n.as_str() == name => return Ok((text, i + 1)),
            other => return Err(format!("a <{name}> element is not closed; found {other:?}")),
        }
    }
}

/// Skip the element opened at `at`, nested or not, and return the index after
/// its closing tag. Every open tag in a well-formed document has one close.
fn skip_block(toks: &[Tok], at: usize) -> Result<usize, String> {
    let mut depth = 0usize;
    let mut i = at;
    while let Some(tok) = toks.get(i) {
        match tok {
            Tok::Open(_) => depth += 1,
            Tok::Close(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            Tok::Empty(_) | Tok::Text(_) => {}
        }
        i += 1;
    }
    Err("a dictionary or array is not closed".to_string())
}

/// The value that follows a key, at index `at`, and the index after it.
fn read_value(toks: &[Tok], at: usize) -> Result<(PlistValue, usize), String> {
    match toks.get(at) {
        Some(Tok::Empty(name)) => Ok(match name.as_str() {
            "true" => (PlistValue::Bool(true), at + 1),
            "false" => (PlistValue::Bool(false), at + 1),
            "string" => (PlistValue::Str(String::new()), at + 1),
            _ => (PlistValue::Other, at + 1),
        }),
        Some(Tok::Open(name)) => match name.as_str() {
            "dict" | "array" => Ok((PlistValue::Other, skip_block(toks, at)?)),
            "string" => {
                let (text, next) = read_text(toks, at + 1, "string")?;
                Ok((PlistValue::Str(text), next))
            }
            "integer" => {
                let (text, next) = read_text(toks, at + 1, "integer")?;
                let n = text
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("an integer {text:?} is not a 64-bit Int"))?;
                Ok((PlistValue::Int(n), next))
            }
            other => {
                let (_, next) = read_text(toks, at + 1, other)?;
                Ok((PlistValue::Other, next))
            }
        },
        other => Err(format!("a key has no value; found {other:?}")),
    }
}

/// The keys and values of the document's top-level dictionary. A document with
/// no dictionary, or one that is cut short, is an error with the reason; the
/// caller decides what a bad document means for its builtin.
pub(crate) fn parse_top_dict(text: &str) -> Result<BTreeMap<String, PlistValue>, String> {
    let toks = tokens(text)?;
    let start = toks
        .iter()
        .position(|t| matches!(t, Tok::Open(n) if n.as_str() == "dict"))
        .ok_or_else(|| "the document has no top-level dictionary".to_string())?;
    let mut map = BTreeMap::new();
    let mut i = start + 1;
    loop {
        match toks.get(i) {
            None => return Err("the top-level dictionary is not closed".to_string()),
            Some(Tok::Close(n)) if n.as_str() == "dict" => return Ok(map),
            Some(Tok::Open(n)) if n.as_str() == "key" => {
                let (key, next) = read_text(&toks, i + 1, "key")?;
                let (value, after) = read_value(&toks, next)?;
                if map.len() >= MAX_KEYS && !map.contains_key(&key) {
                    return Err(format!("more than {MAX_KEYS} keys"));
                }
                map.insert(key, value);
                i = after;
            }
            Some(other) => {
                return Err(format!(
                    "a dictionary holds only key and value pairs; found {other:?}"
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<!-- a comment with <string>inside</string> -->
	<key>DeviceIdentifier</key>
	<string>disk3s1s1</string>
	<key>DeviceNode</key>
	<string>/dev/disk3s1s1</string>
	<key>FilesystemType</key>
	<string>apfs</string>
	<key>VolumeName</key>
	<string>Macintosh &amp; Cheese</string>
	<key>VolumeUUID</key>
	<string>1A2B3C4D-0000-4000-8000-000000000001</string>
	<key>Size</key>
	<integer>494384795648</integer>
	<key>Writable</key>
	<true/>
	<key>Internal</key>
	<false/>
	<key>APFSPhysicalStores</key>
	<array>
		<dict>
			<key>APFSPhysicalStore</key>
			<string>disk0s2</string>
		</dict>
	</array>
	<key>Empty</key>
	<string></string>
</dict>
</plist>
"#;

    #[test]
    fn top_level_strings_integers_and_booleans_are_read() {
        let map = parse_top_dict(SAMPLE).expect("sample parses");
        let s = |k: &str| match map.get(k) {
            Some(PlistValue::Str(v)) => v.clone(),
            other => panic!("{k}: {other:?}"),
        };
        assert_eq!(s("DeviceNode"), "/dev/disk3s1s1");
        assert_eq!(s("FilesystemType"), "apfs");
        assert_eq!(s("VolumeName"), "Macintosh & Cheese");
        assert_eq!(s("Empty"), "");
        assert_eq!(map.get("Size"), Some(&PlistValue::Int(494384795648)));
        assert_eq!(map.get("Writable"), Some(&PlistValue::Bool(true)));
        assert_eq!(map.get("Internal"), Some(&PlistValue::Bool(false)));
    }

    #[test]
    fn nested_containers_are_skipped_and_do_not_leak_keys() {
        let map = parse_top_dict(SAMPLE).expect("sample parses");
        assert_eq!(map.get("APFSPhysicalStores"), Some(&PlistValue::Other));
        // The nested dict's key is not a top-level key.
        assert!(!map.contains_key("APFSPhysicalStore"));
        // The comment's <string> is not a value either.
        assert!(!map.values().any(|v| v == &PlistValue::Str("inside".into())));
        assert_eq!(map.len(), 10);
    }

    #[test]
    fn a_document_without_a_dictionary_is_an_error() {
        let err = parse_top_dict("<plist><string>x</string></plist>").unwrap_err();
        assert!(err.contains("no top-level dictionary"), "{err}");
        assert!(parse_top_dict("").is_err());
    }

    #[test]
    fn a_truncated_document_is_an_error_not_a_partial_map() {
        let cut = &SAMPLE[..SAMPLE.find("<key>VolumeUUID").unwrap()];
        assert!(parse_top_dict(cut).is_err());
        let open_string = "<plist><dict><key>A</key><string>x";
        assert!(parse_top_dict(open_string).is_err());
    }

    #[test]
    fn a_non_integer_in_an_integer_element_is_an_error() {
        let text = "<plist><dict><key>N</key><integer>twelve</integer></dict></plist>";
        assert!(parse_top_dict(text).is_err());
    }

    #[test]
    fn entities_decode_once() {
        let text = "<plist><dict><key>A</key><string>&amp;lt; &lt;b&gt; &quot;q&quot;</string></dict></plist>";
        let map = parse_top_dict(text).expect("parses");
        assert_eq!(
            map.get("A"),
            Some(&PlistValue::Str("&lt; <b> \"q\"".to_string()))
        );
    }
}
