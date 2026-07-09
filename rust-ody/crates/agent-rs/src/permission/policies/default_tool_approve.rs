use std::collections::HashSet;

use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

/// Sync with TS `DEFAULT_APPROVE_TOOLS`
pub fn default_approve_tools_set() -> HashSet<&'static str> {
    [
        "Read",
        "Grep",
        "Glob",
        "ReadMediaFile",
        "SetTodoList",
        "TodoList",
        "TaskList",
        "TaskOutput",
        "CronList",
        "WebSearch",
        "FetchURL",
        "Agent",
        "AskUserQuestion",
        "Skill",
        "GetGoal",
        "SetGoalBudget",
        "UpdateGoal",
        "AppendBuilderProfile",
    ]
    .iter()
    .cloned()
    .collect()
}

pub struct DefaultToolApprove;

impl PermissionPolicy for DefaultToolApprove {
    fn name(&self) -> &str {
        "default-tool-approve"
    }

    fn evaluate(
        &self,
        context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        if default_approve_tools_set().contains(context.tool_call.name.as_str()) {
            Some(PermissionPolicyResolution::Approve {
                reason: None,
                execution_metadata: None,
            })
        } else {
            None
        }
    }
}
