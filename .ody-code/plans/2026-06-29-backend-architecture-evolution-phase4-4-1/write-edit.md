# Part 2 — Line-Endings Helper + `WriteTool` + `EditTool`

Scope: extend the line-ending helper with `materialize_model_text`, then implement the two state-mutating file tools (`WriteTool`, `EditTool`) on top of the `BuiltinTool` trait from Part 1.

## Dependency Overview

```
Part 1: trait-read.md
  │
  ├──► Task 1 (line_endings.rs extension)
  │      ├──► Task 2 (WriteTool)
  │      └──► Task 3 (EditTool)
```

- `WriteTool` and `EditTool` are independent of each other after Task 1.
- Both depend only on symbols created in Part 1 and on `kaos_rs::text::ErrorMode` (existing 4.4.0).

## Task 1: Extend `line_endings.rs` with `materialize_model_text`

**Depends on:** Part 1 (`builtin/mod.rs`, `line_endings.rs`)

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/line_endings.rs` (add function + tests)

### Step 1 — Write the failing test

Replace the existing test module with the expanded version that expects `materialize_model_text`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lf() {
        assert_eq!(detect_line_ending_style("a\nb"), LineEndingStyle::Lf);
    }

    #[test]
    fn detects_crlf() {
        assert_eq!(detect_line_ending_style("a\r\nb"), LineEndingStyle::Crlf);
    }

    #[test]
    fn detects_mixed_when_lone_cr_present() {
        assert_eq!(detect_line_ending_style("a\rb"), LineEndingStyle::Mixed);
    }

    #[test]
    fn detects_mixed_when_crlf_and_lf_mixed() {
        assert_eq!(detect_line_ending_style("a\r\nb\nc"), LineEndingStyle::Mixed);
    }

    #[test]
    fn to_model_view_normalizes_crlf() {
        let v = to_model_text_view("a\r\nb");
        assert_eq!(v.text, "a\nb");
        assert_eq!(v.line_ending_style, LineEndingStyle::Crlf);
    }

    #[test]
    fn makes_lone_cr_visible() {
        assert_eq!(make_carriage_returns_visible("a\rb"), "a\\rb");
    }

    #[test]
    fn materialize_leaves_lf_unchanged() {
        assert_eq!(materialize_model_text("a\nb", LineEndingStyle::Lf), "a\nb");
    }

    #[test]
    fn materialize_leaves_mixed_unchanged() {
        assert_eq!(materialize_model_text("a\r\nb\nc", LineEndingStyle::Mixed), "a\r\nb\nc");
    }

    #[test]
    fn materialize_converts_lf_to_crlf() {
        assert_eq!(materialize_model_text("a\nb", LineEndingStyle::Crlf), "a\r\nb");
    }

    #[test]
    fn materialize_normalizes_existing_crlf_before_expanding() {
        // Model view text may still contain a literal \r\n if the user included it.
        // The function first normalizes all \r\n to \n, then expands all \n to \r\n.
        assert_eq!(materialize_model_text("a\r\nb", LineEndingStyle::Crlf), "a\r\nb");
    }
}
```

### Step 2 — Run the failing test

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::line_endings::tests
```

Expected failure: compilation error `cannot find function materialize_model_text in this scope`.

### Step 3 — Write the minimal implementation

Update `rust-ody/crates/tools-rs/src/builtin/line_endings.rs` to include the new function. The full file becomes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndingStyle {
    Lf,
    Crlf,
    Mixed,
}

pub fn detect_line_ending_style(text: &str) -> LineEndingStyle {
    let mut has_crlf = false;
    let mut has_lf = false;
    let mut has_lone_cr = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\r' {
            if i + 1 < chars.len() && chars[i + 1] == '\n' {
                has_crlf = true;
                i += 2;
                continue;
            } else {
                has_lone_cr = true;
            }
        } else if ch == '\n' {
            has_lf = true;
        }
        i += 1;
    }
    if has_lone_cr || (has_crlf && has_lf) {
        LineEndingStyle::Mixed
    } else if has_crlf {
        LineEndingStyle::Crlf
    } else {
        LineEndingStyle::Lf
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTextView {
    pub text: String,
    pub line_ending_style: LineEndingStyle,
}

pub fn to_model_text_view(raw: &str) -> ModelTextView {
    let style = detect_line_ending_style(raw);
    let text = if style == LineEndingStyle::Crlf {
        raw.replace("\r\n", "\n")
    } else {
        raw.to_string()
    };
    ModelTextView { text, line_ending_style: style }
}

pub fn materialize_model_text(text: &str, line_ending_style: LineEndingStyle) -> String {
    if line_ending_style != LineEndingStyle::Crlf {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

pub fn make_carriage_returns_visible(text: &str) -> String {
    text.replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lf() {
        assert_eq!(detect_line_ending_style("a\nb"), LineEndingStyle::Lf);
    }

    #[test]
    fn detects_crlf() {
        assert_eq!(detect_line_ending_style("a\r\nb"), LineEndingStyle::Crlf);
    }

    #[test]
    fn detects_mixed_when_lone_cr_present() {
        assert_eq!(detect_line_ending_style("a\rb"), LineEndingStyle::Mixed);
    }

    #[test]
    fn detects_mixed_when_crlf_and_lf_mixed() {
        assert_eq!(detect_line_ending_style("a\r\nb\nc"), LineEndingStyle::Mixed);
    }

    #[test]
    fn to_model_view_normalizes_crlf() {
        let v = to_model_text_view("a\r\nb");
        assert_eq!(v.text, "a\nb");
        assert_eq!(v.line_ending_style, LineEndingStyle::Crlf);
    }

    #[test]
    fn makes_lone_cr_visible() {
        assert_eq!(make_carriage_returns_visible("a\rb"), "a\\rb");
    }

    #[test]
    fn materialize_leaves_lf_unchanged() {
        assert_eq!(materialize_model_text("a\nb", LineEndingStyle::Lf), "a\nb");
    }

    #[test]
    fn materialize_leaves_mixed_unchanged() {
        assert_eq!(materialize_model_text("a\r\nb\nc", LineEndingStyle::Mixed), "a\r\nb\nc");
    }

    #[test]
    fn materialize_converts_lf_to_crlf() {
        assert_eq!(materialize_model_text("a\nb", LineEndingStyle::Crlf), "a\r\nb");
    }

    #[test]
    fn materialize_normalizes_existing_crlf_before_expanding() {
        assert_eq!(materialize_model_text("a\r\nb", LineEndingStyle::Crlf), "a\r\nb");
    }
}
```

### Step 4 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::line_endings::tests
```

Expected: 10 tests pass.

### Step 5 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin/line_endings.rs && git commit -m "feat(tools-rs): materialize_model_text for CRLF round-trip"
```

---

## Task 2: Implement `WriteTool`

**Depends on:** Task 1 (line-ending helper), Part 1 (`BuiltinTool` trait)

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/write.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (add `pub mod write;`)
- Test: inline in `write.rs`

### Step 1 — Write the failing test

Create `rust-ody/crates/tools-rs/src/builtin/write.rs` with the tests first. The tool body is a stub that returns `todo!()`.

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

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn dirname(path: &str) -> String {
    if path == "/" {
        return "/".to_string();
    }
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(idx) => path[..idx].to_string(),
        None => ".".to_string(),
    }
}

fn write_parameters() -> Value {
    InputSchema::object(vec![
        (
            "path",
            InputSchema::string()
                .description("Path to the file to create, append to, or completely overwrite. Relative paths resolve against the working directory; a path outside the working directory must be absolute. The parent directory must already exist."),
        ),
        (
            "content",
            InputSchema::string()
                .description("Raw full file content to write exactly as provided. This does not use the Read/Edit text view."),
        ),
        (
            "mode",
            InputSchema::string_enum(&["overwrite", "append"])
                .optional()
                .description("Write mode. Defaults to overwrite. append adds content to the end exactly as provided and does not add a newline."),
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
        "Write raw content to a file, overwriting by default or appending when requested."
    }

    fn parameters(&self) -> Value {
        write_parameters()
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

    async fn run_write(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = WriteTool::new(kaos.clone(), workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await
    }

    #[tokio::test]
    async fn writes_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_write(&tmp, json!({"path": "out.txt", "content": "hello"})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("Wrote 5 bytes to out.txt"));
        let bytes = tokio::fs::read(tmp.path().join("out.txt")).await.unwrap();
        assert_eq!(bytes, b"hello");
    }
}
```

Add `pub mod write;` to `rust-ody/crates/tools-rs/src/builtin/mod.rs`.

### Step 2 — Run the failing test

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::write
```

Expected failure: test panics at `todo!()` in `resolve_execution`.

### Step 3 — Write the minimal implementation

Replace the `todo!()` body in `resolve_execution` and add the `execution` helper. The full file becomes:

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};
use std::io::ErrorKind;

use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};
use crate::policies::path_access::{
    assert_path_allowed, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn dirname(path: &str) -> String {
    if path == "/" {
        return "/".to_string();
    }
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(idx) => path[..idx].to_string(),
        None => ".".to_string(),
    }
}

fn write_parameters() -> Value {
    InputSchema::object(vec![
        (
            "path",
            InputSchema::string()
                .description("Path to the file to create, append to, or completely overwrite. Relative paths resolve against the working directory; a path outside the working directory must be absolute. The parent directory must already exist."),
        ),
        (
            "content",
            InputSchema::string()
                .description("Raw full file content to write exactly as provided. This does not use the Read/Edit text view."),
        ),
        (
            "mode",
            InputSchema::string_enum(&["overwrite", "append"])
                .optional()
                .description("Write mode. Defaults to overwrite. append adds content to the end exactly as provided and does not add a newline."),
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
        "Write raw content to a file, overwriting by default or appending when requested."
    }

    fn parameters(&self) -> Value {
        write_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("path is required".into()))?;
        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("content is required".into()))?;

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
        Some(s) => s,
        None => {
            return ExecutableToolResult::error_text(
                "content is required".into(),
                "Invalid args".into(),
            );
        }
    };
    let mode = args.get("mode").and_then(Value::as_str).unwrap_or("overwrite");
    if mode != "overwrite" && mode != "append" {
        return ExecutableToolResult::error_text(
            format!("Invalid mode: {}", mode),
            "Invalid args".into(),
        );
    }

    let parent = dirname(&safe_path);
    match kaos.stat(&parent, false).await {
        Ok(stat) => {
            if (stat.st_mode & S_IFMT) != S_IFDIR {
                return ExecutableToolResult::error_text(
                    format!("Parent path is not a directory: {}.", parent),
                    "Not a directory".into(),
                );
            }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return ExecutableToolResult::error_text(
                format!(
                    "Parent directory does not exist: {}. Create it before writing this file.",
                    parent
                ),
                "Missing parent directory".into(),
            );
        }
        Err(_) => {
            // Inconclusive check; let the underlying write surface the real error.
        }
    }

    let write_mode = if mode == "append" { Some("a") } else { None };
    if let Err(e) = kaos.write_text(&safe_path, content, write_mode, None).await {
        let message = if e.kind() == ErrorKind::NotFound {
            format!("Failed to write {}: parent directory does not exist.", display_path)
        } else {
            format!("Failed to write {}: {}", display_path, e)
        };
        return ExecutableToolResult::error_text(message.clone(), message);
    }

    let bytes_written = content.len();
    let verb = if mode == "append" { "Appended" } else { "Wrote" };
    ExecutableToolResult::ok_text(format!(
        "{} {} bytes to {}",
        verb, bytes_written, display_path
    ))
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

    async fn run_write(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = WriteTool::new(kaos.clone(), workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await
    }

    #[tokio::test]
    async fn writes_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_write(&tmp, json!({"path": "out.txt", "content": "hello"})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("Wrote 5 bytes to out.txt"));
        let bytes = tokio::fs::read(tmp.path().join("out.txt")).await.unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[tokio::test]
    async fn overwrites_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("out.txt"), "old").await.unwrap();
        let result = run_write(&tmp, json!({"path": "out.txt", "content": "new"})).await;
        assert!(!result.is_error);
        let bytes = tokio::fs::read(tmp.path().join("out.txt")).await.unwrap();
        assert_eq!(bytes, b"new");
    }

    #[tokio::test]
    async fn appends_to_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("out.txt"), "hello").await.unwrap();
        let result = run_write(
            &tmp,
            json!({"path": "out.txt", "content": " world", "mode": "append"}),
        )
        .await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("Appended 6 bytes to out.txt"));
        let bytes = tokio::fs::read(tmp.path().join("out.txt")).await.unwrap();
        assert_eq!(bytes, b"hello world");
    }

    #[tokio::test]
    async fn reports_utf8_byte_count_for_non_ascii() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_write(&tmp, json!({"path": "out.txt", "content": "é"})).await;
        assert!(!result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("Wrote 2 bytes to out.txt"));
    }

    #[tokio::test]
    async fn rejects_missing_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_write(
            &tmp,
            json!({"path": "missing/out.txt", "content": "x"}),
        )
        .await;
        assert!(result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("Parent directory does not exist"));
    }
}
```

### Step 4 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::write
```

Expected: all 5 tests pass.

### Step 5 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin/write.rs rust-ody/crates/tools-rs/src/builtin/mod.rs && git commit -m "feat(tools-rs): WriteTool"
```

---

## Task 3: Implement `EditTool`

**Depends on:** Task 1 (`materialize_model_text`, `to_model_text_view`), Part 1 (`BuiltinTool` trait)

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/edit.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (add `pub mod edit;`)
- Test: inline in `edit.rs`

### Step 1 — Write the failing test

Create `rust-ody/crates/tools-rs/src/builtin/edit.rs` with tests first. Stub `resolve_execution` with `todo!()`.

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};

use crate::builtin::line_endings::{materialize_model_text, to_model_text_view};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolExecution, ToolError};
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
            InputSchema::string()
                .description("Path to the text file to edit. Relative paths resolve against the working directory; a path outside the working directory must be absolute."),
        ),
        (
            "old_string",
            InputSchema::string()
                .min_length(1)
                .description("Exact content to replace from the Read output view, without the line-number prefix. Use LF for pure CRLF files; use actual \\r escapes where Read shows \\r."),
        ),
        (
            "new_string",
            InputSchema::string()
                .description("Replacement text in the same Read output view. LF is written back as CRLF only for pure CRLF files."),
        ),
        (
            "replace_all",
            InputSchema::boolean()
                .optional()
                .description("Set true only when every occurrence of old_string should be replaced."),
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
        "Exact string replacement in a text file."
    }

    fn parameters(&self) -> Value {
        edit_parameters()
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

    async fn run_edit(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = EditTool::new(kaos.clone(), workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await
    }

    #[tokio::test]
    async fn replaces_once_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello world\n").await.unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "a.txt", "old_string": "hello", "new_string": "hi"}),
        )
        .await;
        assert!(!result.is_error);
        let bytes = tokio::fs::read(tmp.path().join("a.txt")).await.unwrap();
        assert_eq!(bytes, b"hi world\n");
    }
}
```

Add `pub mod edit;` to `rust-ody/crates/tools-rs/src/builtin/mod.rs`.

### Step 2 — Run the failing test

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::edit
```

Expected failure: test panics at `todo!()`.

### Step 3 — Write the minimal implementation

Replace the `todo!()` body and add the `execution` helper. The full file becomes:

```rust
use kaos_rs::kaos::Kaos;
use kaos_rs::text::ErrorMode;
use serde_json::{json, Value};

use crate::builtin::line_endings::{materialize_model_text, to_model_text_view};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};
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
            InputSchema::string()
                .description("Path to the text file to edit. Relative paths resolve against the working directory; a path outside the working directory must be absolute."),
        ),
        (
            "old_string",
            InputSchema::string()
                .min_length(1)
                .description("Exact content to replace from the Read output view, without the line-number prefix. Use LF for pure CRLF files; use actual \\r escapes where Read shows \\r."),
        ),
        (
            "new_string",
            InputSchema::string()
                .description("Replacement text in the same Read output view. LF is written back as CRLF only for pure CRLF files."),
        ),
        (
            "replace_all",
            InputSchema::boolean()
                .optional()
                .description("Set true only when every occurrence of old_string should be replaced."),
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
        "Exact string replacement in a text file."
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
            return Err(ToolError::InvalidArgs("old_string must not be empty".into()));
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
    let mut count = 0;
    let mut pos = 0;
    while let Some(idx) = content[pos..].find(needle) {
        count += 1;
        pos += idx + needle.len();
    }
    count
}

fn replace_once(content: &str, old: &str, new: &str) -> String {
    match content.find(old) {
        Some(idx) => format!("{}{}{}", &content[..idx], new, &content[idx + old.len()..]),
        None => content.to_string(),
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
            "Aborted before edit started".into(),
            "Aborted".into(),
        );
    }

    let old_string = match args.get("old_string").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => {
            return ExecutableToolResult::error_text(
                "old_string is required and must not be empty".into(),
                "Invalid args".into(),
            );
        }
    };
    let new_string = args.get("new_string").and_then(Value::as_str).unwrap_or("");
    if old_string == new_string {
        return ExecutableToolResult::error_text(
            "No changes to make: old_string and new_string are exactly the same.".into(),
            "No changes".into(),
        );
    }

    let raw = match kaos.read_text(&safe_path, None, Some(ErrorMode::Strict)).await {
        Ok(s) => s,
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to read \"{}\": {}", display_path, e),
                "Read failed".into(),
            );
        }
    };

    let model_view = to_model_text_view(&raw);
    let content = model_view.text;
    let replace_all = args.get("replace_all").and_then(Value::as_bool).unwrap_or(false);

    let new_content = if !replace_all {
        let count = count_occurrences(&content, old_string);
        if count == 0 {
            return ExecutableToolResult::error_text(
                format!(
                    "old_string not found in {}, The file contents may be out of date. Please use the Read Tool to reload the content.",
                    display_path
                ),
                "old_string not found".into(),
            );
        }
        if count > 1 {
            return ExecutableToolResult::error_text(
                format!(
                    "old_string is not unique in {} (found {} occurrences). To replace every occurrence, set replace_all=true. To replace only one occurrence, include more surrounding context in old_string.",
                    display_path, count
                ),
                "old_string not unique".into(),
            );
        }
        replace_once(&content, old_string, new_string)
    } else {
        let parts: Vec<&str> = content.split(old_string).collect();
        let count = parts.len().saturating_sub(1);
        if count == 0 {
            return ExecutableToolResult::error_text(
                format!(
                    "old_string not found in {}, The file contents may be out of date. Please use the Read Tool to reload the content.",
                    display_path
                ),
                "old_string not found".into(),
            );
        }
        parts.join(new_string)
    };

    let materialized = materialize_model_text(&new_content, model_view.line_ending_style);
    if let Err(e) = kaos.write_text(&safe_path, &materialized, None, None).await {
        return ExecutableToolResult::error_text(
            format!("Failed to write \"{}\": {}", display_path, e),
            "Write failed".into(),
        );
    }

    let count = if replace_all {
        content.split(old_string).count().saturating_sub(1)
    } else {
        1
    };
    ExecutableToolResult::ok_text(format!(
        "Replaced {} occurrence{} in {}",
        count,
        if count == 1 { "" } else { "s" },
        display_path
    ))
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

    async fn run_edit(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = EditTool::new(kaos.clone(), workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await
    }

    #[tokio::test]
    async fn replaces_once_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello world\n").await.unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "a.txt", "old_string": "hello", "new_string": "hi"}),
        )
        .await;
        assert!(!result.is_error);
        let bytes = tokio::fs::read(tmp.path().join("a.txt")).await.unwrap();
        assert_eq!(bytes, b"hi world\n");
    }

    #[tokio::test]
    async fn replace_all_replaces_every_occurrence() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "abc abc abc\n").await.unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "a.txt", "old_string": "abc", "new_string": "x", "replace_all": true}),
        )
        .await;
        assert!(!result.is_error);
        let bytes = tokio::fs::read(tmp.path().join("a.txt")).await.unwrap();
        assert_eq!(bytes, b"x x x\n");
    }

    #[tokio::test]
    async fn rejects_non_unique_old_string_when_not_replace_all() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "abc abc\n").await.unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "a.txt", "old_string": "abc", "new_string": "x"}),
        )
        .await;
        assert!(result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("not unique"));
    }

    #[tokio::test]
    async fn rejects_missing_old_string() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello\n").await.unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "a.txt", "old_string": "xyz", "new_string": "x"}),
        )
        .await;
        assert!(result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("old_string not found"));
    }

    #[tokio::test]
    async fn rejects_identical_old_and_new() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello\n").await.unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "a.txt", "old_string": "hello", "new_string": "hello"}),
        )
        .await;
        assert!(result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("No changes to make"));
    }

    #[tokio::test]
    async fn preserves_crlf_line_endings() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), b"hello world\r\n").await.unwrap();
        let result = run_edit(
            &tmp,
            json!({"path": "a.txt", "old_string": "world", "new_string": "Rust"}),
        )
        .await;
        assert!(!result.is_error);
        let bytes = tokio::fs::read(tmp.path().join("a.txt")).await.unwrap();
        assert_eq!(bytes, b"hello Rust\r\n");
    }
}
```

### Step 4 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::edit
```

Expected: all 6 tests pass.

### Step 5 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin/edit.rs rust-ody/crates/tools-rs/src/builtin/mod.rs && git commit -m "feat(tools-rs): EditTool"
```

---

## Local Self-Review

- [x] 1. Spec coverage: 4.4.1.2 (WriteTool overwrite/append/parent check/byte count) covered by Task 2; 4.4.1.3 (EditTool replace once/all, uniqueness, CRLF materialize) covered by Task 3; line-ending materialize helper covered by Task 1.
- [x] 2. Placeholder scan: no TODO/TBD; every function body is concrete.
- [x] 3. No phantom tasks: each task adds code + passing tests + commit.
- [x] 4. Dependency soundness: Task 1 depends only on Part 1; Tasks 2 and 3 depend on Task 1 and Part 1. No later symbols used.
- [x] 5. Shared-signature changes: `line_endings.rs` only adds a new function; `BuiltinTool` trait is unchanged. No callers to update.
- [x] 6. Test-the-risk: WriteTool tests assert file bytes after overwrite/append and reject missing parent. EditTool tests assert file bytes after replace, uniqueness rejection, CRLF preservation. Constants (S_IFDIR, bytes via `String::len`) are traced in assertions.
- [x] 7. Type consistency: uses `ExecutableToolResult`, `ExecutableToolOutput`, `ToolExecution`, `AbortSignal`, `ToolAccesses`, `InputSchema`, and `PathAccessOperation::Write` exactly as defined in Part 1 / 4.4.0.
