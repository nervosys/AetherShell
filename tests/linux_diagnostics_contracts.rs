//! Contracts for the Linux diagnostic builtins: blkid, lsof, monitor_tcpdump,
//! monitor_ethtool, monitor_perf_stat and monitor_perf_record.
//!
//! Ordinary tests run everywhere: a bad argument is refused with a coded
//! E_BAD_ARG before any effect, pipeline input is refused, and off Linux each
//! Linux-only tool is E_UNIMPLEMENTED with nothing run.
//!
//! The tests that run the tools are `#[ignore]`d and run only in the disposable
//! distro, via scripts/disposable_linux_tests.sh, as root. Each verifies the
//! effect with a source other than the builtin's own answer (lsblk, the
//! process id, a UDP socket, a direct run of the tool) and removes what it made.

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

use aethershell::env::Env;
use aethershell::value::Value;

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn argv(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|item| s(item)).collect())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call_with_input(name, args, Some(input), &mut env)
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

fn expect_code(name: &str, args: Vec<Value>, code: &str) {
    match call(name, args.clone()) {
        Ok(v) => panic!("{name}{args:?} should be refused with {code}, got {v:?}"),
        Err(e) => assert_eq!(code_of(&e), code, "{name}: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Ordinary tests: argument refusal and pipeline input, everywhere.
// ---------------------------------------------------------------------------

#[test]
fn blkid_refuses_a_bad_device_before_any_effect() {
    expect_code("blkid", vec![s("sda1")], "E_BAD_ARG");
    expect_code("blkid", vec![s("/dev/../etc/shadow")], "E_BAD_ARG");
    expect_code("blkid", vec![s("/dev/sda; id")], "E_BAD_ARG");
    expect_code("blkid", vec![s("-o")], "E_BAD_ARG");
    expect_code("blkid", vec![], "E_BAD_ARG");
    expect_code("blkid", vec![s("/dev/sda1"), s("extra")], "E_BAD_ARG");
    expect_code("blkid", vec![Value::Int(5)], "E_BAD_ARG");
}

#[test]
fn blkid_refuses_pipeline_input() {
    assert!(call_piped("blkid", vec![s("/dev/sda1")], s("x")).is_err());
}

#[test]
fn lsof_refuses_option_like_and_surplus_arguments() {
    expect_code("lsof", vec![s("-x")], "E_BAD_ARG");
    expect_code("lsof", vec![s("")], "E_BAD_ARG");
    expect_code("lsof", vec![Value::Int(3)], "E_BAD_ARG");
    expect_code("lsof", vec![s("a"), s("b")], "E_BAD_ARG");
    assert!(call_piped("lsof", vec![], s("x")).is_err());
}

#[cfg(unix)]
#[test]
fn lsof_refuses_a_missing_path_by_name() {
    expect_code(
        "lsof",
        vec![s("/nonexistent/ae-lsof-contract")],
        "E_NOT_FOUND",
    );
}

#[test]
fn monitor_tcpdump_refuses_bad_arguments_before_any_effect() {
    expect_code("monitor_tcpdump", vec![s("-i")], "E_BAD_ARG");
    expect_code("monitor_tcpdump", vec![s("lo -w /tmp/x")], "E_BAD_ARG");
    expect_code(
        "monitor_tcpdump",
        vec![s("averyveryverylongname")],
        "E_BAD_ARG",
    );
    expect_code("monitor_tcpdump", vec![s("lo"), Value::Int(0)], "E_BAD_ARG");
    expect_code(
        "monitor_tcpdump",
        vec![s("lo"), Value::Int(1001)],
        "E_BAD_ARG",
    );
    expect_code("monitor_tcpdump", vec![s("lo"), s("10")], "E_BAD_ARG");
    expect_code(
        "monitor_tcpdump",
        vec![s("lo"), Value::Int(1), Value::Int(0)],
        "E_BAD_ARG",
    );
    expect_code(
        "monitor_tcpdump",
        vec![s("lo"), Value::Int(1), Value::Int(61)],
        "E_BAD_ARG",
    );
    expect_code(
        "monitor_tcpdump",
        vec![s("lo"), Value::Int(1), Value::Int(1), Value::Int(1)],
        "E_BAD_ARG",
    );
    assert!(call_piped("monitor_tcpdump", vec![s("lo")], s("x")).is_err());
}

#[test]
fn monitor_ethtool_refuses_bad_device_names() {
    expect_code("monitor_ethtool", vec![], "E_BAD_ARG");
    expect_code("monitor_ethtool", vec![s("eth0; id")], "E_BAD_ARG");
    expect_code("monitor_ethtool", vec![s("-h")], "E_BAD_ARG");
    expect_code("monitor_ethtool", vec![s("../eth0")], "E_BAD_ARG");
    expect_code("monitor_ethtool", vec![Value::Int(0)], "E_BAD_ARG");
    assert!(call_piped("monitor_ethtool", vec![s("lo")], s("x")).is_err());
}

#[test]
fn monitor_perf_stat_requires_an_argv_array() {
    expect_code("monitor_perf_stat", vec![s("sleep 1")], "E_BAD_ARG");
    expect_code("monitor_perf_stat", vec![], "E_BAD_ARG");
    expect_code("monitor_perf_stat", vec![Value::Array(vec![])], "E_BAD_ARG");
    expect_code(
        "monitor_perf_stat",
        vec![Value::Array(vec![Value::Int(1)])],
        "E_BAD_ARG",
    );
    expect_code("monitor_perf_stat", vec![argv(&["-x"])], "E_BAD_ARG");
    expect_code(
        "monitor_perf_stat",
        vec![argv(&["/usr/bin/true", ""])],
        "E_BAD_ARG",
    );
    expect_code(
        "monitor_perf_stat",
        vec![argv(&["/usr/bin/true"]), s("-e")],
        "E_BAD_ARG",
    );
    expect_code(
        "monitor_perf_stat",
        vec![argv(&["/usr/bin/true"]), s("cycles;rm")],
        "E_BAD_ARG",
    );
    expect_code(
        "monitor_perf_stat",
        vec![argv(&["/usr/bin/true"]), s("cycles"), s("x")],
        "E_BAD_ARG",
    );
    assert!(call_piped("monitor_perf_stat", vec![argv(&["/usr/bin/true"])], s("x")).is_err());
}

#[test]
fn monitor_perf_record_refuses_option_like_or_unreachable_output() {
    expect_code("monitor_perf_record", vec![s("sleep 1")], "E_BAD_ARG");
    expect_code(
        "monitor_perf_record",
        vec![argv(&["/usr/bin/true"]), s("-o")],
        "E_BAD_ARG",
    );
    expect_code(
        "monitor_perf_record",
        vec![argv(&["/usr/bin/true"]), Value::Int(1)],
        "E_BAD_ARG",
    );
    expect_code(
        "monitor_perf_record",
        vec![argv(&["/usr/bin/true"]), s("/nonexistent-ae-dir/perf.data")],
        "E_NOT_FOUND",
    );
    assert!(call_piped(
        "monitor_perf_record",
        vec![argv(&["/usr/bin/true"])],
        s("x")
    )
    .is_err());
}

#[cfg(not(target_os = "linux"))]
#[test]
fn linux_only_tools_refuse_coded_and_run_nothing_elsewhere() {
    for (name, args) in [
        ("blkid", vec![s("/dev/sda1")]),
        ("monitor_ethtool", vec![s("eth0")]),
        ("monitor_perf_stat", vec![argv(&["/usr/bin/true"])]),
        ("monitor_perf_record", vec![argv(&["/usr/bin/true"])]),
    ] {
        match call(name, args) {
            Ok(v) => panic!("{name} ran off Linux and answered {v:?}"),
            Err(e) => {
                assert_eq!(code_of(&e), "E_UNIMPLEMENTED", "{name}: {e}");
                let text = e.to_string();
                assert!(text.contains("Linux only"), "{name}: {text}");
                assert!(text.contains("NOTHING WAS"), "{name}: {text}");
            }
        }
    }
}

#[cfg(not(unix))]
#[test]
fn lsof_and_tcpdump_refuse_coded_off_unix() {
    // On Windows lsof lists sockets with no path; a path is not read there.
    #[cfg(windows)]
    let e = call("lsof", vec![s("C:/ae-lsof-contract")]).expect_err("lsof path must refuse here");
    #[cfg(not(windows))]
    let e = call("lsof", vec![]).expect_err("lsof must refuse on this OS");
    assert_eq!(code_of(&e), "E_UNIMPLEMENTED", "{e}");
    let e = call("monitor_tcpdump", vec![]).expect_err("tcpdump must refuse on this OS");
    assert_eq!(code_of(&e), "E_UNIMPLEMENTED", "{e}");
}

// ---------------------------------------------------------------------------
// Disposable tests: run the real tools as root in the disposable distro.
// ---------------------------------------------------------------------------

/// A scratch directory under the system temp dir, removed on drop.
#[cfg(target_os = "linux")]
struct Scratch(std::path::PathBuf);

#[cfg(target_os = "linux")]
impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ae-linux-diag-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }
    fn path(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

#[cfg(target_os = "linux")]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(target_os = "linux")]
fn record_field<'a>(rec: &'a Value, key: &str) -> Option<&'a Value> {
    match rec {
        Value::Record(map) => map.get(key),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn str_field(rec: &Value, key: &str) -> String {
    match record_field(rec, key) {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("expected a String field {key}, got {other:?} in {rec:?}"),
    }
}

/// Runs a tool directly, with no shell, to check the builtin's answer against it.
#[cfg(target_os = "linux")]
fn run_ok(program: &str, args: &[&str]) -> std::process::Output {
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("could not run {program}: {e}"))
}

/// Detaches the loop device on drop.
#[cfg(target_os = "linux")]
struct Loop(String);

#[cfg(target_os = "linux")]
impl Drop for Loop {
    fn drop(&mut self) {
        let _ = run_ok("losetup", &["-d", &self.0]);
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn blkid_reports_the_type_of_a_loop_device_ext4_image() {
    disposable::require();
    let scratch = Scratch::new("blkid");
    let image = scratch.path("disk.img");
    std::fs::File::create(&image)
        .and_then(|f| f.set_len(64 * 1024 * 1024))
        .expect("create image");
    let attach = run_ok("losetup", &["-f", "--show", image.to_str().unwrap()]);
    assert!(
        attach.status.success(),
        "losetup: {}",
        String::from_utf8_lossy(&attach.stderr)
    );
    let device = String::from_utf8_lossy(&attach.stdout).trim().to_string();
    let _loop = Loop(device.clone());
    let mkfs = run_ok("mkfs.ext4", &["-q", "-F", &device]);
    assert!(
        mkfs.status.success(),
        "mkfs.ext4: {}",
        String::from_utf8_lossy(&mkfs.stderr)
    );

    let rec = call("blkid", vec![s(&device)]).expect("blkid on an ext4 image");
    assert_eq!(str_field(&rec, "type"), "ext4", "{rec:?}");
    // Independent check: the ext4 superblock magic (0xEF53, little-endian) at
    // byte 1024 + 0x38, read straight off the device rather than through blkid.
    {
        use std::io::{Read, Seek, SeekFrom};
        let mut dev = std::fs::File::open(&device).expect("open loop device");
        let mut magic = [0u8; 2];
        dev.seek(SeekFrom::Start(1024 + 0x38)).expect("seek");
        dev.read_exact(&mut magic).expect("read superblock magic");
        assert_eq!(magic, [0x53, 0xEF], "no ext superblock magic on {device}");
    }
    assert!(!str_field(&rec, "uuid").is_empty());

    let e = call("blkid", vec![s("/dev/ae-no-such-device")]).expect_err("missing device");
    assert_eq!(code_of(&e), "E_NOT_FOUND", "{e}");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn lsof_finds_a_file_this_process_holds_open() {
    disposable::require();
    let scratch = Scratch::new("lsof");
    let path = scratch.path("held.txt");
    let held = std::fs::File::create(&path).expect("create held file");
    let path_text = path.to_str().unwrap().to_string();

    let answer = call("lsof", vec![s(&path_text)]).expect("lsof on an open file");
    let rows = match &answer {
        Value::Array(rows) => rows,
        other => panic!("lsof should answer an Array, got {other:?}"),
    };
    let me = std::process::id().to_string();
    let found = rows
        .iter()
        .any(|row| str_field(row, "pid") == me && str_field(row, "name") == path_text);
    assert!(
        found,
        "lsof did not list this process ({me}) holding {path_text}: {answer:?}"
    );
    drop(held);

    // Once closed, nothing holds it.
    let after = call("lsof", vec![s(&path_text)]).expect("lsof after close");
    assert!(
        matches!(after, Value::Array(ref rows) if rows.is_empty()),
        "{after:?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn tcpdump_capture_sees_a_udp_datagram_on_lo() {
    use std::net::UdpSocket;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    disposable::require();
    let receiver = UdpSocket::bind("127.0.0.1:0").expect("bind receiver");
    let port = receiver.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let sender_stop = Arc::clone(&stop);
    // Keep sending until the capture returns, so tcpdump's start-up does not
    // race the traffic.
    let sender = std::thread::spawn(move || {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind sender");
        while !sender_stop.load(Ordering::SeqCst) {
            let _ = socket.send_to(b"ae-capture", ("127.0.0.1", port));
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let started = std::time::Instant::now();
    let answer = call(
        "monitor_tcpdump",
        vec![s("lo"), Value::Int(5), Value::Int(30)],
    );
    stop.store(true, Ordering::SeqCst);
    sender.join().expect("sender thread");
    drop(receiver);
    let answer = answer.expect("tcpdump_capture on lo");

    assert!(
        started.elapsed() < Duration::from_secs(45),
        "capture was not bounded"
    );
    assert_eq!(str_field(&answer, "interface"), "lo");
    let captured = match record_field(&answer, "captured") {
        Some(Value::Int(n)) => *n,
        other => panic!("captured should be an Int, got {other:?}"),
    };
    let packets = match record_field(&answer, "packets") {
        Some(Value::Array(p)) => p,
        other => panic!("packets should be an Array, got {other:?}"),
    };
    assert_eq!(captured as usize, packets.len());
    assert!(captured >= 1, "no packet was captured: {answer:?}");
    let needle = format!(".{port}:");
    let seen = packets
        .iter()
        .any(|p| str_field(p, "line").contains(&needle));
    assert!(seen, "no captured line mentions port {port}: {answer:?}");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn ethtool_info_on_lo_answers_or_fails_as_a_tool_failure() {
    disposable::require();
    let direct = run_ok("ethtool", &["lo"]);
    match call("monitor_ethtool", vec![s("lo")]) {
        Ok(rec) => {
            assert!(
                direct.status.success(),
                "ethtool failed but the builtin answered: {rec:?}"
            );
            assert_eq!(str_field(&rec, "device"), "lo");
        }
        Err(e) => {
            assert!(
                !direct.status.success(),
                "the builtin refused but ethtool lo succeeded: {e}"
            );
            assert_eq!(code_of(&e), "E_TOOL_FAILED", "{e}");
        }
    }
}

/// `/proc/sys/kernel/perf_event_paranoid` set to -1 for one test, restored on drop.
#[cfg(target_os = "linux")]
struct Paranoid(String);

#[cfg(target_os = "linux")]
impl Paranoid {
    const PATH: &'static str = "/proc/sys/kernel/perf_event_paranoid";

    fn lower_to_minus_one() -> Self {
        let before = std::fs::read_to_string(Self::PATH)
            .expect("read perf_event_paranoid")
            .trim()
            .to_string();
        std::fs::write(Self::PATH, "-1\n").expect("set perf_event_paranoid to -1");
        Paranoid(before)
    }
}

#[cfg(target_os = "linux")]
impl Drop for Paranoid {
    fn drop(&mut self) {
        let _ = std::fs::write(Self::PATH, format!("{}\n", self.0));
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn perf_stat_counts_a_true_process_or_fails_as_a_tool_failure() {
    disposable::require();
    let _paranoid = Paranoid::lower_to_minus_one();
    match call("monitor_perf_stat", vec![argv(&["/usr/bin/true"])]) {
        Ok(rec) => {
            let counters = match record_field(&rec, "counters") {
                Some(Value::Array(c)) => c,
                other => panic!("counters should be an Array, got {other:?}"),
            };
            assert!(!counters.is_empty(), "perf reported no counters: {rec:?}");
            for c in counters {
                assert!(!str_field(c, "name").is_empty(), "{c:?}");
            }
        }
        Err(e) => {
            // Only acceptable if perf itself cannot count on this kernel.
            let direct = run_ok("perf", &["stat", "--", "/usr/bin/true"]);
            assert!(
                !direct.status.success(),
                "perf works but the builtin failed: {e}"
            );
            assert_eq!(code_of(&e), "E_TOOL_FAILED", "{e}");
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn perf_record_writes_only_to_the_path_it_is_given() {
    disposable::require();
    let _paranoid = Paranoid::lower_to_minus_one();
    let scratch = Scratch::new("perf-record");
    let out = scratch.path("profile.data");
    let out_text = out.to_str().unwrap().to_string();

    match call(
        "monitor_perf_record",
        vec![argv(&["/usr/bin/true"]), s(&out_text)],
    ) {
        Ok(rec) => {
            assert_eq!(str_field(&rec, "output_file"), out_text);
            let meta = std::fs::metadata(&out).expect("perf wrote the named file");
            assert!(meta.len() > 0, "empty profile");
            let bytes = match record_field(&rec, "bytes") {
                Some(Value::Int(n)) => *n as u64,
                other => panic!("bytes should be an Int, got {other:?}"),
            };
            assert_eq!(bytes, meta.len());
        }
        Err(e) => {
            let direct = run_ok("perf", &["record", "-o", &out_text, "--", "/usr/bin/true"]);
            assert!(
                !direct.status.success(),
                "perf works but the builtin failed: {e}"
            );
            assert_eq!(code_of(&e), "E_TOOL_FAILED", "{e}");
        }
    }
}
