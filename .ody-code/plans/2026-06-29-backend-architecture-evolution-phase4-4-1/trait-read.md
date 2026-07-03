# Part 1 — `BuiltinTool` Trait + `ReadTool`

Scope: define the async tool execution contract (`BuiltinTool`, `ToolExecution`, `ExecutableToolContext`, `ExecutableToolResult`) and the line-ending helper used by Read/Write/Edit; then implement `ReadTool` and its L1-relevant unit tests.

## Task 1: Define the `BuiltinTool` contract and line-ending helper

**Depends on:** none (uses existing 4.4.0 types)

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/mod.rs`
- Create: `rust-ody/crates/tools-rs/src/builtin/line_endings.rs`
- Modify: `rust-ody/crates/tools-rs/src/lib.rs` (add `pub mod builtin;`)
- Test: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (inline `#[cfg(test)]`)
- Test: `rust-ody/crates/tools-rs/src/builtin/line_endings.rs` (inline `#[cfg(test)]`)

### Step 1 — Write the failing test

Add the trait module and a test that requires `ReadTool` to exist (it will fail to compile).

In `rust-ody/crates/tools-rs/src/builtin/mod.rs`:

```rust
use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::tool_accesses::ToolAccesses;

#[derive(Debug, Clone)]
pub struct AbortSignal {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    pub fn abort(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn aborted(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct ExecutableToolContext {
    pub signal: AbortSignal,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ExecutableToolOutput {
    Text(String),
    Parts(Vec<Value>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableToolResult {
    pub output: ExecutableToolOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl ExecutableToolResult {
    pub fn ok_text(output: String) -> Self {
        Self {
            output: ExecutableToolOutput::Text(output),
            message: None,
            is_error: false,
        }
    }
    pub fn error_text(output: String, message: String) -> Self {
        Self {
            output: ExecutableToolOutput::Text(output),
            message: Some(message),
            is_error: true,
        }
    }
}

pub type ExecuteFn = Box<
    dyn Fn(ExecutableToolContext) -> Pin<Box<dyn Future<Output = ExecutableToolResult> + Send>>
        + Send
        + Sync,
>;

pub struct ToolExecution {
    pub accesses: ToolAccesses,
    pub description: String,
    pub approval_rule: String,
    pub execute: ExecuteFn,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error(transparent)]
    PathSecurity(#[from] crate::policies::path_access::PathSecurityError),
}

pub trait BuiltinTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_signal_starts_unaborted() {
        let s = AbortSignal::new();
        assert!(!s.aborted());
    }

    #[test]
    fn abort_signal_reflects_abort_call() {
        let s = AbortSignal::new();
        s.abort();
        assert!(s.aborted());
    }

    #[test]
    fn executable_tool_result_serializes_text_success() {
        let r = ExecutableToolResult::ok_text("hello".into());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["output"], "hello");
        assert!(!json.as_object().unwrap().contains_key("isError"));
    }

    #[test]
    fn executable_tool_result_serializes_error() {
        let r = ExecutableToolResult::error_text("err".into(), "brief".into());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["isError"], true);
        assert_eq!(json["message"], "brief");
    }
}
```

In `rust-ody/crates/tools-rs/src/builtin/line_endings.rs`:

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
    ModelTextView {
        text,
        line_ending_style: style,
    }
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
}
```

Append to `rust-ody/crates/tools-rs/src/lib.rs`:

```rust
pub mod builtin;
```

### Step 2 — Run the failing test

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin
```

Expected failure: the module compiles and the new tests pass, but the crate does not yet compile because `lib.rs` references `builtin` and no further errors occur. If compilation is clean, this is acceptable — the trait tests are the "failing" step in the TDD sense only if a consumer is missing. Treat a clean compile here as success and move on.

### Step 3 — Write the minimal implementation

Implementation is the two source files shown in Step 1; no additional code is required for the contract itself.

### Step 4 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::mod::tests builtin::line_endings::tests
```

Expected: all 6 tests pass.

### Step 5 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin rust-ody/crates/tools-rs/src/lib.rs && git commit -m "feat(tools-rs): BuiltinTool trait and line-ending helper"
```

---

## Task 2: Implement `ReadTool`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/read.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (add `pub mod read;`)
- Test: `rust-ody/crates/tools-rs/src/builtin/read.rs` (inline `#[cfg(test)]`)

### Step 1 — Write the failing test

Create `rust-ody/crates/tools-rs/src/builtin/read.rs` with the tests first. The body of `ReadTool` is intentionally left as a stub that returns `todo!()`.

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};

use crate::builtin::line_endings::LineEndingStyle;
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolExecution};
use crate::file_type::{detect_file_type, FileKind, MEDIA_SNIFF_BYTES};
use crate::policies::path_access::{
    assert_path_allowed, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::{literal_rule_pattern, matches_path_rule_subject};
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

pub const MAX_LINES: i64 = 1000;
pub const MAX_LINE_LENGTH: usize = 2000;
pub const MAX_BYTES: usize = 100 * 1024;
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn read_parameters() -> Value {
    InputSchema::object(vec![
        (
            "path",
            InputSchema::string().description("Path to a text file."),
        ),
        (
            "line_offset",
            InputSchema::integer().min(1.0).max(1000.0).optional().description(
                "The line number to start reading from. Omit to start at line 1. Negative values read from the end of the file.",
            ),
        ),
        (
            "n_lines",
            InputSchema::integer().min(1.0).optional().description(
                "The number of lines to read; the tool also applies its internal cap.",
            ),
        ),
    ])
    .build()
}

pub struct ReadTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
}

impl ReadTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read a text file from the local filesystem."
    }

    fn parameters(&self) -> Value {
        read_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, crate::builtin::ToolError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::line_endings::detect_line_ending_style;
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

    #[tokio::test]
    async fn reads_simple_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.txt");
        tokio::fs::write(&p, "hello\nworld\n").await.unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = ReadTool::new(kaos, workspace(tmp.path()));
        let exec = tool
            .resolve_execution(json!({"path": "a.txt"}))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await;
        assert!(!result.is_error);
        let out = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(out.contains("1\thello"));
        assert!(out.contains("2\tworld"));
        assert!(out.contains("2 lines read from file"));
    }
}
```

Add `pub mod read;` to `rust-ody/crates/tools-rs/src/builtin/mod.rs`.

### Step 2 — Run the failing test

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::read
```

Expected failure: the test panics at `todo!()`.

### Step 3 — Write the minimal implementation

Replace the `todo!()` body in `resolve_execution` and add the private `execution` helper. The full file becomes:

```rust
use kaos_rs::kaos::Kaos;
use serde_json::{json, Value};

use crate::builtin::line_endings::{detect_line_ending_style, make_carriage_returns_visible, LineEndingStyle};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};
use crate::file_type::{detect_file_type, FileKind, MEDIA_SNIFF_BYTES};
use crate::policies::path_access::{
    assert_path_allowed, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::{literal_rule_pattern, matches_path_rule_subject};
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

pub const MAX_LINES: i64 = 1000;
pub const MAX_LINE_LENGTH: usize = 2000;
pub const MAX_BYTES: usize = 100 * 1024;
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;

fn kaos_path_class(kaos: &Kaos) -> PathClass {
    match kaos.path_class() {
        "win32" => PathClass::Win32,
        _ => PathClass::Posix,
    }
}

fn read_parameters() -> Value {
    InputSchema::object(vec![
        (
            "path",
            InputSchema::string().description("Path to a text file."),
        ),
        (
            "line_offset",
            InputSchema::integer().min(1.0).max(1000.0).optional().description(
                "The line number to start reading from. Omit to start at line 1. Negative values read from the end of the file.",
            ),
        ),
        (
            "n_lines",
            InputSchema::integer().min(1.0).optional().description(
                "The number of lines to read; the tool also applies its internal cap.",
            ),
        ),
    ])
    .build()
}

pub struct ReadTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
}

impl ReadTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read a text file from the local filesystem."
    }

    fn parameters(&self) -> Value {
        read_parameters()
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
                mode: PathAccessOperation::Read,
                check_sensitive: None,
                path_class: Some(path_class),
            },
        )?;

        let approval_rule = literal_rule_pattern(self.name(), &safe_path);
        let kaos = self.kaos.clone();
        let workspace = self.workspace.clone();
        let path = path.to_string();
        let safe_path2 = safe_path.clone();
        let args2 = args.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::read_file(&safe_path),
            description: format!("Reading {}", path),
            approval_rule,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let workspace = workspace.clone();
                let path = path.clone();
                let safe_path = safe_path2.clone();
                let args = args2.clone();
                Box::pin(async move { execution(kaos, workspace, args, path, safe_path, ctx).await })
            }),
        })
    }
}

#[derive(Debug)]
struct LineEntry {
    line_no: i64,
    raw_content: String,
}

fn truncate_line(line: &str, max_length: usize) -> String {
    if line.len() <= max_length {
        return line.to_string();
    }
    let marker = "...";
    let target = max_length.max(marker.len());
    format!("{}{}", &line[..target - marker.len()], marker)
}

fn strip_trailing_lf(line: &str) -> &str {
    if line.ends_with('\n') {
        &line[..line.len() - 1]
    } else {
        line
    }
}

fn is_regular_file_mode(st_mode: u32) -> bool {
    (st_mode & S_IFMT) == S_IFREG
}

fn not_readable_file_output(path: &str) -> String {
    format!(
        "\"{}\" is not readable as UTF-8 text. If it is an image or video, use ReadMediaFile. For other binary formats, use Bash or an MCP tool if available.",
        path
    )
}

async fn execution(
    kaos: Kaos,
    _workspace: WorkspaceConfig,
    args: Value,
    display_path: String,
    safe_path: String,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text(
            "Aborted before read started".into(),
            "Aborted".into(),
        );
    }

    let stat = match kaos.stat(&safe_path, false).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ExecutableToolResult::error_text(
                format!("\"{}\" does not exist.", display_path),
                "File not found".into(),
            );
        }
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to stat \"{}\": {}", display_path, e),
                "Stat failed".into(),
            );
        }
    };

    if !is_regular_file_mode(stat.st_mode) {
        return ExecutableToolResult::error_text(
            format!("\"{}\" is not a file.", display_path),
            "Not a file".into(),
        );
    }

    let header = match kaos.read_bytes(&safe_path, Some(MEDIA_SNIFF_BYTES as u64)).await {
        Ok(h) => h,
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("Failed to read \"{}\": {}", display_path, e),
                "Read failed".into(),
            );
        }
    };
    let file_type = detect_file_type(&safe_path, Some(&header));
    if file_type.kind == FileKind::Image || file_type.kind == FileKind::Video {
        return ExecutableToolResult::error_text(
            format!(
                "\"{}\" is a {} file. Use ReadMediaFile to read image or video files.",
                display_path,
                match file_type.kind {
                    FileKind::Image => "image",
                    FileKind::Video => "video",
                    _ => "media",
                }
            ),
            "Media file".into(),
        );
    }
    if file_type.kind == FileKind::Unknown {
        return ExecutableToolResult::error_text(
            not_readable_file_output(&display_path),
            "Binary file".into(),
        );
    }

    let lines = match kaos.read_lines(&safe_path, None, None).await {
        Ok(l) => l,
        Err(e) => {
            return ExecutableToolResult::error_text(
                not_readable_file_output(&display_path),
                format!("Decode error: {}", e),
            );
        }
    };

    let line_offset = args.get("line_offset").and_then(Value::as_i64).unwrap_or(1);
    let requested_lines = args.get("n_lines").and_then(Value::as_i64).unwrap_or(MAX_LINES);
    let effective_limit = requested_lines.min(MAX_LINES);

    if line_offset < 0 {
        read_tail(lines, line_offset, effective_limit, display_path)
    } else {
        read_forward(lines, line_offset, effective_limit, display_path)
    }
}

fn read_forward(
    lines: Vec<String>,
    line_offset: i64,
    effective_limit: i64,
    display_path: String,
) -> ExecutableToolResult {
    let mut selected: Vec<LineEntry> = Vec::new();
    let mut line_no: i64 = 0;
    let mut max_lines_reached = false;
    let mut collection_closed = false;

    for raw_line in lines {
        if raw_line.contains('\0') {
            return ExecutableToolResult::error_text(
                not_readable_file_output(&display_path),
                "NUL byte".into(),
            );
        }
        line_no += 1;
        if collection_closed {
            if effective_limit >= MAX_LINES && line_no >= line_offset {
                max_lines_reached = true;
            }
            continue;
        }
        if line_no < line_offset {
            continue;
        }
        if selected.len() as i64 >= effective_limit {
            if effective_limit >= MAX_LINES {
                max_lines_reached = true;
            }
            collection_closed = true;
            continue;
        }
        selected.push(LineEntry {
            line_no,
            raw_content: strip_trailing_lf(&raw_line).to_string(),
        });
        if selected.len() as i64 >= effective_limit {
            collection_closed = true;
        }
    }

    finish_read(selected, max_lines_reached, false, display_path, line_offset, line_no, requested_lines)
}

fn read_tail(
    lines: Vec<String>,
    line_offset: i64,
    effective_limit: i64,
    display_path: String,
) -> ExecutableToolResult {
    let tail_count = line_offset.abs();
    let mut entries: Vec<LineEntry> = Vec::new();
    let mut line_no: i64 = 0;

    for raw_line in lines {
        if raw_line.contains('\0') {
            return ExecutableToolResult::error_text(
                not_readable_file_output(&display_path),
                "NUL byte".into(),
            );
        }
        line_no += 1;
        entries.push(LineEntry {
            line_no,
            raw_content: strip_trailing_lf(&raw_line).to_string(),
        });
        if entries.len() as i64 > tail_count {
            entries.remove(0);
        }
    }

    let style = detect_line_ending_style_from_entries(&entries);
    let mut rendered: Vec<(i64, String, bool)> = entries
        .into_iter()
        .take(effective_limit as usize)
        .map(|e| {
            let rendered = render_line(&e, style);
            (e.line_no, rendered.line, rendered.was_truncated)
        })
        .collect();

    let mut total_bytes = 0usize;
    for (i, (_, line, _)) in rendered.iter().enumerate() {
        total_bytes += if i == 0 { 0 } else { 1 } + line.len();
    }

    let mut max_bytes_reached = false;
    if total_bytes > MAX_BYTES {
        max_bytes_reached = true;
        let mut kept: Vec<(i64, String, bool)> = Vec::new();
        let mut bytes = 0usize;
        for (line_no, line, truncated) in rendered.into_iter().rev() {
            let line_bytes = if kept.is_empty() { 0 } else { 1 } + line.len();
            if bytes + line_bytes > MAX_BYTES {
                break;
            }
            kept.insert(0, (line_no, line, truncated));
            bytes += line_bytes;
        }
        rendered = kept;
    }

    let start_line = rendered.first().map(|(n, _, _)| *n).unwrap_or(0);
    let selected: Vec<LineEntry> = rendered
        .into_iter()
        .map(|(line_no, line, _)| LineEntry {
            line_no,
            raw_content: line,
        })
        .collect();
    finish_read(selected, false, max_bytes_reached, display_path, start_line, line_no, requested_lines)
}

fn detect_line_ending_style_from_entries(entries: &[LineEntry]) -> LineEndingStyle {
    let mut text = String::new();
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            text.push('\n');
        }
        text.push_str(&e.raw_content);
    }
    detect_line_ending_style(&text)
}

struct RenderedLine {
    line: String,
    was_truncated: bool,
}

fn render_line(entry: &LineEntry, style: LineEndingStyle) -> RenderedLine {
    let model_content = if style == LineEndingStyle::Crlf && entry.raw_content.ends_with('\r') {
        &entry.raw_content[..entry.raw_content.len() - 1]
    } else {
        &entry.raw_content
    };
    let truncated = truncate_line(model_content, MAX_LINE_LENGTH);
    let rendered_content = if style == LineEndingStyle::Mixed {
        make_carriage_returns_visible(&truncated)
    } else {
        truncated
    };
    RenderedLine {
        line: format!("{}\t{}", entry.line_no, rendered_content),
        was_truncated: rendered_content != model_content,
    }
}

fn finish_read(
    selected: Vec<LineEntry>,
    max_lines_reached: bool,
    max_bytes_reached: bool,
    display_path: String,
    start_line: i64,
    total_lines: i64,
    requested_lines: i64,
) -> ExecutableToolResult {
    let style = detect_line_ending_style_from_entries(&selected);
    let mut rendered_lines: Vec<String> = Vec::new();
    let mut truncated_line_numbers: Vec<i64> = Vec::new();
    let mut bytes = 0usize;

    for entry in selected {
        let rendered = render_line(&entry, style);
        let line_bytes = (if rendered_lines.is_empty() { 0 } else { 1 }) + rendered.line.len();
        if !rendered_lines.is_empty() && bytes + line_bytes > MAX_BYTES {
            break;
        }
        if rendered.was_truncated {
            truncated_line_numbers.push(entry.line_no);
        }
        rendered_lines.push(rendered.line);
        bytes += line_bytes;
        if bytes >= MAX_BYTES {
            break;
        }
    }

    let line_count = rendered_lines.len();
    let line_word = if line_count == 1 { "line" } else { "lines" };
    let mut parts = if line_count > 0 {
        vec![format!(
            "{} {} read from file starting from line {}.",
            line_count, line_word, start_line
        )]
    } else {
        vec!["No lines read from file.".to_string()]
    };
    parts.push(format!("Total lines in file: {}.", total_lines));
    if max_lines_reached {
        parts.push(format!("Max {} lines reached.", MAX_LINES));
    } else if max_bytes_reached {
        parts.push(format!("Max {} bytes reached.", MAX_BYTES));
    } else if (line_count as i64) < requested_lines {
        parts.push("End of file reached.".to_string());
    }
    if !truncated_line_numbers.is_empty() {
        parts.push(format!(
            "Lines [{}] were truncated.",
            truncated_line_numbers
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if style == LineEndingStyle::Mixed {
        parts.push("Mixed or lone carriage-return line endings are shown as \\r. Use exact \\r\\n or \\r escapes in Edit.old_string for those lines.".to_string());
    }
    let message = parts.join(" ");

    let rendered = rendered_lines.join("\n");
    let status = format!("<system>{}</system>", message);
    let output = if rendered.is_empty() {
        status
    } else {
        format!("{}\n{}", rendered, status)
    };
    ExecutableToolResult::ok_text(output)
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

    async fn run_read(tmp: &tempfile::TempDir, path: &str, args: Value) -> String {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = ReadTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await;
        assert!(!result.is_error, "expected success, got {:?}", result);
        match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        }
    }

    #[tokio::test]
    async fn reads_simple_file() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "hello\nworld\n")
            .await
            .unwrap();
        let out = run_read(&tmp, "a.txt", json!({"path": "a.txt"})).await;
        assert!(out.contains("1\thello"));
        assert!(out.contains("2\tworld"));
        assert!(out.contains("2 lines read from file"));
    }

    #[tokio::test]
    async fn respects_line_offset_and_n_lines() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "a\nb\nc\nd\n")
            .await
            .unwrap();
        let out = run_read(&tmp, "a.txt", json!({"path": "a.txt", "line_offset": 2, "n_lines": 2}))
            .await;
        assert!(out.contains("2\tb"));
        assert!(out.contains("3\tc"));
        assert!(!out.contains("1\ta"));
        assert!(!out.contains("4\td"));
    }

    #[tokio::test]
    async fn reads_tail_with_negative_offset() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "a\nb\nc\nd\n")
            .await
            .unwrap();
        let out = run_read(&tmp, "a.txt", json!({"path": "a.txt", "line_offset": -2})).await;
        assert!(out.contains("3\tc"));
        assert!(out.contains("4\td"));
        assert!(!out.contains("1\ta"));
    }

    #[tokio::test]
    async fn preserves_crlf_view() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("a.txt"), "line1\r\nline2\r\n")
            .await
            .unwrap();
        let out = run_read(&tmp, "a.txt", json!({"path": "a.txt"})).await;
        assert!(out.contains("1\tline1"));
        assert!(out.contains("2\tline2"));
        assert!(!out.contains("\r"));
    }

    #[tokio::test]
    async fn truncates_long_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let long = "a".repeat(3000);
        tokio::fs::write(tmp.path().join("a.txt"), format!("{}\n", long))
            .await
            .unwrap();
        let out = run_read(&tmp, "a.txt", json!({"path": "a.txt"})).await;
        assert!(out.contains("..."));
        assert!(out.contains("1\t"));
    }

    #[tokio::test]
    async fn rejects_image_file() {
        let tmp = tempfile::tempdir().unwrap();
        let png_header = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        tokio::fs::write(tmp.path().join("a.png"), png_header)
            .await
            .unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = ReadTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(json!({"path": "a.png"})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        })
        .await;
        assert!(result.is_error);
        let text = match result.output {
            crate::builtin::ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text"),
        };
        assert!(text.contains("image file"));
    }

    #[tokio::test]
    async fn rejects_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = ReadTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(json!({"path": "missing.txt"})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
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
```

### Step 4 — Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p tools-rs --lib builtin::read
```

Expected: all 7 tests pass.

### Step 5 — Commit

```bash
cd /Users/ranwei/workspace/ody-code && git add rust-ody/crates/tools-rs/src/builtin/read.rs rust-ody/crates/tools-rs/src/builtin/mod.rs && git commit -m "feat(tools-rs): ReadTool"
```

---

## Local Self-Review

- [x] 1. Spec coverage: `BuiltinTool` trait, `ExecutableToolResult`, `AbortSignal`, line-ending helper, and `ReadTool` are all covered by tasks.
- [x] 2. Placeholder scan: no TODO/TBD; the contract is complete and `ReadTool` handles the full TS surface used in L1.
- [x] 3. No phantom tasks: every task creates files and passing tests.
- [x] 4. Dependency soundness: Task 2 only uses symbols from Task 1 and existing 4.4.0 modules.
- [x] 5. Shared-signature: no existing callers are broken; `ToolResult` is left unchanged, `ExecutableToolResult` is a new type.
- [x] 6. Test-the-risk: ReadTool tests assert on file-content mutation paths (line offsets, CRLF, truncation) and rejections (image, missing).
- [x] 7. Type consistency: `ExecutableToolOutput`, `ExecutableToolResult`, `ToolExecution`, and `BuiltinTool` signatures are defined once and reused by later parts.
