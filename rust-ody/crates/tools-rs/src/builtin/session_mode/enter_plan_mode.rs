use serde_json::Value;
use std::sync::Arc;

use crate::builtin::session_mode::{
    planning::plan_mode_entry_message, SessionModeKind, SessionModeProvider,
};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

pub struct EnterPlanModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnterPlanModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "EnterPlanMode"
    }

    fn description(&self) -> &str {
        "Enter implementation-planning mode. Produces a step-by-step plan file."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Requesting to enter plan mode".into(),
            approval_rule: "EnterPlanMode".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    if provider.is_session_mode_active() {
                        let active = match provider.session_mode_kind() {
                            Some(SessionModeKind::Design) => "Design",
                            Some(SessionModeKind::OfficeHours) => "Office-hours",
                            Some(SessionModeKind::GameDesign) => "Game-design",
                            _ => "Plan",
                        };
                        let exit_tool = match provider.session_mode_kind() {
                            Some(SessionModeKind::Design) => "ExitDesignMode",
                            Some(SessionModeKind::OfficeHours) => "ExitOfficeHoursMode",
                            Some(SessionModeKind::GameDesign) => "ExitGameDesignMode",
                            _ => "ExitPlanMode",
                        };
                        return ExecutableToolResult::error_text(
                            format!(
                                "{} mode is already active. Use {} when you are ready to exit {} mode; do not try to enter another mode on top of it.",
                                active,
                                exit_tool,
                                active.to_lowercase()
                            ),
                            "session mode already active".into(),
                        );
                    }

                    if let Err(e) = provider.enter_session_mode(SessionModeKind::Plan).await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to enter plan mode: {}", e),
                            "enter failed".into(),
                        );
                    }

                    provider.telemetry().track(
                        "plan_enter_resolved",
                        std::collections::HashMap::from([(
                            "outcome".into(),
                            "auto_approved".into(),
                        )]),
                    );

                    let msg = plan_mode_entry_message(provider.session_mode_file_path().as_deref());
                    ExecutableToolResult::ok_text(msg)
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::tests::MockSessionModeProvider;

    #[tokio::test]
    async fn enter_plan_mode_succeeds_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = EnterPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("Plan mode is now active"));
        assert!(provider
            .entered
            .lock()
            .unwrap()
            .contains(&SessionModeKind::Plan));
    }

    #[tokio::test]
    async fn enter_plan_mode_fails_when_already_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Design));
        let tool = EnterPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(result.is_error);
        assert!(result.to_text().contains("Design mode is already active"));
        assert!(result.to_text().contains("ExitDesignMode"));
    }
}
