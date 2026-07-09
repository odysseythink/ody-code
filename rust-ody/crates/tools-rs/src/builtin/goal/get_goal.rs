use serde_json::Value;
use std::sync::Arc;

use super::GoalStore;
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

pub struct GetGoalTool {
    store: Arc<dyn GoalStore>,
}

impl GetGoalTool {
    pub fn new(store: Arc<dyn GoalStore>) -> Self {
        Self { store }
    }
}

impl BuiltinTool for GetGoalTool {
    fn name(&self) -> &str {
        "GetGoal"
    }
    fn description(&self) -> &str {
        "Returns the current goal snapshot."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let store = Arc::clone(&self.store);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Reading the current goal".into(),
            approval_rule: "GetGoal".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                Box::pin(async move {
                    let result = store.get_goal();
                    ExecutableToolResult::ok_text(
                        serde_json::to_string(&result).unwrap_or_default(),
                    )
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::{
        GoalActor, GoalBudgetReport, GoalSnapshot, GoalStatus, MockGoalStore,
    };
    use std::sync::Arc;

    #[test]
    fn returns_null_when_no_goal() {
        let store = Arc::new(MockGoalStore::new(None));
        let tool = GetGoalTool::new(store);
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("\"goal\":null"));
    }

    #[test]
    fn returns_goal_when_present() {
        let snapshot = GoalSnapshot {
            goal_id: "g1".into(),
            objective: "test".into(),
            completion_criterion: None,
            status: GoalStatus::Active,
            created_at: "now".into(),
            updated_at: "now".into(),
            started_by: GoalActor::User,
            updated_by: GoalActor::User,
            turns_used: 0,
            tokens_used: 0,
            wall_clock_ms: 0,
            budget: GoalBudgetReport {
                token_budget: None,
                turn_budget: None,
                wall_clock_budget_ms: None,
                remaining_tokens: None,
                remaining_turns: None,
                remaining_wall_clock_ms: None,
                token_budget_reached: false,
                turn_budget_reached: false,
                wall_clock_budget_reached: false,
                over_budget: false,
            },
            terminal_reason: None,
        };
        let store = Arc::new(MockGoalStore::new(Some(snapshot)));
        let tool = GetGoalTool::new(store);
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("\"goalId\":\"g1\""));
    }
}
