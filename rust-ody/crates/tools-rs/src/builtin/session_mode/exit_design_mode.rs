use serde_json::Value;
use std::sync::Arc;

use crate::builtin::session_mode::planning::{
    declared_option_label, selected_approach_prefix, selected_label_of, ExitModeOption,
};
use crate::builtin::session_mode::SessionModeProvider;
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExitDesignModeInput {
    #[serde(default)]
    pub options: Vec<ExitModeOption>,
}

pub struct ExitDesignModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitDesignModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitDesignModeTool {
    fn name(&self) -> &str {
        "ExitDesignMode"
    }

    fn description(&self) -> &str {
        "Present the finalized design document to the user and exit design mode."
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
                    "description": "When the design presents multiple alternative directions, list them here so the user can choose which one to pursue."
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: ExitDesignModeInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let provider = Arc::clone(&self.provider);

        // If active, run completeness check before building the review display.
        if provider.is_session_mode_active() {
            if let Some(path) = provider.session_mode_file_path() {
                let content = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(provider.kaos().read_text(&path))
                });
                if let Ok(content) = content {
                    let missing = find_missing_design_sections(&content);
                    if !missing.is_empty() {
                        let list = missing
                            .iter()
                            .map(|m| format!("- {}", m))
                            .collect::<Vec<_>>()
                            .join("\n");
                        return Ok(ToolExecution {
                            accesses: Default::default(),
                            description: "Design is incomplete".into(),
                            approval_rule: "ExitDesignMode".into(),
                            matches_rule: None,
                            display: None,
                            execute: Box::new(move |_ctx| {
                                let list = list.clone();
                                Box::pin(async move {
                                    ExecutableToolResult::error_text(
                                        format!("Design is incomplete. Missing:\n{}\n\nPlease add the missing sections to the design file, then call ExitDesignMode again.", list),
                                        "incomplete design".into(),
                                    )
                                })
                            }),
                        });
                    }
                }
            }
        }

        let display = build_design_review_display(&*provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Presenting design and exiting design mode".into(),
            approval_rule: "ExitDesignMode".into(),
            matches_rule: None,
            display,
            execute: Box::new(move |ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move { execute_exit_design_mode(provider, input, ctx).await })
            }),
        })
    }
}

pub fn find_missing_design_sections(content: &str) -> Vec<String> {
    let mut missing = Vec::new();
    let trimmed = content.trim();

    if trimmed.len() < 300 {
        missing.push("sufficient content (design appears incomplete or empty)".into());
    }

    let heading_count =
        trimmed.matches("\n## ").count() + if trimmed.starts_with("## ") { 1 } else { 0 };
    if heading_count < 3 {
        missing.push(format!(
            "at least 3 design sections (found {})",
            heading_count
        ));
    }

    let checks: Vec<(&str, regex::Regex)> = vec![
        ("Scope or Scope In/Out section", regex::Regex::new(r"(?im)^#{1,3}\s+(scope|in/out|范围|scope\s+in)\b").unwrap()),
        ("Architecture or Design section", regex::Regex::new(r"(?im)^#{1,3}\s+(architecture|design|approach|overview|架构|设计方案)\b").unwrap()),
        ("Data Models section", regex::Regex::new(r"(?im)^#{1,3}\s+(data\s*models?|数据模型|models?|data\s+&?\s*state)\b").unwrap()),
        ("Algorithms section", regex::Regex::new(r"(?im)^#{1,3}\s+(algorithms?|算法|pseudocode|implementation\s+notes?)\b").unwrap()),
        ("Error Handling section", regex::Regex::new(r"(?im)^#{1,3}\s+(error\s*handling|错误处理|errors?|degradation|failure\s+scenarios?)\b").unwrap()),
        ("Self-Review section", regex::Regex::new(r"(?im)^#{1,3}\s+(self[- ]?review|自检|review|audit)\b").unwrap()),
        ("User Approval", regex::Regex::new(r"(?im)^#{1,3}\s+(user\s+(final\s+)?approval|用户批准|批准状态|approved?)\b").unwrap()),
        ("Reuse Analysis section", regex::Regex::new(r"(?im)^#{1,3}\s+(reuse\s+analysis|复用分析|component\s+reuse|existing\s+components?)\b").unwrap()),
    ];

    for (name, re) in checks {
        if !re.is_match(trimmed) {
            missing.push(name.into());
        }
    }

    missing
}

fn build_design_review_display(provider: &dyn SessionModeProvider) -> Option<Value> {
    if !provider.is_session_mode_active() {
        return None;
    }
    let path = provider.session_mode_file_path()?;
    let content = read_session_mode_file_sync(provider.kaos().as_ref(), &path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "kind": "plan_review",
        "plan": trimmed,
        "path": path,
    }))
}

fn read_session_mode_file_sync(
    kaos: &dyn crate::builtin::session_mode::SessionModeContext,
    path: &str,
) -> anyhow::Result<String> {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(kaos.read_text(path)))
}

async fn execute_exit_design_mode(
    provider: Arc<dyn SessionModeProvider>,
    input: ExitDesignModeInput,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if !provider.is_session_mode_active() {
        return ExecutableToolResult::error_text(
            "ExitDesignMode can only be called while design mode is active. Use EnterDesignMode (or /design) first.".into(),
            "not in design mode".into(),
        );
    }

    let path = provider.session_mode_file_path();
    if path.is_none() {
        return ExecutableToolResult::error_text(
            "No design file found. Write the design to the current design file first, then call ExitDesignMode.".into(),
            "no design file".into(),
        );
    }

    let option_label = declared_option_label(
        Some(&input.options),
        selected_label_of(ctx.metadata.as_ref()).as_deref(),
    );

    if let Err(e) = provider.handoff_to("plan", option_label.clone()).await {
        return ExecutableToolResult::error_text(
            format!("Failed to exit design mode: {}", e),
            "handoff failed".into(),
        );
    }

    let saved_to = path
        .as_ref()
        .map(|p| format!("Design saved to: {}\n\n", p))
        .unwrap_or_default();
    let output = format!(
        "{}Design mode deactivated. Now in plan mode.\n\n{}Create a concrete, step-by-step implementation plan based on the approved design document.",
        selected_approach_prefix(option_label.as_deref()),
        saved_to,
    );

    ExecutableToolResult {
        output: ExecutableToolOutput::Text(output),
        message: None,
        is_error: false,
        stop_turn: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::tests::MockSessionModeProvider;

    fn complete_design() -> String {
        "# Feature Spec\n\nThis document describes the feature in enough detail that an engineer with no prior context can implement it. It covers scope, architecture, data models, algorithms, error handling, self-review, user approval, and reuse analysis.\n\n## Scope\nThe scope includes the user-facing behavior, the API contract, and the persistence layer. Out of scope are third-party integrations and mobile clients.\n\n## Architecture\nThe system uses a layered architecture with a controller layer, a service layer, and a repository layer. A sequence diagram is included below.\n\n## Data Models\nThe primary entity is the Task with id, title, status, and due date. Relationships to User and Project are many-to-one.\n\n## Algorithms\nThe scheduling algorithm picks the next task by priority and deadline using a weighted score. Pseudocode is provided.\n\n## Error Handling\nInvalid input returns 400 with a structured error. Unexpected failures are logged and return 500. Degradation falls back to cached results.\n\n## Self-Review\nAssumptions: the user has write access, the project root is writable, and the runtime supports async IO.\n\n## User Final Approval\nPending user approval before implementation begins.\n\n## Reuse Analysis\nReuse the existing task repository and the shared validation utilities. No new dependencies are required.\n".into()
    }

    #[test]
    fn find_missing_sections_flags_short_content() {
        let missing = find_missing_design_sections("Too short.");
        assert!(missing.iter().any(|m| m.contains("sufficient content")));
    }

    #[test]
    fn find_missing_sections_flags_missing_architecture() {
        let mut content = complete_design();
        content = content.replace("## Architecture\n", "");
        let missing = find_missing_design_sections(&content);
        assert!(
            missing.iter().any(|m| m.contains("Architecture")),
            "missing: {:?}",
            missing
        );
    }

    #[test]
    fn complete_design_has_no_missing_sections() {
        let missing = find_missing_design_sections(&complete_design());
        assert!(missing.is_empty(), "missing: {:?}", missing);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exit_design_mode_hands_off_to_plan() {
        let provider = Arc::new(MockSessionModeProvider::design_mode_with_content(
            &complete_design(),
        ));
        let tool = ExitDesignModeTool::new(provider.clone());
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
        assert!(provider
            .handed_off_to
            .lock()
            .unwrap()
            .contains(&("plan".to_string(), None)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exit_design_mode_rejects_incomplete_design() {
        let provider = Arc::new(MockSessionModeProvider::design_mode_with_content(
            "## Scope\nOnly.",
        ));
        let tool = ExitDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(result.is_error);
        assert!(result.to_text().contains("Design is incomplete"));
    }
}
