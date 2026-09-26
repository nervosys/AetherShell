//! `capabilities()` must report what the kernel reports.
//!
//! It parsed `capsh --print` and never extracted the effective set, because
//! capsh writes that line with a colon (`Current: =`) and the parser split on
//! `=`. The record looked plausible -- it had `bounding`, `ambient`, a `uid` --
//! so nothing flagged that the one set an agent needs was missing, or that
//! `uid` was the string "1000(test) euid=1000(test)". This compares every set
//! against the masks in /proc/self/status, which is the source capsh reads.

#![cfg(target_os = "linux")]

use aethershell::value::Value;

fn call(name: &str) -> Value {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, vec![], &mut env).expect("builtin call")
}

fn kernel_mask(key: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status.lines().find(|l| l.starts_with(&format!("{key}:"))).unwrap();
    u64::from_str_radix(line.split_once(':').unwrap().1.trim(), 16).unwrap()
}

#[test]
fn every_set_matches_the_kernel_mask() {
    let Value::Record(rec) = call("capabilities") else {
        panic!("capabilities() did not return a record");
    };
    for (field, key) in [
        ("inheritable", "CapInh"),
        ("permitted", "CapPrm"),
        ("effective", "CapEff"),
        ("bounding", "CapBnd"),
        ("ambient", "CapAmb"),
    ] {
        let Some(Value::Array(caps)) = rec.get(field) else {
            panic!("{field} is missing or not an array: {:?}", rec.get(field));
        };
        assert_eq!(
            caps.len() as u32,
            kernel_mask(key).count_ones(),
            "{field} has a different number of capabilities than {key}"
        );
        for c in caps {
            let Value::Str(s) = c else { panic!("{field} holds a non-string") };
            assert!(s.starts_with("cap_"), "{field} holds {s:?}");
        }
    }
}

#[test]
fn names_follow_the_kernel_numbering() {
    let Value::Record(rec) = call("capabilities") else { panic!() };
    let Some(Value::Array(bounding)) = rec.get("bounding") else { panic!() };
    // Bit 0 is CAP_CHOWN and bit 21 is CAP_SYS_ADMIN on every kernel; if the
    // bounding set holds them (it does outside a locked-down container), the
    // table is aligned.
    let names: Vec<&str> = bounding
        .iter()
        .filter_map(|v| if let Value::Str(s) = v { Some(s.as_str()) } else { None })
        .collect();
    let mask = kernel_mask("CapBnd");
    assert_eq!(names.contains(&"cap_chown"), mask & 1 != 0);
    assert_eq!(names.contains(&"cap_sys_admin"), mask & (1 << 21) != 0);
}

#[test]
fn ids_are_integers() {
    let Value::Record(rec) = call("capabilities") else { panic!() };
    for field in ["uid", "euid", "gid", "egid"] {
        assert!(matches!(rec.get(field), Some(Value::Int(_))), "{field}: {:?}", rec.get(field));
    }
    assert!(matches!(rec.get("no_new_privs"), Some(Value::Bool(_))));
}
