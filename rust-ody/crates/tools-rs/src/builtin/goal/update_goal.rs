use serde_json::Value;
use std::sync::Arc;

use super::{build_goal_completion_message, GoalActor, GoalStore};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

type AppendReminderFn = Arc<dyn Fn(String) + Send + Sync>;

pub struct UpdateGoalTool {
    store: Arc<dyn GoalStore>,
    append_system_reminder: Option<AppendReminderFn>,
}

impl UpdateGoalTool {
    pub fn new(
        store: Arc<dyn GoalStore>,
        append_system_reminder: Option<AppendReminderFn>,
    ) -> Self {
        Self {
            store,
            append_system_reminder,
        }
    }
}

impl BuiltinTool for UpdateGoalTool {
    fn name(&self) -> &str {
        "UpdateGoal"
    }
    fn description(&self) -> &str {
        "Updates the current goal status (active/complete/paused/blocked)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["active", "complete", "paused", "blocked"] }
            },
            "required": ["status"],
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let status = args["status"].as_str().unwrap_or("active").to_string();
        let store = Arc::clone(&self.store);
        let reminder = self.append_system_reminder.clone();
        let description = format!("Setting goal status: {}", status);
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "UpdateGoal".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let status = status.clone();
                let reminder = reminder.clone();
                Box::pin(async move {
                    match status.as_str() {
                        "active" => match store.resume_goal(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal resumed.".into()),
                            Err(e) => {
                                ExecutableToolResult::error_text(e.to_string(), e.to_string())
                            }
                        },
                        "complete" => match store.mark_complete(GoalActor::Model) {
                            Ok(Some(completed)) => {
                                if let Some(r) = &reminder {
                                    r(build_goal_completion_message(&completed));
                                }
                                ExecutableToolResult::ok_text("Goal marked complete.".into())
                            }
                            Ok(None) => {
                                ExecutableToolResult::ok_text("Goal marked complete.".into())
                            }
                            Err(e) => {
                                ExecutableToolResult::error_text(e.to_string(), e.to_string())
                            }
                        },
                        "blocked" => match store.mark_blocked(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal marked blocked.".into()),
                            Err(e) => {
                                ExecutableToolResult::error_text(e.to_string(), e.to_string())
                            }
                        },
                        "paused" => match store.pause_goal(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal paused.".into()),
                            Err(e) => {
                                ExecutableToolResult::error_text(e.to_string(), e.to_string())
                            }
                        },
                        _ => ExecutableToolResult::error_text(
                            "Unknown status".into(),
                            "Unknown status".into(),
                        ),
                    }
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
    use std::sync::{Arc, Mutex};

    fn make_active_goal(turns: u64, tokens: u64, wall_ms: u64) -> GoalSnapshot {
        GoalSnapshot {
            goal_id: "g1".into(),
            objective: "test".into(),
            completion_criterion: None,
            status: GoalStatus::Active,
            created_at: "now".into(),
            updated_at: "now".into(),
            started_by: GoalActor::User,
            updated_by: GoalActor::User,
            turns_used: turns,
            tokens_used: tokens,
            wall_clock_ms: wall_ms,
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
        }
    }

    #[test]
    fn updates_to_complete_appends_reminder() {
        let store = Arc::new(MockGoalStore::new(Some(make_active_goal(5, 1000, 30000))));
        let reminders: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let r = Arc::clone(&reminders);
        let reminder_fn: AppendReminderFn = Arc::new(move |msg| {
            r.lock().unwrap().push(msg);
        });
        let tool = UpdateGoalTool::new(store, Some(reminder_fn));
        let args = serde_json::json!({"status": "complete"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Goal marked complete"));
        let reminders = reminders.lock().unwrap();
        assert_eq!(reminders.len(), 1);
        assert!(reminders[0].contains("✓ Goal complete"));
    }

    #[test]
    fn update_to_paused() {
        let store = Arc::new(MockGoalStore::new(Some(make_active_goal(0, 0, 0))));
        let tool = UpdateGoalTool::new(store, None);
        let args = serde_json::json!({"status": "paused"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Goal paused"));
    }
}
