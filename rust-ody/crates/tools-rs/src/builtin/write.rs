use kaos_rs::kaos::Kaos;
use serde_json::Value;

#[cfg(test)]
use serde_json::json;

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

fn write_parameters() -> Value {
    InputSchema::object(vec![
        (
            "path",
            InputSchema::string().description("The absolute path, or a path relative to the current working directory, to write or append to. The parent directory must already exist."),
        ),
        (
            "content",
            InputSchema::string().description("Raw full file content to write exactly as provided."),
        ),
        (
            "mode",
            InputSchema::string_enum(&["overwrite", "append"])
                .optional()
                .description("Write mode. Defaults to overwrite."),
        ),
    ])
    .build()
}

pub struct WriteTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
}

impl WriteTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for WriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Overwrite or append to a file with content exactly as provided, creating the file if needed; the parent directory must already exist."
    }

    fn parameters(&self) -> Value {
        write_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("path is required".into()))?;
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
            accesses: ToolAccesses::write_file(&safe_path),
            description: format!("Writing {}", path),
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

async fn execution(
    kaos: Kaos,
    args: Value,
    display_path: String,
    safe_path: String,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text(
            "Aborted before write started".into(),
            "Aborted".into(),
        );
    }

    let content = match args.get("content").and_then(Value::as_str) {
        Some(c) => c,
        None => {
            return ExecutableToolResult::error_text(
                "content is required".into(),
                "content is required".into(),
            );
        }
    };
    let mode = args
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("overwrite");

    if mode != "overwrite" && mode != "append" {
        return ExecutableToolResult::error_text(
            format!(
                "Invalid mode: {}. Must be \"overwrite\" or \"append\".",
                mode
            ),
            "Invalid mode".into(),
        );
    }

    // Stat parent directory
    let parent = std::path::Path::new(&safe_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    match kaos.stat(&parent, false).await {
        Ok(stat) => {
            if !stat.is_dir() {
                return ExecutableToolResult::error_text(
                    "Parent directory does not exist".into(),
                    "Parent path is not a directory".into(),
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ExecutableToolResult::error_text(
                "Parent directory does not exist".into(),
                "Parent directory does not exist".into(),
            );
        }
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to stat parent directory: {}", e),
                "Stat failed".into(),
            );
        }
    }

    let write_mode: Option<&str> = if mode == "append" { Some("a") } else { None };
    let bytes_written = content.len();

    match kaos.write_text(&safe_path, content, write_mode, None).await {
        Ok(_) => {
            let verb = if mode == "append" {
                "Appended"
            } else {
                "Wrote"
            };
            ExecutableToolResult::ok_text(format!(
                "{} {} bytes to {}",
                verb, bytes_written, display_path
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ExecutableToolResult::error_text(
            "Parent directory does not exist".into(),
            "Parent directory does not exist".into(),
        ),
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

    async fn run_write(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = WriteTool::new(kaos.clone(), workspace(tmp.path()));
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
    async fn writes_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_write(&tmp, json!({"path": "hello.txt", "content": "hello"})).await;
        assert!(!result.is_error, "expected success, got {:?}", result);
        let output = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(output.contains("Wrote 5 bytes"));
        let content = tokio::fs::read_to_string(tmp.path().join("hello.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn overwrites_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("f.txt"), "old")
            .await
            .unwrap();
        let result = run_write(&tmp, json!({"path": "f.txt", "content": "new"})).await;
        assert!(!result.is_error);
        let content = tokio::fs::read_to_string(tmp.path().join("f.txt"))
            .await
            .unwrap();
        assert_eq!(content, "new");
    }

    #[tokio::test]
    async fn appends_to_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("f.txt"), "hello")
            .await
            .unwrap();
        let result = run_write(
            &tmp,
            json!({"path": "f.txt", "content": " world", "mode": "append"}),
        )
        .await;
        assert!(!result.is_error);
        let output = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(output.contains("Appended 6 bytes"));
        let content = tokio::fs::read_to_string(tmp.path().join("f.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn reports_utf8_byte_count_for_non_ascii() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_write(&tmp, json!({"path": "e.txt", "content": "é"})).await;
        assert!(!result.is_error);
        let output = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(output.contains("Wrote 2 bytes"));
    }

    #[tokio::test]
    async fn rejects_missing_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_write(&tmp, json!({"path": "missing/out.txt", "content": "x"})).await;
        assert!(result.is_error);
        let output = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(output.contains("Parent directory does not exist"));
    }
}
