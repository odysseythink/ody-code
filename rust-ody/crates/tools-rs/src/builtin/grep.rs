use std::collections::HashSet;
use std::time::Duration;

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
use crate::policies::sensitive::is_sensitive_file;
use crate::rg_locator::{ensure_rg_path, rg_unavailable_message, EnsureRgOptions};
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
const POLL_INTERVAL_MS: u64 = 100;
const DEFAULT_TIMEOUT_S: u64 = 20;
const GRACE_PERIOD_S: u64 = 5;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn grep_parameters() -> Value {
    InputSchema::object(vec![
        (
            "pattern",
            InputSchema::string().description("Regular expression to search for."),
        ),
        (
            "path",
            InputSchema::string()
                .optional()
                .description("File or directory to search. Accepts an absolute path, or a path relative to the current working directory."),
        ),
        (
            "glob",
            InputSchema::string()
                .optional()
                .description("Optional glob filter passed to ripgrep."),
        ),
        (
            "type",
            InputSchema::string()
                .optional()
                .description("Optional ripgrep file type filter, such as ts or py."),
        ),
        (
            "output_mode",
            InputSchema::string_enum(&["content", "files_with_matches", "count_matches"])
                .default(serde_json::Value::String("files_with_matches".to_string()))
                .description("Shape of the result. `content` shows matching lines; `files_with_matches` shows only the paths of files that contain a match; `count_matches` shows the total number of matches."),
        ),
        (
            "-i",
            InputSchema::boolean()
                .optional()
                .description("Perform a case-insensitive search. Defaults to false."),
        ),
        (
            "-n",
            InputSchema::boolean()
                .optional()
                .description("Prefix each matching line with its line number. Applies only when `output_mode` is `content`. Defaults to true."),
        ),
        (
            "-A",
            InputSchema::integer()
                .min(0.0)
                .optional()
                .description("Number of lines to show after each match."),
        ),
        (
            "-B",
            InputSchema::integer()
                .min(0.0)
                .optional()
                .description("Number of lines to show before each match."),
        ),
        (
            "-C",
            InputSchema::integer()
                .min(0.0)
                .optional()
                .description("Number of lines to show before and after each match."),
        ),
        (
            "head_limit",
            InputSchema::integer()
                .min(0.0)
                .default(serde_json::Value::from(250))
                .description("Limit output to the first N lines/entries. Defaults to 250. Pass 0 for unlimited."),
        ),
        (
            "offset",
            InputSchema::integer()
                .min(0.0)
                .optional()
                .description("Number of leading lines/entries to skip before applying `head_limit`."),
        ),
        (
            "multiline",
            InputSchema::boolean()
                .optional()
                .description("Enable multiline matching, where the pattern can span line boundaries."),
        ),
        (
            "include_ignored",
            InputSchema::boolean()
                .optional()
                .description("Also search files excluded by ignore files such as `.gitignore`."),
        ),
    ])
    .build()
}

pub struct GrepTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
}

impl GrepTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regular expressions (powered by ripgrep)."
    }

    fn parameters(&self) -> Value {
        grep_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let pattern = args
            .get("pattern")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("pattern is required".into()))?;

        let path_str = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or(&self.workspace.workspace_dir);

        let path_class = kaos_path_class(&self.kaos);
        let safe_path = assert_path_allowed(
            path_str,
            &self.kaos.getcwd(),
            &self.workspace,
            AssertPathOptions {
                mode: PathAccessOperation::Search,
                check_sensitive: Some(false),
                path_class: Some(path_class),
            },
        )?;

        let approval_rule = literal_rule_pattern(self.name(), &safe_path);
        let kaos = self.kaos.clone();
        let pattern = pattern.to_string();
        let safe_path2 = safe_path.clone();
        let args2 = args.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::search_tree(&safe_path),
            description: format!("Grepping {} in {}", pattern, path_str),
            matches_rule: None,
            display: None,
            approval_rule,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let pattern = pattern.clone();
                let safe_path = safe_path2.clone();
                let args = args2.clone();
                Box::pin(async move { execution(kaos, args, pattern, safe_path, ctx).await })
            }),
        })
    }
}

async fn execution(
    kaos: Kaos,
    args: Value,
    pattern: String,
    safe_path: String,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
    }

    // Locate rg binary
    let rg_resolution = match ensure_rg_path(EnsureRgOptions {
        share_dir: None,
        cancel: None,
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return ExecutableToolResult::error_text(
                rg_unavailable_message(&e),
                "ripgrep unavailable".into(),
            );
        }
    };

    let rg_path = rg_resolution.path.to_string_lossy().to_string();

    let output_mode = args
        .get("output_mode")
        .and_then(Value::as_str)
        .unwrap_or("files_with_matches");

    let case_insensitive = args.get("-i").and_then(Value::as_bool).unwrap_or(false);
    let show_line_numbers = args.get("-n").and_then(Value::as_bool).unwrap_or(true);
    let after_context = args.get("-A").and_then(Value::as_i64);
    let before_context = args.get("-B").and_then(Value::as_i64);
    let context = args.get("-C").and_then(Value::as_i64);
    let head_limit = args
        .get("head_limit")
        .and_then(Value::as_i64)
        .unwrap_or(250);
    let offset = args.get("offset").and_then(Value::as_i64).unwrap_or(0);
    let multiline = args
        .get("multiline")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_ignored = args
        .get("include_ignored")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let glob_filter = args.get("glob").and_then(Value::as_str);
    let type_filter = args.get("type").and_then(Value::as_str);

    // Build rg args
    let mut rg_args: Vec<String> = Vec::new();
    rg_args.push(rg_path);

    // Null-delimited output for parsing
    rg_args.push("--null".to_string());

    if case_insensitive {
        rg_args.push("-i".to_string());
    }

    if multiline {
        rg_args.push("--multiline".to_string());
    }

    if !include_ignored {
        // Exclude VCS directories
        rg_args.push("-g".to_string());
        rg_args.push("!.git".to_string());
        rg_args.push("-g".to_string());
        rg_args.push("!.svn".to_string());
        rg_args.push("-g".to_string());
        rg_args.push("!.hg".to_string());
    }

    if let Some(g) = glob_filter {
        rg_args.push("-g".to_string());
        rg_args.push(g.to_string());
    }

    if let Some(t) = type_filter {
        rg_args.push("-t".to_string());
        rg_args.push(t.to_string());
    }

    match output_mode {
        "files_with_matches" => {
            rg_args.push("--files-with-matches".to_string());
        }
        "count_matches" => {
            rg_args.push("--count".to_string());
        }
        "content" => {
            rg_args.push("--no-heading".to_string());
            rg_args.push("--color".to_string());
            rg_args.push("never".to_string());
            if show_line_numbers {
                rg_args.push("--line-number".to_string());
            }
            if let Some(n) = after_context {
                rg_args.push("-A".to_string());
                rg_args.push(n.to_string());
            }
            if let Some(n) = before_context {
                rg_args.push("-B".to_string());
                rg_args.push(n.to_string());
            }
            if let Some(n) = context {
                rg_args.push("-C".to_string());
                rg_args.push(n.to_string());
            }
        }
        _ => {}
    }

    rg_args.push("--".to_string());
    rg_args.push(pattern);
    rg_args.push(safe_path.clone());

    let rg_args_refs: Vec<&str> = rg_args.iter().map(|s| s.as_str()).collect();

    // Spawn rg
    let proc = match kaos.exec_with_env(&rg_args_refs, &[]).await {
        Ok(p) => p,
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to spawn rg: {}", e),
                "Spawn failed".into(),
            );
        }
    };

    // Poll loop with timeout
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(DEFAULT_TIMEOUT_S);
    let grace = Duration::from_secs(GRACE_PERIOD_S);

    loop {
        if ctx.signal.aborted() {
            let _ = proc.kill(None).await;
            return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
        }

        if let Some(_code) = proc.exit_code() {
            break;
        }

        if start.elapsed() > timeout + grace {
            let _ = proc.kill(Some("SIGKILL")).await;
            return ExecutableToolResult::error_text("Grep timed out".into(), "Timeout".into());
        }

        if start.elapsed() > timeout {
            let _ = proc.kill(None).await;
            // Give it grace period — will exit on next iteration
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }

    // Wait for process to fully finish
    proc.wait().await;

    let stdout = proc.stdout().await;
    let stdout_str = String::from_utf8_lossy(&stdout).to_string();

    // Parse null-delimited output
    if stdout_str.is_empty() {
        return ExecutableToolResult::ok_text("No matches found.".into());
    }

    let parts: Vec<&str> = stdout_str.split('\0').collect();

    match output_mode {
        "files_with_matches" => {
            let mut files: Vec<String> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for part in &parts {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if is_sensitive_file(trimmed) {
                    continue;
                }
                if seen.insert(trimmed.to_string()) {
                    files.push(trimmed.to_string());
                }
            }
            // Apply offset/head_limit pagination
            let offset = offset.max(0) as usize;
            let limited: Vec<&String> = if head_limit == 0 {
                files.iter().skip(offset).collect()
            } else {
                files
                    .iter()
                    .skip(offset)
                    .take(head_limit as usize)
                    .collect()
            };
            let output = limited
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if output.len() > MAX_OUTPUT_BYTES {
                let truncated = &output[..MAX_OUTPUT_BYTES];
                return ExecutableToolResult::ok_text(format!(
                    "{}\n<system>Output truncated at {} bytes.</system>",
                    truncated, MAX_OUTPUT_BYTES
                ));
            }
            ExecutableToolResult::ok_text(output)
        }
        "count_matches" => {
            let mut lines: Vec<String> = Vec::new();
            let mut total = 0u64;
            for chunk in parts.chunks(2) {
                if chunk.len() < 2 {
                    continue;
                }
                let filename = chunk[0].trim();
                let count_str = chunk[1].trim();
                if filename.is_empty() {
                    continue;
                }
                if is_sensitive_file(filename) {
                    continue;
                }
                if let Ok(count) = count_str.parse::<u64>() {
                    total += count;
                    lines.push(format!("{}:{}", filename, count));
                }
            }
            if lines.is_empty() {
                return ExecutableToolResult::ok_text("No matches found.".into());
            }
            // Apply offset/head_limit
            let offset = offset.max(0) as usize;
            let limited: Vec<&String> = if head_limit == 0 {
                lines.iter().skip(offset).collect()
            } else {
                lines
                    .iter()
                    .skip(offset)
                    .take(head_limit as usize)
                    .collect()
            };
            let mut output = limited
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            output.push_str(&format!("\n{} total matches.", total));
            ExecutableToolResult::ok_text(output)
        }
        _ => {
            // content mode — ripgrep emits one line per match:
            // path<NUL>line:content\n. Reconstruct "path:line:content" per record.
            let mut lines: Vec<String> = Vec::new();
            for record in stdout_str.lines() {
                let record = record.trim();
                if record.is_empty() || record == "--" {
                    continue;
                }
                let mut splits = record.split('\0');
                let filename = match splits.next() {
                    Some(f) => f.trim(),
                    None => continue,
                };
                let content = match splits.next() {
                    Some(c) => c.trim(),
                    None => continue,
                };
                if filename.is_empty() || is_sensitive_file(filename) {
                    continue;
                }
                if content.is_empty() {
                    lines.push(filename.to_string());
                } else {
                    lines.push(format!("{}:{}", filename, content));
                }
            }
            if lines.is_empty() {
                return ExecutableToolResult::ok_text("No matches found.".into());
            }
            // Apply offset/head_limit
            let offset = offset.max(0) as usize;
            let limited: Vec<&String> = if head_limit == 0 {
                lines.iter().skip(offset).collect()
            } else {
                lines
                    .iter()
                    .skip(offset)
                    .take(head_limit as usize)
                    .collect()
            };
            let output = limited
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if output.len() > MAX_OUTPUT_BYTES {
                let truncated = &output[..MAX_OUTPUT_BYTES];
                return ExecutableToolResult::ok_text(format!(
                    "{}\n<system>Output truncated at {} bytes.</system>",
                    truncated, MAX_OUTPUT_BYTES
                ));
            }
            ExecutableToolResult::ok_text(output)
        }
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

    async fn run_grep(tmp: &tempfile::TempDir, args: Value) -> String {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GrepTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        }
    }

    #[tokio::test]
    async fn files_with_matches_finds_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello world\n")
            .await
            .unwrap();
        let out = run_grep(
            &tmp,
            json!({"pattern": "hello", "output_mode": "files_with_matches"}),
        )
        .await;
        assert!(out.contains("a.txt"));
    }

    #[tokio::test]
    async fn files_with_matches_respects_path_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        tokio::fs::create_dir(&sub).await.unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello\n")
            .await
            .unwrap();
        tokio::fs::write(sub.join("b.txt"), "hello\n")
            .await
            .unwrap();

        let out = run_grep(
            &tmp,
            json!({"pattern": "hello", "output_mode": "files_with_matches", "path": sub.to_string_lossy().to_string()}),
        )
        .await;
        assert!(out.contains("b.txt"));
        assert!(!out.contains("a.txt"));
    }

    #[tokio::test]
    async fn handles_missing_rg_gracefully() {
        // This test verifies that when rg is not found, we get an error result
        // The rg locator will try to find rg; if it fails, we get the error message
        // Note: this test assumes rg may or may not be installed.
        // We test the error path by checking that the tool either succeeds (rg is installed)
        // or returns an error with the expected message.
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello\n")
            .await
            .unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GrepTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(json!({"pattern": "hello"})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        // If rg is installed, should succeed. If not, error message should mention ripgrep.
        if result.is_error {
            let text = match &result.output {
                crate::builtin::ExecutableToolOutput::Text(s) => s.clone(),
                _ => String::new(),
            };
            assert!(
                text.contains("ripgrep") || text.contains("rg"),
                "unexpected error: {}",
                text
            );
        }
    }
}
