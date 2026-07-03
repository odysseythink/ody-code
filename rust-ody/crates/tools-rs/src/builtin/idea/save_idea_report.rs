use kaos_rs::kaos::Kaos;
use serde_json::Value;

use crate::builtin::idea::{
    build_idea_report_body, ensure_ideas_directory, generate_idea_file_path,
    validate_idea_report_input, IdeaReportContext, IdeaReportInput,
};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::policies::path_access::{
    assert_path_allowed, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

pub struct SaveIdeaReportTool<C: IdeaReportContext> {
    kaos: Kaos,
    workspace: WorkspaceConfig,
    context: C,
}

impl<C: IdeaReportContext> SaveIdeaReportTool<C> {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig, context: C) -> Self {
        Self {
            kaos,
            workspace,
            context,
        }
    }
}

impl<C: IdeaReportContext> BuiltinTool for SaveIdeaReportTool<C> {
    fn name(&self) -> &str {
        "SaveIdeaReport"
    }

    fn description(&self) -> &str {
        include_str!("save-idea-report.md")
    }

    fn parameters(&self) -> Value {
        parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        if !self.context.is_idea_skill_active() {
            return Ok(ToolExecution {
                accesses: ToolAccesses::none(),
                description: "Rejecting SaveIdeaReport: idea skill not active".into(),
                approval_rule: "SaveIdeaReport".into(),
                matches_rule: None,
                display: None,
                execute: Box::new(move |_ctx| {
                    Box::pin(async move {
                        ExecutableToolResult::error_text(
                            "SaveIdeaReport can only be used after idea-generator or idea-evaluator has been activated.".into(),
                            "Idea skill not active".into(),
                        )
                    })
                }),
            });
        }

        let input: IdeaReportInput = serde_json::from_value(args.clone())
            .map_err(|e| ToolError::InvalidArgs(format!("Invalid arguments: {}", e)))?;
        let validated = validate_idea_report_input(&input).map_err(ToolError::InvalidArgs)?;

        let cwd = self.kaos.getcwd();
        let ideas_dir = format!("{}/.ody-code/ideas", cwd);
        let path_class = kaos_path_class(&self.kaos);
        let safe_ideas_dir = assert_path_allowed(
            &ideas_dir,
            &cwd,
            &self.workspace,
            AssertPathOptions {
                mode: PathAccessOperation::Write,
                check_sensitive: None,
                path_class: Some(path_class),
            },
        )?;

        let now = self.context.now();
        let file_path = generate_idea_file_path(&safe_ideas_dir, &validated.title, &now, |p| {
            std::fs::metadata(p).is_ok()
        });
        let safe_file_path = assert_path_allowed(
            &file_path,
            &cwd,
            &self.workspace,
            AssertPathOptions {
                mode: PathAccessOperation::Write,
                check_sensitive: None,
                path_class: Some(path_class),
            },
        )?;

        let body = build_idea_report_body(&validated, &now);
        let approval_rule = literal_rule_pattern(self.name(), &safe_file_path);
        let kaos = self.kaos.clone();
        let display_path = file_path.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::write_file(&safe_file_path),
            description: format!("Saving idea report to {}", display_path),
            approval_rule,
            matches_rule: None,
            display: Some(serde_json::json!({
                "kind": "file_io",
                "operation": "write",
                "path": display_path,
                "content": body,
            })),
            execute: Box::new(move |_ctx| {
                let kaos = kaos.clone();
                let safe_file_path = safe_file_path.clone();
                let display_path = display_path.clone();
                let body = body.clone();
                Box::pin(async move { execution(kaos, safe_file_path, display_path, body).await })
            }),
        })
    }
}

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

async fn execution(
    kaos: Kaos,
    safe_file_path: String,
    display_path: String,
    body: String,
) -> ExecutableToolResult {
    if let Err(e) = ensure_ideas_directory(&kaos, &safe_file_path).await {
        return ExecutableToolResult::error_text(
            format!("Failed to prepare ideas directory: {}", e),
            "Directory setup failed".into(),
        );
    }

    match kaos
        .write_text(&safe_file_path, &body, Some("w"), None)
        .await
    {
        Ok(_) => ExecutableToolResult::ok_text(format!("Saved idea report to {}", display_path)),
        Err(e) => ExecutableToolResult::error_text(
            format!("Failed to write idea report: {}", e),
            "Write failed".into(),
        ),
    }
}

fn parameters() -> Value {
    InputSchema::object(vec![
        (
            "title",
            InputSchema::string().description("Short, filesystem-safe title for the report."),
        ),
        (
            "content",
            InputSchema::string().description("Full Markdown report body."),
        ),
        (
            "type",
            InputSchema::string_enum(&["generator", "evaluator"]).description("Report kind."),
        ),
        (
            "score",
            InputSchema::number()
                .min(0.0)
                .max(10.0)
                .optional()
                .description("Final 0-10 score; required for evaluator reports."),
        ),
        (
            "tags",
            InputSchema::array(InputSchema::string())
                .optional()
                .description("Optional tags such as [\"B2B\", \"AI\"]."),
        ),
    ])
    .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::idea::MockIdeaReportContext;
    use crate::workspace::WorkspaceConfig;
    use kaos_rs::environment::Environment;
    use serde_json::json;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        }
    }

    fn workspace(tmp: &std::path::Path) -> WorkspaceConfig {
        WorkspaceConfig::new(tmp.to_string_lossy().to_string())
    }

    fn fixed_now() -> chrono::DateTime<chrono::Utc> {
        "2026-01-02T00:00:00Z".parse().unwrap()
    }

    async fn run_save(tmp: &tempfile::TempDir, args: Value, active: bool) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let ctx = MockIdeaReportContext::new(active, fixed_now());
        let tool = SaveIdeaReportTool::new(kaos.clone(), workspace(tmp.path()), ctx);
        let exec = match tool.resolve_execution(args) {
            Ok(e) => e,
            Err(e) => return ExecutableToolResult::error_text(e.to_string(), e.to_string()),
        };
        (exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await
    }

    #[tokio::test]
    async fn saves_report_when_skill_active() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_save(
            &tmp,
            json!({
                "title": "B2B AI Assistant",
                "content": "# Idea\n\nDetails.",
                "type": "generator",
                "tags": ["B2B", "AI"]
            }),
            true,
        )
        .await;
        assert!(!result.is_error, "expected success, got {:?}", result);
        let text = result.to_text();
        assert!(text.contains("Saved idea report to"));

        let expected_path = tmp
            .path()
            .join(".ody-code/ideas/2026-01-02-b2b-ai-assistant.md");
        assert!(expected_path.exists());
        let content = tokio::fs::read_to_string(&expected_path).await.unwrap();
        assert!(content.contains("title: B2B AI Assistant"));
        assert!(content.contains("type: generator"));
        assert!(content.contains("- B2B"));
        assert!(content.ends_with("# Idea\n\nDetails.\n"));

        let gitignore = tokio::fs::read_to_string(tmp.path().join(".gitignore"))
            .await
            .unwrap();
        assert!(gitignore.contains(".ody-code/"));
    }

    #[tokio::test]
    async fn rejects_when_skill_inactive() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_save(
            &tmp,
            json!({
                "title": "B2B AI Assistant",
                "content": "# Idea",
                "type": "generator"
            }),
            false,
        )
        .await;
        assert!(result.is_error);
        assert!(result
            .to_text()
            .contains("idea-generator or idea-evaluator"));
    }

    #[tokio::test]
    async fn rejects_missing_title() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_save(
            &tmp,
            json!({"content": "# Idea", "type": "generator"}),
            true,
        )
        .await;
        assert!(result.is_error);
        assert!(result.to_text().contains("title"));
    }

    #[tokio::test]
    async fn increments_filename_when_file_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let ideas_dir = tmp.path().join(".ody-code/ideas");
        tokio::fs::create_dir_all(&ideas_dir).await.unwrap();
        tokio::fs::write(ideas_dir.join("2026-01-02-colliding.md"), "old")
            .await
            .unwrap();

        let result = run_save(
            &tmp,
            json!({
                "title": "Colliding",
                "content": "New",
                "type": "evaluator",
                "score": 7
            }),
            true,
        )
        .await;
        assert!(!result.is_error);
        let expected = ideas_dir.join("2026-01-02-colliding-1.md");
        assert!(expected.exists());
    }
}
