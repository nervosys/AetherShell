//! Pure parsers for the `monitor_*` readers.
//!
//! Each takes the text one finished tool call printed and returns its records.
//! A line it cannot read is a coded `E_TOOL_FAILED` that quotes the line: a
//! reader that skipped it would answer with a partial list as if it were whole.
//! Nothing here runs a program or reads a file, so the unit tests at the bottom
//! run on every platform against fixed sample text.

use super::json_to_value;
use crate::safety;
use crate::value::Value;
use anyhow::Result;

fn record(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

/// A line that could not be read, as a coded failure that quotes it.
fn unreadable(builtin: &str, tool: &str, what: &str, line: &str) -> anyhow::Error {
    let shown: String = line.chars().take(120).collect();
    safety::tool_failed(builtin, tool, None, &format!("{what}: {shown:?}"))
}

fn int(builtin: &str, tool: &str, what: &str, word: &str, line: &str) -> Result<i64> {
    word.parse::<i64>()
        .map_err(|_| unreadable(builtin, tool, what, line))
}

/// Whether a socket line is kept by the filter. The filter is checked here too,
/// so a parser given an unknown one refuses instead of keeping everything.
fn keep_socket(builtin: &str, filter: &str, tcp: bool, listening: bool) -> Result<bool> {
    match filter {
        "all" => Ok(true),
        "tcp" => Ok(tcp),
        "udp" => Ok(!tcp),
        "listening" => Ok(listening),
        other => Err(safety::bad_arg(
            builtin,
            "filter all, tcp, udp or listening",
            other,
        )),
    }
}

/// The records a finished JSON query printed. Empty output is no records, and a
/// lone object is one record: ConvertTo-Json writes a bare object for one item.
pub(crate) fn json_array(builtin: &str, tool: &str, out: &str) -> Result<Value> {
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(serde_json::Value::Array(items)) => {
            Ok(Value::Array(items.into_iter().map(json_to_value).collect()))
        }
        Ok(obj @ serde_json::Value::Object(_)) => Ok(Value::Array(vec![json_to_value(obj)])),
        Ok(_) => Err(safety::tool_failed(
            builtin,
            tool,
            None,
            "printed JSON that is neither an array nor an object",
        )),
        Err(e) => Err(safety::tool_failed(
            builtin,
            tool,
            None,
            &format!("output that is not JSON: {e}"),
        )),
    }
}

/// `nethogs -t` trace output: one `program<TAB>sent_kbps<TAB>received_kbps` line
/// per program. Lines with fewer than three tab-separated fields (its
/// "Refreshing:" headers) are skipped; a rate that is not a finite number is not.
pub(crate) fn nethogs_trace(out: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_nethogs";
    let rate = |word: &str, line: &str| -> Result<Value> {
        match word.trim().parse::<f64>() {
            Ok(f) if f.is_finite() && f >= 0.0 => Ok(Value::Float(f)),
            _ => Err(unreadable(
                B,
                "nethogs",
                "a rate that is not a finite number",
                line,
            )),
        }
    };
    let mut records = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let program = parts[0].trim();
        if program.is_empty() {
            return Err(unreadable(B, "nethogs", "a line with no program", line));
        }
        records.push(record(vec![
            ("program", text(program)),
            ("sent_kbps", rate(parts[1], line)?),
            ("received_kbps", rate(parts[2], line)?),
        ]));
    }
    Ok(records)
}

/// `ss -tuanH` output: no header, numeric, TCP and UDP sockets. Columns are
/// `Netid State Recv-Q Send-Q Local Peer`. Every line is read before the filter
/// is applied, so a malformed line fails the call whatever the filter.
/// "listening" is LISTEN or UNCONN.
pub(crate) fn socket_records_ss(out: &str, filter: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_sockets";
    let mut records = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        if f.len() < 6 {
            return Err(unreadable(
                B,
                "ss",
                "a socket line with fewer than six columns",
                line,
            ));
        }
        let tcp = match f[0] {
            "tcp" => true,
            "udp" => false,
            _ => return Err(unreadable(B, "ss", "a netid other than tcp or udp", line)),
        };
        let recv_q = int(B, "ss", "a queue length that is not an integer", f[2], line)?;
        let send_q = int(B, "ss", "a queue length that is not an integer", f[3], line)?;
        let listening = f[1] == "LISTEN" || f[1] == "UNCONN";
        if !keep_socket(B, filter, tcp, listening)? {
            continue;
        }
        records.push(record(vec![
            ("protocol", text(f[0])),
            ("state", text(f[1])),
            ("recv_q", Value::Int(recv_q)),
            ("send_q", Value::Int(send_q)),
            ("local_address", text(f[4])),
            ("peer_address", text(f[5])),
        ]));
    }
    Ok(records)
}

/// macOS `netstat -an` output. Only `tcp*` and `udp*` lines are sockets; the
/// header and the `unix` lines are other protocols and are skipped. TCP rows
/// have a state column, UDP rows do not, and a UDP socket is listening when its
/// peer is `*.*`.
pub(crate) fn socket_records_bsd(out: &str, filter: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_sockets";
    let mut records = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let Some(first) = f.first() else {
            continue;
        };
        let tcp = first.starts_with("tcp");
        if !tcp && !first.starts_with("udp") {
            continue;
        }
        let need = if tcp { 6 } else { 5 };
        if f.len() < need {
            return Err(unreadable(
                B,
                "netstat",
                "a socket line with too few columns",
                line,
            ));
        }
        let recv_q = int(
            B,
            "netstat",
            "a queue length that is not an integer",
            f[1],
            line,
        )?;
        let send_q = int(
            B,
            "netstat",
            "a queue length that is not an integer",
            f[2],
            line,
        )?;
        let peer = f[4];
        let state = if tcp { f[5] } else { "" };
        let listening = if tcp {
            state == "LISTEN"
        } else {
            peer == "*.*"
        };
        if !keep_socket(B, filter, tcp, listening)? {
            continue;
        }
        let mut fields = vec![
            ("protocol", text(if tcp { "tcp" } else { "udp" })),
            ("recv_q", Value::Int(recv_q)),
            ("send_q", Value::Int(send_q)),
            ("local_address", text(f[3])),
            ("peer_address", text(peer)),
        ];
        if tcp {
            fields.push(("state", text(state)));
        }
        records.push(record(fields));
    }
    Ok(records)
}

/// Windows `netstat -an` output. `TCP local peer STATE` and `UDP local peer`
/// lines are sockets; the header and blank lines are skipped. "listening" is
/// LISTENING for TCP, and an unconnected peer (`*:*`) for UDP.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn socket_records_windows(out: &str, filter: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_sockets";
    let mut records = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let Some(first) = f.first() else {
            continue;
        };
        let tcp = first.eq_ignore_ascii_case("TCP");
        if !tcp && !first.eq_ignore_ascii_case("UDP") {
            continue;
        }
        let need = if tcp { 4 } else { 3 };
        if f.len() < need {
            return Err(unreadable(
                B,
                "netstat",
                "a socket line with too few columns",
                line,
            ));
        }
        let peer = f[2];
        let state = if tcp { f[3] } else { "" };
        let listening = if tcp {
            state == "LISTENING"
        } else {
            peer == "*:*"
        };
        if !keep_socket(B, filter, tcp, listening)? {
            continue;
        }
        let mut fields = vec![
            ("protocol", text(if tcp { "tcp" } else { "udp" })),
            ("local_address", text(f[1])),
            ("peer_address", text(peer)),
        ];
        if tcp {
            fields.push(("state", text(state)));
        }
        records.push(record(fields));
    }
    Ok(records)
}

/// `netstat -s` on Windows: the four counters this reader reports, by their
/// English labels. A counter whose label is not in the output is absent. A
/// counter that is present but not an integer is a coded failure, never a 0.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn netstat_counters(out: &str) -> Result<Vec<(&'static str, Value)>> {
    const B: &str = "monitor_iftop";
    const LABELS: &[(&str, &str)] = &[
        ("segments sent", "tcp_segments_sent"),
        ("segments received", "tcp_segments_received"),
        ("datagrams sent", "udp_datagrams_sent"),
        ("datagrams received", "udp_datagrams_received"),
    ];
    let mut found = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        let lower = t.to_lowercase();
        for (label, key) in LABELS {
            if !lower.contains(label) {
                continue;
            }
            let Some((_, value)) = t.rsplit_once('=') else {
                return Err(unreadable(
                    B,
                    "netstat",
                    "a counter line with no '=' value",
                    t,
                ));
            };
            let n = value
                .trim()
                .replace(',', "")
                .parse::<i64>()
                .map_err(|_| unreadable(B, "netstat", "a counter that is not an integer", t))?;
            found.push((*key, Value::Int(n)));
        }
    }
    Ok(found)
}

/// `who` output on Unix: `user terminal login-time...`.
pub(crate) fn who_records(out: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_users";
    let mut records = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        if f.len() < 3 {
            return Err(unreadable(
                B,
                "who",
                "a session line with fewer than three columns",
                line,
            ));
        }
        records.push(record(vec![
            ("username", text(f[0])),
            ("terminal", text(f[1])),
            ("login_time", text(&f[2..].join(" "))),
        ]));
    }
    Ok(records)
}

/// `query user` output on Windows. The first line is the column header.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn query_user_records(out: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_users";
    let mut records = Vec::new();
    for line in out.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        if f.len() < 4 {
            return Err(unreadable(
                B,
                "query",
                "a session line with fewer than four columns",
                line,
            ));
        }
        let mut fields = vec![
            ("username", text(f[0].trim_start_matches('>'))),
            ("session_name", text(f[1])),
            ("id", text(f[2])),
            ("state", text(f[3])),
        ];
        if f.len() >= 5 {
            fields.push(("idle_time", text(f[4])));
        }
        if f.len() >= 6 {
            fields.push(("logon_time", text(&f[5..].join(" "))));
        }
        records.push(record(fields));
    }
    Ok(records)
}

/// `last` output on Unix. The `wtmp begins` and `btmp begins` trailers are not
/// logins. A host column is present when the line has more than five fields.
pub(crate) fn last_records(out: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_logins";
    let mut records = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("wtmp begins") || t.starts_with("btmp begins") {
            continue;
        }
        let f: Vec<&str> = t.split_whitespace().collect();
        if f.len() < 4 {
            return Err(unreadable(
                B,
                "last",
                "a login line with fewer than four columns",
                line,
            ));
        }
        let mut fields = vec![("user", text(f[0])), ("terminal", text(f[1]))];
        // A host is present unless the third column is already the weekday that
        // starts the login time.
        let weekday = matches!(f[2], "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun");
        let start = if f.len() > 5 && !weekday {
            fields.push(("host", text(f[2])));
            3
        } else {
            2
        };
        fields.push(("login_time", text(&f[start..].join(" "))));
        records.push(record(fields));
    }
    Ok(records)
}

/// The newest `count` lines of a grep over a syslog file, each as a record with
/// `MESSAGE` and the file it came from.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn grep_records(out: &str, count: usize, source: &str) -> Vec<Value> {
    let lines: Vec<&str> = out.lines().collect();
    let start = lines.len().saturating_sub(count);
    lines[start..]
        .iter()
        .map(|l| record(vec![("MESSAGE", text(l)), ("source", text(source))]))
        .collect()
}

/// `journalctl -o json` output: one JSON object per line. The `-- ...` notices
/// journalctl prints itself (for example `-- No entries --`) are skipped; any
/// other line that is not a JSON object is a coded failure.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn journal_records(out: &str) -> Result<Vec<Value>> {
    const B: &str = "monitor_syslog";
    let mut records = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("-- ") {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(t) {
            Ok(v @ serde_json::Value::Object(_)) => records.push(json_to_value(v)),
            _ => {
                return Err(unreadable(
                    B,
                    "journalctl",
                    "an entry that is not a JSON object",
                    line,
                ))
            }
        }
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
        match v {
            Value::Record(m) => m.get(key),
            _ => None,
        }
    }

    fn str_of(v: &Value, key: &str) -> String {
        match field(v, key) {
            Some(Value::Str(s)) => s.clone(),
            other => panic!("{key} is not a String: {other:?}"),
        }
    }

    fn int_of(v: &Value, key: &str) -> i64 {
        match field(v, key) {
            Some(Value::Int(n)) => *n,
            other => panic!("{key} is not an Int: {other:?}"),
        }
    }

    fn rows(r: Result<Vec<Value>>) -> Vec<Value> {
        r.unwrap_or_else(|e| panic!("expected records, got: {e}"))
    }

    fn refused(r: Result<Vec<Value>>) -> String {
        match r {
            Ok(v) => panic!("expected a coded failure, got {} records", v.len()),
            Err(e) => e.to_string(),
        }
    }

    const SS: &str = "tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:*\n\
        tcp ESTAB 0 0 10.0.0.2:22 10.0.0.5:51234\n\
        udp UNCONN 0 0 0.0.0.0:53 0.0.0.0:*\n\
        udp ESTAB 0 0 10.0.0.2:5353 10.0.0.9:5353\n";

    #[test]
    fn ss_filters_mean_what_they_say() {
        assert_eq!(rows(socket_records_ss(SS, "all")).len(), 4);
        // Established connections are in "tcp" and "udp", not only listeners.
        let tcp = rows(socket_records_ss(SS, "tcp"));
        assert_eq!(tcp.len(), 2);
        assert!(tcp.iter().all(|r| str_of(r, "protocol") == "tcp"));
        assert!(tcp.iter().any(|r| str_of(r, "state") == "ESTAB"));
        let udp = rows(socket_records_ss(SS, "udp"));
        assert_eq!(udp.len(), 2);
        assert!(udp.iter().all(|r| str_of(r, "protocol") == "udp"));
        // "listening" is LISTEN or UNCONN, whichever protocol.
        let listening = rows(socket_records_ss(SS, "listening"));
        assert_eq!(listening.len(), 2);
        assert!(listening
            .iter()
            .all(|r| matches!(str_of(r, "state").as_str(), "LISTEN" | "UNCONN")));
        assert_eq!(int_of(&tcp[0], "recv_q"), 0);
        assert_eq!(int_of(&listening[0], "send_q"), 128);
    }

    #[test]
    fn ss_refuses_a_short_line_a_bad_queue_and_an_unknown_netid() {
        let short = refused(socket_records_ss("tcp LISTEN 0\n", "all"));
        assert!(short.contains("fewer than six columns"), "{short}");
        let bad_queue = refused(socket_records_ss(
            "tcp LISTEN x 0 0.0.0.0:22 0.0.0.0:*\n",
            "all",
        ));
        assert!(bad_queue.contains("not an integer"), "{bad_queue}");
        let netid = refused(socket_records_ss("sctp LISTEN 0 0 a:1 b:*\n", "all"));
        assert!(netid.contains("netid"), "{netid}");
        // A malformed line fails the call even when the filter would drop it.
        let dropped = refused(socket_records_ss(&format!("{SS}tcp LISTEN 0\n"), "udp"));
        assert!(dropped.contains("fewer than six columns"), "{dropped}");
    }

    #[test]
    fn an_unknown_socket_filter_is_refused_by_the_parser() {
        let msg = refused(socket_records_ss(SS, "tpc"));
        assert!(msg.contains("E_BAD_ARG"), "{msg}");
    }

    #[test]
    fn bsd_netstat_reads_tcp_state_and_udp_peer() {
        let out = "Active Internet connections (including servers)\n\
            Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)\n\
            tcp4       0      0  *.22                   *.*                    LISTEN\n\
            tcp4       0      0  10.0.0.2.22            10.0.0.5.51234         ESTABLISHED\n\
            udp4       0      0  *.5353                 *.*\n\
            unix  2      0      0x1234                 0x5678\n";
        assert_eq!(rows(socket_records_bsd(out, "all")).len(), 3);
        let listening = rows(socket_records_bsd(out, "listening"));
        assert_eq!(listening.len(), 2);
        assert_eq!(str_of(&listening[0], "state"), "LISTEN");
        assert_eq!(str_of(&listening[1], "protocol"), "udp");
        let udp = rows(socket_records_bsd(out, "udp"));
        assert_eq!(udp.len(), 1);
        assert!(field(&udp[0], "state").is_none());
        let bad = refused(socket_records_bsd("tcp4 0 x *.22 *.* LISTEN\n", "all"));
        assert!(bad.contains("not an integer"), "{bad}");
        let short = refused(socket_records_bsd("tcp4 0 0 *.22\n", "all"));
        assert!(short.contains("too few columns"), "{short}");
    }

    #[test]
    fn windows_netstat_reads_listeners_and_unconnected_udp() {
        let out = "\nActive Connections\n\n  Proto  Local Address          Foreign Address        State\n\
            \x20 TCP    0.0.0.0:135            0.0.0.0:0              LISTENING\n\
            \x20 TCP    10.0.0.2:443           10.0.0.5:51234         ESTABLISHED\n\
            \x20 UDP    0.0.0.0:5353           *:*\n";
        assert_eq!(rows(socket_records_windows(out, "all")).len(), 3);
        let listening = rows(socket_records_windows(out, "listening"));
        assert_eq!(listening.len(), 2);
        let tcp = rows(socket_records_windows(out, "tcp"));
        assert_eq!(tcp.len(), 2);
        assert_eq!(str_of(&tcp[0], "protocol"), "tcp");
        let udp = rows(socket_records_windows(out, "udp"));
        assert_eq!(udp.len(), 1);
        assert_eq!(str_of(&udp[0], "peer_address"), "*:*");
        let short = refused(socket_records_windows("  TCP 0.0.0.0:1\n", "all"));
        assert!(short.contains("too few columns"), "{short}");
    }

    #[test]
    fn nethogs_trace_reads_rates_and_refuses_a_bad_one() {
        let out = "Refreshing:\nsshd/1234/0\t1.5\t2.25\n";
        let recs = rows(nethogs_trace(out));
        assert_eq!(recs.len(), 1);
        assert_eq!(str_of(&recs[0], "program"), "sshd/1234/0");
        assert!(matches!(field(&recs[0], "received_kbps"), Some(Value::Float(f)) if *f == 2.25));
        // An unparseable rate is an error, never a 0.
        let bad = refused(nethogs_trace("sshd\tfast\t2.0\n"));
        assert!(bad.contains("not a finite number"), "{bad}");
        let nan = refused(nethogs_trace("sshd\t1.0\tNaN\n"));
        assert!(nan.contains("not a finite number"), "{nan}");
        let no_program = refused(nethogs_trace("\t1.0\t2.0\n"));
        assert!(no_program.contains("no program"), "{no_program}");
    }

    #[test]
    fn netstat_counters_read_numbers_with_separators() {
        let out = "TCP Statistics\n    Segments Sent                     = 1,234\n    Segments Received = 56\n\
            UDP Statistics\n    Datagrams Sent = 7\n";
        let found = netstat_counters(out).unwrap_or_else(|e| panic!("{e}"));
        let get = |k: &str| {
            found
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.clone())
        };
        assert!(matches!(get("tcp_segments_sent"), Some(Value::Int(1234))));
        assert!(matches!(get("tcp_segments_received"), Some(Value::Int(56))));
        assert!(matches!(get("udp_datagrams_sent"), Some(Value::Int(7))));
        assert!(
            get("udp_datagrams_received").is_none(),
            "absent label stays absent"
        );
    }

    #[test]
    fn netstat_counters_refuse_an_unparsed_value_instead_of_zero() {
        let bad = netstat_counters("    Segments Sent = 12x\n");
        let msg = match bad {
            Ok(v) => panic!("an unparsed counter answered {v:?}"),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains("not an integer"), "{msg}");
        let no_eq = netstat_counters("    Segments Sent 12\n");
        assert!(no_eq.is_err());
    }

    #[test]
    fn who_and_query_user_read_sessions_and_refuse_short_rows() {
        let who = rows(who_records("ada pts/0 2026-10-20 10:00 (10.0.0.5)\n\n"));
        assert_eq!(who.len(), 1);
        assert_eq!(str_of(&who[0], "terminal"), "pts/0");
        assert_eq!(str_of(&who[0], "login_time"), "2026-10-20 10:00 (10.0.0.5)");
        let who_bad = refused(who_records("lonely\n"));
        assert!(who_bad.contains("fewer than three"), "{who_bad}");

        let qu = "USERNAME              SESSIONNAME        ID  STATE   IDLE TIME  LOGON TIME\n\
            >ada                  console             1  Active      .      10/20/2026 10:00\n";
        let rows_qu = rows(query_user_records(qu));
        assert_eq!(rows_qu.len(), 1);
        assert_eq!(str_of(&rows_qu[0], "username"), "ada");
        assert_eq!(str_of(&rows_qu[0], "logon_time"), "10/20/2026 10:00");
        let qu_bad = refused(query_user_records("header\nx\n"));
        assert!(qu_bad.contains("fewer than four"), "{qu_bad}");
    }

    #[test]
    fn last_reads_logins_skips_trailers_and_refuses_a_short_line() {
        let out = "ada    pts/0        10.0.0.5     Mon Oct 20 10:00 - 10:05  (00:05)\n\
            ada    tty1                              Mon Oct 20 09:00   still logged in\n\
            \n\
            wtmp begins Mon Oct  1 00:00:00 2026\n";
        let recs = rows(last_records(out));
        assert_eq!(recs.len(), 2);
        assert_eq!(str_of(&recs[0], "host"), "10.0.0.5");
        assert_eq!(
            str_of(&recs[0], "login_time"),
            "Mon Oct 20 10:00 - 10:05 (00:05)"
        );
        assert!(field(&recs[1], "host").is_none());
        let bad = refused(last_records("x y\n"));
        assert!(bad.contains("fewer than four"), "{bad}");
    }

    #[test]
    fn grep_keeps_the_newest_lines_in_order() {
        let recs = grep_records("a\nb\nc\nd\n", 2, "/var/log/syslog");
        assert_eq!(recs.len(), 2);
        assert_eq!(str_of(&recs[0], "MESSAGE"), "c");
        assert_eq!(str_of(&recs[1], "MESSAGE"), "d");
        assert_eq!(str_of(&recs[1], "source"), "/var/log/syslog");
        assert!(grep_records("", 5, "x").is_empty());
    }

    #[test]
    fn journal_lines_are_objects_and_notices_are_skipped() {
        let out = "{\"MESSAGE\":\"disk error\",\"PRIORITY\":\"3\"}\n-- No entries --\n";
        let recs = rows(journal_records(out));
        assert_eq!(recs.len(), 1);
        assert_eq!(str_of(&recs[0], "MESSAGE"), "disk error");
        let bad = refused(journal_records("{\"MESSAGE\":\"ok\"}\nnot json\n"));
        assert!(bad.contains("not a JSON object"), "{bad}");
    }

    #[test]
    fn json_output_is_an_array_of_records() {
        assert!(matches!(json_array("b", "t", ""), Ok(Value::Array(v)) if v.is_empty()));
        assert!(matches!(json_array("b", "t", "[]\n"), Ok(Value::Array(v)) if v.is_empty()));
        match json_array("b", "t", "{\"InterfaceAlias\":\"Wi-Fi\"}") {
            Ok(Value::Array(v)) => assert_eq!(v.len(), 1, "a lone object is one record"),
            other => panic!("expected one record, got {other:?}"),
        }
        assert!(json_array("b", "t", "5").is_err());
        assert!(json_array("b", "t", "{not json").is_err());
    }
}
