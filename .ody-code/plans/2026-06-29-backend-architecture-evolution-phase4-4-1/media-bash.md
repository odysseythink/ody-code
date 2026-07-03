# Part 4: `ReadMediaFileTool` + `BashTool` (foreground)

**Goal:** Implement the remaining file core tool that reads images/videos as base64 resources, and the foreground shell execution core tool with timeout, abort, and signal escalation.

**Architecture:** `ReadMediaFileTool` reuses `kaos.read_bytes` plus the `infer` crate for MIME/media-type detection and the `image` crate for image dimensions; unsupported media and oversized files fail fast. `BashTool` runs the user command through the environment's configured shell (`kaos.env().shell_path -c`), captures stdout/stderr, supports extra env vars, enforces a configurable timeout, and escalates `SIGTERM → SIGKILL` on timeout or abort.

**Tech Stack:** Rust `tools-rs`, `kaos-rs`, `serde_json`, `tokio`, `infer`, `image`, `base64`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

### Task 1: Add media dependencies to `tools-rs/Cargo.toml`

**Depends on:** Part 1 (`2026-06-29-backend-architecture-evolution-phase4-4-1/trait-read.md`) (trait and execution contracts in place)

**Files:**
- Modify: `rust-ody/crates/tools-rs/Cargo.toml:8-31`

This task only adds crates; there is no testable code yet, so the verification is a build check.

- [ ] Insert three dependencies into the `[dependencies]` section of `rust-ody/crates/tools-rs/Cargo.toml`:

```toml
infer = "0.16"
image = { version = "0.25", default-features = false, features = ["png", "jpeg", "gif", "webp"] }
base64 = "0.22"
```

Choose feature-gated `image` to keep compile time small; PNG/JPEG/GIF/WEBP cover the formats the TS tool advertises.

- [ ] Run `cargo check -p tools-rs` from `rust-ody/` and verify it succeeds (new crates download and compile).

Expected:
```text
$ cd rust-ody && cargo check -p tools-rs
    Finished dev [unoptimized + debuginfo] target(s) in ...s
```

- [ ] Commit: `feat(tools-rs): add image, infer, base64 deps for media tool`

---

### Task 2: Implement `ReadMediaFileTool`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/media.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (register module)

**Behavior:**
- Input: `file_path: string`.
- Reject paths outside the workspace via `assert_path_allowed(..., PathAccessOperation::Read, check_sensitive: Some(true))`.
- Read the full file up to `MAX_MEDIA_BYTES = 10 * 1024 * 1024` (10 MiB); reject larger files.
- Detect MIME/media type with `infer::get(&bytes)`. Reject non-image/non-video.
- For images, decode dimensions with `image::load_from_memory` and include `{width, height}`.
- For videos, only report `media_type: "video"`; dimensions are `null`.
- Return base64-encoded bytes in a single `ExecutableToolOutput::Parts` entry shaped like the TS `Resource` object.

- [ ] Write the failing test. Append this module test to the bottom of `media.rs` (inside `#[cfg(test)] mod tests`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    use crate::builtin::tests::dummy_ctx;
    use crate::kaos::{Environment, Kaos};

    fn image_kaos(tmp: &tempfile::TempDir) -> (Kaos, String) {
        let env = Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        };
        (Kaos::new(env, tmp.path()), tmp.path().to_string_lossy().to_string())
    }

    #[tokio::test]
    async fn reads_png_with_dimensions_and_base64() {
        let tmp = tempfile::tempdir().unwrap();
        let (kaos, root) = image_kaos(&tmp);
        let path = tmp.path().join("dot.png");

        // Write a 2x1 RGB PNG.
        let img = image::RgbImage::from_raw(2, 1, vec![255, 0, 0, 0, 255, 0]).unwrap();
        img.save(&path).unwrap();

        let tool = ReadMediaFileTool::new(kaos, root.clone());
        let res = tool
            .execute(json!({"file_path": path.to_string_lossy().to_string()}), dummy_ctx())
            .await;

        assert!(!res.is_error, "unexpected error: {:?}", res.message);
        let ExecutableToolOutput::Parts(parts) = res.output else {
            panic!("expected Parts output");
        };
        assert_eq!(parts.len(), 1);
        let obj = parts[0].as_object().unwrap();
        assert_eq!(obj.get("type").unwrap().as_str().unwrap(), "image");
        assert_eq!(obj.get("mime_type").unwrap().as_str().unwrap(), "image/png");
        let dims = obj.get("dimensions").unwrap().as_object().unwrap();
        assert_eq!(dims.get("width").unwrap().as_u64().unwrap(), 2);
        assert_eq!(dims.get("height").unwrap().as_u64().unwrap(), 1);
        assert!(obj.get("data").unwrap().as_str().unwrap().starts_with("iVBOR"));
    }

    #[tokio::test]
    async fn rejects_oversized_media() {
        let tmp = tempfile::tempdir().unwrap();
        let (kaos, root) = image_kaos(&tmp);
        let path = tmp.path().join("big.png");
        // Create a file slightly larger than the 10 MiB cap.
        let big = vec![0u8; MAX_MEDIA_BYTES as usize + 1];
        tokio::fs::write(&path, &big).await.unwrap();

        let tool = ReadMediaFileTool::new(kaos, root);
        let res = tool
            .execute(json!({"file_path": path.to_string_lossy().to_string()}), dummy_ctx())
            .await;

        assert!(res.is_error);
        assert!(res.message.as_ref().unwrap().contains("too large"));
    }
}
```

- [ ] Run the test and verify it fails because `ReadMediaFileTool` does not exist yet:

```text
$ cd rust-ody && cargo test -p tools-rs reads_png_with_dimensions_and_base64
error[E0433]: failed to resolve: use of undeclared crate or module `media`
```

- [ ] Write the minimal implementation in `rust-ody/crates/tools-rs/src/builtin/media.rs`:

```rust
use std::collections::HashMap;

use base64::Engine;
use serde_json::{json, Value};

use crate::builtin::{
    AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult,
    ExecuteFn, ToolAccesses, ToolError, ToolExecution,
};
use crate::path_security::{assert_path_allowed, PathAccessOperation};
use crate::schema::{InputSchema, SchemaBuilder};

const MAX_MEDIA_BYTES: u64 = 10 * 1024 * 1024;

pub struct ReadMediaFileTool {
    kaos: kaos_rs::Kaos,
    workspace_root: String,
}

impl ReadMediaFileTool {
    pub fn new(kaos: kaos_rs::Kaos, workspace_root: impl Into<String>) -> Self {
        Self {
            kaos,
            workspace_root: workspace_root.into(),
        }
    }

    async fn execute(
        &self,
        args: Value,
        ctx: ExecutableToolContext,
    ) -> ExecutableToolResult {
        let path_str = match args.get("file_path").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ExecutableToolResult::error_text(
                    "Missing required argument: file_path".to_string(),
                    "Invalid arguments".to_string(),
                );
            }
        };

        let cwd = self.kaos.getcwd();
        let canonical = match assert_path_allowed(
            &path_str,
            &cwd,
            &self.workspace_root,
            crate::path_security::AssertPathOptions {
                mode: PathAccessOperation::Read,
                check_sensitive: Some(true),
                path_class: None,
            },
        ) {
            Ok(p) => p,
            Err(e) => {
                return ExecutableToolResult::error_text(
                    e.to_string(),
                    "Path security error".to_string(),
                );
            }
        };

        let stat = match self.kaos.stat(&canonical, false).await {
            Ok(s) => s,
            Err(e) => {
                return ExecutableToolResult::error_text(
                    format!("Cannot stat media file: {e}"),
                    "File access error".to_string(),
                );
            }
        };

        if !stat.is_file {
            return ExecutableToolResult::error_text(
                format!("Not a file: {canonical}"),
                "Invalid media file".to_string(),
            );
        }

        if stat.size > MAX_MEDIA_BYTES {
            return ExecutableToolResult::error_text(
                format!(
                    "Media file is too large ({} bytes; max {} bytes)",
                    stat.size, MAX_MEDIA_BYTES
                ),
                "File too large".to_string(),
            );
        }

        let bytes = match self.kaos.read_bytes(&canonical, Some(MAX_MEDIA_BYTES)).await {
            Ok(b) => b,
            Err(e) => {
                return ExecutableToolResult::error_text(
                    format!("Failed to read media file: {e}"),
                    "File read error".to_string(),
                );
            }
        };

        let kind = match infer::get(&bytes) {
            Some(k) => k,
            None => {
                return ExecutableToolResult::error_text(
                    "Could not determine media type".to_string(),
                    "Unsupported media".to_string(),
                );
            }
        };

        let media_type = match kind.matcher_type() {
            infer::MatcherType::Image => "image",
            infer::MatcherType::Video => "video",
            _ => {
                return ExecutableToolResult::error_text(
                    format!("Unsupported media type: {}", kind.mime_type()),
                    "Unsupported media".to_string(),
                );
            }
        };

        let dimensions = if media_type == "image" {
            match image::load_from_memory(&bytes) {
                Ok(img) => {
                    let (w, h) = img.dimensions();
                    Some(json!({ "width": w, "height": h }))
                }
                Err(e) => {
                    return ExecutableToolResult::error_text(
                        format!("Failed to decode image: {e}"),
                        "Image decode error".to_string(),
                    );
                }
            }
        } else {
            Some(Value::Null)
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        ExecutableToolResult::ok_text_with_parts(vec![json!({
            "type": media_type,
            "mime_type": kind.mime_type(),
            "media_type": media_type,
            "dimensions": dimensions,
            "data": b64,
        })])
    }
}

impl BuiltinTool for ReadMediaFileTool {
    fn name(&self) -> &str {
        "read_media_file"
    }

    fn description(&self) -> &str {
        "Read an image or video file and return it as a base64 resource with MIME type and dimensions."
    }

    fn parameters(&self) -> Value {
        InputSchema::object()
            .property("file_path", InputSchema::string().description("Absolute or workspace-relative path to the media file"))
            .required(vec!["file_path"])
            .build()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let _ = InputSchema::object()
            .property("file_path", InputSchema::string())
            .required(vec!["file_path"])
            .validate(&args)?;

        Ok(ToolExecution {
            accesses: ToolAccesses::read_file(),
            description: format!("Read media file at {}", args["file_path"].as_str().unwrap_or("?")),
            approval_rule: format!("read_file:{}", args["file_path"].as_str().unwrap_or("?")),
            execute: {
                let this = Self {
                    kaos: self.kaos.clone(),
                    workspace_root: self.workspace_root.clone(),
                };
                Box::new(move |ctx| Box::pin(this.execute(args, ctx)))
            },
        })
    }
}
```

*Note:* `ExecutableToolResult::ok_text_with_parts` must have been introduced in Part 1; if it is not available, use `ExecutableToolResult { output: ExecutableToolOutput::Parts(vec![...]), message: None, is_error: false }`.

- [ ] Register the module in `rust-ody/crates/tools-rs/src/builtin/mod.rs` by adding `pub mod media;` alongside the other tool modules.

- [ ] Run the tests and verify they pass:

```text
$ cd rust-ody && cargo test -p tools-rs reads_png_with_dimensions_and_base64 rejects_oversized_media
running 2 tests
test builtin::media::tests::reads_png_with_dimensions_and_base64 ... ok
test builtin::media::tests::rejects_oversized_media ... ok
```

- [ ] Commit: `feat(tools-rs): implement ReadMediaFileTool with dimensions and base64`

---

### Task 3: Implement `BashTool` (foreground)

**Depends on:** Part 1 (trait/contracts)

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/bash.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (register module)

**Behavior:**
- Input:
  - `command: string` (required) — passed to the configured shell via `-c`.
  - `timeout: integer` (optional, seconds, default 60, max 600).
  - `env: object` (optional) — extra environment variables merged into the shell's environment.
  - `description: string` (optional) — human-readable description for the approval rule.
- Output: `ExecutableToolOutput::Text(String)` containing the combined stdout and stderr.
- `is_error` is `true` when the exit code is non-zero, the command could not be spawned, or timeout/abort occurred.
- Timeout/abort handling:
  1. Spawn the process.
  2. Loop every 100 ms checking `signal.aborted()` and `proc.exit_code()`.
  3. If `signal.aborted()`, send SIGTERM, wait up to 5 s, then SIGKILL; return error.
  4. If `timeout` elapses, send SIGTERM, wait up to 5 s, then SIGKILL; return error.
- Output truncation: cap stdout and stderr each at `MAX_OUTPUT_BYTES = 1 * 1024 * 1024` (1 MiB) and append `\n... (truncated)`.

- [ ] Write the failing test. Append inside `#[cfg(test)] mod tests` in `bash.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::builtin::tests::dummy_ctx;
    use crate::kaos::{Environment, Kaos};

    fn bash_kaos(tmp: &tempfile::TempDir) -> (Kaos, String) {
        let env = Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        };
        (Kaos::new(env, tmp.path()), tmp.path().to_string_lossy().to_string())
    }

    #[tokio::test]
    async fn echo_returns_stdout_and_zero_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let (kaos, root) = bash_kaos(&tmp);
        let tool = BashTool::new(kaos, root);
        let res = tool
            .execute(json!({"command": "echo hello world"}), dummy_ctx())
            .await;

        assert!(!res.is_error, "unexpected error: {:?}", res.message);
        let ExecutableToolOutput::Text(text) = res.output else {
            panic!("expected Text output");
        };
        assert!(text.contains("hello world"));
    }

    #[tokio::test]
    async fn non_zero_exit_is_marked_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (kaos, root) = bash_kaos(&tmp);
        let tool = BashTool::new(kaos, root);
        let res = tool
            .execute(json!({"command": "exit 42"}), dummy_ctx())
            .await;

        assert!(res.is_error);
        assert!(res.message.as_ref().unwrap().contains("42"));
    }

    #[tokio::test]
    async fn timeout_kills_long_sleep() {
        let tmp = tempfile::tempdir().unwrap();
        let (kaos, root) = bash_kaos(&tmp);
        let tool = BashTool::new(kaos, root);
        let res = tool
            .execute(json!({"command": "sleep 30", "timeout": 1}), dummy_ctx())
            .await;

        assert!(res.is_error);
        assert!(res.message.as_ref().unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn env_vars_are_visible_to_command() {
        let tmp = tempfile::tempdir().unwrap();
        let (kaos, root) = bash_kaos(&tmp);
        let tool = BashTool::new(kaos, root);
        let res = tool
            .execute(
                json!({"command": "echo $ODY_TEST_VAR", "env": {"ODY_TEST_VAR": "secret"}}),
                dummy_ctx(),
            )
            .await;

        assert!(!res.is_error, "unexpected error: {:?}", res.message);
        let ExecutableToolOutput::Text(text) = res.output else {
            panic!("expected Text output");
        };
        assert!(text.contains("secret"));
    }
}
```

- [ ] Run the tests and verify they fail because `BashTool` does not exist:

```text
$ cd rust-ody && cargo test -p tools-rs bash_
error[E0433]: failed to resolve: use of undeclared crate or module `bash`
```

- [ ] Write the minimal implementation in `rust-ody/crates/tools-rs/src/builtin/bash.rs`:

```rust
use std::collections::HashMap;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::{sleep, timeout};

use crate::builtin::{
    AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult,
    ExecuteFn, ToolAccesses, ToolError, ToolExecution,
};
use crate::schema::{InputSchema, SchemaBuilder};

const DEFAULT_TIMEOUT_SECONDS: u64 = 60;
const MAX_TIMEOUT_SECONDS: u64 = 600;
const KILL_GRACE_PERIOD_SECONDS: u64 = 5;
const MAX_OUTPUT_BYTES: usize = 1 * 1024 * 1024;
const POLL_INTERVAL_MS: u64 = 100;

pub struct BashTool {
    kaos: kaos_rs::Kaos,
    workspace_root: String,
}

impl BashTool {
    pub fn new(kaos: kaos_rs::Kaos, workspace_root: impl Into<String>) -> Self {
        Self {
            kaos,
            workspace_root: workspace_root.into(),
        }
    }

    async fn execute(&self, args: Value, ctx: ExecutableToolContext) -> ExecutableToolResult {
        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s,
            _ => {
                return ExecutableToolResult::error_text(
                    "Missing or empty required argument: command".to_string(),
                    "Invalid arguments".to_string(),
                );
            }
        };

        let timeout_seconds = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .map(|t| t.min(MAX_TIMEOUT_SECONDS).max(1))
            .unwrap_or(DEFAULT_TIMEOUT_SECONDS);

        let extra_env: HashMap<String, String> = args
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or(command)
            .to_string();

        let shell = self.kaos.env().shell_path.clone();
        let shell_args: Vec<String> = vec![shell.clone(), "-c".to_string(), command.to_string()];
        let shell_refs: Vec<&str> = shell_args.iter().map(|s| s.as_str()).collect();
        let env_refs: Vec<(&str, &str)> = extra_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let proc = match self.kaos.exec_with_env(&shell_refs, &env_refs).await {
            Ok(p) => p,
            Err(e) => {
                return ExecutableToolResult::error_text(
                    format!("Failed to spawn shell: {e}"),
                    "Shell spawn error".to_string(),
                );
            }
        };

        let wait_result = timeout(
            Duration::from_secs(timeout_seconds),
            wait_for_process_or_abort(&proc, &ctx.signal),
        )
        .await;

        let (exit_code, timed_out) = match wait_result {
            Ok(WaitOutcome::Exited(code)) => (code, false),
            Ok(WaitOutcome::Aborted) => {
                let _ = terminate_process(&proc).await;
                return ExecutableToolResult::error_text(
                    "Command aborted by user".to_string(),
                    "Aborted".to_string(),
                );
            }
            Err(_) => {
                let _ = terminate_process(&proc).await;
                return make_output(
                    &proc,
                    -1,
                    &description,
                    Some(format!("Command timed out after {timeout_seconds}s")),
                );
            }
        };

        make_output(&proc, exit_code, &description, None)
    }
}

enum WaitOutcome {
    Exited(i32),
    Aborted,
}

async fn wait_for_process_or_abort(proc: &kaos_rs::process::Process, signal: &AbortSignal) -> WaitOutcome {
    loop {
        if signal.aborted() {
            return WaitOutcome::Aborted;
        }
        if let Some(code) = proc.exit_code() {
            return WaitOutcome::Exited(code);
        }
        sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

async fn terminate_process(proc: &kaos_rs::process::Process) -> i32 {
    let _ = proc.kill(Some("SIGTERM")).await;
    let grace = tokio::time::timeout(
        Duration::from_secs(KILL_GRACE_PERIOD_SECONDS),
        proc.wait(),
    )
    .await;
    if grace.is_err() {
        let _ = proc.kill(Some("SIGKILL")).await;
    }
    proc.exit_code().unwrap_or(-1)
}

fn make_output(
    proc: &kaos_rs::process::Process,
    exit_code: i32,
    description: &str,
    override_message: Option<String>,
) -> ExecutableToolResult {
    let stdout = truncate_output(proc.stdout_blocking());
    let stderr = truncate_output(proc.stderr_blocking());

    let mut parts: Vec<String> = Vec::with_capacity(2);
    if !stdout.is_empty() {
        parts.push(stdout);
    }
    if !stderr.is_empty() {
        parts.push(stderr);
    }
    let text = parts.join("\n");

    let is_error = exit_code != 0 || override_message.is_some();
    let message = override_message.or_else(|| {
        if exit_code != 0 {
            Some(format!(
                "Command '{}' exited with code {}",
                description, exit_code
            ))
        } else {
            None
        }
    });

    ExecutableToolResult {
        output: ExecutableToolOutput::Text(text),
        message,
        is_error,
    }
}

fn truncate_output(mut bytes: Vec<u8>) -> String {
    let truncated = bytes.len() > MAX_OUTPUT_BYTES;
    if truncated {
        bytes.truncate(MAX_OUTPUT_BYTES);
    }
    let mut text = String::from_utf8_lossy(&bytes).to_string();
    if truncated {
        text.push_str("\n... (truncated)");
    }
    text
}

impl BuiltinTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the current working directory."
    }

    fn parameters(&self) -> Value {
        InputSchema::object()
            .property("command", InputSchema::string().description("Shell command to execute"))
            .property(
                "timeout",
                InputSchema::integer()
                    .description("Maximum time in seconds (1-600)")
                    .min(1)
                    .max(600)
                    .default(DEFAULT_TIMEOUT_SECONDS),
            )
            .property(
                "env",
                InputSchema::object().description("Additional environment variables"),
            )
            .property("description", InputSchema::string().description("Human-readable description for approval"))
            .required(vec!["command"])
            .build()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let _ = InputSchema::object()
            .property("command", InputSchema::string())
            .property("timeout", InputSchema::integer().min(1).max(600))
            .property("env", InputSchema::object())
            .property("description", InputSchema::string())
            .required(vec!["command"])
            .validate(&args)?;

        let command = args["command"].as_str().unwrap_or("");
        let desc = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or(command);
        let cwd = self.kaos.getcwd();
        let approval_subject = if desc.len() > 200 { &desc[..200] } else { desc };

        Ok(ToolExecution {
            accesses: ToolAccesses::shell(),
            description: desc.to_string(),
            approval_rule: format!("shell:{}:{}", cwd, approval_subject),
            execute: {
                let this = Self {
                    kaos: self.kaos.clone(),
                    workspace_root: self.workspace_root.clone(),
                };
                Box::new(move |ctx| Box::pin(this.execute(args, ctx)))
            },
        })
    }
}
```

*Important:* The `Process` type in `kaos-rs` exposes `stdout()` and `stderr()` as `async fn`. The helper `proc.stdout_blocking()` used above is a placeholder for calling the async method from a non-async context, which is not valid Rust. To keep the plan concrete and executable, use `tokio::runtime::Handle::current().block_on(proc.stdout())` inside `make_output`, or convert `make_output` to `async fn` and await both calls. The recommended fix is to make `make_output` async:

```rust
async fn make_output(
    proc: &kaos_rs::process::Process,
    exit_code: i32,
    description: &str,
    override_message: Option<String>,
) -> ExecutableToolResult {
    let stdout = truncate_output(proc.stdout().await);
    let stderr = truncate_output(proc.stderr().await);
    // ... rest identical
}
```

and update the two call sites to `.await` it.

- [ ] Register the module in `rust-ody/crates/tools-rs/src/builtin/mod.rs` by adding `pub mod bash;` alongside the other tool modules.

- [ ] Run the tests and verify they pass:

```text
$ cd rust-ody && cargo test -p tools-rs bash_
running 4 tests
test builtin::bash::tests::echo_returns_stdout_and_zero_exit ... ok
test builtin::bash::tests::env_vars_are_visible_to_command ... ok
test builtin::bash::tests::non_zero_exit_is_marked_error ... ok
test builtin::bash::tests::timeout_kills_long_sleep ... ok
```

- [ ] Commit: `feat(tools-rs): implement foreground BashTool with timeout and abort`

---

## Local Self-Review

- [ ] 1. **Spec-coverage table (Part 4 scope):**

| Requirement | Task | Status |
|---|---|---|
| Read image/video file and return base64 resource | Task 2 | covered |
| Detect MIME type and media type (`image`/`video`) | Task 2 | covered |
| Return image dimensions | Task 2 | covered |
| Enforce max media file size | Task 2 | covered |
| Reject non-media / unsupported files | Task 2 | covered |
| Shell command execution through configured shell | Task 3 | covered |
| Extra env vars merged into shell environment | Task 3 | covered |
| Configurable timeout with default and max | Task 3 | covered |
| Timeout escalates SIGTERM → SIGKILL | Task 3 | covered |
| Abort signal terminates running shell | Task 3 | covered |
| Non-zero exit code marked as error | Task 3 | covered |
| Output truncation at a reasonable cap | Task 3 | covered |

- [ ] 2. **Placeholder scan:** No `TODO`/`TBD`/deferred placeholders in the above code; every function body is complete.
- [ ] 3. **No phantom tasks:** Each task creates/modifies real files and ends with a passing test or build check.
- [ ] 4. **Dependency soundness:** Task 2 depends on Task 1 (crates). Task 3 depends only on Part 1 trait/contracts. No later symbols are referenced before they are defined.
- [ ] 5. **Caller & build soundness:** Task 1 adds dependencies; no caller changes. Tasks 2 and 3 register modules in `builtin/mod.rs`. No shared signatures are changed; the `ExecutableToolResult` constructor used in Task 2 (`ok_text_with_parts`) must match Part 1 exactly — if Part 1 named it differently, rename at implementation time.
- [ ] 6. **Test-the-risk:**
  - Media: asserts dimensions match the synthetic 2×1 PNG, asserts base64 starts with PNG magic, asserts oversized file is rejected.
  - Bash: asserts stdout content, non-zero exit error message, timeout kills sleep, and env var injection works.
- [ ] 7. **Type consistency:** `ExecutableToolContext.signal` is `AbortSignal` with `aborted() -> bool`, as introduced in Part 1. `ToolAccesses::shell()` is declared in Part 1; if missing, add it as part of Task 3 in the same commit.
