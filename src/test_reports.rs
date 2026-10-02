//! Pure summaries of supplied LCOV 1.x and completed libtest JSON reports.
use crate::{safety, value::Value};
use anyhow::Result;
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::collections::{BTreeMap, BTreeSet};

fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn budget(name: &str, message: &str) -> anyhow::Error {
    safety::budget_exceeded(
        name,
        message.into(),
        "supply a smaller coverage or test report",
    )
}
fn row(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn report(name: &str, args: &[Value], input: &Option<Value>) -> Result<String> {
    if args.len() != 1 || input.is_some() {
        return Err(bad(
            name,
            "expected one report String and no pipeline input",
        ));
    }
    let Value::Str(text) = &args[0] else {
        return Err(bad(name, "expected report String"));
    };
    if text.len() > 1048576 {
        return Err(budget(name, "report exceeds 1 MiB"));
    }
    if text.lines().count() > 25000 || text.lines().any(|line| line.len() > 65536) {
        return Err(budget(
            name,
            "report exceeds 25000 lines or 64 KiB per line",
        ));
    }
    Ok(text.clone())
}
fn number(name: &str, text: &str, positive: bool) -> Result<i64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad(name, "expected decimal nonnegative Int"));
    }
    let value: i64 = text
        .parse()
        .map_err(|_| bad(name, "integer exceeds Int range"))?;
    if positive && value == 0 {
        return Err(bad(name, "line numbers must be positive"));
    }
    Ok(value)
}
fn label(name: &str, text: &str) -> Result<()> {
    if text.is_empty() || text.contains('\0') {
        return Err(bad(name, "empty or invalid name"));
    }
    if text.len() > 4096 {
        return Err(budget(name, "name exceeds 4096 bytes"));
    }
    Ok(())
}
#[derive(Default)]
struct Coverage {
    lines: BTreeMap<i64, (bool, Option<String>)>,
    functions: BTreeMap<String, i64>,
    function_hits: BTreeMap<String, bool>,
    branches: BTreeMap<(i64, i64, i64), bool>,
    summaries: BTreeMap<String, i64>,
}
fn metric(found: usize, hit: usize) -> Value {
    row(vec![
        ("found", Value::Int(found as i64)),
        ("hit", Value::Int(hit as i64)),
        (
            "percent",
            if found == 0 {
                Value::Null
            } else {
                Value::Float(100.0 * hit as f64 / found as f64)
            },
        ),
    ])
}
impl Coverage {
    fn counts(&self) -> [(usize, usize); 3] {
        [
            (
                self.lines.len(),
                self.lines.values().filter(|(hit, _)| *hit).count(),
            ),
            (
                self.functions.len(),
                self.function_hits.values().filter(|hit| **hit).count(),
            ),
            (
                self.branches.len(),
                self.branches.values().filter(|hit| **hit).count(),
            ),
        ]
    }
    fn validate(&self, name: &str) -> Result<()> {
        if self.functions.keys().ne(self.function_hits.keys()) {
            return Err(bad(name, "FN and FNDA names must agree"));
        }
        for ((found, hit), (found_key, hit_key)) in
            self.counts()
                .into_iter()
                .zip([("LF", "LH"), ("FNF", "FNH"), ("BRF", "BRH")])
        {
            for (key, actual) in [(found_key, found), (hit_key, hit)] {
                if self.summaries.get(key).is_some_and(|n| *n != actual as i64) {
                    return Err(bad(name, "coverage summary contradicts its records"));
                }
            }
        }
        Ok(())
    }
    fn merge(&mut self, other: Coverage, name: &str) -> Result<()> {
        for (line, (hit, checksum)) in other.lines {
            match self.lines.entry(line) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert((hit, checksum));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let (existing_hit, existing_checksum) = entry.get_mut();
                    if existing_checksum.is_some()
                        && checksum.is_some()
                        && *existing_checksum != checksum
                    {
                        return Err(bad(
                            name,
                            "conflicting line checksums across coverage records",
                        ));
                    }
                    *existing_hit |= hit;
                    if existing_checksum.is_none() {
                        *existing_checksum = checksum;
                    }
                }
            }
        }
        for (function, line) in other.functions {
            if self
                .functions
                .insert(function, line)
                .is_some_and(|old| old != line)
            {
                return Err(bad(
                    name,
                    "conflicting function locations across coverage records",
                ));
            }
        }
        for (function, hit) in other.function_hits {
            *self.function_hits.entry(function).or_default() |= hit;
        }
        for (branch, hit) in other.branches {
            *self.branches.entry(branch).or_default() |= hit;
        }
        Ok(())
    }
}
pub(super) fn coverage(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const NAME: &str = "test_coverage";
    let text = report(NAME, &args, &input)?;
    let mut active: Option<(String, Coverage)> = None;
    let mut files = BTreeMap::<String, Coverage>::new();
    let mut records = 0;
    let mut entries = 0;
    for line in text.lines().filter(|line| !line.is_empty()) {
        if line == "end_of_record" {
            let (path, data) = active
                .take()
                .ok_or_else(|| bad(NAME, "end_of_record outside a source record"))?;
            data.validate(NAME)?;
            files.entry(path).or_default().merge(data, NAME)?;
            records += 1;
            if records > 1024 {
                return Err(budget(NAME, "coverage exceeds 1024 source records"));
            }
            continue;
        }
        let (tag, value) = line
            .split_once(':')
            .ok_or_else(|| bad(NAME, "invalid LCOV line"))?;
        if tag == "TN" {
            if active.is_some() {
                return Err(bad(NAME, "TN must precede a source record"));
            }
            if value.len() > 4096 {
                return Err(budget(NAME, "test name exceeds 4096 bytes"));
            }
            if value.contains('\0') {
                return Err(bad(NAME, "invalid test name"));
            }
            continue;
        }
        if tag == "SF" {
            if active.is_some() {
                return Err(bad(NAME, "missing end_of_record"));
            }
            label(NAME, value)?;
            active = Some((value.into(), Coverage::default()));
            continue;
        }
        let (_, data) = active
            .as_mut()
            .ok_or_else(|| bad(NAME, "coverage data outside a source record"))?;
        match tag {
            "DA" => {
                let parts: Vec<_> = value.split(',').collect();
                if !(2..=3).contains(&parts.len()) {
                    return Err(bad(NAME, "invalid DA record"));
                }
                let line = number(NAME, parts[0], true)?;
                let hit = number(NAME, parts[1], false)? > 0;
                let checksum = parts.get(2).map(|s| (*s).to_string());
                if checksum.as_ref().is_some_and(|s| {
                    s.is_empty() || s.len() > 128 || s.bytes().any(|b| b.is_ascii_whitespace())
                }) {
                    return Err(bad(NAME, "invalid line checksum"));
                }
                if data.lines.insert(line, (hit, checksum)).is_some() {
                    return Err(bad(NAME, "duplicate DA line within a record"));
                }
            }
            "FN" | "FNDA" => {
                let (n, function) = value
                    .split_once(',')
                    .ok_or_else(|| bad(NAME, "invalid function record"))?;
                label(NAME, function)?;
                let n = number(NAME, n, tag == "FN")?;
                let duplicate = if tag == "FN" {
                    data.functions.insert(function.into(), n).is_some()
                } else {
                    data.function_hits.insert(function.into(), n > 0).is_some()
                };
                if duplicate {
                    return Err(bad(NAME, "duplicate function within a record"));
                }
            }
            "BRDA" => {
                let parts: Vec<_> = value.split(',').collect();
                if parts.len() != 4 {
                    return Err(bad(NAME, "invalid BRDA record"));
                }
                let key = (
                    number(NAME, parts[0], true)?,
                    number(NAME, parts[1], false)?,
                    number(NAME, parts[2], false)?,
                );
                let hit = parts[3] != "-" && number(NAME, parts[3], false)? > 0;
                if data.branches.insert(key, hit).is_some() {
                    return Err(bad(NAME, "duplicate branch within a record"));
                }
            }
            "LF" | "LH" | "FNF" | "FNH" | "BRF" | "BRH" => {
                let n = number(NAME, value, false)?;
                if data.summaries.insert(tag.into(), n).is_some() {
                    return Err(bad(NAME, "duplicate coverage summary"));
                }
                continue;
            }
            _ => return Err(bad(NAME, "unsupported LCOV 1.x record type")),
        }
        entries += 1;
        if entries > 10000 {
            return Err(budget(NAME, "coverage exceeds 10000 data entries"));
        }
    }
    if active.is_some() || records == 0 {
        return Err(bad(NAME, "expected completed LCOV source records"));
    }
    let mut totals = [(0usize, 0usize); 3];
    let mut rows = Vec::new();
    for (path, data) in files {
        let counts = data.counts();
        for (total, (found, hit)) in totals.iter_mut().zip(counts) {
            total.0 += found;
            total.1 += hit;
        }
        rows.push(row(vec![
            ("path", Value::Str(path)),
            ("lines", metric(counts[0].0, counts[0].1)),
            ("functions", metric(counts[1].0, counts[1].1)),
            ("branches", metric(counts[2].0, counts[2].1)),
        ]));
    }
    Ok(row(vec![
        ("files", Value::Array(rows)),
        ("lines", metric(totals[0].0, totals[0].1)),
        ("functions", metric(totals[1].0, totals[1].1)),
        ("branches", metric(totals[2].0, totals[2].1)),
    ]))
}

// Reject duplicate JSON keys instead of letting a later field override an event.
struct Event(BTreeMap<String, serde_json::Value>);
impl<'de> Deserialize<'de> for Event {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = Event;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a flat JSON event object")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Event, A::Error> {
                let mut fields = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key.len() > 64 || fields.len() >= 16 || fields.contains_key(&key) {
                        return Err(serde::de::Error::custom("invalid or duplicate event field"));
                    }
                    let value = map.next_value::<serde_json::Value>()?;
                    if value.is_array() || value.is_object() {
                        return Err(serde::de::Error::custom("event values must be scalar"));
                    }
                    fields.insert(key, value);
                }
                Ok(Event(fields))
            }
        }
        deserializer.deserialize_map(Object)
    }
}
impl Event {
    fn fields(&self, name: &str, allowed: &[&str]) -> Result<()> {
        if self.0.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(bad(name, "unsupported JSON event field"));
        }
        Ok(())
    }
    fn text(&self, name: &str, key: &str) -> Result<&str> {
        self.0
            .get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad(name, "missing or non-String event field"))
    }
    fn count(&self, name: &str, key: &str) -> Result<i64> {
        self.0
            .get(key)
            .and_then(|v| v.as_i64())
            .filter(|n| *n >= 0)
            .ok_or_else(|| bad(name, "missing or invalid event count"))
    }
    fn optional_text(&self, name: &str, key: &str) -> Result<Value> {
        match self.0.get(key) {
            None => Ok(Value::Null),
            Some(v) => v
                .as_str()
                .map(|s| Value::Str(s.into()))
                .ok_or_else(|| bad(name, "invalid optional event String")),
        }
    }
    fn time(&self, name: &str, key: &str) -> Result<Value> {
        match self.0.get(key) {
            None => Ok(Value::Null),
            Some(v) => v
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(Value::Float)
                .ok_or_else(|| bad(name, "invalid event duration")),
        }
    }
}
#[derive(Default)]
struct Suite {
    count: i64,
    started: BTreeSet<String>,
    timeouts: BTreeSet<String>,
    results: BTreeMap<String, (String, Value)>,
}
pub(super) fn outcomes(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let text = report(name, &args, &input)?;
    let wanted = match name {
        "test_passing" => "passed",
        "test_failing" => "failed",
        "test_skipped" => "skipped",
        _ => unreachable!(),
    };
    let mut active: Option<Suite> = None;
    let mut suites = 0;
    let mut completed = 0;
    let mut output = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let mut decoder = serde_json::Deserializer::from_str(line);
        let event = Event::deserialize(&mut decoder)
            .map_err(|_| bad(name, "invalid or duplicate JSON event fields"))?;
        decoder
            .end()
            .map_err(|_| bad(name, "trailing data after JSON event"))?;
        let kind = event.text(name, "type")?;
        if kind == "report" {
            event.fields(name, &["type", "total_time", "compilation_time"])?;
            if active.is_some()
                || suites == 0
                || !event.0.contains_key("total_time")
                || !event.0.contains_key("compilation_time")
            {
                return Err(bad(name, "timing report outside a completed suite"));
            }
            event.time(name, "total_time")?;
            event.time(name, "compilation_time")?;
            continue;
        }
        let action = event.text(name, "event")?;
        match (kind, action) {
            ("suite", "started") => {
                event.fields(name, &["type", "event", "test_count", "shuffle_seed"])?;
                if active.is_some() {
                    return Err(bad(name, "overlapping test suites"));
                }
                let count = event.count(name, "test_count")?;
                if count > 10000 || suites >= 128 {
                    return Err(budget(name, "report exceeds 128 suites or 10000 tests"));
                }
                if event
                    .0
                    .get("shuffle_seed")
                    .is_some_and(|v| v.as_u64().is_none())
                {
                    return Err(bad(name, "invalid shuffle seed"));
                }
                suites += 1;
                active = Some(Suite {
                    count,
                    ..Suite::default()
                });
            }
            ("suite", "ok" | "failed") => {
                event.fields(
                    name,
                    &[
                        "type",
                        "event",
                        "passed",
                        "failed",
                        "ignored",
                        "measured",
                        "filtered_out",
                        "exec_time",
                    ],
                )?;
                event.time(name, "exec_time")?;
                event.count(name, "filtered_out")?;
                if event.count(name, "measured")? != 0 {
                    return Err(bad(name, "benchmark reports are unsupported"));
                }
                let suite = active
                    .take()
                    .ok_or_else(|| bad(name, "suite result without suite start"))?;
                if suite.results.len() as i64 != suite.count
                    || suite.started.iter().any(|n| !suite.results.contains_key(n))
                {
                    return Err(bad(
                        name,
                        "incomplete test suite, including fail-fast reports",
                    ));
                }
                for (key, status) in [
                    ("passed", "passed"),
                    ("failed", "failed"),
                    ("ignored", "skipped"),
                ] {
                    let actual = suite.results.values().filter(|(s, _)| s == status).count() as i64;
                    if event.count(name, key)? != actual {
                        return Err(bad(name, "suite summary contradicts test outcomes"));
                    }
                }
                if (action == "ok") != (event.count(name, "failed")? == 0) {
                    return Err(bad(name, "suite status contradicts failures"));
                }
                output.extend(
                    suite
                        .results
                        .into_values()
                        .filter_map(|(status, result)| (status == wanted).then_some(result)),
                );
            }
            ("test", "started" | "timeout" | "ok" | "failed" | "ignored") => {
                let test_name = event.text(name, "name")?;
                label(name, test_name)?;
                let suite = active
                    .as_mut()
                    .ok_or_else(|| bad(name, "test event outside a suite"))?;
                if suite.results.contains_key(test_name) {
                    return Err(bad(name, "event after terminal test result"));
                }
                if action == "started" {
                    event.fields(name, &["type", "event", "name"])?;
                    if !suite.started.insert(test_name.into()) {
                        return Err(bad(name, "duplicate test start"));
                    }
                    if suite.started.len() as i64 > suite.count {
                        return Err(bad(name, "more test starts than declared"));
                    }
                    continue;
                }
                if action == "timeout" {
                    event.fields(name, &["type", "event", "name"])?;
                    if !suite.started.contains(test_name)
                        || !suite.timeouts.insert(test_name.into())
                    {
                        return Err(bad(name, "invalid timeout event"));
                    }
                    continue;
                }
                event.fields(
                    name,
                    &[
                        "type",
                        "event",
                        "name",
                        "stdout",
                        "message",
                        "reason",
                        "exec_time",
                    ],
                )?;
                if action != "ignored" && !suite.started.contains(test_name) {
                    return Err(bad(name, "test result without test start"));
                }
                let status = match action {
                    "ok" => "passed",
                    "failed" => "failed",
                    _ => "skipped",
                };
                let result = row(vec![
                    ("suite", Value::Int(suites)),
                    ("name", Value::Str(test_name.into())),
                    ("status", Value::Str(status.into())),
                    ("stdout", event.optional_text(name, "stdout")?),
                    ("message", event.optional_text(name, "message")?),
                    ("reason", event.optional_text(name, "reason")?),
                    ("exec_time", event.time(name, "exec_time")?),
                ]);
                suite
                    .results
                    .insert(test_name.into(), (status.into(), result));
                completed += 1;
                if completed > 10000 {
                    return Err(budget(name, "report exceeds 10000 completed tests"));
                }
                if suite.results.len() as i64 > suite.count {
                    return Err(bad(name, "more test results than declared"));
                }
            }
            _ => return Err(bad(name, "unsupported libtest execution event")),
        }
    }
    if active.is_some() || suites == 0 {
        return Err(bad(name, "expected completed libtest JSON suites"));
    }
    Ok(Value::Array(output))
}
