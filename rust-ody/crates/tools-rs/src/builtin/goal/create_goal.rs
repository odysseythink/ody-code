use serde_json::Value;
use std::sync::Arc;

use super::{CreateGoalInput, GoalActor, GoalStore};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

pub struct CreateGoalTool {
    store: Arc<dyn GoalStore>,
}

impl CreateGoalTool {
    pub fn new(store: Arc<dyn GoalStore>) -> Self {
        Self { store }
    }
}

impl BuiltinTool for CreateGoalTool {
    fn name(&self) -> &str {
        "CreateGoal"
    }
    fn description(&self) -> &str {
        "Create a durable, structured goal that the runtime will pursue across multiple turns."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "objective": { "type": "string", "description": "The objective to pursue." },
                "completionCriterion": { "type": "string", "description": "How to verify completion." },
                "replace": { "type": "boolean", "description": "Replace existing goal." }
            },
            "required": ["objective"],
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let objective = args["objective"].as_str().unwrap_or("").to_string();
        let completion_criterion = args["completionCriterion"].as_str().map(|s| s.to_string());
        let replace = args["replace"].as_bool();
        let input = CreateGoalInput {
            objective,
            completion_criterion,
            replace,
            actor: Some(GoalActor::Model),
        };
        let store = Arc::clone(&self.store);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Creating a goal".into(),
            approval_rule: "CreateGoal".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let input = input.clone();
                Box::pin(async move {
                    match store.create_goal(input) {
                        Ok(snapshot) => {
                            let json = serde_json::json!({"goal": snapshot});
                            ExecutableToolResult::ok_text(
                                serde_json::to_string(&json).unwrap_or_default(),
                            )
                        }
                        Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                    }
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::MockGoalStore;
    use std::sync::Arc;

    #[test]
    fn creates_goal_successfully() {
        let store = Arc::new(MockGoalStore::new(None));
        let tool = CreateGoalTool::new(store);
        let args = serde_json::json!({"objective": "Fix all bugs"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("Fix all bugs"));
    }

    #[test]
    fn rejects_empty_objective() {
        let store = Arc::new(MockGoalStore::new(None));
        let tool = CreateGoalTool::new(store);
        let args = serde_json::json!({"objective": ""});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(result.is_error);
    }
}
