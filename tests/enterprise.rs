// tests/enterprise.rs - Enterprise features tests (RBAC, Audit, SSO, Compliance, Fine-tuning)
use aethershell::builtins::call;
use aethershell::env::Env;
use aethershell::value::Value;

// ===================== RBAC Tests =====================

#[test]
fn test_role_create() {
    let mut env = Env::new();
    let result = call(
        "role_create",
        vec![
            Value::Str("admin".to_string()),
        ],
        &mut env,
    )
    .unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("admin"), "Should create admin role: {}", result_str);
    assert!(result_str.contains("created"), "Should show created status: {}", result_str);
}

#[test]
fn test_role_create_minimal() {
    let mut env = Env::new();
    let result = call(
        "role_create",
        vec![Value::Str("basic".to_string())],
        &mut env,
    )
    .unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("basic"), "Should create basic role: {}", result_str);
}

#[test]
fn test_role_delete() {
    let mut env = Env::new();
    call(
        "role_create",
        vec![Value::Str("temp_role".to_string())],
        &mut env,
    )
    .unwrap();
    
    let result = call(
        "role_delete",
        vec![Value::Str("temp_role".to_string())],
        &mut env,
    )
    .unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("deleted"), "Should show deleted status: {}", result_str);
}

#[test]
fn test_role_grant() {
    let mut env = Env::new();
    // First create the role
    call(
        "role_create",
        vec![Value::Str("editor".to_string())],
        &mut env,
    )
    .unwrap();
    
    // Then grant it to a user (role_grant takes user, role)
    let result = call(
        "role_grant",
        vec![
            Value::Str("alice".to_string()),  // user
            Value::Str("editor".to_string()), // role
        ],
        &mut env,
    )
    .unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("granted"), "Should grant role: {}", result_str);
}

#[test]
fn test_roles_list() {
    let mut env = Env::new();
    call(
        "role_create",
        vec![Value::Str("list_role1".to_string())],
        &mut env,
    )
    .unwrap();
    
    let result = call("roles_list", vec![], &mut env).unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("list_role1"), "Should list created roles: {}", result_str);
}

// ===================== Audit Tests =====================

#[test]
fn test_audit_log_basic() {
    let mut env = Env::new();
    let result = call(
        "audit_log",
        vec![
            Value::Str("login".to_string()),
            Value::Str("auth_service".to_string()),
        ],
        &mut env,
    )
    .unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("logged"), "Should log action: {}", result_str);
    assert!(result_str.contains("audit_"), "Should return audit ID: {}", result_str);
}

// ===================== SSO Tests =====================

#[test]
fn test_sso_init() {
    let mut env = Env::new();
    // sso_init(provider, client_id, issuer_url)
    let result = call(
        "sso_init",
        vec![
            Value::Str("oauth2".to_string()),
            Value::Str("my_app".to_string()),
            Value::Str("https://auth.example.com".to_string()),
        ],
        &mut env,
    )
    .unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("oauth2"), "Should initialize OAuth2 provider: {}", result_str);
    assert!(result_str.contains("initialized"), "Should show initialized: {}", result_str);
}

// ===================== Compliance Tests =====================

#[test]
fn test_compliance_check_gdpr() {
    let mut env = Env::new();
    let result = call("compliance_check", vec![Value::Str("GDPR".to_string())], &mut env).unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("GDPR"), "Should check GDPR: {}", result_str);
}

// ===================== Fine-tuning Tests =====================

#[test]
fn test_finetune_start() {
    let mut env = Env::new();
    let result = call(
        "finetune_start",
        vec![
            Value::Str("gpt-4o-mini".to_string()),
            Value::Str("training_data.jsonl".to_string()),
        ],
        &mut env,
    )
    .unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("job_id"), "Should return job_id: {}", result_str);
}

#[test]
fn test_finetune_list() {
    let mut env = Env::new();
    call(
        "finetune_start",
        vec![
            Value::Str("model1".to_string()),
            Value::Str("data1.jsonl".to_string()),
        ],
        &mut env,
    )
    .unwrap();
    
    let result = call("finetune_list", vec![], &mut env).unwrap();
    let result_str = format!("{}", result);
    assert!(result_str.contains("model1") || result_str.contains("ft_"), 
            "Should list jobs: {}", result_str);
}
