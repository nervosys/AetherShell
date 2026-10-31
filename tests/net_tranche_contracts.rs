//! Contracts for the net_* readers, probes and lookups.
//!
//! Network rules for this file: no test contacts any host other than
//! localhost, 127.0.0.1 or ::1. Refusals for public-looking names are answered
//! before any socket or process exists, so they reach nothing. The port scan
//! probes only a listener this test opened on 127.0.0.1 (and one it closed).
//! Whois, public DNS and traceroute to a remote host are never called here;
//! their tests are refusals only.
//!
//! Ordinary tests run on every platform. Where a host tool may be absent, the
//! test accepts the coded absence (E_TOOL_MISSING) and nothing else.

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::net::TcpListener;

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// The text of a String value, without Display's quoting or colour.
fn text_of(v: &Value) -> String {
    match v {
        Value::Str(text) => text.clone(),
        other => panic!("expected a String, got {other:?}"),
    }
}

fn call_builtin(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
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

fn refused_with(name: &str, args: Vec<Value>) -> String {
    match call_builtin(name, args) {
        Ok(v) => panic!("{name} should refuse, but returned {v:?}"),
        Err(e) => code_of(&e),
    }
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(map) => map
            .get(key)
            .unwrap_or_else(|| panic!("no field {key} in {v:?}")),
        other => panic!("expected a Record, got {other:?}"),
    }
}

/// A coded absence of a host tool, which the machine-dependent tests accept
/// when a tool is not installed or cannot answer here.
fn is_coded_absence(code: &str) -> bool {
    matches!(
        code,
        "E_TOOL_MISSING" | "E_TOOL_FAILED" | "E_NOT_FOUND" | "E_UNIMPLEMENTED"
    )
}

// ── refusals: every builtin refuses bad input before any effect ──────────

#[test]
fn an_option_like_host_never_reaches_a_tool_as_a_flag() {
    // "-f", "--help", "+x" and "-" would be read as options by ping, dig,
    // traceroute or whois. Each builtin refuses them with a coded E_BAD_ARG.
    let hostile = ["-f", "--help", "-", "-n", "+x"];
    for name in [
        "net_ping",
        "net_latency",
        "net_traceroute",
        "net_dns_lookup",
        "net_whois",
        "ping",
    ] {
        for host in hostile {
            assert_eq!(
                refused_with(name, vec![s(host)]),
                "E_BAD_ARG",
                "{name}({host:?}) must be refused as a bad argument"
            );
        }
    }
}

#[test]
fn hostnames_outside_hostname_syntax_are_refused_coded() {
    for host in ["bad host", "a;b", "a|b", "", "x\u{0}y", "$(id)"] {
        for name in ["net_ping", "net_dns_lookup", "net_whois"] {
            assert_eq!(
                refused_with(name, vec![s(host)]),
                "E_BAD_ARG",
                "{name}({host:?}) must be refused as a bad argument"
            );
        }
    }
}

#[test]
fn a_non_string_host_is_refused_coded() {
    assert_eq!(refused_with("net_ping", vec![Value::Int(1)]), "E_BAD_ARG");
    assert_eq!(
        refused_with("net_dns_lookup", vec![Value::Bool(true)]),
        "E_BAD_ARG"
    );
}

#[test]
fn ping_count_outside_1_to_100_is_refused_not_clamped() {
    for count in [0, -1, 101, 1_000_000] {
        assert_eq!(
            refused_with("net_ping", vec![s("127.0.0.1"), Value::Int(count)]),
            "E_BAD_ARG",
            "net_ping count {count} must be refused"
        );
    }
    // A count that is not an Int was silently ignored before; it is refused now.
    assert_eq!(
        refused_with("net_ping", vec![s("127.0.0.1"), s("3")]),
        "E_BAD_ARG"
    );
}

#[test]
fn traceroute_hops_outside_1_to_30_are_refused() {
    for hops in [0, 31, 255] {
        assert_eq!(
            refused_with("net_traceroute", vec![s("127.0.0.1"), Value::Int(hops)]),
            "E_BAD_ARG"
        );
    }
}

#[test]
fn reverse_lookup_takes_an_address_literal_only() {
    // A name used to be passed to dig -x as if it were an address.
    assert_eq!(
        refused_with("net_dns_reverse", vec![s("example.com")]),
        "E_BAD_ARG"
    );
    assert_eq!(refused_with("net_dns_reverse", vec![s("-f")]), "E_BAD_ARG");
}

#[test]
fn surplus_arguments_are_refused_not_ignored() {
    // These readers take no parameters; each extra argument was silently dropped.
    for name in [
        "net_interfaces",
        "net_ip",
        "net_connections",
        "net_ports",
        "net_arp",
        "net_route",
        "net_stats",
        "net_bandwidth",
    ] {
        assert_eq!(
            refused_with(name, vec![s("extra")]),
            "E_BAD_ARG",
            "{name} must refuse an argument it does not take"
        );
    }
}

#[test]
fn pipeline_input_is_refused_by_every_net_builtin() {
    let piped = || Some(Value::Int(1));
    let mut env = Env::new();
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("net_interfaces", vec![]),
        ("net_ip", vec![]),
        ("net_connections", vec![]),
        ("net_ports", vec![]),
        ("net_arp", vec![]),
        ("net_route", vec![]),
        ("net_stats", vec![]),
        ("net_bandwidth", vec![]),
        ("net_ping", vec![s("127.0.0.1")]),
        ("net_latency", vec![s("127.0.0.1")]),
        ("net_traceroute", vec![s("127.0.0.1")]),
        ("net_dns_lookup", vec![s("localhost")]),
        ("net_dns_reverse", vec![s("127.0.0.1")]),
        ("net_whois", vec![s("localhost")]),
        (
            "net_scan",
            vec![s("localhost"), Value::Int(1), Value::Int(2)],
        ),
    ];
    for (name, args) in cases {
        let result = call_with_input(name, args, piped(), &mut env);
        assert!(
            result.is_err(),
            "{name} must refuse pipeline input, but returned {result:?}"
        );
    }
}

#[test]
fn the_alias_spellings_refuse_the_same_way() {
    assert_eq!(refused_with("ping", vec![s("-f")]), "E_BAD_ARG");
    assert_eq!(
        refused_with("ping", vec![s("127.0.0.1"), Value::Int(0)]),
        "E_BAD_ARG"
    );
}

#[test]
fn every_refusal_across_the_tranche_carries_a_code() {
    let refusals: Vec<(&str, Vec<Value>)> = vec![
        ("net_ping", vec![s("-f")]),
        ("net_latency", vec![s("-f")]),
        ("net_traceroute", vec![s("-f")]),
        ("net_dns_lookup", vec![s("-f")]),
        ("net_dns_reverse", vec![s("not-an-address")]),
        ("net_whois", vec![s("-f")]),
        (
            "net_scan",
            vec![s("example.com"), Value::Int(1), Value::Int(2)],
        ),
        (
            "net_scan",
            vec![s("localhost"), Value::Int(1), Value::Int(2000)],
        ),
        ("net_bandwidth", vec![Value::Int(1)]),
    ];
    for (name, args) in refusals {
        let err = call_builtin(name, args).expect_err(name);
        let code = code_of(&err);
        assert!(
            code.starts_with("E_"),
            "{name} refused without a code: {err}"
        );
    }
}

// ── scan: loopback only, bounded, and truthful about a port we opened ────

#[test]
fn scan_refuses_every_host_that_is_not_this_machine() {
    for host in [
        "example.com",
        "8.8.8.8",
        "10.0.0.1",
        "192.168.1.1",
        "::ffff:8.8.8.8",
    ] {
        assert_eq!(
            refused_with("net_scan", vec![s(host), Value::Int(1), Value::Int(2)]),
            "E_POLICY_DENY",
            "net_scan({host}) must be refused by policy"
        );
    }
}

#[test]
fn scan_bounds_its_range_and_timeout() {
    // Reversed, oversized and out-of-range requests are refused before a probe.
    assert_eq!(
        refused_with(
            "net_scan",
            vec![s("localhost"), Value::Int(10), Value::Int(5)]
        ),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with(
            "net_scan",
            vec![s("localhost"), Value::Int(1), Value::Int(1025)]
        ),
        "E_BAD_ARG",
        "1025 ports is over the per-call bound"
    );
    assert_eq!(
        refused_with(
            "net_scan",
            vec![s("localhost"), Value::Int(0), Value::Int(5)]
        ),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with(
            "net_scan",
            vec![s("localhost"), Value::Int(1), Value::Int(2), Value::Int(0)]
        ),
        "E_BAD_ARG",
        "a zero timeout is refused"
    );
    assert_eq!(
        refused_with(
            "net_scan",
            vec![
                s("localhost"),
                Value::Int(1),
                Value::Int(2),
                Value::Int(10001)
            ]
        ),
        "E_BAD_ARG",
        "a timeout over 10000 ms is refused"
    );
}

#[test]
fn scan_reports_an_opened_port_open_and_a_closed_one_closed() {
    // A listener this test owns is open. Its port, once released, is closed.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let open_port = listener.local_addr().expect("local addr").port();
    let closed_port = {
        let probe = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        probe.local_addr().expect("local addr").port()
        // dropped here: nothing listens on closed_port from now on
    };

    let state_of = |port: u16, host: &str| -> String {
        let rows = call_builtin(
            "net_scan",
            vec![
                s(host),
                Value::Int(port as i64),
                Value::Int(port as i64),
                // A refusal on Windows loopback can take about 2 s to arrive.
                Value::Int(5000),
            ],
        )
        .unwrap_or_else(|e| panic!("net_scan({host}, {port}) failed: {e}"));
        match rows {
            Value::Array(rows) => {
                assert_eq!(rows.len(), 1, "one port in, one row out");
                text_of(field(&rows[0], "state"))
            }
            other => panic!("net_scan answered {other:?}, not an Array"),
        }
    };

    let open_seen = state_of(open_port, "127.0.0.1");
    assert!(
        open_seen == "open",
        "scan failed: port {open_port} we listen on reported {open_seen}"
    );
    let localhost_seen = state_of(open_port, "localhost");
    assert!(
        localhost_seen == "open",
        "scan failed: localhost:{open_port} reported {localhost_seen}"
    );
    let closed_seen = state_of(closed_port, "127.0.0.1");
    assert!(
        closed_seen == "closed",
        "scan failed: port {closed_port}, which nothing listens on, reported {closed_seen}"
    );
    drop(listener);
}

#[test]
fn scan_covers_a_range_and_returns_one_row_per_port() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let open_port = listener.local_addr().expect("local addr").port();
    let first = open_port.saturating_sub(1).max(1);
    let last = open_port.saturating_add(1);
    let rows = call_builtin(
        "net_scan",
        vec![
            s("127.0.0.1"),
            Value::Int(first as i64),
            Value::Int(last as i64),
        ],
    )
    .expect("a three-port loopback scan");
    let Value::Array(rows) = rows else {
        panic!("net_scan must answer an Array")
    };
    assert_eq!(rows.len(), (last - first + 1) as usize);
    let state_at = |port: u16| -> String {
        rows.iter()
            .find(|r| matches!(field(r, "port"), Value::Int(p) if *p == port as i64))
            .map(|r| text_of(field(r, "state")))
            .unwrap_or_else(|| panic!("no row for port {port}"))
    };
    assert_eq!(state_at(open_port), "open");
    drop(listener);
}

#[test]
fn scan_reaches_ipv6_loopback_when_this_host_has_it() {
    // Skipped, not failed, on a host without IPv6 loopback.
    let Ok(listener) = TcpListener::bind("[::1]:0") else {
        return;
    };
    let port = listener.local_addr().expect("local addr").port();
    let rows = call_builtin(
        "net_scan",
        vec![s("::1"), Value::Int(port as i64), Value::Int(port as i64)],
    )
    .expect("an IPv6 loopback scan");
    let Value::Array(rows) = rows else {
        panic!("net_scan must answer an Array")
    };
    assert_eq!(text_of(field(&rows[0], "state")), "open");
    drop(listener);
}

// ── host tools on loopback: answer, or the coded absence ─────────────────

#[test]
fn ping_of_loopback_answers_with_a_record_or_a_coded_absence() {
    match call_builtin("net_ping", vec![s("127.0.0.1"), Value::Int(1)]) {
        Ok(rec) => {
            assert_eq!(text_of(field(&rec, "host")), "127.0.0.1");
            assert!(matches!(field(&rec, "raw"), Value::Str(_)));
        }
        Err(e) => {
            let code = code_of(&e);
            assert!(
                is_coded_absence(&code),
                "net_ping(127.0.0.1) failed without a coded absence: {e}"
            );
        }
    }
}

#[test]
fn latency_of_loopback_is_a_float_or_a_coded_absence() {
    match call_builtin("net_latency", vec![s("127.0.0.1")]) {
        Ok(Value::Float(ms)) => assert!(ms >= 0.0 && ms.is_finite(), "latency {ms}"),
        Ok(other) => panic!("net_latency must answer a Float, got {other:?}"),
        Err(e) => assert!(
            is_coded_absence(&code_of(&e)),
            "net_latency(127.0.0.1) failed without a coded absence: {e}"
        ),
    }
}

#[test]
fn traceroute_of_loopback_is_an_array_or_a_coded_absence() {
    match call_builtin("net_traceroute", vec![s("127.0.0.1"), Value::Int(1)]) {
        Ok(Value::Array(hops)) => assert!(hops.len() <= 1, "one hop at most"),
        Ok(other) => panic!("net_traceroute must answer an Array, got {other:?}"),
        Err(e) => assert!(
            is_coded_absence(&code_of(&e)),
            "net_traceroute(127.0.0.1) failed without a coded absence: {e}"
        ),
    }
}

#[test]
#[cfg(windows)]
fn dns_lookup_of_localhost_resolves_locally_on_windows() {
    // Only Windows is asserted: its resolver answers localhost from the hosts
    // file. Other resolvers may query a DNS server, which this test must not do.
    let answers = call_builtin("net_dns_lookup", vec![s("localhost")])
        .expect("localhost resolves locally on Windows");
    let Value::Array(answers) = answers else {
        panic!("net_dns_lookup must answer an Array")
    };
    assert!(
        answers.iter().any(|a| a.to_string().contains("127.0.0.1")),
        "localhost should resolve to 127.0.0.1, got {answers:?}"
    );
}

// ── machine-dependent readers answer in their type or fail coded ─────────

#[test]
fn machine_readers_answer_in_their_declared_type_or_fail_coded() {
    let answers: Vec<(&str, Vec<Value>, &str)> = vec![
        ("net_interfaces", vec![], "Array"),
        ("net_connections", vec![], "Array"),
        ("net_ports", vec![], "Array"),
        ("net_arp", vec![], "Array"),
        ("net_route", vec![], "Array"),
        ("net_stats", vec![], "Array"),
        ("net_bandwidth", vec![], "Array"),
    ];
    for (name, args, want) in answers {
        match call_builtin(name, args) {
            Ok(v) => {
                let got = match &v {
                    Value::Array(_) => "Array",
                    Value::Str(_) => "String",
                    other => panic!("{name} answered {other:?}"),
                };
                assert_eq!(got, want, "{name} answered the wrong type");
            }
            Err(e) => assert!(
                is_coded_absence(&code_of(&e)),
                "{name} failed without a coded absence: {e}"
            ),
        }
    }
}

#[test]
fn every_listed_port_is_a_listening_socket() {
    // The listener is opened before the read, so on a host where the socket
    // table can be read it must appear as a LISTEN row on its port.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    match call_builtin("net_ports", vec![]) {
        Ok(Value::Array(rows)) => {
            for row in &rows {
                let state = text_of(field(row, "state"));
                assert!(
                    state == "LISTEN" || state == "UNCONN",
                    "net_ports listed a {state} socket"
                );
            }
            let ours = rows
                .iter()
                .any(|r| text_of(field(r, "local")).ends_with(&format!(":{port}")));
            assert!(ours, "the listener on {port} is missing from net_ports");
        }
        Ok(other) => panic!("net_ports answered {other:?}, not an Array"),
        Err(e) => assert!(
            is_coded_absence(&code_of(&e)),
            "net_ports failed without a coded absence: {e}"
        ),
    }
    drop(listener);
}

#[test]
fn ip_is_a_string_or_array_or_a_coded_absence() {
    match call_builtin("net_ip", vec![]) {
        Ok(Value::Str(_)) | Ok(Value::Array(_)) => {}
        Ok(other) => panic!("net_ip answered {other:?}"),
        Err(e) => assert!(
            is_coded_absence(&code_of(&e)),
            "net_ip failed without a coded absence: {e}"
        ),
    }
}
