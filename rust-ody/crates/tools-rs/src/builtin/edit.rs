use kaos_rs::kaos::Kaos;
use serde_json::Value;

#[cfg(test)]
use serde_json::json;

use crate::builtin::line_endings::{materialize_model_text, to_model_text_view};
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

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn edit_parameters() -> Value {
    InputSchema::object(vec![
        (
            "path",
            InputSchema::string().description("Path to the text file to edit."),
        ),
        (
            "old_string",
            InputSchema::string()
                .min_length(1)
                .description("Exact content to replace from the file."),
        ),
        (
            "new_string",
            InputSchema::string().description("Replacement text."),
        ),
        (
            "replace_all",
            InputSchema::boolean().optional().description(
                "Set true only when every occurrence of old_string should be replaced.",
            ),
        ),
    ])
    .build()
}

pub struct EditTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
}

impl EditTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for EditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Perform exact string replacements against a text file."
    }

    fn parameters(&self) -> Value {
        edit_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("path is required".into()))?;
        let old_string = args
            .get("old_string")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("old_string is required".into()))?;
        if old_string.is_empty() {
            return Err(ToolError::InvalidArgs(
                "old_string must not be empty".into(),
            ));
        }
        let path_class = kaos_path_class(&self.kaos);
        let safe_path = assert_path_allowed(
            path,
            &self.kaos.getcwd(),
            &self.workspace,
            AssertPathOptions {
                mode: PathAccessOperation::Write,
                check_sensitive: None,
                path_class: Some(path_class),
            },
        )?;

        let approval_rule = literal_rule_pattern(self.name(), &safe_path);
        let kaos = self.kaos.clone();
        let path = path.to_string();
        let safe_path2 = safe_path.clone();
        let args2 = args.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::read_write_file(&safe_path),
            description: format!("Editing {}", path),
            matches_rule: None,
            display: None,
            approval_rule,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let path = path.clone();
                let safe_path = safe_path2.clone();
                let args = args2.clone();
                Box::pin(async move { execution(kaos, args, path, safe_path, ctx).await })
            }),
        })
    }
}

fn count_occurrences(content: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    content.matches(needle).count()
}

fn replace_once(content: &str, old: &str, new: &str) -> Option<String> {
    content.find(old).map(|pos| {
        let mut result = String::with_capacity(content.len() + new.len() - old.len());
        result.push_str(&content[..pos]);
        result.push_str(new);
        result.push_str(&content[pos + old.len()..]);
        result
    })
}

async fn execution(
    kaos: Kaos,
    args: Value,
    display_path: String,
    safe_path: String,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text(
            "Aborted before edit started".into(),
            "Aborted".into(),
        );
    }

    let old_string = match args.get("old_string").and_then(Value::as_str) {
        Some(s) => s,
        None => {
            return ExecutableToolResult::error_text(
                "old_string is required".into(),
                "old_string is required".into(),
            );
        }
    };
    let new_string = args.get("new_string").and_then(Value::as_str).unwrap_or("");
    let replace_all = args
        .get("replace_all")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Read the file with ErrorMode::Replace for invalid UTF-8 sequences
    let raw = match kaos
        .read_text(&safe_path, None, Some(kaos_rs::text::ErrorMode::Replace))
        .await
    {
        Ok(text) => text,
        Err(e) => {
            let is_not_found = match &e {
                kaos_rs::file::KaosIoError::Io(io) => io.kind() == std::io::ErrorKind::NotFound,
                _ => false,
            };
            if is_not_found {
                return ExecutableToolResult::error_text(
                    format!("\"{}\" does not exist.", display_path),
                    "File not found".into(),
                );
            }
            return ExecutableToolResult::error_text(
                format!("Failed to read \"{}\": {}", display_path, e),
                "Read failed".into(),
            );
        }
    };

    // Convert to model text view (LF-only for matching)
    let view = to_model_text_view(&raw);
    let model_text = view.text;
    let line_ending_style = view.line_ending_style;

    // Count occurrences of old_string in the model text
    let count = count_occurrences(&model_text, old_string);

    if count == 0 {
        return ExecutableToolResult::error_text(
            "old_string not found".into(),
            "old_string not found".into(),
        );
    }

    if !replace_all && count > 1 {
        return ExecutableToolResult::error_text(
            format!(
                "old_string occurs {} times in the file. Set replace_all to true to replace all occurrences.",
                count
            ),
            "Multiple occurrences".into(),
        );
    }

    let replaced = if replace_all {
        model_text.replace(old_string, new_string)
    } else {
        match replace_once(&model_text, old_string, new_string) {
            Some(r) => r,
            None => {
                return ExecutableToolResult::error_text(
                    "old_string not found".into(),
                    "old_string not found".into(),
                );
            }
        }
    };

    // Materialize back to the original line ending style
    let output_text = materialize_model_text(&replaced, line_ending_style);

    // Write back
    match kaos.write_text(&safe_path, &output_text, None, None).await {
        Ok(_) => ExecutableToolResult::ok_text(format!("Edited {}", display_path)),
        Err(e) => ExecutableToolResult::error_text(
            format!("Failed to write \"{}\": {}", display_path, e),
            "Write failed".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceConfig;
    use kaos_rs::environment::Environment;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    fn workspace(tmp: &std::path::Path) -> WorkspaceConfig {
        WorkspaceConfig::new(tmp.to_string_lossy().to_string())
    }

    async fn run_edit(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = EditTool::new(kaos.clone(), workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await
    }

    #[tokio::test]
    async fn replaces_once_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("f.txt"), "hello foo world")
            .await
            .unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "f.txt", "old_string": "foo", "new_string": "bar"}),
        )
        .await;
        assert!(!result.is_error, "expected success, got {:?}", result);
        let content = tokio::fs::read_to_string(tmp.path().join("f.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello bar world");
    }

    #[tokio::test]
    async fn replaces_all_when_flag_set() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("f.txt"), "foo foo foo")
            .await
            .unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "f.txt", "old_string": "foo", "new_string": "bar", "replace_all": true}),
        )
        .await;
        assert!(!result.is_error);
        let content = tokio::fs::read_to_string(tmp.path().join("f.txt"))
            .await
            .unwrap();
        assert_eq!(content, "bar bar bar");
    }

    #[tokio::test]
    async fn errors_on_non_unique_match_without_replace_all() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("f.txt"), "foo foo")
            .await
            .unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "f.txt", "old_string": "foo", "new_string": "bar"}),
        )
        .await;
        assert!(result.is_error);
        let output = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(output.contains("Multiple occurrences") || output.contains("2 times"));
    }

    #[tokio::test]
    async fn errors_on_missing_old_string() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("f.txt"), "hello")
            .await
            .unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "f.txt", "old_string": "xx", "new_string": "yy"}),
        )
        .await;
        assert!(result.is_error);
        let output = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(output.contains("old_string not found"));
    }

    #[tokio::test]
    async fn preserves_crlf_line_endings() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("f.txt"), "line1\r\nline2\r\n")
            .await
            .unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "f.txt", "old_string": "line1", "new_string": "hello"}),
        )
        .await;
        assert!(!result.is_error);
        let content = tokio::fs::read_to_string(tmp.path().join("f.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello\r\nline2\r\n");
    }
}
