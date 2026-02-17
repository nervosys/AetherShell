//! Tests for proactive monitoring and alerting builtins

use aethershell::builtins::*;
use aethershell::value::Value;

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
            assert!(rec.contains_key("memory_percent"), "should have memory_percent");
            assert!(rec.contains_key("disk_percent"), "should have disk_percent");
            assert!(rec.contains_key("process_count"), "should have process_count");
            assert!(rec.contains_key("alerts"), "should have alerts array");
            assert!(rec.contains_key("timestamp"), "should have timestamp");
            // Status should be one of healthy, warning, critical
            if let Some(Value::Str(s)) = rec.get("status") {
                assert!(s == "healthy" || s == "warning" || s == "critical",
                    "status should be healthy/warning/critical, got: {}", s);
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
            assert_eq!(rec.get("severity"), Some(&Value::Str("critical".to_string())));
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
    let result = bi_alert_create(
        vec![Value::Str("test_alert".to_string()), Value::Str("memory".to_string())],
        None,
    );
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert_eq!(rec.get("condition"), Some(&Value::Str(">".to_string())));
            assert_eq!(rec.get("threshold"), Some(&Value::Float(80.0)));
            assert_eq!(rec.get("severity"), Some(&Value::Str("warning".to_string())));
        }
        _ => panic!("should return Record"),
    }
}

// ============================================================================
// Alert List & History
// ============================================================================

#[test]
fn test_alert_list_returns_array() {
    let result = bi_alert_list(vec![], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Array(arr) => {
            assert!(!arr.is_empty(), "should return default alert rules");
            if let Value::Record(rec) = &arr[0] {
                assert!(rec.contains_key("name"));
                assert!(rec.contains_key("metric"));
                assert!(rec.contains_key("threshold"));
                assert!(rec.contains_key("triggered"));
            }
        }
        other => panic!("alert_list should return Array, got: {:?}", other),
    }
}

#[test]
fn test_alert_history_returns_array() {
    let result = bi_alert_history(vec![], None);
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), Value::Array(_)));
}

// ============================================================================
// Alert Remove
// ============================================================================

#[test]
fn test_alert_remove_returns_confirmation() {
    let result = bi_alert_remove(vec![Value::Str("alert_123".to_string())], None);
    assert!(result.is_ok());
    match result.unwrap() {
        Value::Record(rec) => {
            assert_eq!(rec.get("id"), Some(&Value::Str("alert_123".to_string())));
            assert_eq!(rec.get("removed"), Some(&Value::Bool(true)));
        }
        other => panic!("alert_remove should return Record, got: {:?}", other),
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
