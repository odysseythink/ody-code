# Part 3 — `GlobTool` + `GrepTool`

Scope: implement the two search tools. `GlobTool` reuses `kaos.glob` plus a local brace-expansion layer and mtime sorting. `GrepTool` shells out to `ripgrep` via `kaos.exec`, parsing `--null` output for the three output modes, with timeout/abort, sensitive filtering, and pagination.

## Dependency Overview

```
Part 1: trait-read.md
  │
  ├──► Task 1: GlobTool (brace expansion + glob)
  └──► Task 2–3: GrepTool (files_with_matches, then content/count + pagination)
```

- `GrepTool` does not depend on `GlobTool`; the two tasks are independent after Part 1 and can be done in either order.
- Both depend on the `BuiltinTool` trait, `AbortSignal`, and path policy from Part 1.

## Task 1: Implement `GlobTool`

**Depends on:** Part 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/glob.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (add `pub mod glob;`)
- Test: inline in `glob.rs`

### Step 1 — Write the failing test

Create `rust-ody/crates/tools-rs/src/builtin/glob.rs` with tests first. Stub `resolve_execution` with `todo!()`.

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};

use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolExecution, ToolError};
use crate::policies::path_access::{
    assert_path_allowed, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

pub const MAX_MATCHES: usize = 100;
const MAX_BRACE_EXPANSIONS: usize = 64;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
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
                .default(Value::Bool(true))
                .optional()
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
        "Find files matching a glob pattern."
    }

    fn parameters(&self) -> Value {
        glob_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        todo!()
    }
}

mod braces {
    pub fn expand_braces(pattern: &str) -> Vec<String> {
        vec![pattern.to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    async fn run_glob(tmp: &tempfile::TempDir, args: Value) -> String {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GlobTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await;
        assert!(!result.is_error, "{:?}", result);
        match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn expand_braces_expands_alternation() {
        let mut out = braces::expand_braces("*.{ts,tsx}");
        out.sort();
        assert_eq!(out, vec!["*.ts", "*.tsx"]);
    }

    #[tokio::test]
    async fn finds_files_matching_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "").await.unwrap();
        tokio::fs::write(tmp.path().join("b.txt"), "").await.unwrap();
        tokio::fs::write(tmp.path().join("c.rs"), "").await.unwrap();
        let out = run_glob(&tmp, json!({"pattern": "*.txt"})).await;
        assert!(out.contains("a.txt"));
        assert!(out.contains("b.txt"));
        assert!(!out.contains("c.rs"));
    }
}
```

Add `pub mod glob;` to `rust-ody/crates/tools-rs/src/builtin/mod.rs`.

### Step 2 — Run the failing test

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::glob
```

Expected failure: tests panic at `todo!()`.

### Step 3 — Write the minimal implementation

Replace the `todo!()` and the stub brace module with the full implementation. The file becomes:

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::ErrorKind;

use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};
use crate::policies::path_access::{
    assert_path_allowed, is_within_directory, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

pub const MAX_MATCHES: usize = 100;
const MAX_BRACE_EXPANSIONS: usize = 64;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
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
                .default(Value::Bool(true))
                .optional()
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
        "Find files matching a glob pattern."
    }

    fn parameters(&self) -> Value {
        glob_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let pattern = args
            .get("pattern")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("pattern is required".into()))?;
        let optional_path = args.get("path").and_then(Value::as_str);

        let path_class = kaos_path_class(&self.kaos);
        let search_root = match optional_path {
            Some(p) => assert_path_allowed(
                p,
                &self.kaos.getcwd(),
                &self.workspace,
                AssertPathOptions {
                    mode: PathAccessOperation::Search,
                    check_sensitive: Some(false),
                    path_class: Some(path_class),
                },
            )?,
            None => self.workspace.workspace_dir.clone(),
        };

        let approval_rule = literal_rule_pattern(self.name(), pattern);
        let kaos = self.kaos.clone();
        let workspace = self.workspace.clone();
        let pattern = pattern.to_string();
        let search_root2 = search_root.clone();
        let args2 = args.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::search_tree(&search_root),
            description: format!("Searching {}", pattern),
            approval_rule,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let workspace = workspace.clone();
                let pattern = pattern.clone();
                let search_root = search_root2.clone();
                let args = args2.clone();
                Box::pin(async move { execution(kaos, workspace, pattern, search_root, args, ctx).await })
            }),
        })
    }
}

async fn execution(
    kaos: Kaos,
    workspace: WorkspaceConfig,
    pattern: String,
    search_root: String,
    args: Value,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
    }

    // Pre-check that the root exists and is a directory.
    match kaos.iterdir(&search_root).await {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return ExecutableToolResult::error_text(
                format!("{} does not exist", search_root),
                "Root missing".into(),
            );
        }
        Err(e) if e.kind() == ErrorKind::NotADirectory => {
            return ExecutableToolResult::error_text(
                format!("{} is not a directory", search_root),
                "Root not a directory".into(),
            );
        }
        Err(_) => {
            // Inconclusive — let the glob run and surface its own error.
        }
    }

    let sub_patterns = braces::expand_braces(&pattern)
        .into_iter()
        .map(|p| p.replace('\\', "/"))
        .collect::<Vec<_>>();

    let include_dirs = args.get("include_dirs").and_then(Value::as_bool).unwrap_or(true);

    let mut seen = HashSet::new();
    let mut entries: Vec<GlobEntry> = Vec::new();
    let mut truncated = false;

    'outer: for sub in &sub_patterns {
        match kaos.glob(&search_root, sub, true).await {
            Ok(paths) => {
                for path in paths {
                    if ctx.signal.aborted() {
                        return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
                    }
                    if seen.contains(&path) {
                        continue;
                    }
                    if entries.len() >= MAX_MATCHES {
                        truncated = true;
                        break 'outer;
                    }
                    seen.insert(path.clone());
                    let (is_dir, mtime) = match kaos.stat(&path, false).await {
                        Ok(st) => (st.is_dir(), st.st_mtime),
                        Err(_) => (false, 0.0),
                    };
                    if !include_dirs && is_dir {
                        continue;
                    }
                    entries.push(GlobEntry { path, is_dir, mtime });
                }
            }
            Err(_) => {
                // Continue with other sub-patterns.
            }
        }
    }

    entries.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));

    let path_class = kaos_path_class(&kaos);
    let should_relativize = is_within_directory(&search_root, &workspace.workspace_dir, path_class);
    let mut lines: Vec<String> = Vec::new();
    if truncated {
        lines.push(format!(
            "[Truncated at {} matches — {} matched so far, use a more specific pattern]",
            MAX_MATCHES,
            seen.len()
        ));
        lines.push(format!("Only the first {} matches are returned.", MAX_MATCHES));
    }
    for entry in &entries {
        let display = if should_relativize {
            relativize_if_under(&entry.path, &search_root, path_class)
        } else {
            entry.path.clone()
        };
        lines.push(display);
    }
    if !truncated && entries.len() == MAX_MATCHES {
        lines.push(format!("Found {} matches", entries.len()));
    }

    if entries.is_empty() && !truncated {
        ExecutableToolResult::ok_text("No matches found".into())
    } else {
        ExecutableToolResult::ok_text(lines.join("\n"))
    }
}

struct GlobEntry {
    path: String,
    is_dir: bool,
    mtime: f64,
}

fn relativize_if_under(candidate: &str, base: &str, path_class: PathClass) -> String {
    let norm_candidate = crate::policies::path_access::normalize_path(candidate, path_class);
    let norm_base = crate::policies::path_access::normalize_path(base, path_class);
    let cmp_candidate = if path_class == PathClass::Win32 {
        norm_candidate.to_lowercase()
    } else {
        norm_candidate.clone()
    };
    let cmp_base = if path_class == PathClass::Win32 {
        norm_base.to_lowercase()
    } else {
        norm_base.clone()
    };
    if cmp_candidate == cmp_base {
        return ".".to_string();
    }
    let prefix = if cmp_base.ends_with('/') {
        cmp_base
    } else {
        format!("{}/", cmp_base)
    };
    if cmp_candidate.starts_with(&prefix) {
        return norm_candidate[prefix.len()..].to_string();
    }
    norm_candidate
}

mod braces {
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

#[cfg(test)]
mod tests {
    use super::*;
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

    async fn run_glob(tmp: &tempfile::TempDir, args: Value) -> String {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GlobTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await;
        assert!(!result.is_error, "{:?}", result);
        match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn expand_braces_expands_alternation() {
        let mut out = braces::expand_braces("*.{ts,tsx}");
        out.sort();
        assert_eq!(out, vec!["*.ts", "*.tsx"]);
    }

    #[test]
    fn expand_braces_cartesian_product() {
        let mut out = braces::expand_braces("{a,b}/{c,d}.ts");
        out.sort();
        assert_eq!(out, vec!["a/c.ts", "a/d.ts", "b/c.ts", "b/d.ts"]);
    }

    #[test]
    fn expand_braces_ignores_unbalanced() {
        assert_eq!(braces::expand_braces("{a,b"), vec!["{a,b"]);
    }

    #[test]
    fn expand_braces_falls_through_literal_group() {
        assert_eq!(braces::expand_braces("{abc}"), vec!["{abc}"]);
    }

    #[tokio::test]
    async fn finds_files_matching_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "").await.unwrap();
        tokio::fs::write(tmp.path().join("b.txt"), "").await.unwrap();
        tokio::fs::write(tmp.path().join("c.rs"), "").await.unwrap();
        let out = run_glob(&tmp, json!({"pattern": "*.txt"})).await;
        assert!(out.contains("a.txt"));
        assert!(out.contains("b.txt"));
        assert!(!out.contains("c.rs"));
    }

    #[tokio::test]
    async fn expands_braces_at_tool_layer() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.ts"), "").await.unwrap();
        tokio::fs::write(tmp.path().join("b.tsx"), "").await.unwrap();
        tokio::fs::write(tmp.path().join("c.rs"), "").await.unwrap();
        let out = run_glob(&tmp, json!({"pattern": "*.{ts,tsx}"})).await;
        assert!(out.contains("a.ts"));
        assert!(out.contains("b.tsx"));
        assert!(!out.contains("c.rs"));
    }

    #[tokio::test]
    async fn excludes_dirs_when_requested() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir(tmp.path().join("sub")).await.unwrap();
        tokio::fs::write(tmp.path().join("sub").join("a.txt"), "").await.unwrap();
        tokio::fs::write(tmp.path().join("b.txt"), "").await.unwrap();
        let out = run_glob(&tmp, json!({"pattern": "*", "include_dirs": false})).await;
        assert!(out.contains("b.txt"));
        assert!(!out.contains("sub"));
    }

    #[tokio::test]
    async fn reports_missing_root() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GlobTool::new(kaos, workspace(tmp.path()));
        let exec = tool
            .resolve_execution(json!({"pattern": "*", "path": "/does/not/exist"}))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await;
        assert!(result.is_error);
    }
}
```

### Step 4 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::glob
```

Expected: all 9 tests pass.

### Step 5 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin/glob.rs rust-ody/crates/tools-rs/src/builtin/mod.rs && git commit -m "feat(tools-rs): GlobTool"
```

---

## Task 2: Implement `GrepTool` — `files_with_matches` mode

**Depends on:** Part 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/grep.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (add `pub mod grep;`)
- Test: inline in `grep.rs`

### Step 1 — Write the failing test

Create `rust-ody/crates/tools-rs/src/builtin/grep.rs` with tests first. Stub `resolve_execution` with `todo!()`.

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};

use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolExecution, ToolError};
use crate::policies::path_access::{
    assert_path_allowed, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const DEFAULT_TIMEOUT_MS: u64 = 20_000;
const SIGTERM_GRACE_MS: u64 = 5_000;
const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
const DEFAULT_HEAD_LIMIT: usize = 250;
const RG_MAX_COLUMNS: usize = 500;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn grep_parameters() -> Value {
    InputSchema::object(vec![
        ("pattern", InputSchema::string().description("Regular expression to search for.")),
        (
            "path",
            InputSchema::string()
                .optional()
                .description("File or directory to search. Accepts an absolute path, or a path relative to the current working directory. Omit to search the current working directory."),
        ),
        ("glob", InputSchema::string().optional().description("Optional glob filter passed to ripgrep.")),
        (
            "type",
            InputSchema::string().optional().description("Optional ripgrep file type filter, such as ts or py."),
        ),
        (
            "output_mode",
            InputSchema::string_enum(&["content", "files_with_matches", "count_matches"])
                .optional()
                .description("Shape of the result. Defaults to files_with_matches."),
        ),
        ("-i", InputSchema::boolean().optional().description("Perform a case-insensitive search. Defaults to false.")),
        (
            "-n",
            InputSchema::boolean()
                .optional()
                .description("Prefix each matching line with its line number. Applies only when output_mode is content. Defaults to true."),
        ),
        (
            "-A",
            InputSchema::integer().min(0.0).optional().description("Number of lines to show after each match. Applies only when output_mode is content."),
        ),
        (
            "-B",
            InputSchema::integer().min(0.0).optional().description("Number of lines to show before each match. Applies only when output_mode is content."),
        ),
        (
            "-C",
            InputSchema::integer().min(0.0).optional().description("Number of lines to show before and after each match. Applies only when output_mode is content; takes precedence over -A and -B."),
        ),
        (
            "head_limit",
            InputSchema::integer().min(0.0).optional().description("Limit output to the first N lines/entries after offset. Defaults to 250. Pass 0 for unlimited."),
        ),
        (
            "offset",
            InputSchema::integer().min(0.0).optional().description("Number of leading lines/entries to skip before applying head_limit. Defaults to 0."),
        ),
        (
            "multiline",
            InputSchema::boolean()
                .optional()
                .description("Enable multiline matching, where the pattern can span line boundaries and . also matches newlines. Defaults to false."),
        ),
        (
            "include_ignored",
            InputSchema::boolean()
                .optional()
                .description("Also search files excluded by ignore files such as .gitignore. Sensitive files remain filtered out for safety. Defaults to false."),
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
        "Search file contents using ripgrep."
    }

    fn parameters(&self) -> Value {
        grep_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    async fn run_grep(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GrepTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await
    }

    #[tokio::test]
    async fn files_with_matches_finds_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello world").await.unwrap();
        let result = run_grep(&tmp, json!({"pattern": "hello"})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("a.txt"));
    }
}
```

Add `pub mod grep;` to `rust-ody/crates/tools-rs/src/builtin/mod.rs`.

### Step 2 — Run the failing test

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::grep
```

Expected failure: tests panic at `todo!()`.

### Step 3 — Write the minimal implementation

Replace the `todo!()` and add the helper functions. The first cut supports only `files_with_matches`. The full file:

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;

use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};
use crate::policies::path_access::{
    assert_path_allowed, is_within_directory, normalize_path, AssertPathOptions, PathAccessOperation,
    PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::policies::sensitive::is_sensitive_file;
use crate::rg_locator::{ensure_rg_path, rg_unavailable_message, EnsureRgOptions};
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const DEFAULT_TIMEOUT_MS: u64 = 20_000;
const SIGTERM_GRACE_MS: u64 = 5_000;
const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
const DEFAULT_HEAD_LIMIT: usize = 250;
const RG_MAX_COLUMNS: usize = 500;

const VCS_DIRECTORIES: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"];
const SENSITIVE_GLOBS: &[&str] = &[
    "**/.env",
    "**/id_rsa",
    "**/id_ed25519",
    "**/id_ecdsa",
    "**/.aws/credentials",
    "**/.gcp/credentials",
];

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn grep_parameters() -> Value {
    InputSchema::object(vec![
        ("pattern", InputSchema::string().description("Regular expression to search for.")),
        (
            "path",
            InputSchema::string()
                .optional()
                .description("File or directory to search. Accepts an absolute path, or a path relative to the current working directory. Omit to search the current working directory."),
        ),
        ("glob", InputSchema::string().optional().description("Optional glob filter passed to ripgrep.")),
        (
            "type",
            InputSchema::string().optional().description("Optional ripgrep file type filter, such as ts or py."),
        ),
        (
            "output_mode",
            InputSchema::string_enum(&["content", "files_with_matches", "count_matches"])
                .optional()
                .description("Shape of the result. Defaults to files_with_matches."),
        ),
        ("-i", InputSchema::boolean().optional().description("Perform a case-insensitive search. Defaults to false.")),
        (
            "-n",
            InputSchema::boolean()
                .optional()
                .description("Prefix each matching line with its line number. Applies only when output_mode is content. Defaults to true."),
        ),
        (
            "-A",
            InputSchema::integer().min(0.0).optional().description("Number of lines to show after each match. Applies only when output_mode is content."),
        ),
        (
            "-B",
            InputSchema::integer().min(0.0).optional().description("Number of lines to show before each match. Applies only when output_mode is content."),
        ),
        (
            "-C",
            InputSchema::integer().min(0.0).optional().description("Number of lines to show before and after each match. Applies only when output_mode is content; takes precedence over -A and -B."),
        ),
        (
            "head_limit",
            InputSchema::integer().min(0.0).optional().description("Limit output to the first N lines/entries after offset. Defaults to 250. Pass 0 for unlimited."),
        ),
        (
            "offset",
            InputSchema::integer().min(0.0).optional().description("Number of leading lines/entries to skip before applying head_limit. Defaults to 0."),
        ),
        (
            "multiline",
            InputSchema::boolean()
                .optional()
                .description("Enable multiline matching, where the pattern can span line boundaries and . also matches newlines. Defaults to false."),
        ),
        (
            "include_ignored",
            InputSchema::boolean()
                .optional()
                .description("Also search files excluded by ignore files such as .gitignore. Sensitive files remain filtered out for safety. Defaults to false."),
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
        "Search file contents using ripgrep."
    }

    fn parameters(&self) -> Value {
        grep_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let pattern = args
            .get("pattern")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("pattern is required".into()))?;
        let optional_path = args.get("path").and_then(Value::as_str);

        let path_class = kaos_path_class(&self.kaos);
        let search_path = match optional_path {
            Some(p) => assert_path_allowed(
                p,
                &self.kaos.getcwd(),
                &self.workspace,
                AssertPathOptions {
                    mode: PathAccessOperation::Search,
                    check_sensitive: Some(false),
                    path_class: Some(path_class),
                },
            )?,
            None => self.workspace.workspace_dir.clone(),
        };

        let approval_rule = literal_rule_pattern(self.name(), pattern);
        let kaos = self.kaos.clone();
        let workspace = self.workspace.clone();
        let pattern = pattern.to_string();
        let search_path2 = search_path.clone();
        let args2 = args.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::search_tree(&search_path),
            description: format!("Searching for '{}' in {}", pattern, search_path),
            approval_rule,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let workspace = workspace.clone();
                let pattern = pattern.clone();
                let search_path = search_path2.clone();
                let args = args2.clone();
                Box::pin(async move { execution(kaos, workspace, pattern, search_path, args, ctx).await })
            }),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrepMode {
    Content,
    FilesWithMatches,
    CountMatches,
}

impl GrepMode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "content" => Some(GrepMode::Content),
            "files_with_matches" => Some(GrepMode::FilesWithMatches),
            "count_matches" => Some(GrepMode::CountMatches),
            _ => None,
        }
    }
}

async fn execution(
    kaos: Kaos,
    workspace: WorkspaceConfig,
    pattern: String,
    search_path: String,
    args: Value,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text("Aborted before search started".into(), "Aborted".into());
    }

    let mode = args
        .get("output_mode")
        .and_then(Value::as_str)
        .and_then(GrepMode::from_str)
        .unwrap_or(GrepMode::FilesWithMatches);

    let rg_path = match ensure_rg_path(EnsureRgOptions {
        share_dir: None,
        cancel: None,
    })
    .await
    {
        Ok(r) => r.path,
        Err(e) => {
            return ExecutableToolResult::error_text(
                rg_unavailable_message(&e),
                "rg unavailable".into(),
            );
        }
    };

    let rg_args = build_rg_args(&rg_path, &args, mode, &[search_path.clone()]);
    let rg_arg_refs: Vec<&str> = rg_args.iter().map(|s| s.as_str()).collect();

    let proc = match kaos.exec(&rg_arg_refs).await {
        Ok(p) => Arc::new(p),
        Err(e) => {
            let msg = if e.kind() == ErrorKind::NotFound {
                rg_unavailable_message(&crate::rg_locator::RgError::Io(std::sync::Arc::new(e)))
            } else {
                format!("Failed to spawn rg: {}", e)
            };
            return ExecutableToolResult::error_text(msg.clone(), msg);
        }
    };

    let proc_for_abort = proc.clone();
    let abort_signal = ctx.signal.clone();
    let abort_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if abort_signal.aborted() {
                let _ = proc_for_abort.kill(None).await;
                return;
            }
            if proc_for_abort.exit_code().is_some() {
                return;
            }
        }
    });

    let timeout = tokio::time::timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS), proc.wait());
    let (exit_code, timed_out) = match timeout.await {
        Ok(code) => (code, false),
        Err(_) => {
            let _ = proc.kill(None).await;
            // Give SIGTERM a grace period, then SIGKILL.
            let grace = tokio::time::timeout(Duration::from_millis(SIGTERM_GRACE_MS), proc.wait());
            let code = match grace.await {
                Ok(c) => c,
                Err(_) => {
                    let _ = proc.kill(Some("SIGKILL")).await;
                    proc.wait().await
                }
            };
            (code, true)
        }
    };
    abort_handle.abort();

    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text("Grep aborted".into(), "Aborted".into());
    }

    let stdout = proc.stdout().await;
    let stderr = proc.stderr().await;
    let stdout_text = String::from_utf8_lossy(&stdout).to_string();
    let stderr_text = String::from_utf8_lossy(&stderr).to_string();
    let buffer_truncated = stdout.len() >= MAX_OUTPUT_BYTES;

    if exit_code != 0 && exit_code != 1 && !timed_out {
        return ExecutableToolResult::error_text(
            format_ripgrep_error(exit_code, &stderr_text),
            "rg error".into(),
        );
    }

    if timed_out && stdout_text.trim().is_empty() {
        return ExecutableToolResult::error_text(
            format!("Grep timed out after {}s. Try a more specific path or pattern.", DEFAULT_TIMEOUT_MS / 1000),
            "Timeout".into(),
        );
    }

    let path_class = kaos_path_class(&kaos);
    let workspace_dir = workspace.workspace_dir.clone();

    match mode {
        GrepMode::FilesWithMatches => {
            let mut paths = parse_files_with_matches(&stdout_text);
            paths.sort();
            paths.dedup();
            paths.retain(|p| !is_sensitive_file(p));

            // Sort by mtime descending.
            let mut with_mtime: Vec<(String, f64)> = Vec::new();
            for p in paths {
                let mtime = kaos.stat(&p, false).await.map(|s| s.st_mtime).unwrap_or(0.0);
                with_mtime.push((p, mtime));
            }
            with_mtime.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let head_limit = args.get("head_limit").and_then(Value::as_u64).unwrap_or(DEFAULT_HEAD_LIMIT as u64) as usize;

            let display: Vec<String> = with_mtime
                .iter()
                .map(|(p, _)| relativize_if_under(p, &workspace_dir, path_class))
                .collect();
            let after_offset = if offset > 0 { display.iter().skip(offset).cloned().collect::<Vec<_>>() } else { display };
            let limit_active = head_limit > 0;
            let limited = if limit_active {
                after_offset.into_iter().take(head_limit).collect::<Vec<_>>()
            } else {
                after_offset
            };
            let pagination_truncated = limit_active && limited.len() < after_offset.len();

            let mut lines = limited;
            if pagination_truncated {
                lines.push(format!(
                    "Results truncated to {} lines (total: {}). Use offset={} to see more.",
                    head_limit,
                    with_mtime.len(),
                    offset + head_limit
                ));
            }
            if buffer_truncated {
                lines.push(format!(
                    "[stdout truncated at {} bytes; incomplete trailing line omitted]",
                    MAX_OUTPUT_BYTES
                ));
            }
            if timed_out {
                lines.push(format!(
                    "Grep timed out after {}s; partial results returned",
                    DEFAULT_TIMEOUT_MS / 1000
                ));
            }

            if lines.is_empty() {
                ExecutableToolResult::ok_text("No matches found".into())
            } else {
                ExecutableToolResult::ok_text(lines.join("\n"))
            }
        }
        _ => ExecutableToolResult::error_text(
            "Mode not yet implemented".into(),
            "Not implemented".into(),
        ),
    }
}

fn build_rg_args(rg_path: &std::path::Path, args: &Value, mode: GrepMode, search_paths: &[String]) -> Vec<String> {
    let mut cmd: Vec<String> = Vec::new();
    cmd.push(rg_path.to_string_lossy().to_string());
    cmd.push("--hidden".into());
    if mode != GrepMode::Content {
        cmd.push("--max-columns".into());
        cmd.push(RG_MAX_COLUMNS.to_string());
    }
    cmd.push("--null".into());
    for dir in VCS_DIRECTORIES {
        cmd.push("--glob".into());
        cmd.push(format!("!{}", dir));
    }

    match mode {
        GrepMode::FilesWithMatches => cmd.push("-l".into()),
        GrepMode::CountMatches => {
            cmd.push("--count-matches".into());
            cmd.push("--with-filename".into());
        }
        GrepMode::Content => {
            cmd.push("--with-filename".into());
            let line_numbers = args.get("-n").and_then(Value::as_bool).unwrap_or(true);
            if line_numbers {
                cmd.push("-n".into());
            } else {
                cmd.push("--field-context-separator".into());
                cmd.push(":".into());
            }
            if let Some(c) = args.get("-C").and_then(Value::as_i64) {
                cmd.push("-C".into());
                cmd.push(c.to_string());
            } else {
                if let Some(a) = args.get("-A").and_then(Value::as_i64) {
                    cmd.push("-A".into());
                    cmd.push(a.to_string());
                }
                if let Some(b) = args.get("-B").and_then(Value::as_i64) {
                    cmd.push("-B".into());
                    cmd.push(b.to_string());
                }
            }
        }
    }

    if args.get("-i").and_then(Value::as_bool).unwrap_or(false) {
        cmd.push("-i".into());
    }
    if let Some(g) = args.get("glob").and_then(Value::as_str) {
        cmd.push("--glob".into());
        cmd.push(g.into());
    }
    if let Some(t) = args.get("type").and_then(Value::as_str) {
        cmd.push("--type".into());
        cmd.push(t.into());
    }
    if args.get("multiline").and_then(Value::as_bool).unwrap_or(false) {
        cmd.push("-U".into());
        cmd.push("--multiline-dotall".into());
    }
    if args.get("include_ignored").and_then(Value::as_bool).unwrap_or(false) {
        cmd.push("--no-ignore".into());
    }
    for glob in SENSITIVE_GLOBS {
        cmd.push("--glob".into());
        cmd.push(format!("!{}", glob));
    }

    let pattern = args.get("pattern").and_then(Value::as_str).unwrap_or("");
    cmd.push("--".into());
    cmd.push(pattern.into());
    cmd.extend(search_paths.iter().cloned());
    cmd
}

fn parse_files_with_matches(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\0')
        .map(|s| s.trim_end_matches('\r').trim_end_matches('\n').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn format_ripgrep_error(exit_code: i32, stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return format!("Failed to grep: ripgrep exited with code {}", exit_code);
    }
    format!("Failed to grep: {}\n\nripgrep stderr:\n{}", trimmed.lines().last().unwrap_or(trimmed), trimmed)
}

fn relativize_if_under(candidate: &str, base: &str, path_class: PathClass) -> String {
    let norm_candidate = normalize_path(candidate, path_class);
    let norm_base = normalize_path(base, path_class);
    let cmp_candidate = if path_class == PathClass::Win32 {
        norm_candidate.to_lowercase()
    } else {
        norm_candidate.clone()
    };
    let cmp_base = if path_class == PathClass::Win32 {
        norm_base.to_lowercase()
    } else {
        norm_base.clone()
    };
    if cmp_candidate == cmp_base {
        return ".".to_string();
    }
    let prefix = if cmp_base.ends_with('/') {
        cmp_base
    } else {
        format!("{}/", cmp_base)
    };
    if cmp_candidate.starts_with(&prefix) {
        return norm_candidate[prefix.len()..].to_string();
    }
    norm_candidate
}

#[cfg(test)]
mod tests {
    use super::*;
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

    async fn run_grep(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = GrepTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await
    }

    #[tokio::test]
    async fn files_with_matches_finds_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello world").await.unwrap();
        let result = run_grep(&tmp, json!({"pattern": "hello"})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("a.txt"));
    }

    #[tokio::test]
    async fn files_with_matches_is_case_sensitive_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "Hello world").await.unwrap();
        let result = run_grep(&tmp, json!({"pattern": "hello"})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(!text.contains("a.txt"));
    }

    #[tokio::test]
    async fn files_with_matches_case_insensitive_when_requested() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "Hello world").await.unwrap();
        let result = run_grep(&tmp, json!({"pattern": "hello", "-i": true})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("a.txt"));
    }

    #[tokio::test]
    async fn files_with_matches_filters_sensitive_files() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join(".env"), "SECRET=1").await.unwrap();
        let result = run_grep(&tmp, json!({"pattern": "SECRET"})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(!text.contains(".env"));
    }
}
```

### Step 4 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::grep
```

Expected: all 4 tests pass.

### Step 5 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin/grep.rs rust-ody/crates/tools-rs/src/builtin/mod.rs && git commit -m "feat(tools-rs): GrepTool files_with_matches mode"
```

---

## Task 3: Extend `GrepTool` to `content` and `count_matches` modes

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/grep.rs` (add parsing/formatting arms)
- Test: inline additions in `grep.rs`

### Step 1 — Write the failing tests

Append these tests to the existing `#[cfg(test)]` block in `grep.rs`:

```rust
    #[tokio::test]
    async fn content_mode_shows_matching_lines() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "line one\nhello world\nline three\n")
            .await
            .unwrap();
        let result = run_grep(
            &tmp,
            json!({"pattern": "hello", "output_mode": "content"}),
        )
        .await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("a.txt"));
        assert!(text.contains("hello world"));
    }

    #[tokio::test]
    async fn count_matches_mode_reports_total() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello hello").await.unwrap();
        tokio::fs::write(tmp.path().join("b.txt"), "hello").await.unwrap();
        let result = run_grep(
            &tmp,
            json!({"pattern": "hello", "output_mode": "count_matches"}),
        )
        .await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("a.txt:2"));
        assert!(text.contains("b.txt:1"));
    }
```

Run the tests and confirm they FAIL with "Mode not yet implemented".

### Step 2 — Write the minimal implementation

Replace the placeholder `_ => ExecutableToolResult::error_text(...)` arm in `execution` with full `content` and `count_matches` handling. The implementation arms use the helper functions below.

Add these helper functions to the bottom of `grep.rs` (before `#[cfg(test)]`):

```rust
#[derive(Debug, Clone)]
enum ParsedLine {
    Record { path: String, payload: String },
    Separator,
    Legacy { text: String },
}

fn parse_ripgrep_output(text: &str, mode: GrepMode) -> Vec<ParsedLine> {
    if text.is_empty() {
        return Vec::new();
    }
    if !text.contains('\0') {
        return text
            .split('\n')
            .map(|s| s.trim_end_matches('\r'))
            .filter(|s| !s.is_empty())
            .map(|s| {
                if mode == GrepMode::Content && s == "--" {
                    ParsedLine::Separator
                } else {
                    ParsedLine::Legacy { text: s.to_string() }
                }
            })
            .collect();
    }

    if mode == GrepMode::FilesWithMatches {
        return text
            .split('\0')
            .map(|s| s.trim_end_matches('\r').trim_end_matches('\n'))
            .filter(|s| !s.is_empty())
            .map(|s| ParsedLine::Record {
                path: s.to_string(),
                payload: String::new(),
            })
            .collect();
    }

    let mut records = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        if text[cursor..].starts_with('\n') {
            cursor += 1;
            continue;
        }
        if text[cursor..].starts_with("--\r\n") {
            records.push(ParsedLine::Separator);
            cursor += 4;
            continue;
        }
        if text[cursor..].starts_with("--\n") {
            records.push(ParsedLine::Separator);
            cursor += 3;
            continue;
        }

        let nul = match text[cursor..].find('\0') {
            Some(i) => cursor + i,
            None => {
                let tail = text[cursor..].trim_end_matches('\r').trim_end_matches('\n');
                if !tail.is_empty() {
                    records.push(ParsedLine::Legacy { text: tail.to_string() });
                }
                break;
            }
        };
        let newline = text[nul + 1..].find('\n').map(|i| nul + 1 + i);
        let path = text[cursor..nul].to_string();
        let payload_end = newline.unwrap_or(text.len());
        let payload = text[nul + 1..payload_end]
            .trim_end_matches('\r')
            .to_string();
        records.push(ParsedLine::Record { path, payload });
        cursor = newline.map(|i| i + 1).unwrap_or(text.len());
    }
    records
}

fn parsed_path(line: &ParsedLine, mode: GrepMode) -> Option<String> {
    match line {
        ParsedLine::Record { path, .. } => Some(path.clone()),
        ParsedLine::Separator => None,
        ParsedLine::Legacy { text } => {
            if mode == GrepMode::FilesWithMatches {
                return Some(text.clone());
            }
            if mode == GrepMode::CountMatches {
                let idx = text.rfind(':')?;
                if idx > 0 {
                    return Some(text[..idx].to_string());
                }
            }
            extract_content_path(text)
        }
    }
}

fn extract_content_path(line: &str) -> Option<String> {
    // Try "path:line:..." or "path-line ..."
    let re = regex::Regex::new(r"^(.*?)([:-])(\d+)\2").unwrap();
    re.captures(line).map(|c| c[1].to_string())
}

fn filter_sensitive(lines: Vec<ParsedLine>, mode: GrepMode) -> (Vec<ParsedLine>, HashSet<String>) {
    let mut filtered = HashSet::new();
    let mut kept = Vec::new();
    for line in lines {
        if let Some(path) = parsed_path(&line, mode) {
            if is_sensitive_file(&path) {
                filtered.insert(path);
                continue;
            }
        }
        kept.push(line);
    }
    if mode == GrepMode::Content {
        kept = normalize_separators(kept);
    }
    (kept, filtered)
}

fn normalize_separators(lines: Vec<ParsedLine>) -> Vec<ParsedLine> {
    let mut out = Vec::new();
    for line in lines {
        if matches!(line, ParsedLine::Separator) && (out.is_empty() || matches!(out.last().unwrap(), ParsedLine::Separator)) {
            continue;
        }
        out.push(line);
    }
    while out.last().map_or(false, |l| matches!(l, ParsedLine::Separator)) {
        out.pop();
    }
    out
}

fn format_content_line(
    line: &ParsedLine,
    workspace_dir: &str,
    path_class: PathClass,
    line_numbers: bool,
) -> String {
    match line {
        ParsedLine::Separator => "--".to_string(),
        ParsedLine::Record { path, payload } => {
            let display_path = relativize_if_under(path, workspace_dir, path_class);
            if line_numbers {
                format!("{}:{}", display_path, payload)
            } else {
                format!("{}:{}", display_path, payload)
            }
        }
        ParsedLine::Legacy { text } => text.clone(),
    }
}

fn format_count_line(line: &ParsedLine, workspace_dir: &str, path_class: PathClass) -> String {
    match line {
        ParsedLine::Record { path, payload } => {
            let display_path = relativize_if_under(path, workspace_dir, path_class);
            format!("{}:{}", display_path, payload)
        }
        ParsedLine::Legacy { text } => text.clone(),
        ParsedLine::Separator => String::new(),
    }
}

fn format_count_summary(lines: &[ParsedLine]) -> String {
    let mut total = 0usize;
    let mut files = 0usize;
    for line in lines {
        let raw = match line {
            ParsedLine::Record { payload, .. } => payload.as_str(),
            ParsedLine::Legacy { text } => {
                text.rfind(':').map(|i| &text[i + 1..]).unwrap_or("")
            }
            ParsedLine::Separator => continue,
        };
        if let Ok(n) = raw.parse::<usize>() {
            total += n;
            files += 1;
        }
    }
    format!(
        "Found {} {} across {} {}.",
        total,
        if total == 1 { "occurrence" } else { "occurrences" },
        files,
        if files == 1 { "file" } else { "files" }
    )
}
```

Then replace the `match mode { ... }` body in `execution` with:

```rust
    let path_class = kaos_path_class(&kaos);
    let workspace_dir = workspace.workspace_dir.clone();

    let records = parse_ripgrep_output(&stdout_text, mode);
    let (mut records, filtered_sensitive) = filter_sensitive(records, mode);

    if mode == GrepMode::FilesWithMatches {
        let mut paths: Vec<String> = Vec::new();
        for r in records {
            if let ParsedLine::Record { path, .. } = r {
                paths.push(path);
            }
        }
        paths.sort();
        paths.dedup();
        let mut with_mtime: Vec<(String, f64)> = Vec::new();
        for p in paths {
            let mtime = kaos.stat(&p, false).await.map(|s| s.st_mtime).unwrap_or(0.0);
            with_mtime.push((p, mtime));
        }
        with_mtime.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let head_limit = args.get("head_limit").and_then(Value::as_u64).unwrap_or(DEFAULT_HEAD_LIMIT as u64) as usize;

        let display: Vec<String> = with_mtime
            .iter()
            .map(|(p, _)| relativize_if_under(p, &workspace_dir, path_class))
            .collect();
        let after_offset = if offset > 0 { display.iter().skip(offset).cloned().collect::<Vec<_>>() } else { display };
        let limit_active = head_limit > 0;
        let limited = if limit_active { after_offset.into_iter().take(head_limit).collect::<Vec<_>>() } else { after_offset };
        let pagination_truncated = limit_active && limited.len() < with_mtime.len().saturating_sub(offset);

        let mut lines = limited;
        if filtered_sensitive.len() > 0 {
            let shown: Vec<String> = filtered_sensitive
                .iter()
                .map(|p| relativize_if_under(p, &workspace_dir, path_class))
                .collect();
            lines.push(format!("Filtered {} sensitive file(s): {}", shown.len(), shown.join(", ")));
        }
        if pagination_truncated {
            lines.push(format!(
                "Results truncated to {} lines (total: {}). Use offset={} to see more.",
                head_limit,
                with_mtime.len(),
                offset + head_limit
            ));
        }
        if buffer_truncated {
            lines.push(format!(
                "[stdout truncated at {} bytes; incomplete trailing line omitted]",
                MAX_OUTPUT_BYTES
            ));
        }
        if timed_out {
            lines.push(format!(
                "Grep timed out after {}s; partial results returned",
                DEFAULT_TIMEOUT_MS / 1000
            ));
        }
        return if lines.is_empty() {
            ExecutableToolResult::ok_text("No matches found".into())
        } else {
            ExecutableToolResult::ok_text(lines.join("\n"))
        };
    }

    // Content and count modes.
    let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let head_limit = args.get("head_limit").and_then(Value::as_u64).unwrap_or(DEFAULT_HEAD_LIMIT as u64) as usize;

    let after_offset = if offset > 0 { records.into_iter().skip(offset).collect::<Vec<_>>() } else { records };
    let limit_active = head_limit > 0;
    let limited = if limit_active { after_offset.into_iter().take(head_limit).collect::<Vec<_>>() } else { after_offset };
    let pagination_truncated = limit_active && limited.len() < records.len().saturating_sub(offset);

    let line_numbers = args.get("-n").and_then(Value::as_bool).unwrap_or(true);
    let mut body_lines: Vec<String> = match mode {
        GrepMode::Content => limited
            .iter()
            .map(|l| format_content_line(l, &workspace_dir, path_class, line_numbers))
            .collect(),
        GrepMode::CountMatches => limited
            .iter()
            .map(|l| format_count_line(l, &workspace_dir, path_class))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => unreachable!(),
    };

    let mut messages: Vec<String> = Vec::new();
    if filtered_sensitive.len() > 0 {
        let shown: Vec<String> = filtered_sensitive
            .iter()
            .map(|p| relativize_if_under(p, &workspace_dir, path_class))
            .collect();
        messages.push(format!("Filtered {} sensitive file(s): {}", shown.len(), shown.join(", ")));
    }
    if mode == GrepMode::CountMatches && !body_lines.is_empty() {
        messages.push(format_count_summary(&limited));
    }
    if pagination_truncated {
        messages.push(format!(
            "Results truncated to {} lines (total: {}). Use offset={} to see more.",
            head_limit,
            records.len(),
            offset + head_limit
        ));
    }
    if buffer_truncated {
        messages.push(format!(
            "[stdout truncated at {} bytes; incomplete trailing line omitted]",
            MAX_OUTPUT_BYTES
        ));
    }
    if timed_out {
        messages.push(format!(
            "Grep timed out after {}s; partial results returned",
            DEFAULT_TIMEOUT_MS / 1000
        ));
    }

    let body = if body_lines.is_empty() {
        if filtered_sensitive.len() > 0 {
            "No non-sensitive matches found".to_string()
        } else {
            "No matches found".to_string()
        }
    } else {
        body_lines.join("\n")
    };

    let output = if messages.is_empty() {
        body
    } else if body_lines.is_empty() {
        messages.join("\n")
    } else {
        format!("{}\n{}", body, messages.join("\n"))
    };

    ExecutableToolResult::ok_text(output)
```

Note: this replacement also removes the previous `files_with_matches` arm duplication by folding it into the unified flow. Update the `match mode { ... }` in `execution` accordingly.

### Step 3 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::grep
```

Expected: all 6 tests pass.

### Step 4 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin/grep.rs && git commit -m "feat(tools-rs): GrepTool content and count_matches modes"
```

---

## Local Self-Review

- [x] 1. Spec coverage: 4.4.1.4 (GlobTool brace expansion, MAX_MATCHES, include_dirs, mtime sort) covered by Task 1; 4.4.1.5 (GrepTool rg args, output modes, sensitive filtering, pagination) covered by Tasks 2–3.
- [x] 2. Placeholder scan: no TODO/TBD; every function body is concrete.
- [x] 3. No phantom tasks: each task adds code + passing tests + commit.
- [x] 4. Dependency soundness: Task 1 depends only on Part 1; Tasks 2–3 depend on Part 1. `GrepTool` uses `rg_locator` and `sensitive` from 4.4.0. No later symbols used.
- [x] 5. Shared-signature changes: no shared signatures changed; `BuiltinTool` trait is unchanged.
- [x] 6. Test-the-risk: GlobTool tests assert brace expansion, file/dir filtering, and missing-root rejection. GrepTool tests assert actual rg process output, case sensitivity, sensitive filtering, content lines, and count summary. Constants (MAX_MATCHES, RG_MAX_COLUMNS, timeout) are traced.
- [x] 7. Type consistency: reuses `ExecutableToolResult`, `PathClass`, `AssertPathOptions`, `ToolAccesses`, `InputSchema` exactly as defined earlier.
