use std::collections::HashSet;

use kaos_rs::kaos::Kaos;
use serde_json::Value;

#[cfg(test)]
use serde_json::json;

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::policies::path_access::{
    assert_path_allowed, is_within_directory, normalize_path, AssertPathOptions,
    PathAccessOperation, PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const MAX_MATCHES: usize = 100;
const MAX_BRACE_EXPANSIONS: usize = 64;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

mod braces {
    use super::MAX_BRACE_EXPANSIONS;

    pub fn expand_braces(pattern: &str) -> Vec<String> {
        let mut out = Vec::new();
        if !expand_into(pattern, &mut out, MAX_BRACE_EXPANSIONS) {
            return vec![pattern.to_string()];
        }
        out
    }

    fn expand_into(pattern: &str, out: &mut Vec<String>, cap: usize) -> bool {
        let mut depth = 0i32;
        let mut start: Option<usize> = None;
        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if ch == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            if ch == '{' {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
                i += 1;
                continue;
            }
            if ch == '}' {
                if depth == 0 {
                    return push_literal(pattern, out, cap);
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        let inner: String = chars[s + 1..i].iter().collect();
                        let parts = split_top_level_commas(&inner);
                        if parts.len() < 2 {
                            start = None;
                            i += 1;
                            continue;
                        }
                        let prefix: String = chars[..s].iter().collect();
                        let suffix: String = chars[i + 1..].iter().collect();
                        for part in parts {
                            if out.len() >= cap {
                                return false;
                            }
                            let combined = format!("{}{}{}", prefix, part, suffix);
                            if !expand_into(&combined, out, cap) {
                                return false;
                            }
                        }
                        return true;
                    }
                }
                i += 1;
                continue;
            }
            i += 1;
        }
        if depth != 0 {
            return push_literal(pattern, out, cap);
        }
        push_literal(pattern, out, cap)
    }

    fn push_literal(pattern: &str, out: &mut Vec<String>, cap: usize) -> bool {
        if out.len() >= cap {
            return false;
        }
        out.push(pattern.to_string());
        true
    }

    fn split_top_level_commas(s: &str) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        let mut depth = 0i32;
        let mut last = 0;
        let mut parts = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if ch == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            } else if ch == ',' && depth == 0 {
                parts.push(chars[last..i].iter().collect());
                last = i + 1;
            }
            i += 1;
        }
        parts.push(chars[last..].iter().collect());
        parts
    }
}

fn glob_parameters() -> Value {
    InputSchema::object(vec![
        (
            "pattern",
            InputSchema::string().description("Glob pattern to match files/directories."),
        ),
        (
            "path",
            InputSchema::string()
                .optional()
                .description("Absolute path to the directory to search in. Defaults to the current working directory."),
        ),
        (
            "include_dirs",
            InputSchema::boolean()
                .default(serde_json::Value::Bool(true))
                .description("Whether to include directories in results. Defaults to true. Set false to return only files."),
        ),
    ])
    .build()
}

pub struct GlobTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
}

impl GlobTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files (and optionally directories) by glob pattern, sorted by modification time (most recent first)."
    }

    fn parameters(&self) -> Value {
        glob_parameters()
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

        let include_dirs = args
            .get("include_dirs")
            .and_then(Value::as_bool)
            .unwrap_or(true);

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
        let workspace_dir = self.workspace.workspace_dir.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::search_tree(&safe_path),
            description: format!("Globbing {} in {}", pattern, path_str),
            matches_rule: None,
            display: None,
            approval_rule,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let pattern = pattern.clone();
                let safe_path = safe_path2.clone();
                let workspace_dir = workspace_dir.clone();
                Box::pin(async move {
                    execution(kaos, pattern, safe_path, include_dirs, workspace_dir, ctx).await
                })
            }),
        })
    }
}

async fn execution(
    kaos: Kaos,
    pattern: String,
    safe_path: String,
    include_dirs: bool,
    workspace_dir: String,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
    }

    // Check if root exists
    match kaos.stat(&safe_path, false).await {
        Ok(stat) => {
            if !stat.is_dir() {
                return ExecutableToolResult::error_text(
                    format!("\"{}\" is not a directory", safe_path),
                    "Not a directory".into(),
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ExecutableToolResult::error_text(
                format!("\"{}\" does not exist", safe_path),
                "Path not found".into(),
            );
        }
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to access \"{}\": {}", safe_path, e),
                "Access error".into(),
            );
        }
    }

    let path_class = match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    };

    // Expand braces and collect results
    let expanded = braces::expand_braces(&pattern);
    let mut seen: HashSet<String> = HashSet::new();
    let mut results: Vec<String> = Vec::new();

    for expanded_pattern in &expanded {
        if results.len() >= MAX_MATCHES {
            break;
        }
        match kaos.glob(&safe_path, expanded_pattern, true).await {
            Ok(matches) => {
                for m in matches {
                    if results.len() >= MAX_MATCHES {
                        break;
                    }
                    let normalized = normalize_path(&m, path_class);
                    if !seen.insert(normalized.clone()) {
                        continue;
                    }
                    let stat = match kaos.stat(&m, false).await {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    if stat.is_dir() && !include_dirs {
                        continue;
                    }
                    results.push(normalized);
                }
            }
            Err(_) => {
                continue;
            }
        }
    }

    if results.is_empty() {
        return ExecutableToolResult::ok_text("No files matched the glob pattern.".into());
    }

    // Collect stat info for sorting
    let mut entries: Vec<(String, f64)> = Vec::new();
    for abs_path in &results {
        let mtime = match kaos.stat(abs_path, false).await {
            Ok(s) => s.st_mtime,
            Err(_) => 0.0,
        };
        entries.push((abs_path.clone(), mtime));
    }
    // Sort by mtime descending (most recent first)
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let workspace_normalized = normalize_path(&workspace_dir, path_class);

    let display_paths: Vec<String> = entries
        .iter()
        .map(|(abs, _)| {
            if is_within_directory(abs, &workspace_normalized, path_class) {
                // Relativize
                let prefix = if workspace_normalized.ends_with('/') {
                    workspace_normalized.clone()
                } else {
                    format!("{}/", workspace_normalized)
                };
                let rel = if abs.starts_with(&prefix) {
                    abs[prefix.len()..].to_string()
                } else if *abs == workspace_normalized {
                    ".".to_string()
                } else {
                    abs.clone()
                };
                if rel.is_empty() {
                    ".".to_string()
                } else {
                    rel
                }
            } else {
                abs.clone()
            }
        })
        .collect();

    let mut lines = display_paths.join("\n");
    let total = results.len();

    if total >= MAX_MATCHES {
        lines.push_str(&format!(
            "\n<system>... and {} more results were truncated.</system>",
            total.saturating_sub(MAX_MATCHES)
        ));
    }

    ExecutableToolResult::ok_text(lines)
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

    #[test]
    fn expand_braces_expands_alternation() {
        let result = braces::expand_braces("*.{ts,tsx}");
        assert_eq!(result, vec!["*.ts", "*.tsx"]);
    }

    #[test]
    fn expand_braces_cartesian_product() {
        let result = braces::expand_braces("{a,b}/{c,d}.ts");
        assert_eq!(result, vec!["a/c.ts", "a/d.ts", "b/c.ts", "b/d.ts"]);
    }

    #[test]
    fn expand_braces_ignores_unbalanced() {
        let result = braces::expand_braces("{a,b");
        assert_eq!(result, vec!["{a,b"]);
    }

    #[test]
    fn expand_braces_falls_through_literal_group() {
        let result = braces::expand_braces("{abc}");
        assert_eq!(result, vec!["{abc}"]);
    }

    async fn run_glob(tmp: &tempfile::TempDir, args: Value) -> String {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GlobTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error, "expected success, got {:?}", result);
        match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        }
    }

    #[tokio::test]
    async fn finds_files_matching_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "x")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("b.txt"), "y")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("c.rs"), "z")
            .await
            .unwrap();

        let out = run_glob(&tmp, json!({"pattern": "*.txt"})).await;
        assert!(out.contains("a.txt"));
        assert!(out.contains("b.txt"));
        assert!(!out.contains("c.rs"));
    }

    #[tokio::test]
    async fn expands_braces_at_tool_layer() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.ts"), "x")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("b.tsx"), "y")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("c.rs"), "z")
            .await
            .unwrap();

        let out = run_glob(&tmp, json!({"pattern": "*.{ts,tsx}"})).await;
        assert!(out.contains("a.ts"));
        assert!(out.contains("b.tsx"));
        assert!(!out.contains("c.rs"));
    }

    #[tokio::test]
    async fn excludes_dirs_when_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        tokio::fs::create_dir(&sub).await.unwrap();
        tokio::fs::write(sub.join("a.txt"), "x").await.unwrap();
        tokio::fs::write(tmp.path().join("b.txt"), "y")
            .await
            .unwrap();

        let out = run_glob(&tmp, json!({"pattern": "*", "include_dirs": false})).await;
        assert!(out.contains("b.txt"));
        assert!(!out.contains("sub"));
    }

    #[tokio::test]
    async fn reports_missing_root() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("does_not_exist");
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GlobTool::new(
            kaos,
            WorkspaceConfig::new(nonexistent.to_string_lossy().to_string()),
        );
        let exec = tool
            .resolve_execution(
                json!({"pattern": "*", "path": nonexistent.to_string_lossy().to_string()}),
            )
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("does not exist"));
    }
}
