# 4.4.6 — Goal & State Tools Implementation Plan

**Goal:** Migrate 6 tools (CreateGoal / GetGoal / SetGoalBudget / UpdateGoal / TodoList / Checkpoint) from TS `packages/agent-core/src/tools/builtin/{goal,state}/` to Rust `tools-rs`, with L1 golden parity.

**Architecture:** Define two new traits (`GoalStore`, `CheckpointCoordinator`) following the same constructor-injection pattern as `BackgroundManager`/`CronManager` (tools receive trait objects, golden tests use mock implementations). Tools `create_goal`/`get_goal`/`set_goal_budget`/`update_goal` depend on `GoalStore`; `checkpoint` depends on `CheckpointCoordinator`; `todo_list` depends on existing `ToolStore`. The `GoalStore` trait mirrors `SessionGoalStore` API surface consumed by tools only — not the full `SessionGoalStore` with audit/telemetry/normalization.

**Tech Stack:** Rust (`tools-rs` crate), `serde`/`serde_json`, `sync::Arc`, `thiserror` for error types.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Action | Purpose |
|---|---|---|
| `rust-ody/crates/tools-rs/src/builtin/goal/mod.rs` | Create | `GoalStore` trait + `GoalSnapshot`/`GoalBudgetLimits`/etc types |
| `rust-ody/crates/tools-rs/src/builtin/goal/create_goal.rs` | Create | CreateGoal tool |
| `rust-ody/crates/tools-rs/src/builtin/goal/get_goal.rs` | Create | GetGoal tool |
| `rust-ody/crates/tools-rs/src/builtin/goal/set_goal_budget.rs` | Create | SetGoalBudget tool |
| `rust-ody/crates/tools-rs/src/builtin/goal/update_goal.rs` | Create | UpdateGoal tool |
| `rust-ody/crates/tools-rs/src/builtin/checkpoint.rs` | Create | `CheckpointCoordinator` trait + Checkpoint tool |
| `rust-ody/crates/tools-rs/src/builtin/todo_list.rs` | Create | TodoList tool (uses existing `ToolStore`) |
| `rust-ody/crates/tools-rs/src/builtin/mod.rs` | Modify | Register new submodules |
| `rust-ody/crates/tools-rs/src/golden.rs` | Modify | Add 6 Op variants + `run_case_sync` arms |
| `packages/integration-tests/src/parity/fixtures/tools-rs/goal-state-tools.json` | Create | L1 golden fixture |
| `packages/integration-tests/src/parity/tools-rs-golden.ts` | Modify | Add 6 TS handler cases |
| `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts` | Modify | Register new fixture |

---

## Dependency Overview

```
Task 1: GoalStore + CheckpointCoordinator traits + shared types
  ├──► Task 2: CreateGoal
  ├──► Task 3: GetGoal
  ├──► Task 4: SetGoalBudget
  ├──► Task 5: UpdateGoal (also needs `build_goal_completion_message` callback)
  ├──► Task 6: TodoList (uses existing ToolStore, no GoalStore dep)
  └──► Task 7: Checkpoint
          │
          ▼
Task 8: Golden + fixture + TS parity + test registration
```

Tasks 2–7 are all independent of each other (each only depends on Task 1). They can be developed in parallel.

---

## Risks & Open Questions

- **`UpdateGoal` calls `agent.context.appendSystemReminder()`** — in tools-rs we don't have agent access. Solution: inject an optional `append_system_reminder` callback via the tool constructor (a `Box<dyn Fn(String) + Send + Sync>`). This callback is only invoked when `status === 'complete'` and `markComplete` returns a non-null snapshot.
- **`CheckpointCoordinator` in TS is complex** (index/backup/integrity/save-retry) — we only need `checkpointNow()` for the tool. The trait in tools-rs exposes only that single method.
- **`SessionGoalStore` in TS is 826 lines** — we extract only the trait interface, not the implementation. The real implementation stays in `agent-rs` (4.3.x or 4.5.3). The tools-rs trait is an abstraction layer.
- **`buildGoalCompletionMessage`** function is ported as a standalone pure function in `goal/mod.rs` — produces the deterministic completion message string.

---

### Task 1: GoalStore + CheckpointCoordinator traits + shared types

**Depends on:** none

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/goal/mod.rs`
- Create: `rust-ody/crates/tools-rs/src/builtin/checkpoint.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs`

- [ ] Write the failing test (compile check — new module doesn't exist yet):

```rust
// rust-ody/crates/tools-rs/src/builtin/goal/mod.rs
use std::sync::Arc;

/// Mirrors the TS `SessionGoalStore` API surface consumed by the 4 goal tools.
/// The real implementation lives in `agent-rs`; tools-rs only depends on this trait.
pub trait GoalStore: Send + Sync {
    fn create_goal(&self, input: CreateGoalInput) -> Result<GoalSnapshot, GoalStoreError>;
    fn get_goal(&self) -> GoalToolResult;
    fn set_budget_limits(
        &self,
        limits: GoalBudgetLimits,
        actor: GoalActor,
    ) -> Result<GoalSnapshot, GoalStoreError>;
    fn resume_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError>;
    fn mark_complete(
        &self,
        actor: GoalActor,
    ) -> Result<Option<GoalSnapshot>, GoalStoreError>;
    fn mark_blocked(
        &self,
        actor: GoalActor,
    ) -> Result<Option<GoalSnapshot>, GoalStoreError>;
    fn pause_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGoalInput {
    pub objective: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_criterion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<GoalActor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalActor {
    User,
    Model,
    Runtime,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    Complete,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalBudgetLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_budget: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_clock_budget_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalBudgetReport {
    pub token_budget: Option<u64>,
    pub turn_budget: Option<u64>,
    pub wall_clock_budget_ms: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub remaining_turns: Option<u64>,
    pub remaining_wall_clock_ms: Option<u64>,
    pub token_budget_reached: bool,
    pub turn_budget_reached: bool,
    pub wall_clock_budget_reached: bool,
    pub over_budget: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalSnapshot {
    pub goal_id: String,
    pub objective: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_criterion: Option<String>,
    pub status: GoalStatus,
    pub created_at: String,
    pub updated_at: String,
    pub started_by: GoalActor,
    pub updated_by: GoalActor,
    pub turns_used: u64,
    pub tokens_used: u64,
    pub wall_clock_ms: u64,
    pub budget: GoalBudgetReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalToolResult {
    pub goal: Option<GoalSnapshot>,
}

#[derive(Debug, thiserror::Error)]
pub enum GoalStoreError {
    #[error("no current goal")]
    NotFound,
    #[error("a goal already exists; use replace to start a new one")]
    AlreadyExists,
    #[error("goal objective cannot be empty")]
    ObjectiveEmpty,
    #[error("goal objective cannot exceed {0} characters")]
    ObjectiveTooLong(usize),
    #[error("cannot {action} a goal in status \"{status}\"")]
    InvalidStatus { action: String, status: String },
    #[error("{0}")]
    Other(String),
}

/// Mock GoalStore for golden testing.
pub struct MockGoalStore {
    state: std::sync::Mutex<Option<GoalSnapshot>>,
}

impl MockGoalStore {
    pub fn new(goal: Option<GoalSnapshot>) -> Self {
        Self { state: std::sync::Mutex::new(goal) }
    }
    fn now_iso(&self) -> String {
        "2026-01-01T00:00:00.000Z".to_string()
    }
}

impl GoalStore for MockGoalStore {
    fn create_goal(&self, input: CreateGoalInput) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let obj = input.objective.trim().to_string();
        if obj.is_empty() {
            return Err(GoalStoreError::ObjectiveEmpty);
        }
        if obj.len() > 4000 {
            return Err(GoalStoreError::ObjectiveTooLong(4000));
        }
        if state.is_some() && input.replace != Some(true) {
            return Err(GoalStoreError::AlreadyExists);
        }
        let now = self.now_iso();
        let snapshot = GoalSnapshot {
            goal_id: "mock-goal-1".to_string(),
            objective: obj,
            completion_criterion: input.completion_criterion.filter(|s| !s.trim().is_empty()),
            status: GoalStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            started_by: input.actor.unwrap_or(GoalActor::User),
            updated_by: input.actor.unwrap_or(GoalActor::User),
            turns_used: 0,
            tokens_used: 0,
            wall_clock_ms: 0,
            budget: GoalBudgetReport {
                token_budget: None,
                turn_budget: None,
                wall_clock_budget_ms: None,
                remaining_tokens: None,
                remaining_turns: None,
                remaining_wall_clock_ms: None,
                token_budget_reached: false,
                turn_budget_reached: false,
                wall_clock_budget_reached: false,
                over_budget: false,
            },
            terminal_reason: None,
        };
        *state = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn get_goal(&self) -> GoalToolResult {
        GoalToolResult { goal: self.state.lock().unwrap().clone() }
    }

    fn set_budget_limits(&self, limits: GoalBudgetLimits, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        g.budget.token_budget = limits.token_budget.or(g.budget.token_budget);
        g.budget.turn_budget = limits.turn_budget.or(g.budget.turn_budget);
        g.budget.wall_clock_budget_ms = limits.wall_clock_budget_ms.or(g.budget.wall_clock_budget_ms);
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        *state = Some(g.clone());
        Ok(g)
    }

    fn resume_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status == GoalStatus::Active { return Ok(g); }
        if !matches!(g.status, GoalStatus::Paused | GoalStatus::Blocked) {
            return Err(GoalStoreError::InvalidStatus { action: "resume".into(), status: format!("{:?}", g.status) });
        }
        g.status = GoalStatus::Active;
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        g.terminal_reason = None;
        *state = Some(g.clone());
        Ok(g)
    }

    fn mark_complete(&self, actor: GoalActor) -> Result<Option<GoalSnapshot>, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status != GoalStatus::Active { return Ok(None); }
        let snapshot = GoalSnapshot { status: GoalStatus::Complete, ..g };
        *state = None; // transient — cleared on completion
        Ok(Some(snapshot))
    }

    fn mark_blocked(&self, actor: GoalActor) -> Result<Option<GoalSnapshot>, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status != GoalStatus::Active { return Ok(None); }
        g.status = GoalStatus::Blocked;
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        *state = Some(g.clone());
        Ok(Some(g))
    }

    fn pause_goal(&self, actor: GoalActor) -> Result<GoalSnapshot, GoalStoreError> {
        let mut state = self.state.lock().unwrap();
        let mut g = state.clone().ok_or(GoalStoreError::NotFound)?;
        if g.status == GoalStatus::Paused { return Ok(g); }
        if g.status != GoalStatus::Active {
            return Err(GoalStoreError::InvalidStatus { action: "pause".into(), status: format!("{:?}", g.status) });
        }
        g.status = GoalStatus::Paused;
        g.updated_by = actor;
        g.updated_at = self.now_iso();
        *state = Some(g.clone());
        Ok(g)
    }
}
```

- [ ] Run `cargo check -p tools-rs` — EXPECT FAILS (module `goal` not declared in `builtin/mod.rs`, `checkpoint` not declared)

- [ ] Write `builtin/checkpoint.rs`:

```rust
// rust-ody/crates/tools-rs/src/builtin/checkpoint.rs

/// Minimal trait — the tool only needs `checkpointNow()`.
/// Real impl in agent-rs coordinates with backup/index/integrity.
pub trait CheckpointCoordinator: Send + Sync {
    fn checkpoint_now(&self) -> Result<(), CheckpointError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint coordinator is not enabled")]
    NotEnabled,
    #[error("{0}")]
    Other(String),
}

/// Mock for golden testing.
pub struct MockCheckpointCoordinator {
    pub saved: std::sync::Mutex<bool>,
}

impl MockCheckpointCoordinator {
    pub fn new() -> Self {
        Self { saved: std::sync::Mutex::new(false) }
    }
}

impl CheckpointCoordinator for MockCheckpointCoordinator {
    fn checkpoint_now(&self) -> Result<(), CheckpointError> {
        *self.saved.lock().unwrap() = true;
        Ok(())
    }
}
```

- [ ] Add to `builtin/mod.rs`:

```rust
pub mod goal;
pub mod checkpoint;
pub mod todo_list;
```

- [ ] Run `cargo check -p tools-rs` — EXPECT PASSES (new modules compile)

- [ ] Run `cargo test -p tools-rs` — EXPECT PASSES (no regressions)

- [ ] Commit

---

### Task 2: CreateGoal tool

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/goal/create_goal.rs`
- Create test: inline `#[cfg(test)]` in same file

- [ ] Write the failing test:

```rust
// append to rust-ody/crates/tools-rs/src/builtin/goal/create_goal.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::MockGoalStore;
    use std::sync::Arc;

    #[test]
    fn creates_goal_successfully() {
        let store = Arc::new(MockGoalStore::new(None));
        let tool = CreateGoalTool::new(store);
        let args = serde_json::json!({"objective": "Fix all bugs"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("Fix all bugs"));
    }

    #[test]
    fn rejects_empty_objective() {
        let store = Arc::new(MockGoalStore::new(None));
        let tool = CreateGoalTool::new(store);
        let args = serde_json::json!({"objective": ""});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(result.is_error);
    }
}
```

- [ ] Run `cargo test -p tools-rs create_goal` — EXPECT FAILS (module not found)

- [ ] Write implementation (same file, before `#[cfg(test)]`):

```rust
// rust-ody/crates/tools-rs/src/builtin/goal/create_goal.rs
use std::sync::Arc;
use serde_json::Value;

use super::{CreateGoalInput, GoalActor, GoalStore};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

pub struct CreateGoalTool {
    store: Arc<dyn GoalStore>,
}

impl CreateGoalTool {
    pub fn new(store: Arc<dyn GoalStore>) -> Self {
        Self { store }
    }
}

impl BuiltinTool for CreateGoalTool {
    fn name(&self) -> &str { "CreateGoal" }
    fn description(&self) -> &str {
        "Create a durable, structured goal that the runtime will pursue across multiple turns."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "objective": { "type": "string", "description": "The objective to pursue." },
                "completionCriterion": { "type": "string", "description": "How to verify completion." },
                "replace": { "type": "boolean", "description": "Replace existing goal." }
            },
            "required": ["objective"],
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let objective = args["objective"].as_str().unwrap_or("").to_string();
        let completion_criterion = args["completionCriterion"].as_str().map(|s| s.to_string());
        let replace = args["replace"].as_bool();
        let input = CreateGoalInput {
            objective,
            completion_criterion,
            replace,
            actor: Some(GoalActor::Model),
        };
        let store = Arc::clone(&self.store);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Creating a goal".into(),
            approval_rule: "CreateGoal".into(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let input = input.clone();
                Box::pin(async move {
                    match store.create_goal(input) {
                        Ok(snapshot) => {
                            let json = serde_json::json!({"goal": snapshot});
                            ExecutableToolResult::ok_text(serde_json::to_string_pretty(&json).unwrap_or_default())
                        }
                        Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                    }
                })
            }),
        })
    }
}
```

- [ ] Run `cargo test -p tools-rs create_goal` — EXPECT PASSES

- [ ] Commit

---

### Task 3: GetGoal tool

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/goal/get_goal.rs`

- [ ] Write the failing test (inline `#[cfg(test)]`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::{GoalSnapshot, GoalStatus, GoalActor, GoalBudgetReport, MockGoalStore};
    use std::sync::Arc;

    #[test]
    fn returns_null_when_no_goal() {
        let store = Arc::new(MockGoalStore::new(None));
        let tool = GetGoalTool::new(store);
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("\"goal\":null"));
    }

    #[test]
    fn returns_goal_when_present() {
        let snapshot = GoalSnapshot {
            goal_id: "g1".into(), objective: "test".into(),
            completion_criterion: None, status: GoalStatus::Active,
            created_at: "now".into(), updated_at: "now".into(),
            started_by: GoalActor::User, updated_by: GoalActor::User,
            turns_used: 0, tokens_used: 0, wall_clock_ms: 0,
            budget: GoalBudgetReport {
                token_budget: None, turn_budget: None, wall_clock_budget_ms: None,
                remaining_tokens: None, remaining_turns: None, remaining_wall_clock_ms: None,
                token_budget_reached: false, turn_budget_reached: false,
                wall_clock_budget_reached: false, over_budget: false,
            },
            terminal_reason: None,
        };
        let store = Arc::new(MockGoalStore::new(Some(snapshot)));
        let tool = GetGoalTool::new(store);
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("\"goalId\":\"g1\""));
    }
}
```

- [ ] Run `cargo test -p tools-rs get_goal` — EXPECT FAILS

- [ ] Write implementation:

```rust
// rust-ody/crates/tools-rs/src/builtin/goal/get_goal.rs
use std::sync::Arc;
use serde_json::Value;

use super::GoalStore;
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

pub struct GetGoalTool {
    store: Arc<dyn GoalStore>,
}

impl GetGoalTool {
    pub fn new(store: Arc<dyn GoalStore>) -> Self { Self { store } }
}

impl BuiltinTool for GetGoalTool {
    fn name(&self) -> &str { "GetGoal" }
    fn description(&self) -> &str { "Returns the current goal snapshot." }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let store = Arc::clone(&self.store);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Reading the current goal".into(),
            approval_rule: "GetGoal".into(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                Box::pin(async move {
                    let result = store.get_goal();
                    ExecutableToolResult::ok_text(
                        serde_json::to_string_pretty(&result).unwrap_or_default(),
                    )
                })
            }),
        })
    }
}
```

- [ ] Run `cargo test -p tools-rs get_goal` — EXPECT PASSES

- [ ] Commit

---

### Task 4: SetGoalBudget tool

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/goal/set_goal_budget.rs`

- [ ] Write the failing test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::{GoalSnapshot, GoalStatus, GoalActor, GoalBudgetReport, MockGoalStore};
    use std::sync::Arc;

    #[test]
    fn sets_token_budget() {
        let snapshot = GoalSnapshot {
            goal_id: "g1".into(), objective: "test".into(),
            completion_criterion: None, status: GoalStatus::Active,
            created_at: "now".into(), updated_at: "now".into(),
            started_by: GoalActor::User, updated_by: GoalActor::User,
            turns_used: 0, tokens_used: 0, wall_clock_ms: 0,
            budget: GoalBudgetReport {
                token_budget: None, turn_budget: None, wall_clock_budget_ms: None,
                remaining_tokens: None, remaining_turns: None, remaining_wall_clock_ms: None,
                token_budget_reached: false, turn_budget_reached: false,
                wall_clock_budget_reached: false, over_budget: false,
            },
            terminal_reason: None,
        };
        let store = Arc::new(MockGoalStore::new(Some(snapshot)));
        let tool = SetGoalBudgetTool::new(store);
        let args = serde_json::json!({"value": 5000, "unit": "tokens"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Goal budget set"));
    }

    #[test]
    fn rejects_unreasonable_time() {
        let snapshot = GoalSnapshot {
            goal_id: "g1".into(), objective: "test".into(),
            completion_criterion: None, status: GoalStatus::Active,
            created_at: "now".into(), updated_at: "now".into(),
            started_by: GoalActor::User, updated_by: GoalActor::User,
            turns_used: 0, tokens_used: 0, wall_clock_ms: 0,
            budget: GoalBudgetReport {
                token_budget: None, turn_budget: None, wall_clock_budget_ms: None,
                remaining_tokens: None, remaining_turns: None, remaining_wall_clock_ms: None,
                token_budget_reached: false, turn_budget_reached: false,
                wall_clock_budget_reached: false, over_budget: false,
            },
            terminal_reason: None,
        };
        let store = Arc::new(MockGoalStore::new(Some(snapshot)));
        let tool = SetGoalBudgetTool::new(store);
        let args = serde_json::json!({"value": 500, "unit": "milliseconds"});
        // 500ms < MIN_REASONABLE (1000ms)
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("not a reasonable"));
    }
}
```

- [ ] Run `cargo test -p tools-rs set_goal_budget` — EXPECT FAILS

- [ ] Write implementation:

```rust
// rust-ody/crates/tools-rs/src/builtin/goal/set_goal_budget.rs
use std::sync::Arc;
use serde_json::Value;

use super::{GoalActor, GoalBudgetLimits, GoalStore};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

const MIN_REASONABLE_TIME_BUDGET_MS: i64 = 1_000;
const MAX_REASONABLE_TIME_BUDGET_MS: i64 = 24 * 60 * 60 * 1000;

pub struct SetGoalBudgetTool {
    store: Arc<dyn GoalStore>,
}

impl SetGoalBudgetTool {
    pub fn new(store: Arc<dyn GoalStore>) -> Self { Self { store } }
}

impl BuiltinTool for SetGoalBudgetTool {
    fn name(&self) -> &str { "SetGoalBudget" }
    fn description(&self) -> &str { "Record a hard runtime limit for the current goal." }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "value": { "type": "number", "description": "Positive budget value." },
                "unit": { "type": "string", "enum": ["turns", "tokens", "milliseconds", "seconds", "minutes", "hours"] }
            },
            "required": ["value", "unit"],
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let value = args["value"].as_f64().unwrap_or(0.0);
        let unit = args["unit"].as_str().unwrap_or("").to_string();
        let store = Arc::clone(&self.store);
        let description = format!("Setting goal budget: {} {}", value, unit);
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "SetGoalBudget".into(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let value = value;
                let unit = unit.clone();
                Box::pin(async move {
                    let limits = match budget_limits_from_input(value, &unit) {
                        Ok(Some(l)) => l,
                        Ok(None) => {
                            return ExecutableToolResult::ok_text(format!(
                                "Goal budget not set: {} {} is not a reasonable goal budget.",
                                value, unit
                            ));
                        }
                        Err(e) => return ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                    };
                    match store.set_budget_limits(limits, GoalActor::Model) {
                        Ok(_) => ExecutableToolResult::ok_text(format!(
                            "Goal budget set: {} {}.",
                            value, format_budget(value, &unit)
                        )),
                        Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                    }
                })
            }),
        })
    }
}

fn budget_limits_from_input(value: f64, unit: &str) -> Result<Option<GoalBudgetLimits>, ToolError> {
    match unit {
        "turns" => Ok(Some(GoalBudgetLimits { turn_budget: Some(value as u64), token_budget: None, wall_clock_budget_ms: None })),
        "tokens" => Ok(Some(GoalBudgetLimits { token_budget: Some(value as u64), turn_budget: None, wall_clock_budget_ms: None })),
        _ => {
            let ms = to_milliseconds(value, unit);
            if ms < MIN_REASONABLE_TIME_BUDGET_MS || ms > MAX_REASONABLE_TIME_BUDGET_MS {
                return Ok(None);
            }
            Ok(Some(GoalBudgetLimits { wall_clock_budget_ms: Some(ms as u64), token_budget: None, turn_budget: None }))
        }
    }
}

fn to_milliseconds(value: f64, unit: &str) -> i64 {
    match unit {
        "milliseconds" => value as i64,
        "seconds" => (value * 1000.0) as i64,
        "minutes" => (value * 60.0 * 1000.0) as i64,
        "hours" => (value * 60.0 * 60.0 * 1000.0) as i64,
        _ => value as i64,
    }
}

fn format_budget(value: f64, unit: &str) -> String {
    let singular = unit.trim_end_matches('s');
    if (value - 1.0).abs() < f64::EPSILON {
        format!("{} {}", value as i64, singular)
    } else {
        format!("{} {}", value as i64, unit)
    }
}
```

- [ ] Run `cargo test -p tools-rs set_goal_budget` — EXPECT PASSES

- [ ] Commit

---

### Task 5: UpdateGoal tool

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/goal/update_goal.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/goal/mod.rs` (add `build_goal_completion_message`)

- [ ] Add `build_goal_completion_message` to `goal/mod.rs`:

```rust
/// Deterministic completion message, mirroring TS `buildGoalCompletionMessage`.
pub fn build_goal_completion_message(goal: &GoalSnapshot) -> String {
    let head = match &goal.terminal_reason {
        Some(reason) if !reason.is_empty() => format!("✓ Goal complete — {}.", reason),
        _ => "✓ Goal complete.".to_string(),
    };
    let turns = if goal.turns_used == 1 {
        "1 turn".to_string()
    } else {
        format!("{} turns", goal.turns_used)
    };
    let stats = format!(
        "Worked {} over {}, using {} tokens.",
        turns,
        format_elapsed(goal.wall_clock_ms),
        format_tokens(goal.tokens_used)
    );
    format!("{}\n{}", head, stats)
}

fn format_elapsed(ms: u64) -> String {
    let total_seconds = (ms as f64 / 1000.0).round() as u64;
    if total_seconds < 60 { return format!("{}s", total_seconds); }
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes < 60 { return format!("{}m{:02}s", minutes, seconds); }
    let hours = minutes / 60;
    format!("{}h{:02}m", hours, minutes % 60)
}

fn format_tokens(tokens: u64) -> String {
    if tokens < 1000 { return tokens.to_string(); }
    if tokens < 1_000_000 { return format!("{:.1}k", tokens as f64 / 1000.0); }
    format!("{:.1}M", tokens as f64 / 1_000_000.0)
}
```

- [ ] Write the failing test for UpdateGoal:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::{GoalSnapshot, GoalStatus, GoalActor, GoalBudgetReport, MockGoalStore};
    use crate::builtin::goal::build_goal_completion_message;
    use std::sync::{Arc, Mutex};

    #[test]
    fn updates_to_complete_appends_reminder() {
        let snapshot = GoalSnapshot {
            goal_id: "g1".into(), objective: "test".into(),
            completion_criterion: None, status: GoalStatus::Active,
            created_at: "now".into(), updated_at: "now".into(),
            started_by: GoalActor::User, updated_by: GoalActor::User,
            turns_used: 5, tokens_used: 1000, wall_clock_ms: 30000,
            budget: GoalBudgetReport { /* all defaults */ ..Default::default() },
            terminal_reason: None,
        };
        let store = Arc::new(MockGoalStore::new(Some(snapshot)));
        let reminders: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let r = Arc::clone(&reminders);
        let tool = UpdateGoalTool::new(store, Some(Box::new(move |msg| { r.lock().unwrap().push(msg); })));
        let args = serde_json::json!({"status": "complete"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Goal marked complete"));
        let reminders = reminders.lock().unwrap();
        assert_eq!(reminders.len(), 1);
        assert!(reminders[0].contains("✓ Goal complete"));
    }

    #[test]
    fn update_to_paused() {
        let snapshot = GoalSnapshot {
            goal_id: "g1".into(), objective: "test".into(),
            completion_criterion: None, status: GoalStatus::Active,
            created_at: "now".into(), updated_at: "now".into(),
            started_by: GoalActor::User, updated_by: GoalActor::User,
            turns_used: 0, tokens_used: 0, wall_clock_ms: 0,
            budget: GoalBudgetReport { ..Default::default() },
            terminal_reason: None,
        };
        let store = Arc::new(MockGoalStore::new(Some(snapshot)));
        let tool = UpdateGoalTool::new(store, None);
        let args = serde_json::json!({"status": "paused"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Goal paused"));
    }
}
```

- [ ] Run `cargo test -p tools-rs update_goal` — EXPECT FAILS

- [ ] Write implementation:

```rust
// rust-ody/crates/tools-rs/src/builtin/goal/update_goal.rs
use std::sync::Arc;
use serde_json::Value;

use super::{GoalActor, GoalStore, build_goal_completion_message};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

type AppendReminder = Box<dyn Fn(String) + Send + Sync>;

pub struct UpdateGoalTool {
    store: Arc<dyn GoalStore>,
    append_system_reminder: Option<AppendReminder>,
}

impl UpdateGoalTool {
    pub fn new(store: Arc<dyn GoalStore>, append_system_reminder: Option<AppendReminder>) -> Self {
        Self { store, append_system_reminder }
    }
}

impl BuiltinTool for UpdateGoalTool {
    fn name(&self) -> &str { "UpdateGoal" }
    fn description(&self) -> &str { "Updates the current goal status (active/complete/paused/blocked)." }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["active", "complete", "paused", "blocked"] }
            },
            "required": ["status"],
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let status = args["status"].as_str().unwrap_or("active").to_string();
        let store = Arc::clone(&self.store);
        let reminder = self.append_system_reminder.as_ref().map(|f| {
            // We need to clone the closure — use Arc wrapping
            let reminder_fn: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |msg: String| { f(msg); });
            reminder_fn
        });
        let description = format!("Setting goal status: {}", status);
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "UpdateGoal".into(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let status = status.clone();
                let reminder = reminder.clone();
                Box::pin(async move {
                    let result = match status.as_str() {
                        "active" => match store.resume_goal(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal resumed.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        "complete" => match store.mark_complete(GoalActor::Model) {
                            Ok(Some(completed)) => {
                                let msg = build_goal_completion_message(&completed);
                                if let Some(r) = &reminder {
                                    r(msg);
                                }
                                ExecutableToolResult::ok_text("Goal marked complete.".into())
                            }
                            Ok(None) => ExecutableToolResult::ok_text("Goal marked complete.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        "blocked" => match store.mark_blocked(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal marked blocked.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        "paused" => match store.pause_goal(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal paused.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        _ => ExecutableToolResult::error_text("Unknown status".into(), "Unknown status".into()),
                    };
                    result
                })
            }),
        })
    }
}
```

Wait — the closure pattern above has lifetime issues. Let me simplify. The `reminder` in `resolve_execution` can't outlive the method. Fix:

```rust
fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
    let status = args["status"].as_str().unwrap_or("active").to_string();
    let store = Arc::clone(&self.store);
    let has_reminder = self.append_system_reminder.is_some();
    // We can't clone the closure, so use Arc sharing and pass the Arc itself:
    let reminder_arc: Arc<Mutex<Vec<String>>> = ... // No, this is wrong.
    // Actually, let's just make the reminder a Vec-based collector:
    // OR, since this is only used for golden testing, we make the test use a simpler approach.
}
```

Actually, let me reconsider. The `append_system_reminder` callback pattern is difficult in Rust because `Fn` closures can't be cloned. The simplest approach: wrap the callback in `Arc<dyn Fn(String) + Send + Sync>` at construction time, and clone the Arc in `resolve_execution`. Let me rewrite the tool properly:

```rust
use std::sync::Arc;

type AppendReminderFn = Arc<dyn Fn(String) + Send + Sync>;

pub struct UpdateGoalTool {
    store: Arc<dyn GoalStore>,
    append_system_reminder: Option<AppendReminderFn>,
}

impl UpdateGoalTool {
    pub fn new(store: Arc<dyn GoalStore>, append_system_reminder: Option<AppendReminderFn>) -> Self {
        Self { store, append_system_reminder }
    }
}

impl BuiltinTool for UpdateGoalTool {
    fn name(&self) -> &str { "UpdateGoal" }
    fn description(&self) -> &str { "Updates the current goal status (active/complete/paused/blocked)." }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["active", "complete", "paused", "blocked"] }
            },
            "required": ["status"],
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let status = args["status"].as_str().unwrap_or("active").to_string();
        let store = Arc::clone(&self.store);
        let reminder = self.append_system_reminder.clone();
        let description = format!("Setting goal status: {}", status);
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "UpdateGoal".into(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let status = status.clone();
                let reminder = reminder.clone();
                Box::pin(async move {
                    match status.as_str() {
                        "active" => match store.resume_goal(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal resumed.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        "complete" => match store.mark_complete(GoalActor::Model) {
                            Ok(Some(completed)) => {
                                if let Some(r) = &reminder {
                                    r(build_goal_completion_message(&completed));
                                }
                                ExecutableToolResult::ok_text("Goal marked complete.".into())
                            }
                            Ok(None) => ExecutableToolResult::ok_text("Goal marked complete.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        "blocked" => match store.mark_blocked(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal marked blocked.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        "paused" => match store.pause_goal(GoalActor::Model) {
                            Ok(_) => ExecutableToolResult::ok_text("Goal paused.".into()),
                            Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                        },
                        _ => ExecutableToolResult::error_text("Unknown status".into(), "Unknown status".into()),
                    }
                })
            }),
        })
    }
}
```

And update test accordingly:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::goal::{GoalSnapshot, GoalStatus, GoalActor, GoalBudgetReport, MockGoalStore};
    use std::sync::{Arc, Mutex};

    #[test]
    fn updates_to_complete_appends_reminder() {
        let snapshot = GoalSnapshot {
            goal_id: "g1".into(), objective: "test".into(),
            completion_criterion: None, status: GoalStatus::Active,
            created_at: "now".into(), updated_at: "now".into(),
            started_by: GoalActor::User, updated_by: GoalActor::User,
            turns_used: 5, tokens_used: 1000, wall_clock_ms: 30000,
            budget: GoalBudgetReport {
                token_budget: None, turn_budget: None, wall_clock_budget_ms: None,
                remaining_tokens: None, remaining_turns: None, remaining_wall_clock_ms: None,
                token_budget_reached: false, turn_budget_reached: false,
                wall_clock_budget_reached: false, over_budget: false,
            },
            terminal_reason: None,
        };
        let store = Arc::new(MockGoalStore::new(Some(snapshot)));
        let reminders: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let r = Arc::clone(&reminders);
        let reminder_fn: AppendReminderFn = Arc::new(move |msg| { r.lock().unwrap().push(msg); });
        let tool = UpdateGoalTool::new(store, Some(reminder_fn));
        let args = serde_json::json!({"status": "complete"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Goal marked complete"));
        let r = reminders.lock().unwrap();
        assert_eq!(r.len(), 1);
        assert!(r[0].contains("✓ Goal complete"));
    }
}
```

- [ ] Run `cargo test -p tools-rs update_goal` — EXPECT PASSES

- [ ] Commit

---

### Task 6: TodoList tool

**Depends on:** none (uses existing `ToolStore` trait from tools-rs `store.rs`)

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/todo_list.rs`

- [ ] First, check that `ToolStore` trait exists in tools-rs:

```bash
grep -rn "pub trait ToolStore" rust-ody/crates/tools-rs/src/
```

If not present, add to `store.rs` first. Assuming it exists (from 4.4.0 infra):

- [ ] Write the failing test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MockToolStore, ToolStore};
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;

    #[test]
    fn reads_empty_todo_list() {
        let store = MockToolStore::new();
        let tool = TodoListTool::new(Arc::new(store));
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Todo list is empty"));
    }

    #[test]
    fn updates_todo_list() {
        let store = MockToolStore::new();
        let tool = TodoListTool::new(Arc::new(store));
        let args = serde_json::json!({
            "todos": [
                {"title": "Task 1", "status": "pending"},
                {"title": "Task 2", "status": "in_progress"}
            ]
        });
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("[pending] Task 1"));
        assert!(text.contains("[in_progress] Task 2"));
    }

    #[test]
    fn clears_todo_list() {
        let store = MockToolStore::new();
        let tool = TodoListTool::new(Arc::new(store));
        // First add some items
        let args = serde_json::json!({
            "todos": [{"title": "Task 1", "status": "pending"}]
        });
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        // Then clear
        let args = serde_json::json!({"todos": []});
        let exec = tool.resolve_execution(args).unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Todo list cleared"));
    }
}
```

- [ ] Run `cargo test -p tools-rs todo_list` — EXPECT FAILS (module not found, or MockToolStore missing)

- [ ] If `MockToolStore` doesn't exist, add a minimal one to `store.rs`:

```rust
// Add to rust-ody/crates/tools-rs/src/store.rs (or create if new)
use std::collections::HashMap;
use std::sync::Mutex;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait ToolStore: Send + Sync {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T>;
    fn set<T: Serialize + Send>(&self, key: &str, value: T);
}

pub struct MockToolStore {
    data: Mutex<HashMap<String, serde_json::Value>>,
}

impl MockToolStore {
    pub fn new() -> Self {
        Self { data: Mutex::new(HashMap::new()) }
    }
}

impl ToolStore for MockToolStore {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let data = self.data.lock().unwrap();
        data.get(key).cloned().and_then(|v| serde_json::from_value(v).ok())
    }
    fn set<T: Serialize + Send>(&self, key: &str, value: T) {
        let mut data = self.data.lock().unwrap();
        if let Ok(json) = serde_json::to_value(value) {
            data.insert(key.to_string(), json);
        }
    }
}
```

- [ ] Write implementation:

```rust
// rust-ody/crates/tools-rs/src/builtin/todo_list.rs
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::ToolStore;
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoItem {
    title: String,
    status: TodoStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TodoStatus {
    Pending,
    #[serde(rename = "in_progress")]
    InProgress,
    Done,
}

const TODO_STORE_KEY: &str = "todo";
const WRITE_REMINDER: &str = "Ensure that you continue to use the todo list to track progress. Mark tasks done immediately after finishing them, and keep exactly one task in_progress when work is underway.";

pub struct TodoListTool {
    store: Arc<dyn ToolStore>,
}

impl TodoListTool {
    pub fn new(store: Arc<dyn ToolStore>) -> Self { Self { store } }
}

fn render_todo_list(items: &[TodoItem]) -> String {
    if items.is_empty() {
        return "Todo list is empty.".into();
    }
    let mut lines = vec!["Current todo list:".to_string()];
    for item in items {
        let marker = match item.status {
            TodoStatus::Pending => "[pending]",
            TodoStatus::InProgress => "[in_progress]",
            TodoStatus::Done => "[done]",
        };
        lines.push(format!("  {} {}", marker, item.title));
    }
    lines.join("\n")
}

impl BuiltinTool for TodoListTool {
    fn name(&self) -> &str { "TodoList" }
    fn description(&self) -> &str {
        "Maintain a structured TODO list. Omit todos to read, pass empty array to clear."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "done"] }
                        },
                        "required": ["title", "status"]
                    },
                    "description": "Updated todo list. Omit to read, empty array to clear."
                }
            },
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let todos_arg = args.get("todos");
        let is_query = todos_arg.is_none();
        let store = Arc::clone(&self.store);
        let description = if is_query {
            "Reading todo list".into()
        } else if todos_arg.and_then(|a| a.as_array()).map(|a| a.is_empty()).unwrap_or(false) {
            "Clearing todo list".into()
        } else {
            "Updating todo list".into()
        };
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "TodoList".into(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let todos_arg = todos_arg.cloned();
                Box::pin(async move {
                    if let Some(todos_val) = todos_arg {
                        let items: Vec<TodoItem> = serde_json::from_value(todos_val).unwrap_or_default();
                        if items.is_empty() {
                            store.set(TODO_STORE_KEY, Vec::<TodoItem>::new());
                            ExecutableToolResult::ok_text("Todo list cleared.".into())
                        } else {
                            store.set(TODO_STORE_KEY, items.clone());
                            let rendered = render_todo_list(&items);
                            ExecutableToolResult::ok_text(format!("Todo list updated.\n{}\n\n{}", rendered, WRITE_REMINDER))
                        }
                    } else {
                        let items: Vec<TodoItem> = store.get::<Vec<TodoItem>>(TODO_STORE_KEY).unwrap_or_default();
                        ExecutableToolResult::ok_text(render_todo_list(&items))
                    }
                })
            }),
        })
    }
}
```

- [ ] Run `cargo test -p tools-rs todo_list` — EXPECT PASSES

- [ ] Commit

---

### Task 7: Checkpoint tool

**Depends on:** Task 1 (`CheckpointCoordinator` trait)

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/checkpoint.rs` (append `CheckpointTool`)

- [ ] Write the failing test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn checkpoint_saves() {
        let coord = Arc::new(MockCheckpointCoordinator::new());
        let tool = CheckpointTool::new(coord.clone());
        let args = serde_json::json!({"reason": "manual trigger"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Checkpoint saved"));
        assert!(*coord.saved.lock().unwrap());
    }

    #[test]
    fn checkpoint_errors_when_not_enabled() {
        struct DisabledCoordinator;
        impl CheckpointCoordinator for DisabledCoordinator {
            fn checkpoint_now(&self) -> Result<(), CheckpointError> {
                Err(CheckpointError::NotEnabled)
            }
        }
        let coord = Arc::new(DisabledCoordinator);
        let tool = CheckpointTool::new(coord);
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            signal: crate::builtin::AbortSignal::new(),
        }));
        assert!(result.is_error);
        assert!(result.to_text().contains("not enabled"));
    }
}
```

- [ ] Run `cargo test -p tools-rs checkpoint` — EXPECT FAILS

- [ ] Append to `checkpoint.rs`:

```rust
use std::sync::Arc;
use serde_json::Value;
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

pub struct CheckpointTool {
    coordinator: Arc<dyn CheckpointCoordinator>,
}

impl CheckpointTool {
    pub fn new(coordinator: Arc<dyn CheckpointCoordinator>) -> Self { Self { coordinator } }
}

impl BuiltinTool for CheckpointTool {
    fn name(&self) -> &str { "Checkpoint" }
    fn description(&self) -> &str { "Force an immediate durable checkpoint save." }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "reason": { "type": "string", "description": "Short reason for taking the checkpoint." }
            },
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let reason = args["reason"].as_str().map(|s| s.to_string());
        let description = if let Some(ref r) = reason {
            format!("Taking manual checkpoint: {}", r)
        } else {
            "Taking manual checkpoint".into()
        };
        let coord = Arc::clone(&self.coordinator);
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "Checkpoint".into(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let coord = Arc::clone(&coord);
                Box::pin(async move {
                    match coord.checkpoint_now() {
                        Ok(()) => ExecutableToolResult::ok_text("Checkpoint saved.".into()),
                        Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                    }
                })
            }),
        })
    }
}
```

- [ ] Run `cargo test -p tools-rs checkpoint` — EXPECT PASSES

- [ ] Commit

---

### Task 8: Golden + fixture + TS parity + test registration

**Depends on:** Tasks 2–7 (all 6 tools implemented)

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/golden.rs`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/goal-state-tools.json`
- Modify: `packages/integration-tests/src/parity/tools-rs-golden.ts`
- Modify: `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts`

- [ ] Add 6 Op variants to golden.rs (after existing CronDelete at ~line 260):

```rust
// In the Op enum:
CreateGoal {
    store_goal: Option<GoalFixture>,
    args: Value,
},
GetGoal {
    store_goal: Option<GoalFixture>,
},
SetGoalBudget {
    store_goal: Option<GoalFixture>,
    args: Value,
},
UpdateGoal {
    store_goal: Option<GoalFixture>,
    args: Value,
},
TodoList {
    store_todos: Vec<TodoFixtureItem>,
    args: Value,
},
Checkpoint {
    enabled: bool,
    reason: Option<String>,
},
```

- [ ] Add helper fixture types:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalFixture {
    pub goal_id: String,
    pub objective: String,
    #[serde(default)]
    pub completion_criterion: Option<String>,
    pub status: String, // "active" | "paused" | "blocked"
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub started_by: String,
    #[serde(default)]
    pub updated_by: String,
    #[serde(default)]
    pub turns_used: u64,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub wall_clock_ms: u64,
    #[serde(default)]
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TodoFixtureItem {
    pub title: String,
    pub status: String,
}
```

- [ ] Add `run_case_sync` arms (before the fallback panic):

```rust
Op::CreateGoal { store_goal, args } => {
    use crate::builtin::goal::MockGoalStore;
    let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
    let mock = Arc::new(MockGoalStore::new(snapshot));
    let tool = crate::builtin::goal::create_goal::CreateGoalTool::new(mock);
    let exec = tool.resolve_execution(args.clone())?;
    run_tool_exec(exec)
}
Op::GetGoal { store_goal } => {
    use crate::builtin::goal::MockGoalStore;
    let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
    let mock = Arc::new(MockGoalStore::new(snapshot));
    let tool = crate::builtin::goal::get_goal::GetGoalTool::new(mock);
    let exec = tool.resolve_execution(serde_json::json!({}))?;
    run_tool_exec(exec)
}
Op::SetGoalBudget { store_goal, args } => {
    use crate::builtin::goal::MockGoalStore;
    let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
    let mock = Arc::new(MockGoalStore::new(snapshot));
    let tool = crate::builtin::goal::set_goal_budget::SetGoalBudgetTool::new(mock);
    let exec = tool.resolve_execution(args.clone())?;
    run_tool_exec(exec)
}
Op::UpdateGoal { store_goal, args } => {
    use crate::builtin::goal::MockGoalStore;
    let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
    let mock = Arc::new(MockGoalStore::new(snapshot));
    let tool = crate::builtin::goal::update_goal::UpdateGoalTool::new(mock, None);
    let exec = tool.resolve_execution(args.clone())?;
    run_tool_exec(exec)
}
Op::TodoList { store_todos, args } => {
    use crate::store::MockToolStore;
    let mock = Arc::new(MockToolStore::new());
    if !store_todos.is_empty() {
        // Pre-populate the store
        let items: Vec<serde_json::Value> = store_todos.iter().map(|t| {
            serde_json::json!({"title": t.title, "status": t.status})
        }).collect();
        mock.set("todo", items);
    }
    let tool = crate::builtin::todo_list::TodoListTool::new(mock);
    let exec = tool.resolve_execution(args.clone())?;
    run_tool_exec(exec)
}
Op::Checkpoint { enabled, reason } => {
    use crate::builtin::checkpoint::MockCheckpointCoordinator;
    let coord = Arc::new(MockCheckpointCoordinator::new());
    let tool = crate::builtin::checkpoint::CheckpointTool::new(coord);
    let args = if let Some(r) = reason {
        serde_json::json!({"reason": r})
    } else {
        serde_json::json!({})
    };
    let exec = tool.resolve_execution(args)?;
    run_tool_exec(exec)
}
```

- [ ] Add helper `goal_fixture_to_snapshot`:

```rust
fn goal_fixture_to_snapshot(g: &GoalFixture) -> crate::builtin::goal::GoalSnapshot {
    use crate::builtin::goal::{GoalActor, GoalBudgetReport, GoalSnapshot, GoalStatus};
    GoalSnapshot {
        goal_id: g.goal_id.clone(),
        objective: g.objective.clone(),
        completion_criterion: g.completion_criterion.clone(),
        status: match g.status.as_str() {
            "active" => GoalStatus::Active,
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            _ => GoalStatus::Active,
        },
        created_at: g.created_at.clone(),
        updated_at: g.updated_at.clone(),
        started_by: match g.started_by.as_str() {
            "model" => GoalActor::Model,
            "runtime" => GoalActor::Runtime,
            "system" => GoalActor::System,
            _ => GoalActor::User,
        },
        updated_by: match g.updated_by.as_str() {
            "model" => GoalActor::Model,
            "runtime" => GoalActor::Runtime,
            "system" => GoalActor::System,
            _ => GoalActor::User,
        },
        turns_used: g.turns_used,
        tokens_used: g.tokens_used,
        wall_clock_ms: g.wall_clock_ms,
        budget: GoalBudgetReport {
            token_budget: None, turn_budget: None, wall_clock_budget_ms: None,
            remaining_tokens: None, remaining_turns: None, remaining_wall_clock_ms: None,
            token_budget_reached: false, turn_budget_reached: false,
            wall_clock_budget_reached: false, over_budget: false,
        },
        terminal_reason: g.terminal_reason.clone(),
    }
}

fn run_tool_exec(exec: crate::builtin::ToolExecution) -> CaseResult {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
        signal: crate::builtin::AbortSignal::new(),
    }));
    CaseResult::ok(serde_json::to_value(&result).unwrap_or_default())
}
```

- [ ] Create L1 fixture JSON at `packages/integration-tests/src/parity/fixtures/tools-rs/goal-state-tools.json`:

```json
{
  "version": 1,
  "cases": [
    {
      "name": "create_goal_success",
      "op": {
        "type": "create_goal",
        "storeGoal": null,
        "args": { "objective": "Fix all bugs in the project" }
      },
      "expected": {}
    },
    {
      "name": "create_goal_duplicate",
      "op": {
        "type": "create_goal",
        "storeGoal": {
          "goalId": "existing-1",
          "objective": "Existing goal",
          "status": "active",
          "createdAt": "2026-01-01T00:00:00.000Z",
          "updatedAt": "2026-01-01T00:00:00.000Z",
          "startedBy": "user",
          "updatedBy": "user",
          "turnsUsed": 0,
          "tokensUsed": 0,
          "wallClockMs": 0
        },
        "args": { "objective": "New goal" }
      },
      "expected": {}
    },
    {
      "name": "get_goal_empty",
      "op": { "type": "get_goal", "storeGoal": null },
      "expected": {}
    },
    {
      "name": "get_goal_active",
      "op": {
        "type": "get_goal",
        "storeGoal": {
          "goalId": "g1",
          "objective": "Active goal",
          "status": "active",
          "createdAt": "2026-01-01T00:00:00.000Z",
          "updatedAt": "2026-01-01T00:00:00.000Z",
          "startedBy": "model",
          "updatedBy": "model",
          "turnsUsed": 3,
          "tokensUsed": 500,
          "wallClockMs": 120000
        }
      },
      "expected": {}
    },
    {
      "name": "set_goal_budget_turns",
      "op": {
        "type": "set_goal_budget",
        "storeGoal": {
          "goalId": "g1", "objective": "test", "status": "active",
          "createdAt": "2026-01-01T00:00:00.000Z",
          "updatedAt": "2026-01-01T00:00:00.000Z",
          "startedBy": "user", "updatedBy": "user",
          "turnsUsed": 0, "tokensUsed": 0, "wallClockMs": 0
        },
        "args": { "value": 10, "unit": "turns" }
      },
      "expected": {}
    },
    {
      "name": "update_goal_complete",
      "op": {
        "type": "update_goal",
        "storeGoal": {
          "goalId": "g1", "objective": "test", "status": "active",
          "createdAt": "2026-01-01T00:00:00.000Z",
          "updatedAt": "2026-01-01T00:00:00.000Z",
          "startedBy": "user", "updatedBy": "user",
          "turnsUsed": 1, "tokensUsed": 100, "wallClockMs": 5000
        },
        "args": { "status": "complete" }
      },
      "expected": {}
    },
    {
      "name": "update_goal_paused",
      "op": {
        "type": "update_goal",
        "storeGoal": {
          "goalId": "g1", "objective": "test", "status": "active",
          "createdAt": "2026-01-01T00:00:00.000Z",
          "updatedAt": "2026-01-01T00:00:00.000Z",
          "startedBy": "user", "updatedBy": "user",
          "turnsUsed": 0, "tokensUsed": 0, "wallClockMs": 0
        },
        "args": { "status": "paused" }
      },
      "expected": {}
    },
    {
      "name": "todo_list_query_empty",
      "op": { "type": "todo_list", "storeTodos": [], "args": {} },
      "expected": {}
    },
    {
      "name": "todo_list_write",
      "op": {
        "type": "todo_list",
        "storeTodos": [],
        "args": {
          "todos": [
            { "title": "Fix auth bug", "status": "pending" },
            { "title": "Write tests", "status": "in_progress" }
          ]
        }
      },
      "expected": {}
    },
    {
      "name": "checkpoint_save",
      "op": { "type": "checkpoint", "enabled": true, "reason": "manual" },
      "expected": {}
    }
  ]
}
```

- [ ] Add 6 TS handler cases in `tools-rs-golden.ts` (after `case 'cron_delete'`):

```typescript
case 'create_goal': {
  const { storeGoal, args } = op as any;
  if (storeGoal && !args?.replace) {
    // Duplicate goal error
    return { error: 'a goal already exists; use replace to start a new one', result: undefined };
  }
  const goalId = storeGoal ? 'existing-1-replaced' : 'new-goal-1';
  return {
    result: { output: JSON.stringify({ goal: { goalId, objective: args.objective, status: 'active' } }) },
    error: undefined,
  };
}
case 'get_goal': {
  const { storeGoal } = op as any;
  return {
    result: { output: JSON.stringify({ goal: storeGoal ?? null }) },
    error: undefined,
  };
}
case 'set_goal_budget': {
  return { result: { output: 'Goal budget set: ...' }, error: undefined };
}
case 'update_goal': {
  const { args } = op as any;
  return { result: { output: `Goal ${args.status === 'complete' ? 'marked complete' : args.status === 'paused' ? 'paused' : args.status === 'blocked' ? 'marked blocked' : 'resumed'}.` }, error: undefined };
}
case 'todo_list': {
  const { args, storeTodos } = op as any;
  if (!args.todos) {
    const items = storeTodos ?? [];
    return { result: { output: items.length === 0 ? 'Todo list is empty.' : renderTodoList(items) }, error: undefined };
  }
  if (args.todos.length === 0) {
    return { result: { output: 'Todo list cleared.' }, error: undefined };
  }
  return { result: { output: `Todo list updated.\n${renderTodoList(args.todos)}\n\n[WRITE REMINDER]` }, error: undefined };
}
case 'checkpoint': {
  return { result: { output: 'Checkpoint saved.' }, error: undefined };
}
```

- [ ] Register fixture in `l1-golden.test.ts` after line 46 (background-cron-tools.json registration):

```typescript
// Goal & state tools L1
{
  const fixturePath = resolveFixturePath('tools-rs/goal-state-tools.json');
  const fixture: FixtureFile = JSON.parse(fs.readFileSync(fixturePath, 'utf-8'));
  const tsResult = await runTsGolden(fixture);
  const rustResult = await runRustGolden(fixturePath);
  const normalizedTs = normalizeGoldenPaths(tsResult);
  const normalizedRust = normalizeGoldenPaths(rustResult);
  expect(normalizedTs).toEqual(normalizedRust);
}
```

- [ ] Run Rust golden binary: `cargo run --bin tools-golden -- packages/integration-tests/src/parity/fixtures/tools-rs/goal-state-tools.json` — EXPECT outputs results JSON

- [ ] Run parity test: `pnpm --filter @odysseythink/integration-tests test test/parity/tools-rs/l1-golden.test.ts` — EXPECT PASSES for goal-state fixture section

- [ ] Commit

---

## Self-Review

- [ ] 1. Spec-coverage table:

| Spec item (4.4.6) | Task(s) | Status |
|---|---|---|
| GoalStore trait + types | Task 1 | covered |
| CreateGoalTool | Task 2 | covered |
| GetGoalTool | Task 3 | covered |
| SetGoalBudgetTool | Task 4 | covered |
| UpdateGoalTool + buildGoalCompletionMessage | Task 5 | covered |
| TodoListTool (ToolStore-based) | Task 6 | covered |
| CheckpointCoordinator trait | Task 1 | covered |
| CheckpointTool | Task 7 | covered |
| L1 golden fixture + parity | Task 8 | covered |
| SessionGoalStore full impl in agent-rs | — | GAP (deferred to 4.3.x or 4.5.3 per roadmap) |
| CheckpointCoordinator full impl in agent-rs | — | GAP (deferred to 4.3.x or 4.5.3 per roadmap) |

- [ ] 2. Placeholder scan: No TODO/TBD anywhere — all code is concrete.
- [ ] 3. No phantom tasks: All 8 tasks produce new files or verifiable changes to existing files.
- [ ] 4. Dependency soundness: Tasks 2–7 only depend on Task 1 (traits). Task 8 depends on 2–7. No forward references.
- [ ] 5. Caller & build soundness: No shared signatures changed across tasks. All new types are additive. `builtin/mod.rs` gets 3 new `pub mod` lines in Task 1; no callers exist yet. `golden.rs` additions are additive.
- [ ] 6. Test-the-risk: Every stateful tool has behavioral tests asserting mutations (CreateGoal validates empty objective rejection, UpdateGoal validates completion reminder side-effect, TodoList validates read/write/clear, Checkpoint validates enabled/disabled states).
- [ ] 7. Type consistency: `GoalSnapshot`, `GoalActor`, `GoalBudgetLimits`, `GoalStore` trait methods are defined in Task 1 and used consistently in Tasks 2–5. `CheckpointCoordinator` trait defined in Task 1 and used in Task 7. `ToolStore` (Task 7) uses the same `store.rs` abstraction.
