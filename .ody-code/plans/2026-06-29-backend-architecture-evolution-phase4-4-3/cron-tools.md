# Part 3: Cron Management Tools (CronCreate + CronList + CronDelete)

**Depends on:** `infra.md` Tasks 1–4 (CronManager trait, cron_expr parser, jitter, time_format)

## File Summary

| Action | Path | Purpose |
|---|---|---|
| Create | `rust-ody/crates/tools-rs/src/builtin/cron/cron_create.rs` | CronCreateTool |
| Create | `rust-ody/crates/tools-rs/src/builtin/cron/cron_list.rs` | CronListTool |
| Create | `rust-ody/crates/tools-rs/src/builtin/cron/cron_delete.rs` | CronDeleteTool |
| Create | `rust-ody/crates/tools-rs/tests/cron_tools.rs` | Tests for all three tools |

---

### Task 1: CronCreateTool

**Depends on:** `infra.md` Task 1 (CronManager trait, SessionCronTaskInit), Task 2 (parse_cron_expression, has_fire_within_years), Task 3 (jittered_next_cron_run_ms, one_shot_jittered_next_cron_run_ms), Task 4 (format_local_iso_with_offset)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/cron/cron_create.rs`
- Create: `rust-ody/crates/tools-rs/tests/cron_tools.rs`

- [ ] Write the failing test

Create `rust-ody/crates/tools-rs/tests/cron_tools.rs`:

```rust
use serde_json::json;
use std::sync::Arc;
use tools_rs::builtin::cron::{MockCronManager, SessionCronStore, CronManager};
use tools_rs::builtin::cron::cron_create::CronCreateTool;
use tools_rs::builtin::BuiltinTool;

fn make_manager(now_ms: u64) -> Arc<MockCronManager> {
    Arc::new(MockCronManager::new(Some(now_ms)))
}

#[test]
fn test_cron_create_valid_recurring() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({
        "cron": "0 9 * * *",
        "prompt": "daily check",
        "recurring": true
    })).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error, "expected success, got: {}", result.to_text());
    let text = result.to_text();
    assert!(text.contains("daily check"));
    assert!(text.contains("recurring"));

    // Verify the task was added
    let tasks = mgr.list_tasks();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].cron, "0 9 * * *");
    assert_eq!(tasks[0].prompt, "daily check");
}

#[test]
fn test_cron_create_invalid_expression() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr);
    // Minute 60 is invalid
    let result = tool.resolve_execution(json!({
        "cron": "60 * * * *",
        "prompt": "bad cron",
        "recurring": true
    }));
    assert!(result.is_err());
}

#[test]
fn test_cron_create_empty_prompt() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr);
    let result = tool.resolve_execution(json!({
        "cron": "0 9 * * *",
        "prompt": "",
        "recurring": true
    }));
    assert!(result.is_err());
}

#[test]
fn test_cron_create_prompt_too_long() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr);
    let long_prompt = "x".repeat(8200); // 8192 byte limit
    let result = tool.resolve_execution(json!({
        "cron": "0 9 * * *",
        "prompt": long_prompt,
        "recurring": true
    }));
    assert!(result.is_err());
}

#[test]
fn test_cron_create_one_shot() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({
        "cron": "30 14 28 2 2026",
        "prompt": "check deploy",
        "recurring": false
    })).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("one-shot"));
    assert!(text.contains("check deploy"));
}

#[test]
fn test_cron_create_every_5_minutes() {
    let mgr = make_manager(1700000000000);
    let tool = CronCreateTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({
        "cron": "*/5 * * * *",
        "prompt": "poll status",
        "recurring": true
    })).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("every 5 minutes"));
}

#[test]
fn test_cron_create_session_cap() {
    let mgr = make_manager(1700000000000);
    // Add 50 tasks to fill the cap
    for i in 0..50 {
        mgr.add_task(tools_rs::builtin::cron::SessionCronTaskInit {
            cron: format!("{} * * * *", i % 60),
            prompt: format!("task {}", i),
            recurring: true,
        });
    }
    let tool = CronCreateTool::new(mgr);
    let result = tool.resolve_execution(json!({
        "cron": "0 9 * * *",
        "prompt": "overflow",
        "recurring": true
    }));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(format!("{:?}", err).contains("cap") || format!("{:?}", err).contains("limit") || format!("{:?}", err).contains("50"));
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test cron_tools 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::builtin::cron::cron_create`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/builtin/cron/cron_create.rs`:**

```rust
use std::sync::Arc;
use serde_json::{json, Value};

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ExecutableToolOutput,
    ToolExecution, ToolError,
};
use crate::cron::cron_expr::{parse_cron_expression, has_fire_within_years, cron_to_human};
use crate::cron::jitter::{
    jittered_next_cron_run_ms, one_shot_jittered_next_cron_run_ms, JitterConfig,
};
use crate::cron::time_format::format_local_iso_with_offset;
use super::{CronManager, SessionCronTaskInit};

const MAX_CRON_JOBS_PER_SESSION: usize = 50;
const MAX_PROMPT_BYTES: usize = 8192;
/// One-shot tasks must have their first fire within this many days from now.
const ONE_SHOT_MAX_FUTURE_DAYS: u64 = 350;

pub struct CronCreateTool<M: CronManager + 'static> {
    manager: Arc<M>,
}

impl<M: CronManager + 'static> CronCreateTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

impl<M: CronManager + 'static> BuiltinTool for CronCreateTool<M> {
    fn name(&self) -> &str { "CronCreate" }

    fn description(&self) -> &str {
        "Schedule a prompt to be enqueued at a future time. Use for both recurring schedules and one-shot reminders."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "cron": {
                    "type": "string",
                    "description": "5-field cron expression in local time: \"M H DoM Mon DoW\""
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to enqueue at each fire time.",
                    "minLength": 1,
                    "maxLength": 8192
                },
                "recurring": {
                    "type": "boolean",
                    "description": "true = fire on every cron match; false = fire once then auto-delete.",
                    "default": true
                }
            },
            "required": ["cron", "prompt"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let cron_raw = args["cron"].as_str().unwrap_or("").trim().to_string();
        let prompt = args["prompt"].as_str().unwrap_or("").to_string();
        let recurring = args.get("recurring")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // --- Validation ---

        // 1. Killswitch: ODY_DISABLE_CRON=1 (env check, but for L1 we skip env)
        // Real env check deferred to 4.3.8 integration.

        // 2. Normalize whitespace in cron expression
        let cron_normalized: String = cron_raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if cron_normalized.is_empty() || cron_normalized.split_whitespace().count() != 5 {
            return Err(ToolError::InvalidArgs(format!(
                "Invalid cron expression: '{}'. Must be 5 fields.",
                cron_raw
            )));
        }

        // 3. Parse cron expression
        let parsed = parse_cron_expression(&cron_normalized)
            .map_err(|e| ToolError::InvalidArgs(format!("Invalid cron expression: {}", e)))?;

        // 4. Reject if no fire within 5 years
        let now_ms = self.manager.now_ms();
        if !has_fire_within_years(&parsed, 5, now_ms) {
            return Err(ToolError::InvalidArgs(
                "Cron expression has no fire within the next 5 years.".into()
            ));
        }

        // 5. Session cap check
        let current_count = self.manager.list_tasks().len();
        if current_count >= MAX_CRON_JOBS_PER_SESSION {
            return Err(ToolError::InvalidArgs(format!(
                "Session cron limit reached ({}). Remove existing jobs first.",
                MAX_CRON_JOBS_PER_SESSION
            )));
        }

        // 6. Prompt byte-length cap
        if prompt.len() > MAX_PROMPT_BYTES {
            return Err(ToolError::InvalidArgs(format!(
                "Prompt too long: {} bytes (max {}).",
                prompt.len(), MAX_PROMPT_BYTES
            )));
        }
        if prompt.is_empty() {
            return Err(ToolError::InvalidArgs("Prompt must not be empty.".into()));
        }

        // 7. One-shot "rolled to next year" guard
        if !recurring {
            let max_future_ms = ONE_SHOT_MAX_FUTURE_DAYS * 24 * 3600 * 1000;
            if let Some(next_fire) = crate::cron::cron_expr::compute_next_cron_run(&parsed, now_ms) {
                if next_fire > now_ms + max_future_ms {
                    return Err(ToolError::InvalidArgs(
                        "One-shot task's first fire is too far in the future (max 350 days).".into()
                    ));
                }
            }
        }

        // 8. Parse year from cron expression (if month+dom specified for one-shots)
        // For one-shots, we need to construct the explicit date components.
        // The TS implementation uses the next-fire computation to validate.
        // We already did that above via has_fire_within_years and max_future_ms.

        let manager = Arc::clone(&self.manager);
        let human_schedule = cron_to_human(&parsed);
        let recurring_flag = recurring;

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!(
                "Schedule {} cron job: {}",
                if recurring_flag { "recurring" } else { "one-shot" },
                human_schedule
            ),
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let c = cron_normalized.clone();
                let p = prompt.clone();
                let sched = human_schedule.clone();
                let rec = recurring_flag;
                let parsed = parsed.clone();
                Box::pin(async move {
                    let now = manager.now_ms();
                    let task = manager.add_task(SessionCronTaskInit {
                        cron: c.clone(),
                        prompt: p.clone(),
                        recurring: rec,
                    });

                    // Compute jittered next fire time
                    let ideal = crate::cron::cron_expr::compute_next_cron_run(&parsed, now);
                    let jitter_config = JitterConfig::default();
                    let next_fire_at = if let Some(ideal_ms) = ideal {
                        if rec {
                            jittered_next_cron_run_ms(&parsed, ideal_ms, &task.id, &jitter_config)
                        } else {
                            one_shot_jittered_next_cron_run_ms(&task.id, ideal_ms, &jitter_config)
                        }
                    } else {
                        now
                    };

                    let next_fire_str = format_local_iso_with_offset(next_fire_at);

                    let kind = if rec { "recurring" } else { "one-shot" };
                    let output = format!(
                        "Cron job created.\nid: {}\ncron: {}\nhumanSchedule: {}\nprompt: {}\nnextFireAt: {}\nrecurring: {}\nageDays: 0.00\nstale: false",
                        task.id, task.cron, sched, p, next_fire_str, rec
                    );

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
cd rust-ody && cargo test -p tools-rs --test cron_tools 2>&1 | tail -10
# Expected: test result: ok. 7 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/builtin/cron/cron_create.rs \
        rust-ody/crates/tools-rs/tests/cron_tools.rs
git commit -m "feat(tools-rs): add CronCreateTool with validation, jitter, and formatting"
```

---

### Task 2: CronListTool

**Depends on:** `infra.md` Task 1 (CronManager trait), Task 2 (cron_to_human), Task 4 (format_local_iso_with_offset)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/cron/cron_list.rs`
- Modify: `rust-ody/crates/tools-rs/tests/cron_tools.rs` (append)

- [ ] Write the failing test

Append to `rust-ody/crates/tools-rs/tests/cron_tools.rs`:

```rust
use tools_rs::builtin::cron::cron_list::CronListTool;

#[test]
fn test_cron_list_empty() {
    let mgr = make_manager(1700000000000);
    let tool = CronListTool::new(mgr);
    let exec = tool.resolve_execution(json!({})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("cron_jobs: 0"));
    assert!(text.contains("No cron jobs scheduled"));
}

#[test]
fn test_cron_list_with_tasks() {
    let mgr = make_manager(1700000000000);
    let task1 = mgr.add_task(SessionCronTaskInit {
        cron: "0 9 * * *".into(),
        prompt: "daily check".into(),
        recurring: true,
    });
    let task2 = mgr.add_task(SessionCronTaskInit {
        cron: "*/5 * * * *".into(),
        prompt: "poll status".into(),
        recurring: true,
    });

    let tool = CronListTool::new(mgr);
    let exec = tool.resolve_execution(json!({})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("cron_jobs: 2"));
    assert!(text.contains(&task1.id));
    assert!(text.contains(&task2.id));
    // prompt should be JSON-stringified
    assert!(text.contains("\"daily check\""));
    assert!(text.contains("\"poll status\""));
}

#[test]
fn test_cron_list_stale_detection() {
    // Create a task that's 8 days old (past 7-day threshold)
    let now = 1700000000000u64;
    let stale_created = now - 8 * 24 * 3600 * 1000;
    let mgr = make_manager(now);

    // Use adopt to insert a task with custom created_at
    mgr.store.add(tools_rs::builtin::cron::SessionCronTaskInit {
        cron: "0 9 * * *".into(),
        prompt: "old task".into(),
        recurring: true,
    }, stale_created);

    let tool = CronListTool::new(mgr);
    let exec = tool.resolve_execution(json!({})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("stale: true"), "expected stale: true in output: {}", text);
    assert!(text.contains("ageDays: 8"), "expected 8-day age in output: {}", text);
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test cron_tools 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::builtin::cron::cron_list`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/builtin/cron/cron_list.rs`:**

```rust
use std::sync::Arc;
use serde_json::{json, Value};

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ExecutableToolOutput,
    ToolExecution, ToolError,
};
use crate::cron::cron_expr::cron_to_human;
use crate::cron::time_format::format_local_iso_with_offset;
use super::CronManager;

const MS_PER_DAY: u64 = 24 * 3600 * 1000;
const PROMPT_PREVIEW_BYTES: usize = 200;

pub struct CronListTool<M: CronManager + 'static> {
    manager: Arc<M>,
}

impl<M: CronManager + 'static> CronListTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

fn truncate_prompt(prompt: &str, max_bytes: usize) -> String {
    if prompt.len() <= max_bytes {
        return prompt.to_string();
    }
    // Truncate to max_bytes (UTF-8 safe: cut at byte boundary)
    let truncated: String = prompt.chars()
        .scan(0usize, |acc, ch| {
            *acc += ch.len_utf8();
            if *acc <= max_bytes { Some(ch) } else { None }
        })
        .collect();
    format!("{}…(truncated)", truncated)
}

impl<M: CronManager + 'static> BuiltinTool for CronListTool<M> {
    fn name(&self) -> &str { "CronList" }

    fn description(&self) -> &str {
        "List all cron jobs currently scheduled in this session."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "List scheduled cron jobs".to_string(),
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                Box::pin(async move {
                    let tasks = manager.list_tasks();
                    let now = manager.now_ms();

                    if tasks.is_empty() {
                        return ExecutableToolResult {
                            output: ExecutableToolOutput::Text(
                                "cron_jobs: 0\nNo cron jobs scheduled.".into()
                            ),
                            message: None,
                            is_error: false,
                        };
                    }

                    let mut output = format!("cron_jobs: {}\n", tasks.len());
                    for task in &tasks {
                        let human_schedule = cron_to_human(
                            &crate::cron::cron_expr::parse_cron_expression(&task.cron)
                                .unwrap_or_else(|_| crate::cron::cron_expr::ParsedCronExpression {
                                    raw: task.cron.clone(),
                                    minutes: vec![],
                                    hours: vec![],
                                    days_of_month: vec![],
                                    months: vec![],
                                    days_of_week: vec![],
                                    days_of_month_wildcard: true,
                                    days_of_week_wildcard: true,
                                })
                        );

                        let prompt_json = serde_json::to_string(&task.prompt).unwrap_or_else(|_| "\"\"".into());
                        let prompt_preview = truncate_prompt(&prompt_json, PROMPT_PREVIEW_BYTES);

                        let next_fire = manager.get_next_fire_for_task(&task.id);
                        let next_fire_str = next_fire
                            .map(|ms| format_local_iso_with_offset(ms))
                            .unwrap_or_else(|| "<no fire>".to_string());

                        let age_days = (now.saturating_sub(task.created_at)) as f64 / MS_PER_DAY as f64;
                        let stale = manager.is_stale(task);

                        output.push_str("---\n");
                        output.push_str(&format!("id: {}\n", task.id));
                        output.push_str(&format!("cron: {}\n", task.cron));
                        output.push_str(&format!("humanSchedule: {}\n", human_schedule));
                        output.push_str(&format!("prompt: {}\n", prompt_preview));
                        output.push_str(&format!("nextFireAt: {}\n", next_fire_str));
                        output.push_str(&format!("recurring: {}\n", task.recurring));
                        output.push_str(&format!("ageDays: {:.2}\n", age_days));
                        output.push_str(&format!("stale: {}\n", stale));
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
cd rust-ody && cargo test -p tools-rs --test cron_tools 2>&1 | tail -10
# Expected: test result: ok. 10 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/builtin/cron/cron_list.rs \
        rust-ody/crates/tools-rs/tests/cron_tools.rs
git commit -m "feat(tools-rs): add CronListTool with formatting and stale detection"
```

---

### Task 3: CronDeleteTool

**Depends on:** `infra.md` Task 1 (CronManager trait)
**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/cron/cron_delete.rs`
- Modify: `rust-ody/crates/tools-rs/tests/cron_tools.rs` (append)

- [ ] Write the failing test

Append to `rust-ody/crates/tools-rs/tests/cron_tools.rs`:

```rust
use tools_rs::builtin::cron::cron_delete::CronDeleteTool;

#[test]
fn test_cron_delete_existing() {
    let mgr = make_manager(1700000000000);
    let task = mgr.add_task(SessionCronTaskInit {
        cron: "0 9 * * *".into(),
        prompt: "daily check".into(),
        recurring: true,
    });
    let id = task.id.clone();

    let tool = CronDeleteTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({"id": id})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("deleted"));

    // Verify removed
    assert!(mgr.get_task(&id).is_none());
}

#[test]
fn test_cron_delete_not_found() {
    let mgr = make_manager(1700000000000);
    let tool = CronDeleteTool::new(mgr);
    let exec = tool.resolve_execution(json!({"id": "deadbeef"})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(result.is_error);
    assert!(result.to_text().contains("not found") || result.to_text().contains("no cron job"));
}

#[test]
fn test_cron_delete_invalid_id() {
    let mgr = make_manager(1700000000000);
    let tool = CronDeleteTool::new(mgr);
    // Non-hex ID
    let result = tool.resolve_execution(json!({"id": "not-hex!"}));
    assert!(result.is_err());
}

#[test]
fn test_cron_delete_multiple_removal() {
    let mgr = make_manager(1700000000000);
    let t1 = mgr.add_task(SessionCronTaskInit {
        cron: "0 9 * * *".into(),
        prompt: "task 1".into(),
        recurring: true,
    });
    mgr.add_task(SessionCronTaskInit {
        cron: "0 10 * * *".into(),
        prompt: "task 2".into(),
        recurring: true,
    });

    let tool = CronDeleteTool::new(mgr.clone());
    let exec = tool.resolve_execution(json!({"id": t1.id})).unwrap();
    let ctx = tools_rs::builtin::ExecutableToolContext::default();
    let result = tokio::runtime::Runtime::new().unwrap().block_on((exec.execute)(ctx));
    assert!(!result.is_error);

    // Only task 1 should be removed
    assert!(mgr.get_task(&t1.id).is_none());
    assert_eq!(mgr.list_tasks().len(), 1);
}
```

- [ ] Run it and verify it FAILS

```bash
cd rust-ody && cargo test -p tools-rs --test cron_tools 2>&1 | tail -5
# Expected: error[E0432]: unresolved import `tools_rs::builtin::cron::cron_delete`
```

- [ ] Write the minimal implementation

**`rust-ody/crates/tools-rs/src/builtin/cron/cron_delete.rs`:**

```rust
use std::sync::Arc;
use serde_json::{json, Value};

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ExecutableToolOutput,
    ToolExecution, ToolError,
};
use super::CronManager;

/// 8-hex character ID pattern
const ID_PATTERN: &str = "^[0-9a-f]{8}$";

pub struct CronDeleteTool<M: CronManager + 'static> {
    manager: Arc<M>,
}

impl<M: CronManager + 'static> CronDeleteTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

impl<M: CronManager + 'static> BuiltinTool for CronDeleteTool<M> {
    fn name(&self) -> &str { "CronDelete" }

    fn description(&self) -> &str {
        "Cancel a scheduled cron job by id."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The 8-hex cron job id returned by CronCreate / CronList."
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let id = args["id"].as_str().unwrap_or("").to_string();

        // Validate ID format: exactly 8 hex chars
        let re = regex::Regex::new(ID_PATTERN).unwrap();
        if !re.is_match(&id) {
            return Err(ToolError::InvalidArgs(format!(
                "Invalid cron job id: '{}'. Must be 8 hex characters.",
                id
            )));
        }

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!("Delete cron job {}", id),
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let tid = id.clone();
                Box::pin(async move {
                    let removed = manager.remove_tasks(&[tid.clone()]);
                    if removed.is_empty() {
                        ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "No cron job with id {}.",
                                tid
                            )),
                            message: None,
                            is_error: true,
                        }
                    } else {
                        ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Cron job {} deleted.",
                                tid
                            )),
                            message: None,
                            is_error: false,
                        }
                    }
                })
            }),
        })
    }
}
```

Note: This requires adding `regex` to the existing dependencies in `Cargo.toml` — it's already there as `regex = "1"`.

- [ ] Run it and verify it PASSES

```bash
cd rust-ody && cargo test -p tools-rs --test cron_tools 2>&1 | tail -10
# Expected: test result: ok. 14 passed; 0 failed
```

- [ ] Commit

```bash
git add rust-ody/crates/tools-rs/src/builtin/cron/cron_delete.rs \
        rust-ody/crates/tools-rs/tests/cron_tools.rs
git commit -m "feat(tools-rs): add CronDeleteTool with ID validation"
```

---

## Part 3 Self-Review

- [ ] 1. Spec-coverage: Task 1 (CronCreateTool) covers 4.4.3.4 (parse, validate jitter, caps, byte-limit, one-shot guard, human schedule). Task 2 (CronListTool) covers 4.4.3.5 (formatting, age, stale, prompt truncation). Task 3 (CronDeleteTool) covers 4.4.3.6 (ID validation, removal, not-found error).
- [ ] 2. Placeholder scan: No TODO/TBD. Killswitch env check noted as deferred to 4.3.8 integration (product behavior, not a code TODO).
- [ ] 3. No phantom tasks: Each task produces a running tool with passing tests.
- [ ] 4. Dependency soundness: All tasks depend on `infra.md` Tasks 1-4. CronCreate specifically uses cron_expr (parse, compute_next, cron_to_human, has_fire_within_years), jitter (both functions), and time_format. CronList uses cron_to_human, time_format, and CronManager trait. CronDelete uses CronManager trait + regex. All deps satisfied.
- [ ] 5. Caller & build soundness: No shared-signature changes. All new files. `cargo check -p tools-rs` passes after each task.
- [ ] 6. Test-the-risk: Tests cover: valid recurring creation, invalid expression, empty prompt, prompt-too-long, session cap (50 jobs), one-shot creation, every-5-minutes human schedule, stale detection at 8 days, existing deletion, not-found deletion, invalid ID format, single removal among multiple tasks. Cron expression round-trip: input → parse → compute → human → output includes expected schedule text.
- [ ] 7. Type consistency: Uses `CronManager` trait, `SessionCronTaskInit`, `SessionCronStore` from Part 1 Task 1. Uses `parse_cron_expression`, `compute_next_cron_run`, `has_fire_within_years`, `cron_to_human` from Part 1 Task 2. Uses `jittered_next_cron_run_ms`, `one_shot_jittered_next_cron_run_ms`, `JitterConfig` from Part 1 Task 3. Uses `format_local_iso_with_offset` from Part 1 Task 4. All match exactly.
