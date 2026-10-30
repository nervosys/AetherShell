//! Contracts for the older monitor_* readers: nethogs, iftop, sockets, the ip
//! family (addr, route, link), ethtool, users, logins and syslog.
//!
//! Refusals run everywhere and touch no tool: piped input, surplus arguments,
//! wrong types and option-like text are each a coded E_BAD_ARG, decided before
//! any program is started. The real calls are read-only queries of this machine.
//! Each accepts the declared return shape, or a coded tool outcome
//! (E_TOOL_MISSING, E_TOOL_FAILED, E_NOT_FOUND, E_UNIMPLEMENTED or
//! E_BUDGET_EXCEEDED), and the assertion message says which one it saw. The
//! parsers are unit-tested on fixed text in src/monitor_parse.rs.

use iron_shell::env::Env;
use iron_shell::value::Value;

/// The ten readers this file covers.
const READERS: &[&str] = &[
    "monitor_nethogs",
    "monitor_iftop",
    "monitor_sockets",
    "monitor_ip_addr",
    "monitor_ip_route",
    "monitor_ip_link",
    "monitor_ethtool",
    "monitor_users",
    "monitor_logins",
    "monitor_syslog",
];

/// The coded outcomes a real call may end in on a machine that lacks the tool,
/// the privilege, or the platform.
const TOOL_OUTCOMES: &[&str] = &[
    "E_TOOL_MISSING",
    "E_TOOL_FAILED",
    "E_NOT_FOUND",
    "E_UNIMPLEMENTED",
    "E_BUDGET_EXCEEDED",
];

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call_with_input(name, args, Some(input), &mut env)
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn expect_refused(name: &str, args: Vec<Value>, code: &str) {
    match call(name, args.clone()) {
        Ok(v) => panic!("{name}{args:?} should be refused with {code}, got {v:?}"),
        Err(e) => assert_eq!(code_of(&e), code, "{name}{args:?}: {e}"),
    }
}

/// A real call's answer: its value, or None after checking that it failed with
/// one of the coded outcomes a missing tool or privilege produces.
fn answer_or_tool_outcome(name: &str, result: anyhow::Result<Value>) -> Option<Value> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            let code = code_of(&e);
            assert!(
                TOOL_OUTCOMES.contains(&code.as_str()),
                "{name} failed with {code}, which is not a tool outcome: {e}"
            );
            eprintln!("{name}: accepted {code} on this machine: {e}");
            None
        }
    }
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    match v {
        Value::Record(m) => match m.get(key) {
            Some(Value::Str(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Refusals: decided before any tool runs, everywhere.
// ---------------------------------------------------------------------------

#[test]
fn piped_input_is_refused_by_every_reader() {
    for name in READERS {
        match call_piped(name, Vec::new(), s("x")) {
            Ok(v) => panic!("{name} accepted pipeline input and answered {v:?}"),
            Err(e) => assert_eq!(code_of(&e), "E_BAD_ARG", "{name}: {e}"),
        }
    }
}

#[test]
fn surplus_arguments_are_refused_by_every_reader() {
    expect_refused("monitor_nethogs", vec![s("x")], "E_BAD_ARG");
    expect_refused("monitor_iftop", vec![s("x")], "E_BAD_ARG");
    expect_refused("monitor_ip_route", vec![s("x")], "E_BAD_ARG");
    expect_refused("monitor_users", vec![s("x")], "E_BAD_ARG");
    expect_refused("monitor_sockets", vec![s("tcp"), s("x")], "E_BAD_ARG");
    expect_refused("monitor_ip_addr", vec![s("lo"), s("x")], "E_BAD_ARG");
    expect_refused("monitor_ip_link", vec![s("lo"), s("x")], "E_BAD_ARG");
    expect_refused(
        "monitor_logins",
        vec![Value::Int(5), Value::Int(6)],
        "E_BAD_ARG",
    );
    expect_refused(
        "monitor_syslog",
        vec![s("error"), Value::Int(5), Value::Int(6)],
        "E_BAD_ARG",
    );
    expect_refused("monitor_ethtool", vec![s("eth0"), s("x")], "E_BAD_ARG");
}

#[test]
fn a_wrong_type_is_refused_rather_than_defaulted() {
    // A non-String filter used to become "all".
    expect_refused("monitor_sockets", vec![Value::Int(5)], "E_BAD_ARG");
    expect_refused("monitor_sockets", vec![Value::Bool(true)], "E_BAD_ARG");
    // An unknown filter used to be checked; a misspelling must still refuse.
    expect_refused("monitor_sockets", vec![s("tpc")], "E_BAD_ARG");
    expect_refused("monitor_ip_addr", vec![Value::Int(3)], "E_BAD_ARG");
    expect_refused("monitor_ip_link", vec![Value::Array(vec![])], "E_BAD_ARG");
    // A non-Int count used to become 20.
    expect_refused("monitor_logins", vec![s("5")], "E_BAD_ARG");
    expect_refused("monitor_logins", vec![Value::Float(5.0)], "E_BAD_ARG");
    expect_refused("monitor_logins", vec![Value::Int(0)], "E_BAD_ARG");
    expect_refused("monitor_logins", vec![Value::Int(1001)], "E_BAD_ARG");
    expect_refused("monitor_syslog", vec![Value::Int(5)], "E_BAD_ARG");
    expect_refused("monitor_syslog", Vec::new(), "E_BAD_ARG");
    expect_refused("monitor_syslog", vec![s("error"), s("5")], "E_BAD_ARG");
    expect_refused(
        "monitor_syslog",
        vec![s("error"), Value::Int(0)],
        "E_BAD_ARG",
    );
    expect_refused(
        "monitor_syslog",
        vec![s("error"), Value::Int(1001)],
        "E_BAD_ARG",
    );
    expect_refused("monitor_ethtool", vec![Value::Int(0)], "E_BAD_ARG");
}

#[test]
fn option_like_and_malformed_text_is_refused_before_any_argv_is_built() {
    expect_refused("monitor_sockets", vec![s("-tcp")], "E_BAD_ARG");
    expect_refused("monitor_ip_addr", vec![s("-h")], "E_BAD_ARG");
    expect_refused("monitor_ip_addr", vec![s("--help")], "E_BAD_ARG");
    expect_refused("monitor_ip_addr", vec![s("")], "E_BAD_ARG");
    expect_refused("monitor_ip_link", vec![s("-o")], "E_BAD_ARG");
    expect_refused("monitor_ip_link", vec![s("lo\nx")], "E_BAD_ARG");
    expect_refused("monitor_syslog", vec![s("-e")], "E_BAD_ARG");
    expect_refused("monitor_syslog", vec![s("")], "E_BAD_ARG");
    expect_refused("monitor_syslog", vec![s("a\nb")], "E_BAD_ARG");
    expect_refused("monitor_syslog", vec![s(&"a".repeat(257))], "E_BAD_ARG");
    expect_refused("monitor_ethtool", vec![s("-h")], "E_BAD_ARG");
    expect_refused("monitor_ethtool", vec![s("eth0; id")], "E_BAD_ARG");
    expect_refused("monitor_ethtool", vec![s("../eth0")], "E_BAD_ARG");
}

#[cfg(not(windows))]
#[test]
fn unix_interface_names_follow_the_interface_rule() {
    // ip and ifconfig take the name as an argument, so it must be a plain name.
    expect_refused("monitor_ip_addr", vec![s("eth 0")], "E_BAD_ARG");
    expect_refused(
        "monitor_ip_link",
        vec![s("this-name-is-too-long-for-ifnamsiz")],
        "E_BAD_ARG",
    );
}

// ---------------------------------------------------------------------------
// Real calls: read-only queries. Each answers its declared type or a coded
// tool outcome.
// ---------------------------------------------------------------------------

#[test]
fn nethogs_answers_an_array_or_a_tool_outcome() {
    if let Some(v) = answer_or_tool_outcome("monitor_nethogs", call("monitor_nethogs", Vec::new()))
    {
        assert!(
            matches!(v, Value::Array(_)),
            "monitor_nethogs returned {v:?}, not an Array"
        );
    }
}

#[test]
fn iftop_answers_a_record_or_a_tool_outcome() {
    if let Some(v) = answer_or_tool_outcome("monitor_iftop", call("monitor_iftop", Vec::new())) {
        assert!(
            matches!(v, Value::Record(_)),
            "monitor_iftop returned {v:?}, not a Record"
        );
    }
}

#[test]
fn sockets_answer_records_whose_filter_holds_on_every_platform() {
    for filter in ["all", "tcp", "udp", "listening"] {
        let answer =
            answer_or_tool_outcome("monitor_sockets", call("monitor_sockets", vec![s(filter)]));
        let Some(Value::Array(records)) = answer else {
            continue;
        };
        for r in &records {
            let protocol = str_field(r, "protocol")
                .unwrap_or_else(|| panic!("filter {filter}: a record has no protocol: {r:?}"));
            assert!(
                protocol == "tcp" || protocol == "udp",
                "filter {filter}: protocol {protocol}"
            );
            match filter {
                "tcp" => assert_eq!(protocol, "tcp", "filter tcp kept {r:?}"),
                "udp" => assert_eq!(protocol, "udp", "filter udp kept {r:?}"),
                _ => {}
            }
            // Only Linux's state names are checked exactly: ss prints LISTEN and
            // UNCONN, the same names the filter documents.
            if filter == "listening" && cfg!(target_os = "linux") {
                let state = str_field(r, "state").unwrap_or_default();
                assert!(
                    state == "LISTEN" || state == "UNCONN",
                    "filter listening kept state {state}: {r:?}"
                );
            }
        }
    }
}

#[test]
fn ip_addr_and_link_answer_arrays_and_an_unknown_interface_is_an_error() {
    for name in ["monitor_ip_addr", "monitor_ip_link"] {
        if let Some(v) = answer_or_tool_outcome(name, call(name, Vec::new())) {
            assert!(
                matches!(v, Value::Array(_)),
                "{name} returned {v:?}, not an Array"
            );
        }
    }
    // An interface that does not exist is an error on every platform, never [].
    // The name is a valid one (Linux allows at most 15 bytes), so the refusal
    // comes from the lookup and not from the name check.
    match call("monitor_ip_addr", vec![s("aenoiface0")]) {
        Ok(v) => panic!("an unknown interface answered {v:?}, not an error"),
        Err(e) => {
            let code = code_of(&e);
            assert!(
                code == "E_NOT_FOUND" || code == "E_TOOL_FAILED",
                "unknown interface gave {code}: {e}"
            );
        }
    }
}

#[test]
fn ip_route_answers_an_array_or_a_tool_outcome() {
    if let Some(v) =
        answer_or_tool_outcome("monitor_ip_route", call("monitor_ip_route", Vec::new()))
    {
        assert!(
            matches!(v, Value::Array(_)),
            "monitor_ip_route returned {v:?}, not an Array"
        );
    }
}

#[test]
fn ethtool_answers_a_record_or_a_tool_outcome() {
    if let Some(v) =
        answer_or_tool_outcome("monitor_ethtool", call("monitor_ethtool", vec![s("lo")]))
    {
        assert!(
            matches!(v, Value::Record(_)),
            "monitor_ethtool returned {v:?}, not a Record"
        );
    }
}

#[test]
fn users_and_logins_answer_arrays_or_a_tool_outcome() {
    if let Some(v) = answer_or_tool_outcome("monitor_users", call("monitor_users", Vec::new())) {
        assert!(
            matches!(v, Value::Array(_)),
            "monitor_users returned {v:?}, not an Array"
        );
    }
    if let Some(v) = answer_or_tool_outcome(
        "monitor_logins",
        call("monitor_logins", vec![Value::Int(5)]),
    ) {
        match v {
            Value::Array(rows) => assert!(
                rows.len() <= 5,
                "monitor_logins(5) gave {} rows",
                rows.len()
            ),
            other => panic!("monitor_logins returned {other:?}, not an Array"),
        }
    }
}

#[test]
fn syslog_answers_an_array_no_longer_than_its_count_or_a_tool_outcome() {
    if let Some(v) = answer_or_tool_outcome(
        "monitor_syslog",
        call(
            "monitor_syslog",
            vec![s("ironshell-no-such-keyword"), Value::Int(5)],
        ),
    ) {
        match v {
            Value::Array(rows) => assert!(
                rows.len() <= 5,
                "monitor_syslog(_, 5) gave {} rows",
                rows.len()
            ),
            other => panic!("monitor_syslog returned {other:?}, not an Array"),
        }
    }
}
