// Minimal agent skeleton

use anyhow::Result;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub tool: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub goal: String,
    pub steps: Vec<Step>,
}

pub fn plan(goal: &str) -> Plan {
    // trivial planner for the skeleton
    Plan {
        goal: goal.to_string(),
        steps: vec![],
    }
}

pub fn execute(_plan: &Plan) -> Result<()> {
    // TODO: wire to builtins with allowlist
    Ok(())
}
