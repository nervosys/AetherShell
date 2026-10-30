//! Tests for proactive monitoring and alerting builtins

use iron_shell::builtins::*;
use iron_shell::value::Value;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// The alert store lives under IRONSHELL_DATA_HOME and is one process-wide file, so
/// every test that touches it holds this lock and points the store at its own directory.
static STORE: Mutex<()> = Mutex::new(());

struct IsolatedStore {
    dir: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

fn isolated_store() -> IsolatedStore {
    let guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!(
        "ae-monitoring-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("create the temporary data directory");
    std::env::set_var("IRONSHELL_DATA_HOME", &dir);
    IsolatedStore { dir, _guard: guard }
}

impl Drop for IsolatedStore {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_DATA_HOME");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ============================================================================
// Health Check
// ============================================================================

#[test]
fn test_health_check_returns_record() {
    let result = bi_health_check(vec![], None);
    assert!(result.is_ok(), "health_check should succeed");
    match result.unwrap() {
        Value::Record(rec) => {
            assert!(rec.contains_key("status"), "should have status field");
            assert!(rec.contains_key("cpu_percent"), "should have cpu_percent");
            assert!(
                rec.contains_key("memory_percent"),
                "should have memory_percent"
            );
            assert!(rec.contains_key("disk_percent"), "should have disk_percent");
            assert!(
                rec.contains_key("process_count"),
                "should have process_count"
            );
            assert!(rec.contains_key("alerts"), "should have alerts array");
            assert!(rec.contains_key("timestamp"), "should have timestamp");
            // Status should be one of healthy, warning, critical
            if let Some(Value::Str(s)) = rec.get("status") {
                assert!(
                    s == "healthy" || s == "warning" || s == "critical",
                    "status should be healthy/warning/critical, got: {}",
                    s
                );
            }
            assert!(matches!(rec.get("alerts"), Some(Value::Array(_))));
        }
        other => panic!("health_check should return Record, got: {:?}", other),
    }
}

// ============================================================================
// Alert Create
// ============================================================================

#[test]
fn test_alert_create_returns_record() {
    let _store = isolated_store();
    let result = bi_alert_create(
        vec![
            Value::Str("high_cpu".to_string()),
            Value::Str("cpu".to_string()),
            Value::Str(">".to_string()),
            Value::Float(90.0),
            Value::Str("critical".to_string()),
        ],
        None,
    );
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert_eq!(rec.get("name"), Some(&Value::Str("high_cpu".to_string())));
            assert_eq!(rec.get("metric"), Some(&Value::Str("cpu".to_string())));
            assert_eq!(rec.get("threshold"), Some(&Value::Float(90.0)));
            assert_eq!(
                rec.get("severity"),
                Some(&Value::Str("critical".to_string()))
            );
            assert_eq!(rec.get("status"), Some(&Value::Str("active".to_string())));
            assert!(rec.contains_key("id"));
        }
        other => panic!("alert_create should return Record, got: {:?}", other),
    }
}

#[test]
fn test_alert_create_requires_name() {
    let result = bi_alert_create(vec![], None);
    assert!(result.is_err(), "alert_create with no args should fail");
}

#[test]
fn test_alert_create_requires_metric() {
    let result = bi_alert_create(vec![Value::Str("test".to_string())], None);
    assert!(result.is_err(), "alert_create without metric should fail");
}

#[test]
fn test_alert_create_defaults() {
    let _store = isolated_store();
    let result = bi_alert_create(
        vec![
            Value::Str("test_alert".to_string()),
            Value::Str("memory".to_string()),
        ],
        None,
    );
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert_eq!(rec.get("condition"), Some(&Value::Str(">".to_string())));
            assert_eq!(rec.get("threshold"), Some(&Value::Float(80.0)));
            assert_eq!(
                rec.get("severity"),
                Some(&Value::Str("warning".to_string()))
            );
        }
        _ => panic!("should return Record"),
    }
}

// ============================================================================
// Alert List & History
// ============================================================================

#[test]
fn test_alert_list_returns_array() {
    let _store = isolated_store();
    // alert_list reports only stored rules; fixed default rules were removed (commit 11beb85).
    match bi_alert_list(vec![], None).unwrap() {
        Value::Array(arr) => assert!(arr.is_empty(), "a fresh store lists no rules, got {arr:?}"),
        other => panic!("alert_list should return Array, got: {:?}", other),
    }
    bi_alert_create(
        vec![
            Value::Str("high_cpu".to_string()),
            Value::Str("cpu".to_string()),
        ],
        None,
    )
    .unwrap();
    match bi_alert_list(vec![], None).unwrap() {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 1, "the one stored rule is listed");
            if let Value::Record(rec) = &arr[0] {
                assert!(rec.contains_key("name"));
                assert!(rec.contains_key("metric"));
                assert!(rec.contains_key("threshold"));
                assert!(rec.contains_key("triggered"));
            } else {
                panic!("a rule is a Record, got {:?}", arr[0]);
            }
        }
        other => panic!("alert_list should return Array, got: {:?}", other),
    }
}

#[test]
fn test_alert_history_returns_array() {
    let _store = isolated_store();
    let result = bi_alert_history(vec![], None);
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), Value::Array(_)));
}

// ============================================================================
// Alert Remove
// ============================================================================

#[test]
fn test_alert_remove_returns_confirmation() {
    let _store = isolated_store();
    let created = match bi_alert_create(
        vec![
            Value::Str("high_cpu".to_string()),
            Value::Str("cpu".to_string()),
        ],
        None,
    )
    .unwrap()
    {
        Value::Record(rec) => rec,
        other => panic!("alert_create should return Record, got: {:?}", other),
    };
    let id = match created.get("id") {
        Some(Value::Str(id)) => id.clone(),
        other => panic!("the created rule has no string id: {other:?}"),
    };

    let result = bi_alert_remove(vec![Value::Str(id.clone())], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert_eq!(rec.get("id"), Some(&Value::Str(id.clone())));
            assert_eq!(rec.get("removed"), Some(&Value::Bool(true)));
        }
        other => panic!("alert_remove should return Record, got: {:?}", other),
    }

    // Removing a rule that is gone, or was never stored, is a coded not-found refusal.
    for unknown in [id.as_str(), "alert_123"] {
        let err = bi_alert_remove(vec![Value::Str(unknown.to_string())], None)
            .expect_err("removing an unknown alert must fail");
        assert!(err.to_string().contains("E_NOT_FOUND"), "{err}");
    }
}

#[test]
fn test_alert_remove_requires_id() {
    let result = bi_alert_remove(vec![], None);
    assert!(result.is_err());
}

// ============================================================================
// Watch CPU
// ============================================================================

#[test]
fn test_watch_cpu_returns_record() {
    let result = bi_watch_cpu(vec![Value::Float(80.0), Value::Int(2)], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert_eq!(rec.get("threshold"), Some(&Value::Float(80.0)));
            assert_eq!(rec.get("samples"), Some(&Value::Int(2)));
            assert!(rec.contains_key("average_cpu"));
            assert!(rec.contains_key("breaches"));
            assert!(rec.contains_key("alert"));
            if let Some(Value::Array(readings)) = rec.get("readings") {
                assert_eq!(readings.len(), 2, "should have 2 samples");
            }
        }
        other => panic!("watch_cpu should return Record, got: {:?}", other),
    }
}

// ============================================================================
// Watch Memory
// ============================================================================

#[test]
fn test_watch_memory_returns_record() {
    let result = bi_watch_memory(vec![Value::Float(80.0)], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert!(rec.contains_key("status"));
            assert!(rec.contains_key("total_bytes"));
            assert!(rec.contains_key("use_percent"));
            assert!(rec.contains_key("above_threshold"));
            assert!(rec.contains_key("swap_total"));
            assert!(rec.contains_key("timestamp"));
            if let Some(Value::Int(total)) = rec.get("total_bytes") {
                assert!(*total > 0, "total_bytes should be positive");
            }
        }
        other => panic!("watch_memory should return Record, got: {:?}", other),
    }
}

// ============================================================================
// Watch Disk
// ============================================================================

#[test]
fn test_watch_disk_returns_record() {
    let result = bi_watch_disk(vec![Value::Float(85.0)], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert_eq!(rec.get("threshold"), Some(&Value::Float(85.0)));
            assert!(rec.contains_key("alert"));
            assert!(rec.contains_key("disk_count"));
            assert!(rec.contains_key("disks"));
            if let Some(Value::Array(disks)) = rec.get("disks") {
                assert!(!disks.is_empty(), "should have at least one disk");
                if let Value::Record(disk) = &disks[0] {
                    assert!(disk.contains_key("mount"));
                    assert!(disk.contains_key("total_bytes"));
                    assert!(disk.contains_key("use_percent"));
                    assert!(disk.contains_key("status"));
                }
            }
        }
        other => panic!("watch_disk should return Record, got: {:?}", other),
    }
}

// ============================================================================
// Existing monitoring builtins (now wired into dispatch)
// ============================================================================

#[test]
fn test_htop_snapshot_returns_array() {
    let result = bi_htop_snapshot(vec![], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Array(arr) => {
            assert!(!arr.is_empty(), "should have at least one process");
            if let Value::Record(rec) = &arr[0] {
                assert!(rec.contains_key("pid"));
                assert!(rec.contains_key("name"));
                assert!(rec.contains_key("memory_bytes"));
            }
        }
        other => panic!("htop_snapshot should return Array, got: {:?}", other),
    }
}

#[test]
fn test_nmon_snapshot_returns_record() {
    let result = bi_nmon_snapshot(vec![], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert!(rec.contains_key("cpu_count"));
            assert!(rec.contains_key("memory_total"));
            assert!(rec.contains_key("timestamp"));
        }
        other => panic!("nmon_snapshot should return Record, got: {:?}", other),
    }
}

#[test]
fn test_free_mem_returns_record() {
    let result = bi_free_mem(vec![], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert!(rec.contains_key("total"));
            assert!(rec.contains_key("used"));
            assert!(rec.contains_key("free"));
            assert!(rec.contains_key("use_percent"));
        }
        other => panic!("free_mem should return Record, got: {:?}", other),
    }
}

#[test]
fn test_dstat_info_returns_record() {
    let result = bi_dstat_info(vec![], None);
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), Value::Record(_)));
}
