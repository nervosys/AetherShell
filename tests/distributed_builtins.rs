//! Tests for distributed computing builtins

use aether_shell::builtins::call;
use aether_shell::env::Env;
use aether_shell::value::Value;
use std::collections::BTreeMap;

// ===================== Cluster Management Tests =====================

#[test]
fn test_cluster_create_default() {
    let mut env = Env::new();
    let result = call("cluster_create", vec![], &mut env).unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("name"));
        assert!(rec.contains_key("status"));
        assert_eq!(rec.get("status"), Some(&Value::Str("created".to_string())));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_cluster_create_named() {
    let mut env = Env::new();
    let result = call(
        "cluster_create",
        vec![Value::Str("my-cluster".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("name"), Some(&Value::Str("my-cluster".to_string())));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_cluster_alias() {
    let mut env = Env::new();
    let result = call("cluster", vec![Value::Str("test".to_string())], &mut env).unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("name"), Some(&Value::Str("test".to_string())));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_cluster_add_node() {
    let mut env = Env::new();
    let result = call(
        "cluster_add_node",
        vec![
            Value::Str("node-1".to_string()),
            Value::Str("192.168.1.10:8080".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("id"), Some(&Value::Str("node-1".to_string())));
        assert_eq!(
            rec.get("address"),
            Some(&Value::Str("192.168.1.10:8080".to_string()))
        );
        assert_eq!(rec.get("status"), Some(&Value::Str("online".to_string())));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_cluster_add_node_with_capabilities() {
    let mut env = Env::new();
    let result = call(
        "cluster_add_node",
        vec![
            Value::Str("gpu-node".to_string()),
            Value::Str("192.168.1.20:8080".to_string()),
            Value::Array(vec![
                Value::Str("gpu".to_string()),
                Value::Str("compute".to_string()),
            ]),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        if let Some(Value::Array(caps)) = rec.get("capabilities") {
            assert!(caps.contains(&Value::Str("gpu".to_string())));
            assert!(caps.contains(&Value::Str("compute".to_string())));
        } else {
            panic!("Expected capabilities array");
        }
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_add_node_alias() {
    let mut env = Env::new();
    let result = call(
        "add_node",
        vec![
            Value::Str("node-alias".to_string()),
            Value::Str("10.0.0.1:9000".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("id"), Some(&Value::Str("node-alias".to_string())));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_cluster_nodes_list() {
    let mut env = Env::new();

    // Add a node first
    call(
        "cluster_add_node",
        vec![
            Value::Str("list-test-node".to_string()),
            Value::Str("10.0.0.5:8080".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call("cluster_nodes", vec![], &mut env).unwrap();

    if let Value::Array(nodes) = result {
        assert!(!nodes.is_empty());
    } else {
        panic!("Expected Array, got {:?}", result);
    }
}

#[test]
fn test_nodes_alias() {
    let mut env = Env::new();
    let result = call("nodes", vec![], &mut env).unwrap();

    if let Value::Array(_) = result {
        // Success - got array
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_cluster_remove_node() {
    let mut env = Env::new();

    // Add node
    call(
        "cluster_add_node",
        vec![
            Value::Str("to-remove".to_string()),
            Value::Str("127.0.0.1:8080".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    // Remove node
    let result = call(
        "cluster_remove_node",
        vec![Value::Str("to-remove".to_string())],
        &mut env,
    )
    .unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_cluster_remove_nonexistent() {
    let mut env = Env::new();
    let result = call(
        "cluster_remove_node",
        vec![Value::Str("nonexistent-node".to_string())],
        &mut env,
    )
    .unwrap();

    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_cluster_status() {
    let mut env = Env::new();
    let result = call("cluster_status", vec![], &mut env).unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("total_nodes"));
        assert!(rec.contains_key("online_nodes"));
        assert!(rec.contains_key("total_jobs"));
        assert!(rec.contains_key("average_load"));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

// ===================== Job Management Tests =====================

#[test]
fn test_job_submit() {
    let mut env = Env::new();
    let result = call(
        "job_submit",
        vec![
            Value::Str("test-job".to_string()),
            Value::Str("echo hello".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("job_id"));
        assert_eq!(rec.get("status"), Some(&Value::Str("pending".to_string())));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_submit_alias() {
    let mut env = Env::new();
    let result = call(
        "submit",
        vec![
            Value::Str("alias-job".to_string()),
            Value::Str("ls -la".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("job_id"));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_job_submit_with_priority() {
    let mut env = Env::new();
    let result = call(
        "job_submit",
        vec![
            Value::Str("high-priority-job".to_string()),
            Value::Str("important task".to_string()),
            Value::Int(10),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("job_id"));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_job_list() {
    let mut env = Env::new();

    // Submit a job
    call(
        "job_submit",
        vec![
            Value::Str("list-test-job".to_string()),
            Value::Str("task".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call("job_list", vec![], &mut env).unwrap();

    if let Value::Array(jobs) = result {
        assert!(!jobs.is_empty());
    } else {
        panic!("Expected Array, got {:?}", result);
    }
}

#[test]
fn test_jobs_alias() {
    let mut env = Env::new();
    let result = call("jobs", vec![], &mut env).unwrap();

    if let Value::Array(_) = result {
        // Success
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_job_status() {
    let mut env = Env::new();

    // Submit a job
    let submit_result = call(
        "job_submit",
        vec![
            Value::Str("status-test".to_string()),
            Value::Str("task".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let job_id = if let Value::Record(rec) = submit_result {
        rec.get("job_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    let result = call("job_status", vec![job_id], &mut env).unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("status"));
        assert!(rec.contains_key("name"));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_job_cancel() {
    let mut env = Env::new();

    // Submit a job
    let submit_result = call(
        "job_submit",
        vec![
            Value::Str("cancel-test".to_string()),
            Value::Str("long task".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let job_id = if let Value::Record(rec) = submit_result {
        rec.get("job_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    let result = call("job_cancel", vec![job_id], &mut env).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_job_results_empty() {
    let mut env = Env::new();
    let result = call(
        "job_results",
        vec![Value::Str("nonexistent-job".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Array(results) = result {
        assert!(results.is_empty());
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_results_alias() {
    let mut env = Env::new();
    let result = call(
        "results",
        vec![Value::Str("some-job".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Array(_) = result {
        // Success
    } else {
        panic!("Expected Array");
    }
}

// ===================== Remote Execution Tests =====================

#[test]
fn test_remote_exec() {
    let mut env = Env::new();

    // First add a node
    call(
        "cluster_add_node",
        vec![
            Value::Str("exec-node".to_string()),
            Value::Str("10.0.0.1:8080".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call(
        "remote_exec",
        vec![
            Value::Str("exec-node".to_string()),
            Value::Str("ls -la".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("status"), Some(&Value::Str("executed".to_string())));
        assert!(rec.contains_key("output"));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_exec_remote_alias() {
    let mut env = Env::new();

    // Add node
    call(
        "cluster_add_node",
        vec![
            Value::Str("alias-exec-node".to_string()),
            Value::Str("10.0.0.2:8080".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call(
        "exec_remote",
        vec![
            Value::Str("alias-exec-node".to_string()),
            Value::Str("pwd".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("status"), Some(&Value::Str("executed".to_string())));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_remote_exec_node_not_found() {
    let mut env = Env::new();
    let result = call(
        "remote_exec",
        vec![
            Value::Str("nonexistent-node".to_string()),
            Value::Str("echo test".to_string()),
        ],
        &mut env,
    );

    assert!(result.is_err());
}

// ===================== Result Aggregation Tests =====================

#[test]
fn test_aggregate_concat() {
    let mut env = Env::new();
    let result = call(
        "aggregate_results",
        vec![
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            Value::Str("concat".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Array(arr) = result {
        assert_eq!(arr.len(), 3);
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_aggregate_sum() {
    let mut env = Env::new();
    let result = call(
        "aggregate_results",
        vec![
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            Value::Str("sum".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    assert_eq!(result, Value::Float(6.0));
}

#[test]
fn test_aggregate_count() {
    let mut env = Env::new();
    let result = call(
        "aggregate_results",
        vec![
            Value::Array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
            ]),
            Value::Str("count".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    assert_eq!(result, Value::Int(4));
}

#[test]
fn test_aggregate_first() {
    let mut env = Env::new();
    let result = call(
        "aggregate_results",
        vec![
            Value::Array(vec![
                Value::Str("a".to_string()),
                Value::Str("b".to_string()),
            ]),
            Value::Str("first".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    assert_eq!(result, Value::Str("a".to_string()));
}

#[test]
fn test_aggregate_last() {
    let mut env = Env::new();
    let result = call(
        "aggregate_results",
        vec![
            Value::Array(vec![
                Value::Str("a".to_string()),
                Value::Str("b".to_string()),
            ]),
            Value::Str("last".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    assert_eq!(result, Value::Str("b".to_string()));
}

#[test]
fn test_aggregate_alias() {
    let mut env = Env::new();
    let result = call(
        "aggregate",
        vec![
            Value::Array(vec![Value::Float(1.5), Value::Float(2.5)]),
            Value::Str("sum".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    assert_eq!(result, Value::Float(4.0));
}

#[test]
fn test_aggregate_unknown_strategy() {
    let mut env = Env::new();
    let result = call(
        "aggregate_results",
        vec![
            Value::Array(vec![Value::Int(1)]),
            Value::Str("unknown".to_string()),
        ],
        &mut env,
    );

    assert!(result.is_err());
}
