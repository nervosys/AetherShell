//! Contracts for the `web_xpath` builtin: the supported XPath subset over the
//! in-tree lenient HTML parser, and the refusals for everything else.
//!
//! Pure: every call here is in-process, with no network, launcher or file
//! access. The parser and evaluator are unit-tested in `src/web_xpath.rs`.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::sync::{Mutex, Once};

fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_web_xpath_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
}

/// `IRONSHELL_MODE` is process-global; every call takes this lock.
static MODE_LOCK: Mutex<()> = Mutex::new(());

fn call(args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    isolate();
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    let mut env = Env::new();
    builtins::call("web_xpath", args, &mut env)
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

fn query(html: &str, expr: &str) -> Vec<String> {
    match call(vec![text(html), text(expr)]) {
        Ok(Value::Array(items)) => items
            .into_iter()
            .map(|v| match v {
                Value::Str(s) => s,
                other => panic!("web_xpath {expr}: a non-string result {other:?}"),
            })
            .collect(),
        other => panic!("web_xpath {expr}: expected an array, got {other:?}"),
    }
}

/// The first `E_...` code in an error's rendered text.
fn code_of(r: &anyhow::Result<Value>) -> String {
    match r {
        Ok(v) => panic!("expected a coded refusal, got {v:?}"),
        Err(e) => {
            let msg = format!("{e:#}");
            let at = msg
                .find("E_")
                .unwrap_or_else(|| panic!("no code in: {msg}"));
            msg[at..]
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                .collect()
        }
    }
}

fn refused(html: &str, expr: &str) -> String {
    let r = call(vec![text(html), text(expr)]);
    let code = code_of(&r);
    assert_eq!(code, "E_BAD_ARG", "{expr}: {r:?}");
    code
}

const LIST: &str = "<ul><li>a</li><li>b</li><li>c</li></ul>";

// ── node tests and axes ─────────────────────────────────────────────────

#[test]
fn name_star_text_and_node_tests() {
    assert_eq!(
        query(LIST, "//li"),
        ["<li>a</li>", "<li>b</li>", "<li>c</li>"]
    );
    assert_eq!(
        query("<p><b>x</b><i>y</i></p>", "/p/*"),
        ["<b>x</b>", "<i>y</i>"]
    );
    assert_eq!(query("<p>a<b>x</b>c</p>", "/p/text()"), ["a", "c"]);
    assert_eq!(query("<p>a<b>x</b></p>", "/p/node()").len(), 2);
    assert_eq!(query("<p>a<b>x</b></p>", "/p/b/text()"), ["x"]);
}

#[test]
fn attribute_tests_return_decoded_values() {
    let html = "<a href='/x' title=\"a &amp; b\" data-n=3 hidden>y</a>";
    assert_eq!(query(html, "//a/@href"), ["/x"]);
    assert_eq!(query(html, "//a/@title"), ["a & b"]);
    assert_eq!(query(html, "//a/@data-n"), ["3"]);
    assert_eq!(query(html, "//a/@hidden"), [""]);
    assert_eq!(query(html, "//a/@*").len(), 4);
}

#[test]
fn slash_and_double_slash_differ_as_in_xpath() {
    let html = "<div><p>1</p><span><p>2</p></span></div>";
    assert_eq!(query(html, "/div/p"), ["<p>1</p>"]);
    assert_eq!(query(html, "/p"), Vec::<String>::new());
    assert_eq!(query(html, "//p"), ["<p>1</p>", "<p>2</p>"]);
    assert_eq!(query(html, "div//p"), ["<p>1</p>", "<p>2</p>"]);
    assert_eq!(
        query(html, "p"),
        Vec::<String>::new(),
        "relative from the root"
    );
}

#[test]
fn position_predicates_count_within_each_parent() {
    let html = "<div><ul><li>1</li><li>2</li></ul><ol><li>3</li></ol></div>";
    // //li[1] is the first li of each parent, not the first in the document.
    assert_eq!(query(html, "//li[1]"), ["<li>1</li>", "<li>3</li>"]);
    assert_eq!(query(html, "//ul/li[2]"), ["<li>2</li>"]);
    // Parenthesised path expressions are not supported, so (//li)[1] is a refusal.
    refused(html, "(//li)[1]");
}

#[test]
fn explicit_axes_match_their_abbreviations() {
    let html = "<div><p id='x'>a</p></div>";
    assert_eq!(query(html, "child::div/child::p"), query(html, "/div/p"));
    assert_eq!(query(html, "/descendant::p"), ["<p id='x'>a</p>"]);
    assert_eq!(query(html, "//p/parent::*"), ["<div><p id='x'>a</p></div>"]);
    assert_eq!(query(html, "//p/self::p"), ["<p id='x'>a</p>"]);
    assert_eq!(query(html, "//p/attribute::id"), ["x"]);
    assert_eq!(query(html, "//p/descendant-or-self::node()/text()"), ["a"]);
    assert_eq!(query(html, "//p/..")[0], "<div><p id='x'>a</p></div>");
}

#[test]
fn union_is_in_document_order_without_duplicates() {
    let html = "<p><b>1</b><i>2</i><b>3</b></p>";
    assert_eq!(
        query(html, "//i | //b"),
        ["<b>1</b>", "<i>2</i>", "<b>3</b>"]
    );
    assert_eq!(query(html, "//b | //b").len(), 2);
    assert_eq!(query(html, "//b | /p/b[1]"), ["<b>1</b>", "<b>3</b>"]);
}

// ── predicates ──────────────────────────────────────────────────────────

#[test]
fn attribute_and_text_predicates() {
    let html = "<a href='/x'>one</a><a>two</a><a href='/y' rel='nofollow'>three</a>";
    assert_eq!(query(html, "//a[@href]").len(), 2);
    assert_eq!(
        query(html, "//a[@href='/y']"),
        ["<a href='/y' rel='nofollow'>three</a>"]
    );
    assert_eq!(query(html, "//a[@href!=\"/x\"]").len(), 1);
    assert_eq!(query(html, "//a[text()='two']"), ["<a>two</a>"]);
    assert_eq!(query(html, "//a[.='one']"), ["<a href='/x'>one</a>"]);
    assert_eq!(query(html, "//a[normalize-space(.)='one']").len(), 1);
}

#[test]
fn child_existence_and_comparison() {
    let html = "<p><b>yes</b></p><p><i>no</i></p>";
    assert_eq!(query(html, "//p[b]"), ["<p><b>yes</b></p>"]);
    assert_eq!(query(html, "//p[b='yes']"), ["<p><b>yes</b></p>"]);
    assert_eq!(query(html, "//p[not(b)]"), ["<p><i>no</i></p>"]);
}

#[test]
fn and_or_not_and_parentheses() {
    let html = "<a href='/x' class='c'>1</a><a href='/y'>2</a><a class='c'>3</a>";
    assert_eq!(
        query(html, "//a[@href and @class]"),
        ["<a href='/x' class='c'>1</a>"]
    );
    assert_eq!(query(html, "//a[@href or @class]").len(), 3);
    assert_eq!(query(html, "//a[not(@class)]"), ["<a href='/y'>2</a>"]);
    assert_eq!(
        query(html, "//a[(@href='/y') or (not(@href) and @class)]"),
        ["<a href='/y'>2</a>", "<a class='c'>3</a>"]
    );
}

#[test]
fn contains_and_starts_with() {
    let html = "<a href='/docs/intro'>i</a><a href='/blog/x'>b</a>";
    assert_eq!(
        query(html, "//a[contains(@href,'docs')]"),
        ["<a href='/docs/intro'>i</a>"]
    );
    assert_eq!(
        query(html, "//a[starts-with(@href, '/blog')]"),
        ["<a href='/blog/x'>b</a>"]
    );
    assert_eq!(query(html, "//a[contains(., '')]").len(), 2);
}

#[test]
fn position_comparisons_and_last() {
    assert_eq!(query(LIST, "//li[position() < 3]").len(), 2);
    assert_eq!(query(LIST, "//li[position() >= 2]").len(), 2);
    assert_eq!(query(LIST, "//li[last()]"), ["<li>c</li>"]);
    assert_eq!(query(LIST, "//li[2][1]"), ["<li>b</li>"]);
}

// ── markup ──────────────────────────────────────────────────────────────

#[test]
fn entities_decode_in_text_and_attributes() {
    let html = "<p>&lt;a&gt; &quot;q&quot; &apos;s&apos; &amp; &#65;&#x42; &nbsp;&bogus;</p>";
    assert_eq!(
        query(html, "//p/text()"),
        ["<a> \"q\" 's' & AB \u{a0}&bogus;"]
    );
    assert_eq!(query("<i t='&lt;&#x3c;'></i>", "//i/@t"), ["<<"]);
}

#[test]
fn comments_doctype_and_script_content_are_not_elements() {
    let html =
        "<!DOCTYPE html><!-- <p>no</p> --><?xml x?><div><script>if (a<b) {}</script><p>x</p></div>";
    assert_eq!(query(html, "//p"), ["<p>x</p>"]);
    assert_eq!(query(html, "//script/text()"), ["if (a<b) {}"]);
    assert_eq!(query(html, "//div/*").len(), 2);
    assert_eq!(
        query("<style>a > b { }</style>", "//style/text()"),
        ["a > b { }"]
    );
}

#[test]
fn cdata_is_text() {
    assert_eq!(
        query("<p><![CDATA[1 < 2 & 3]]></p>", "//p/text()"),
        ["1 < 2 & 3"]
    );
}

#[test]
fn void_and_self_closing_elements_have_no_children() {
    let html = "<p>a<br>b<img src=x.png>c</p><q/>after";
    assert_eq!(query(html, "//br"), ["<br>"]);
    assert_eq!(query(html, "//p/text()"), ["a", "b", "c"]);
    assert_eq!(query(html, "//q/text()"), Vec::<String>::new());
    assert_eq!(query(html, "//img/@src"), ["x.png"]);
}

#[test]
fn unclosed_and_mismatched_tags_are_lenient() {
    // An unclosed <li> runs to the next close; a stray </span> is ignored.
    let html = "<ul><li>a<li>b</span></ul>";
    assert_eq!(query(html, "//ul/li"), ["<li>a<li>b</span>"]);
    // An element open at the end of input is closed there.
    assert_eq!(query("<div><p>open", "//p"), ["<p>open"]);
}

#[test]
fn tag_and_attribute_names_are_case_insensitive() {
    let html = "<DIV ID='x'><P>Hi</P></DIV>";
    assert_eq!(query(html, "//div/p"), ["<P>Hi</P>"]);
    assert_eq!(query(html, "//DIV/@id"), ["x"]);
}

#[test]
fn empty_match_is_an_empty_array_not_a_refusal() {
    assert_eq!(query(LIST, "//table"), Vec::<String>::new());
}

// ── refusals ────────────────────────────────────────────────────────────

#[test]
fn unsupported_expressions_are_refused_with_bad_arg() {
    for expr in [
        "count(//a)",
        "//a/following-sibling::b",
        "$x",
        "//a[",
        "",
        "   ",
        "string-length(//a)",
        "//a[count(b) = 1]",
        "//a[@x = //b]",
        "//a | 1",
        "//a)",
        "/a/namespace::*",
    ] {
        refused("<a><b/></a>", expr);
    }
}

#[test]
fn a_refusal_names_the_unsupported_part() {
    let r = call(vec![text("<a/>"), text("count(//a)")]);
    let msg = format!("{:#}", r.expect_err("refused"));
    assert!(msg.contains("count()"), "{msg}");
    assert!(msg.contains("a supported XPath expression"), "{msg}");
}

#[test]
fn oversized_html_is_a_budget_refusal_before_parsing() {
    let big = "a".repeat(iron_shell::web_xpath::MAX_HTML_BYTES + 1);
    let r = call(vec![text(&big), text("//a")]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED", "{r:?}");
}

#[test]
fn more_than_the_result_cap_is_refused_not_truncated() {
    let html = "<i></i>".repeat(1001);
    let r = call(vec![text(&html), text("//i")]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED", "{r:?}");
    // Exactly the cap is answered in full.
    let html = "<i></i>".repeat(1000);
    assert_eq!(query(&html, "//i").len(), 1000);
}

#[test]
fn nesting_deeper_than_the_cap_is_a_budget_refusal() {
    let deep = "<p>".repeat(iron_shell::web_xpath::MAX_DEPTH + 1);
    let r = call(vec![text(&deep), text("//p")]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED", "{r:?}");
}

#[test]
fn type_errors_and_arity_are_bad_arg() {
    let r = call(vec![Value::Int(1), text("//a")]);
    assert_eq!(code_of(&r), "E_BAD_ARG", "{r:?}");
    let r = call(vec![text("<a/>"), Value::Int(1)]);
    assert_eq!(code_of(&r), "E_BAD_ARG", "{r:?}");
    let r = call(vec![text("<a/>")]);
    assert_eq!(code_of(&r), "E_BAD_ARG", "{r:?}");
    let r = call(vec![text("<a/>"), text("//a"), text("extra")]);
    assert_eq!(code_of(&r), "E_BAD_ARG", "{r:?}");
}

#[test]
fn piped_input_is_refused() {
    let _guard = MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    isolate();
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    let mut env = Env::new();
    let r = builtins::call_with_input(
        "web_xpath",
        vec![text("<a/>"), text("//a")],
        Some(text("piped")),
        &mut env,
    );
    assert_eq!(code_of(&r), "E_BAD_ARG", "{r:?}");
}
