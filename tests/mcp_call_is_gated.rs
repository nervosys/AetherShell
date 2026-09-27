//! mcp_call runs an OS program from the tool catalogue with caller-supplied
//! arguments. It was classified Pure, and Pure calls skip the gate, so in
//! agent mode it did what the gated builtins are refused: `cp` to a path
//! outside the workspace was E_OUTSIDE_WORKSPACE, and `mcp_call("cp", ...)`
//! made the same copy. Its own process, since it sets the mode.

use aethershell::builtins::call;
use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;

#[test]
fn an_agent_cannot_run_a_catalogue_program_through_mcp_call() {
    let root = std::env::temp_dir().join(format!("ae-mcp-gate-{}", std::process::id()));
    let ws = root.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let src = root.join("outside.txt");
    let dst = root.join("copied.txt");
    std::fs::write(&src, "secret").unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &ws);
    std::env::remove_var("AETHER_POLICY");

    let mut args = BTreeMap::new();
    args.insert("source".to_string(), Value::Str(src.display().to_string()));
    args.insert(
        "destination".to_string(),
        Value::Str(dst.display().to_string()),
    );
    let r = call(
        "mcp_call",
        vec![Value::Str("cp".to_string()), Value::Record(args)],
        &mut Env::new(),
    );
    let copied = dst.exists();
    let _ = std::fs::remove_dir_all(&root);
    let e = r.expect_err("mcp_call ran in agent mode without approval");
    let msg = e.to_string();
    assert!(
        msg.contains("E_NEEDS_APPROVAL") || msg.contains("E_POLICY_DENY"),
        "refused, but not by the gate: {msg}"
    );
    assert!(!copied, "the copy happened");
}
