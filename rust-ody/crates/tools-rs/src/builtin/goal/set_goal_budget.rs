use serde_json::Value;
use std::sync::Arc;

use super::{GoalActor, GoalBudgetLimits, GoalStore};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

const MIN_REASONABLE_TIME_BUDGET_MS: i64 = 1_000;
const MAX_REASONABLE_TIME_BUDGET_MS: i64 = 24 * 60 * 60 * 1000;

pub struct SetGoalBudgetTool {
    store: Arc<dyn GoalStore>,
}

impl SetGoalBudgetTool {
    pub fn new(store: Arc<dyn GoalStore>) -> Self {
        Self { store }
    }
}

impl BuiltinTool for SetGoalBudgetTool {
    fn name(&self) -> &str {
        "SetGoalBudget"
    }
    fn description(&self) -> &str {
        "Record a hard runtime limit for the current goal."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "value": { "type": "number", "description": "Positive budget value." },
                "unit": { "type": "string", "enum": ["turns", "tokens", "milliseconds", "seconds", "minutes", "hours"] }
            },
            "required": ["value", "unit"],
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let value = args["value"].as_f64().unwrap_or(0.0);
        let unit = args["unit"].as_str().unwrap_or("").to_string();
        let store = Arc::clone(&self.store);
        let description = format!("Setting goal budget: {} {}", value, unit);
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "SetGoalBudget".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let value = value;
                let unit = unit.clone();
                Box::pin(async move {
                    let limits = match budget_limits_from_input(value, &unit) {
                        Ok(Some(l)) => l,
                        Ok(None) => {
                            return ExecutableToolResult::ok_text(format!(
                                "Goal budget not set: {} {} is not a reasonable goal budget.",
                                value, unit
                            ));
                        }
                        Err(e) => {
                            return ExecutableToolResult::error_text(e.to_string(), e.to_string());
                        }
                    };
                    match store.set_budget_limits(limits, GoalActor::Model) {
                        Ok(_) => ExecutableToolResult::ok_text(format!(
                            "Goal budget set: {}.",
                            format_budget(value, &unit)
                        )),
                        Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                    }
                })
            }),
        })
    }
}

fn budget_limits_from_input(value: f64, unit: &str) -> Result<Option<GoalBudgetLimits>, ToolError> {
    match unit {
        "turns" => Ok(Some(GoalBudgetLimits {
            turn_budget: Some(value as u64),
            token_budget: None,
            wall_clock_budget_ms: None,
        })),
        "tokens" => Ok(Some(GoalBudgetLimits {
            token_budget: Some(value as u64),
            turn_budget: None,
            wall_clock_budget_ms: None,
        })),
        _ => {
            let ms = to_milliseconds(value, unit);
            if ms < MIN_REASONABLE_TIME_BUDGET_MS || ms > MAX_REASONABLE_TIME_BUDGET_MS {
                return Ok(None);
            }
            Ok(Some(GoalBudgetLimits {
                wall_clock_budget_ms: Some(ms as u64),
                token_budget: None,
                turn_budget: None,
            }))
        }
    }
}

fn to_milliseconds(value: f64, unit: &str) -> i64 {
    match unit {
        "milliseconds" => value as i64,
        "seconds" => (value * 1000.0) as i64,
        "minutes" => (value * 60.0 * 1000.0) as i64,
        "hours" => (value * 60.0 * 60.0 * 1000.0) as i64,
        _ => value as i64,
    }
}

fn format_budget(value: f64, unit: &str) -> String {
    let singular = unit.trim_end_matches('s');
    if (value - 1.0).abs() < f64::EPSILON {
        format!("{} {}", value as i64, singular)
    } else {
        format!("{} {}", value as i64, unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::{
        GoalActor, GoalBudgetReport, GoalSnapshot, GoalStatus, MockGoalStore,
    };
    use std::sync::Arc;

    fn make_active_goal() -> GoalSnapshot {
        GoalSnapshot {
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
        }
    }

    #[test]
    fn sets_token_budget() {
        let store = Arc::new(MockGoalStore::new(Some(make_active_goal())));
        let tool = SetGoalBudgetTool::new(store);
        let args = serde_json::json!({"value": 5000, "unit": "tokens"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Goal budget set"));
    }

    #[test]
    fn rejects_unreasonable_time() {
        let store = Arc::new(MockGoalStore::new(Some(make_active_goal())));
        let tool = SetGoalBudgetTool::new(store);
        let args = serde_json::json!({"value": 500, "unit": "milliseconds"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("not a reasonable"));
    }
}
