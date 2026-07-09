use serde_json::Value;
use std::sync::Arc;

use crate::builtin::session_mode::planning::{
    declared_option_label, is_via_approval, selected_approach_prefix, selected_label_of,
    ExitModeOption,
};
use crate::builtin::session_mode::SessionModeProvider;
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExitPlanModeInput {
    #[serde(default)]
    pub options: Vec<ExitModeOption>,
}

pub struct ExitPlanModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitPlanModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "ExitPlanMode"
    }

    fn description(&self) -> &str {
        "Present the finalized plan to the user and exit plan mode."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "options": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 3,
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string", "minLength": 1, "maxLength": 80 },
                            "description": { "type": "string" }
                        },
                        "required": ["label"]
                    },
                    "description": "When the plan contains multiple alternative approaches, list them here so the user can choose which one to execute."
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: ExitPlanModeInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let provider = Arc::clone(&self.provider);
        let display = build_plan_review_display(&*provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Presenting plan and exiting plan mode".into(),
            approval_rule: "ExitPlanMode".into(),
            matches_rule: None,
            display,
            execute: Box::new(move |ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move { execute_exit_plan_mode(provider, input, ctx).await })
            }),
        })
    }
}

fn build_plan_review_display(provider: &dyn SessionModeProvider) -> Option<Value> {
    if !provider.is_session_mode_active() {
        return None;
    }
    let path = provider.session_mode_file_path()?;
    let content = match read_session_mode_file_sync(provider.kaos().as_ref(), &path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let display = serde_json::json!({
        "kind": "plan_review",
        "plan": trimmed,
        "path": path,
    });
    Some(display)
}

fn read_session_mode_file_sync(
    kaos: &dyn crate::builtin::session_mode::SessionModeContext,
    path: &str,
) -> anyhow::Result<String> {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(kaos.read_text(path)))
}

async fn execute_exit_plan_mode(
    provider: Arc<dyn SessionModeProvider>,
    input: ExitPlanModeInput,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if !provider.is_session_mode_active() {
        return ExecutableToolResult::error_text(
            "ExitPlanMode can only be called while plan mode is active. Use EnterPlanMode (or /plan) first.".into(),
            "not in plan mode".into(),
        );
    }

    let (plan, path) = match resolve_plan(&*provider).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let option_label = declared_option_label(
        Some(&input.options),
        selected_label_of(ctx.metadata.as_ref()).as_deref(),
    );

    let metadata = ctx.metadata.as_ref();
    if !is_via_approval(metadata) {
        provider.telemetry().track(
            "plan_submitted",
            std::collections::HashMap::from([(
                "has_options".into(),
                serde_json::Value::Bool(input.options.len() >= 2),
            )]),
        );
    }

    if let Err(e) = provider.handoff_to("normal", option_label.clone()).await {
        return ExecutableToolResult::error_text(
            format!("Failed to exit plan mode: {}", e),
            "handoff failed".into(),
        );
    }

    if is_via_approval(metadata) {
        let raw_label = selected_label_of(metadata);
        let props = if let Some(l) = raw_label {
            std::collections::HashMap::from([
                ("outcome".into(), "approved".into()),
                ("chosen_option".into(), l.into()),
            ])
        } else {
            std::collections::HashMap::from([("outcome".into(), "approved".into())])
        };
        provider.telemetry().track("plan_resolved", props);
    } else {
        provider.telemetry().track(
            "plan_resolved",
            std::collections::HashMap::from([("outcome".into(), "auto_approved".into())]),
        );
    }

    let path_line = path
        .as_ref()
        .map(|p| format!("Plan saved to: {}\n\n", p))
        .unwrap_or_default();

    let output = format!(
        "{}Exited plan mode. {}Plan mode deactivated. The approved plan has been handed off to the main conversation context.\n\n## Approved Plan:\n{}\n\nSTOP — do NOT begin executing now. This turn ends here. The user will start implementation themselves — the plan is now available in their main conversation context.",
        selected_approach_prefix(option_label.as_deref()),
        path_line,
        plan
    );

    ExecutableToolResult {
        output: ExecutableToolOutput::Text(output),
        message: None,
        is_error: false,
        stop_turn: Some(true),
    }
}

async fn resolve_plan(
    provider: &dyn SessionModeProvider,
) -> Result<(String, Option<String>), ExecutableToolResult> {
    let path = provider.session_mode_file_path();
    let content = match path.as_ref() {
        Some(p) => match provider.kaos().read_text(p).await {
            Ok(c) => c,
            Err(e) => {
                return Err(ExecutableToolResult::error_text(
                    format!("Failed to read plan file: {}", e),
                    "read failed".into(),
                ));
            }
        },
        None => String::new(),
    };

    if content.trim().is_empty() {
        return Err(ExecutableToolResult::error_text(
            match path {
                Some(p) => format!("No plan file found. Write your plan to {} first, then call ExitPlanMode.", p),
                None => "No plan file found. Write the plan to the current plan file first, then call ExitPlanMode.".into(),
            },
            "empty plan".into(),
        ));
    }

    Ok((content, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::tests::MockSessionModeProvider;

    #[tokio::test(flavor = "multi_thread")]
    async fn exit_plan_mode_hands_off_to_normal() {
        let provider = Arc::new(MockSessionModeProvider::plan_mode_with_content(
            "## Plan\n\nDo X.",
        ));
        let tool = ExitPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        assert!(exec.display.is_some());
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert_eq!(result.stop_turn, Some(true));
        let text = result.to_text();
        assert!(text.contains("Plan mode deactivated"));
        assert!(provider
            .handed_off_to
            .lock()
            .unwrap()
            .contains(&("normal".to_string(), None)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exit_plan_mode_preserves_selected_label() {
        let provider = Arc::new(MockSessionModeProvider::plan_mode_with_content(
            "## Plan\n\nDo X.",
        ));
        let tool = ExitPlanModeTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({
                "options": [{"label": "Fast", "description": ""}]
            }))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(provider
            .handed_off_to
            .lock()
            .unwrap()
            .contains(&("normal".to_string(), None)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exit_plan_mode_errors_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = ExitPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(result.is_error);
        assert!(result
            .to_text()
            .contains("ExitPlanMode can only be called while plan mode is active"));
    }
}
