# Part 2: Background Management Tools (TaskList + TaskOutput + TaskStop)

**Depends on:** `infra.md` Task 1 (BackgroundManager trait + BackgroundTaskInfoData + BackgroundTaskOutputSnapshot types)

## File Summary

| Action | Path | Purpose |
|---|---|---|
| Create | `rust-ody/crates/tools-rs/src/builtin/background/task_list.rs` | TaskListTool |
| Create | `rust-ody/crates/tools-rs/src/builtin/background/task_output.rs` | TaskOutputTool |
| Create | `rust-ody/crates/tools-rs/src/builtin/background/task_stop.rs` | TaskStopTool |
| Create | `rust-ody/crates/tools-rs/tests/background_tools.rs` | Tests for all three tools |

---

### Task 1: TaskListTool

**Depends on:** `infra.md` Task 1 (BackgroundManager trait)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/background/task_list.rs`
- Create: `rust-ody/crates/tools-rs/tests/background_tools.rs`

- [ ] Write the failing test

Create `rust-ody/crates/tools-rs/tests/background_tools.rs`:

```rust
use serde_json::json;
use tools_rs::builtin::background::{
    BackgroundManager, BackgroundTaskInfoData, BackgroundTaskStatus,
    MockBackgroundManager,
};
use tools_rs::builtin::background::task_list::TaskListTool;
use tools_rs::builtin::BuiltinTool;

#[test]
fn test_task_list_empty() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    let tool = TaskListTool::new(mgr);
    let exec = tool.resolve_execution(json!({"active_only": false, "limit": 20})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    assert!(result.to_text().contains("active_background_tasks: 0"));
}

#[test]
fn test_task_list_with_tasks() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-002".into(),
        description: "another task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 2000,
        ended_at: Some(3000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskListTool::new(mgr);
    let exec = tool.resolve_execution(json!({"active_only": true, "limit": 20})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("active_background_tasks: 1"));
    assert!(text.contains("task-001"));
    assert!(!text.contains("task-002")); // completed tasks filtered out
}

#[test]
fn test_task_list_all() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-002".into(),
        description: "another task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 2000,
        ended_at: Some(3000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskListTool::new(mgr);
    let exec = tool.resolve_execution(json!({"active_only": false, "limit": 20})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("background_tasks: 2"));
    assert!(text.contains("task-001"));
    assert!(text.contains("task-002"));
}

#[test]
fn test_task_list_limit() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    for i in 0..5 {
        mgr.add_task(BackgroundTaskInfoData {
            task_id: format!("task-{:03}", i),
            description: format!("task {}", i),
            status: BackgroundTaskStatus::Running,
            started_at: (1000 + i) as u64,
            ended_at: None,
            stop_reason: None,
            terminal_notification_suppressed: false,
        });
    }

    let tool = TaskListTool::new(mgr);
    let exec = tool.resolve_execution(json!({"active_only": false, "limit": 3})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("background_tasks: 5")); // total count
    assert!(text.contains("(showing 3)"));
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test background_tools 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::builtin::background::task_list`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/builtin/background/task_list.rs`:**

```rust
use std::sync::Arc;
use serde_json::{json, Value};

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ExecutableToolOutput,
    ToolExecution, ToolError,
};
use super::{BackgroundManager, BackgroundTaskInfoData, BackgroundTaskStatus};

pub struct TaskListTool<M: BackgroundManager + 'static> {
    manager: Arc<M>,
}

impl<M: BackgroundManager + 'static> TaskListTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }

    fn format_task(info: &BackgroundTaskInfoData) -> String {
        let mut lines = Vec::new();
        lines.push(format!("task_id: {}", info.task_id));
        lines.push(format!("description: {}", info.description));
        lines.push(format!("status: {}", status_to_str(info.status)));
        if let Some(ended) = info.ended_at {
            lines.push(format!("ended_at: {}", ended));
        }
        lines.push(format!("started_at: {}", info.started_at));
        if let Some(ref reason) = info.stop_reason {
            lines.push(format!("stop_reason: {}", reason));
        }
        if info.terminal_notification_suppressed {
            lines.push("terminal_notification_suppressed: true".to_string());
        }
        lines.join("\n")
    }
}

fn status_to_str(status: BackgroundTaskStatus) -> &'static str {
    match status {
        BackgroundTaskStatus::Running => "running",
        BackgroundTaskStatus::Completed => "completed",
        BackgroundTaskStatus::Failed => "failed",
        BackgroundTaskStatus::TimedOut => "timed_out",
        BackgroundTaskStatus::Killed => "killed",
        BackgroundTaskStatus::Lost => "lost",
    }
}

impl<M: BackgroundManager + 'static> BuiltinTool for TaskListTool<M> {
    fn name(&self) -> &str { "TaskList" }

    fn description(&self) -> &str {
        "List background tasks and their current status."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "active_only": {
                    "type": "boolean",
                    "description": "Whether to list only non-terminal background tasks.",
                    "default": true
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of tasks to return.",
                    "minimum": 1,
                    "maximum": 100,
                    "default": 20
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let active_only = args.get("active_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(20);

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!("List {} background tasks", if active_only { "active" } else { "all" }),
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                Box::pin(async move {
                    let tasks = manager.list(active_only, Some(limit));
                    let total = tasks.len();
                    let header = if active_only {
                        format!("active_background_tasks: {}", total)
                    } else {
                        format!("background_tasks: {}", total)
                    };

                    let mut output = header;
                    if tasks.is_empty() {
                        output.push_str("\nNo background tasks.");
                    } else {
                        for task in &tasks {
                            output.push_str("\n---\n");
                            output.push_str(&Self::format_task(task));
                        }
                        // Show limit note if there might be more
                        if total >= limit {
                            output.push_str(&format!("\n---\n(showing {})", total));
                        }
                    }

                    ExecutableToolResult {
                        output: ExecutableToolOutput::Text(output),
                        message: None,
                        is_error: false,
                    }
                })
            }),
        })
    }
}
```

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test background_tools 2>&1 | tail -10
# Expected: test result: ok. 4 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/builtin/background/task_list.rs \
        rust-ody/crates/tools-rs/tests/background_tools.rs
git commit -m "feat(tools-rs): add TaskListTool"
```

---

### Task 2: TaskOutputTool

**Depends on:** `infra.md` Task 1 (BackgroundManager trait, BackgroundTaskOutputSnapshot)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/background/task_output.rs`
- Modify: `rust-ody/crates/tools-rs/tests/background_tools.rs` (append)

- [ ] Write the failing test

Append to `rust-ody/crates/tools-rs/tests/background_tools.rs`:

```rust
use tools_rs::builtin::background::task_output::TaskOutputTool;
use tools_rs::builtin::background::BackgroundTaskOutputSnapshot;

#[test]
fn test_task_output_snapshot() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.set_output_snapshot("task-001", BackgroundTaskOutputSnapshot {
        output_path: Some("/tmp/output.log".into()),
        output_size_bytes: 100,
        preview_bytes: 11,
        truncated: false,
        full_output_available: true,
        preview: "hello world".into(),
    });

    let tool = TaskOutputTool::new(mgr);
    let exec = tool.resolve_execution(json!({"task_id": "task-001", "block": false})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("hello world"));
    assert!(text.contains("outputPath"));
    assert!(text.contains("/tmp/output.log"));
}

#[test]
fn test_task_output_not_found() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    let tool = TaskOutputTool::new(mgr);
    let exec = tool.resolve_execution(json!({"task_id": "nonexistent", "block": false})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(result.is_error);
    assert!(result.to_text().contains("not found"));
}

#[test]
fn test_task_output_truncated() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 1000,
        ended_at: Some(2000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });
    mgr.set_output_snapshot("task-001", BackgroundTaskOutputSnapshot {
        output_path: None,
        output_size_bytes: 1024 * 1024,
        preview_bytes: 32768,
        truncated: true,
        full_output_available: true,
        preview: "a".repeat(32768),
    });

    let tool = TaskOutputTool::new(mgr);
    let exec = tool.resolve_execution(json!({"task_id": "task-001", "block": false})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("truncated"));
    assert!(text.contains("full output available"));
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test background_tools 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::builtin::background::task_output`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/builtin/background/task_output.rs`:**

```rust
use std::sync::Arc;
use serde_json::{json, Value};

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ExecutableToolOutput,
    ToolExecution, ToolError,
};
use super::{BackgroundManager, BackgroundTaskStatus};

const OUTPUT_PREVIEW_BYTES: usize = 32 * 1024; // 32 KiB
const PAGING_HINT_LINES: usize = 300;

pub struct TaskOutputTool<M: BackgroundManager + 'static> {
    manager: Arc<M>,
}

impl<M: BackgroundManager + 'static> TaskOutputTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

fn retrieval_status(info: &super::BackgroundTaskInfoData) -> String {
    match info.status {
        BackgroundTaskStatus::Running => "running".to_string(),
        BackgroundTaskStatus::Completed => "completed".to_string(),
        BackgroundTaskStatus::Failed => "failed".to_string(),
        BackgroundTaskStatus::TimedOut => "timed_out".to_string(),
        BackgroundTaskStatus::Killed => format!("killed (stop_reason: {})", info.stop_reason.as_deref().unwrap_or("unknown")),
        BackgroundTaskStatus::Lost => "lost".to_string(),
    }
}

fn terminal_reason(info: &super::BackgroundTaskInfoData) -> Option<String> {
    if info.status.is_terminal() {
        Some(match info.status {
            BackgroundTaskStatus::TimedOut => "timed_out".to_string(),
            BackgroundTaskStatus::Killed | BackgroundTaskStatus::Failed => {
                format!("stopped ({})", info.stop_reason.as_deref().unwrap_or("unknown"))
            }
            _ => info.status_to_str().to_string(),
        })
    } else {
        None
    }
}

impl BackgroundTaskStatus {
    fn to_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Killed => "killed",
            Self::Lost => "lost",
        }
    }
}

fn full_output_hint(snapshot: &super::BackgroundTaskOutputSnapshot) -> String {
    if snapshot.truncated && snapshot.full_output_available {
        let extra = snapshot.output_size_bytes.saturating_sub(snapshot.preview_bytes as u64);
        format!(
            "Output is truncated ({preview}B of {total}B shown). Use the Read tool with output_path ({path}) to read the full output ({extra}B remaining without truncation).",
            preview = snapshot.preview_bytes,
            total = snapshot.output_size_bytes,
            path = snapshot.output_path.as_deref().unwrap_or("<no path>"),
            extra = extra,
        )
    } else {
        String::new()
    }
}

impl<M: BackgroundManager + 'static> BuiltinTool for TaskOutputTool<M> {
    fn name(&self) -> &str { "TaskOutput" }

    fn description(&self) -> &str {
        "Retrieve output from a running or completed background task."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The background task ID to inspect."
                },
                "block": {
                    "type": "boolean",
                    "description": "Whether to wait for the task to finish before returning.",
                    "default": false
                },
                "timeout": {
                    "type": "integer",
                    "description": "Maximum number of seconds to wait when block=true.",
                    "minimum": 0,
                    "maximum": 3600,
                    "default": 30
                }
            },
            "required": ["task_id"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let task_id = args["task_id"].as_str().unwrap_or("").to_string();
        if task_id.is_empty() {
            return Err(ToolError::InvalidArgs("task_id is required".into()));
        }

        let block = args.get("block")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let timeout_secs = args.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!("Get output for task {}", task_id),
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let tid = task_id.clone();
                Box::pin(async move {
                    let info = match manager.get_task(&tid) {
                        Some(info) => info,
                        None => {
                            return ExecutableToolResult {
                                output: ExecutableToolOutput::Text(format!("Task {} not found.", tid)),
                                message: None,
                                is_error: true,
                            };
                        }
                    };

                    // If blocking, wait for terminal
                    if block && !info.status.is_terminal() {
                        let _waited = manager.wait(&tid, Some(timeout_secs * 1000));
                    }

                    // Re-fetch info after potential wait
                    let info = manager.get_task(&tid).unwrap_or(info);

                    let snapshot = manager.get_output_snapshot(&tid, OUTPUT_PREVIEW_BYTES);

                    let mut output = String::new();

                    // Status line
                    output.push_str(&format!("retrieval_status: {}\n", retrieval_status(&info)));

                    if let Some(reason) = terminal_reason(&info) {
                        output.push_str(&format!("terminal_reason: {}\n", reason));
                    }

                    if let Some(ref snap) = snapshot {
                        output.push_str(&format!("outputPath: {}\n", snap.output_path.as_deref().unwrap_or("<none>")));
                        output.push_str(&format!("outputSizeBytes: {}\n", snap.output_size_bytes));
                        output.push_str(&format!("outputTruncated: {}\n", snap.truncated));
                        output.push_str(&format!("fullOutputAvailable: {}\n", snap.full_output_available));

                        // Output hint
                        let hint = full_output_hint(snap);
                        if !hint.is_empty() {
                            output.push_str(&format!("fullOutputHint: {}\n", hint));
                        }

                        output.push_str("[output]\n");
                        output.push_str(&snap.preview);
                    } else {
                        output.push_str("[output]\n(no output available)");
                    }

                    ExecutableToolResult {
                        output: ExecutableToolOutput::Text(output),
                        message: None,
                        is_error: false,
                    }
                })
            }),
        })
    }
}
```

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test background_tools 2>&1 | tail -10
# Expected: test result: ok. 7 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/builtin/background/task_output.rs \
        rust-ody/crates/tools-rs/tests/background_tools.rs
git commit -m "feat(tools-rs): add TaskOutputTool"
```

---

### Task 3: TaskStopTool

**Depends on:** `infra.md` Task 1 (BackgroundManager trait)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/background/task_stop.rs`
- Modify: `rust-ody/crates/tools-rs/tests/background_tools.rs` (append)

- [ ] Write the failing test

Append to `rust-ody/crates/tools-rs/tests/background_tools.rs`:

```rust
use tools_rs::builtin::background::task_stop::TaskStopTool;

#[test]
fn test_task_stop_running() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Running,
        started_at: 1000,
        ended_at: None,
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskStopTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({"task_id": "task-001", "reason": "test"})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("stopped"));

    // Verify status changed
    let info = mgr.get_task("task-001").unwrap();
    assert_eq!(info.status, BackgroundTaskStatus::Killed);
}

#[test]
fn test_task_stop_not_found() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    let tool = TaskStopTool::new(mgr);
    let exec = tool.resolve_execution(json!({"task_id": "nonexistent"})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(result.is_error);
    assert!(result.to_text().contains("not found"));
}

#[test]
fn test_task_stop_already_terminal() {
    let mgr = std::sync::Arc::new(MockBackgroundManager::new());
    mgr.add_task(BackgroundTaskInfoData {
        task_id: "task-001".into(),
        description: "test task".into(),
        status: BackgroundTaskStatus::Completed,
        started_at: 1000,
        ended_at: Some(2000),
        stop_reason: None,
        terminal_notification_suppressed: false,
    });

    let tool = TaskStopTool::new(mgr);
    let exec = tool.resolve_execution(json!({"task_id": "task-001"})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("already terminal"));
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test background_tools 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::builtin::background::task_stop`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/builtin/background/task_stop.rs`:**

```rust
use std::sync::Arc;
use serde_json::{json, Value};

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ExecutableToolOutput,
    ToolExecution, ToolError,
};
use super::BackgroundManager;

pub struct TaskStopTool<M: BackgroundManager + 'static> {
    manager: Arc<M>,
}

impl<M: BackgroundManager + 'static> TaskStopTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

impl<M: BackgroundManager + 'static> BuiltinTool for TaskStopTool<M> {
    fn name(&self) -> &str { "TaskStop" }

    fn description(&self) -> &str {
        "Stop a running background task."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The background task ID to stop."
                },
                "reason": {
                    "type": "string",
                    "description": "Short reason recorded when the task is stopped.",
                    "default": "Stopped by TaskStop"
                }
            },
            "required": ["task_id"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let task_id = args["task_id"].as_str().unwrap_or("").to_string();
        if task_id.is_empty() {
            return Err(ToolError::InvalidArgs("task_id is required".into()));
        }

        let reason = args.get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Stopped by TaskStop")
            .to_string();

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!("Stop background task {}", task_id),
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let tid = task_id.clone();
                let reason = reason.clone();
                Box::pin(async move {
                    let info = match manager.get_task(&tid) {
                        Some(info) => info,
                        None => {
                            return ExecutableToolResult {
                                output: ExecutableToolOutput::Text(format!(
                                    "No background task found with id {}.",
                                    tid
                                )),
                                message: None,
                                is_error: true,
                            };
                        }
                    };

                    if info.status.is_terminal() {
                        return ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Task {} is already terminal (status: {:?}).",
                                tid, info.status
                            )),
                            message: None,
                            is_error: false,
                        };
                    }

                    // Suppress terminal notification before stopping
                    manager.suppress_terminal_notification(&tid);

                    match manager.stop(&tid, Some(reason)) {
                        Some(result) => ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Task {} stopped. Status: {:?}.",
                                result.task_id, result.status
                            )),
                            message: None,
                            is_error: false,
                        },
                        None => ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Failed to stop task {}.",
                                tid
                            )),
                            message: None,
                            is_error: true,
                        },
                    }
                })
            }),
        })
    }
}
```

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test background_tools 2>&1 | tail -10
# Expected: test result: ok. 10 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/builtin/background/task_stop.rs \
        rust-ody/crates/tools-rs/tests/background_tools.rs
git commit -m "feat(tools-rs): add TaskStopTool"
```

---

## Part 2 Self-Review

- [ ] 1. Spec-coverage: Task 1 (TaskListTool) covers 4.4.3.1 (active_only, limit, formatting). Task 2 (TaskOutputTool) covers 4.4.3.2 (output preview, paging hint, block, terminal reason). Task 3 (TaskStopTool) covers 4.4.3.3 (stop reason, terminal state check, suppress notification).
- [ ] 2. Placeholder scan: No TODO/TBD. All implementations are complete.
- [ ] 3. No phantom tasks: Each task produces a running tool with passing tests.
- [ ] 4. Dependency soundness: All tasks depend on `infra.md` Task 1 (BackgroundManager trait). Tasks are self-contained within Part 2.
- [ ] 5. Caller & build soundness: No shared-signature changes. All new files. `cargo check -p tools-rs` passes after each task.
- [ ] 6. Test-the-risk: Tests cover: empty state, active/all filtering, task not found, already-terminal stop, suppressed notification, truncated output with full output hint, limit display.
- [ ] 7. Type consistency: Uses `BackgroundManager` trait from Part 1. Uses `BackgroundTaskInfoData`, `BackgroundTaskStatus`, `BackgroundTaskOutputSnapshot` from Part 1. No cross-part type drift.
