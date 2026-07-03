# Part 1 — Shared Infrastructure for Session-Mode Tools

**Scope:** Define the trait boundaries that session-mode tools need from `agent-rs`, extend `ToolExecution` with the `display` field required by plan/design exit tools, propagate the approval `selected_label` through session-mode handoffs, and ensure `Agent` constructs `SessionModeManager` with the default behavior registry.

**Depends on:** none (this is the foundation for 4.4.5).

---

## Task 1: Extend `ToolExecution` with `display` and forward it through `ToolBridge`

**Depends on:** none

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs:96-102`
- Modify: `rust-ody/crates/agent-rs/src/tool/bridge.rs:108-134`
- Modify: every `ToolExecution { ... }` construction site in `tools-rs` and `agent-rs` (see search list below)
- Test: `rust-ody/crates/agent-rs/src/tool/bridge.rs` existing tests + new test

**Why:** `ExitPlanModeTool` and `ExitDesignModeTool` need to return a `plan_review` display object that the host approval surface renders. The `agent_loop::RunnableToolExecution` already has a `display: Option<JsonValue>` field, but `tools_rs::builtin::ToolExecution` does not, and `ToolBridge` currently drops it.

**Steps:**

- [ ] Write the failing test first. In `rust-ody/crates/agent-rs/src/tool/bridge.rs`, add:

```rust
#[tokio::test]
async fn bridge_forwards_display_from_tool_execution() {
    struct DisplayTool;
    impl tools_rs::builtin::BuiltinTool for DisplayTool {
        fn name(&self) -> &str { "Display" }
        fn description(&self) -> &str { "display" }
        fn parameters(&self) -> serde_json::Value { json!({"type":"object"}) }
        fn resolve_execution(
            &self,
            _args: serde_json::Value,
        ) -> Result<tools_rs::builtin::ToolExecution, tools_rs::builtin::ToolError> {
            Ok(tools_rs::builtin::ToolExecution {
                accesses: tools_rs::tool_accesses::ToolAccesses::none(),
                description: "display".into(),
                approval_rule: "Display".into(),
                matches_rule: None,
                display: Some(json!({"kind":"plan_review","plan":"x"})),
                execute: Box::new(|_ctx| Box::pin(async {
                    tools_rs::builtin::ExecutableToolResult::ok_text("ok".into())
                })),
            })
        }
    }

    let bridge = ToolBridge::new(Arc::new(DisplayTool));
    let exec = bridge.resolve_execution(json!({})).await.unwrap();
    match exec {
        crate::agent_loop::types::ToolExecution::Runnable(r) => {
            assert_eq!(r.display, Some(json!({"kind":"plan_review","plan":"x"})));
        }
        _ => panic!("expected Runnable"),
    }
}
```

Run it and verify it FAILS:

```bash
cd rust-ody && cargo test -p agent-rs tool::bridge::tests::bridge_forwards_display_from_tool_execution
```

Expected failure: `no field display on type ToolExecution` or `missing field display in initializer of ToolExecution`.

- [ ] Extend `ToolExecution` in `rust-ody/crates/tools-rs/src/builtin/mod.rs`:

```rust
pub struct ToolExecution {
    pub accesses: ToolAccesses,
    pub description: String,
    pub approval_rule: String,
    pub matches_rule: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    pub display: Option<serde_json::Value>,
    pub execute: ExecuteFn,
}
```

- [ ] Update `ToolBridge` in `rust-ody/crates/agent-rs/src/tool/bridge.rs` to forward `display`:

```rust
Ok(LoopToolExecution::Runnable(RunnableToolExecution {
    is_error: None,
    accesses: Some(convert_tool_accesses(tools_exec.accesses)),
    display: tools_exec.display,
    description: Some(tools_exec.description),
    stop_batch_after_this: None,
    approval_rule: tools_exec.approval_rule,
    matches_rule,
    execute: Box::new(move |loop_ctx: LoopContext| {
        // ... unchanged
    }),
})))
```

- [ ] Update every `ToolExecution { ... }` construction site to include `display: None` (or the real display for exit tools). Use this search to enumerate all sites:

```bash
cd rust-ody && rg -n "ToolExecution \{" crates/tools-rs/src crates/agent-rs/src
```

The affected files are:
- `crates/tools-rs/src/builtin/write.rs`
- `crates/tools-rs/src/builtin/todo_list.rs`
- `crates/tools-rs/src/builtin/read.rs`
- `crates/tools-rs/src/builtin/media.rs`
- `crates/tools-rs/src/builtin/grep.rs`
- `crates/tools-rs/src/builtin/goal/update_goal.rs`
- `crates/tools-rs/src/builtin/goal/set_goal_budget.rs`
- `crates/tools-rs/src/builtin/goal/get_goal.rs`
- `crates/tools-rs/src/builtin/goal/create_goal.rs`
- `crates/tools-rs/src/builtin/glob.rs`
- `crates/tools-rs/src/builtin/edit.rs`
- `crates/tools-rs/src/builtin/cron/cron_list.rs`
- `crates/tools-rs/src/builtin/cron/cron_delete.rs`
- `crates/tools-rs/src/builtin/cron/cron_create.rs`
- `crates/tools-rs/src/builtin/collaboration/skill.rs`
- `crates/tools-rs/src/builtin/collaboration/ask_user.rs`
- `crates/tools-rs/src/builtin/collaboration/agent.rs`
- `crates/tools-rs/src/builtin/checkpoint.rs`
- `crates/tools-rs/src/builtin/bash.rs`
- `crates/tools-rs/src/builtin/background/task_stop.rs`
- `crates/tools-rs/src/builtin/background/task_output.rs`
- `crates/tools-rs/src/builtin/background/task_list.rs`
- `crates/agent-rs/src/tool/bridge.rs` (test-only EchoTool)

For each site add `display: None,` inside the struct literal. No behavioral change for existing tools.

- [ ] Run the new test and the whole-workspace Rust typecheck:

```bash
cd rust-ody && cargo test -p agent-rs tool::bridge::tests::bridge_forwards_display_from_tool_execution
cd rust-ody && cargo check --workspace --all-targets
```

Expected: both green.

- [ ] Commit: `feat(tools-rs): add display field to ToolExecution and forward through ToolBridge`.

---

## Task 2: Propagate `selected_label` through session-mode handoffs

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/session_mode/types.rs:13-17`
- Modify: `rust-ody/crates/agent-rs/src/injection/types.rs` (pending handoff structs)
- Modify: `rust-ody/crates/agent-rs/src/session_mode/manager.rs:277-314`
- Modify: `rust-ody/crates/agent-rs/src/injection/manager.rs` (consumers of pending handoffs)
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs` (if it calls `handoff_to`)
- Test: `rust-ody/crates/agent-rs/tests/session_mode_manager.rs`

**Why:** TS `ExitPlanModeTool` passes the chosen approach label to `sessionMode.handoffTo('normal', { selectedLabel })`, and `ExitDesignModeTool` passes it to `handoffTo('plan', { selectedLabel })`. The Rust manager currently accepts only a `&str` target and stores no label.

**Steps:**

- [ ] Add a handoff-options struct in `rust-ody/crates/agent-rs/src/session_mode/types.rs`:

```rust
#[derive(Debug, Clone, Default)]
pub struct HandoffOptions {
    pub selected_label: Option<String>,
}
```

- [ ] Update `PendingDesignHandoff` and `PendingPlanHandoff` in `rust-ody/crates/agent-rs/src/injection/types.rs` to carry the label:

```rust
#[derive(Debug, Clone)]
pub struct PendingDesignHandoff {
    pub path: String,
    pub filename: String,
    pub selected_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingPlanHandoff {
    pub content: String,
    pub path: String,
    pub selected_label: Option<String>,
}
```

- [ ] Change `SessionModeManager::handoff_to` signature and body in `rust-ody/crates/agent-rs/src/session_mode/manager.rs`:

```rust
pub async fn handoff_to(
    &mut self,
    target: &str,
    options: HandoffOptions,
) -> anyhow::Result<()> {
    match target {
        "plan" => {
            let path = self.last_completed_design_file_path.clone();
            self.exit(None).await?;
            if let Some(path) = path {
                let filename = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.pending_handoff_for_plan = Some(PendingDesignHandoff {
                    path,
                    filename,
                    selected_label: options.selected_label,
                });
            }
        }
        "normal" => {
            let content = self
                .session_mode_file_path
                .as_ref()
                .and_then(|p| self.context.read_file(p).ok())
                .unwrap_or_default();
            let path = self.session_mode_file_path.clone().unwrap_or_default();
            self.exit(None).await?;
            self.pending_handoff_for_normal = Some(PendingPlanHandoff {
                content,
                path,
                selected_label: options.selected_label,
            });
        }
        _ => anyhow::bail!("Unknown handoff target: {}", target),
    }
    Ok(())
}
```

- [ ] Find and update every caller of `handoff_to`:

```bash
cd rust-ody && rg -n "handoff_to\(" crates/agent-rs/src
```

Current callers are in `crates/agent-rs/src/session_mode/manager.rs` (none outside, but verify). If `injection/manager.rs` or `turn_flow.rs` call it, update them to pass `HandoffOptions::default()` or the real label.

- [ ] Add a behavioral test in `rust-ody/crates/agent-rs/tests/session_mode_manager.rs` (create it if it does not exist). The test builds a `SessionModeManager` with a mock `SessionModeContext`, enters design mode, calls `handoff_to("plan", HandoffOptions { selected_label: Some("Approach A".into()) })`, and asserts `consume_pending_handoff_for_plan().selected_label == Some("Approach A".into())`. For plan→normal, assert `consume_pending_handoff_for_normal().selected_label`.

Example test skeleton:

```rust
#[tokio::test]
async fn design_handoff_preserves_selected_label() {
    let ctx = MockSessionModeContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);
    mgr.enter(SessionModeKind::Design, None, None).await.unwrap();
    mgr.handoff_to("plan", HandoffOptions {
        selected_label: Some("Approach A".into()),
    }).await.unwrap();
    let handoff = mgr.consume_pending_handoff_for_plan().expect("handoff");
    assert_eq!(handoff.selected_label, Some("Approach A".into()));
}
```

Run the test and verify it fails before the implementation change, then passes after.

- [ ] Run whole-workspace typecheck:

```bash
cd rust-ody && cargo check --workspace --all-targets
```

- [ ] Commit: `feat(agent-rs): propagate selected_label through session-mode handoffs`.

---

## Task 3: Define `SessionModeProvider` and satellite traits in `tools-rs`

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (add `pub mod session_mode;`)
- Modify: `rust-ody/crates/tools-rs/src/lib.rs` (re-export if needed)
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` (unit tests for mock impls)

**Why:** Session-mode tools must not depend on `agent-rs` directly (avoids circular dependency). We define minimal traits in `tools-rs` and let `agent-rs` implement them.

**Steps:**

- [ ] Create `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` with these traits and types:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;

/// Supported user languages, mirroring TS `SupportedLanguage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Zh,
}

impl Language {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().split('-').next()? {
            "zh" | "zh_cn" | "zh_tw" | "zh_hk" => Some(Language::Zh),
            _ => Some(Language::En),
        }
    }
}

/// Active session mode kind, mirroring TS `RuntimeMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionModeKind {
    Plan,
    Design,
    OfficeHours,
    GameDesign,
}

impl SessionModeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionModeKind::Plan => "plan",
            SessionModeKind::Design => "design",
            SessionModeKind::OfficeHours => "office-hours",
            SessionModeKind::GameDesign => "game-design",
        }
    }
}

/// Minimal filesystem / config surface needed by session-mode tools.
#[async_trait]
pub trait SessionModeContext: Send + Sync {
    fn cwd(&self) -> String;
    fn project_root(&self) -> Option<String>;
    async fn read_text(&self, path: &str) -> anyhow::Result<String>;
    async fn write_text(&self, path: &str, content: &str) -> anyhow::Result<()>;
    async fn stat(&self, path: &str) -> anyhow::Result<()>;
}

/// State-store entry shapes, mirroring TS `LearningEntry` / `BuilderProfileEntry`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningEntry {
    pub ts: String,
    pub skill: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub key: String,
    pub insight: String,
    pub confidence: f64,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderProfileEntry {
    pub date: String,
    pub mode: String,
    pub project_slug: String,
    pub signal_count: u64,
    pub signals: Vec<String>,
    pub design_doc: String,
    pub assignment: String,
    pub resources_shown: Vec<String>,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDesignProfileEntry {
    pub date: String,
    pub mode: String,
    pub project_slug: String,
    pub pillars: String,
    pub audience: String,
    pub platform: String,
    pub genre: String,
    pub signals: Vec<String>,
    pub design_doc: String,
}

#[async_trait]
pub trait OfficeHoursStateStore: Send + Sync {
    async fn append_profile(&self, entry: BuilderProfileEntry) -> anyhow::Result<()>;
    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()>;
    async fn search_learnings(
        &self,
        limit: usize,
        cross_project: bool,
    ) -> anyhow::Result<Vec<LearningEntry>>;
}

#[async_trait]
pub trait GameDesignStateStore: Send + Sync {
    async fn append_profile(&self, entry: GameDesignProfileEntry) -> anyhow::Result<()>;
    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()>;
    async fn search_learnings(
        &self,
        limit: usize,
        branch: Option<String>,
    ) -> anyhow::Result<Vec<LearningEntry>>;
}

/// Minimal telemetry surface.
pub trait TelemetryClient: Send + Sync {
    fn track(&self, event: &str, properties: HashMap<String, Value>);
}

/// Minimal MCP surface for artifact sync tools.
#[async_trait]
pub trait McpProvider: Send + Sync {
    async fn gbrain_available(&self) -> bool;
}

/// The main trait that session-mode tools consume.
#[async_trait]
pub trait SessionModeProvider: Send + Sync {
    fn is_session_mode_active(&self) -> bool;
    fn session_mode_kind(&self) -> Option<SessionModeKind>;
    fn session_mode_file_path(&self) -> Option<String>;
    async fn enter_session_mode(&self, kind: SessionModeKind) -> anyhow::Result<()>;
    async fn exit_session_mode(&self) -> anyhow::Result<()>;
    async fn handoff_to(&self, target: &str, selected_label: Option<String>) -> anyhow::Result<()>;
    fn user_language(&self) -> Language;
    fn set_user_language(&self, lang: Language);
    fn open_external_available(&self) -> bool;
    fn telemetry(&self) -> Arc<dyn TelemetryClient>;
    fn kaos(&self) -> Arc<dyn SessionModeContext>;
    fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore>;
    fn game_design_store(&self) -> Arc<dyn GameDesignStateStore>;
    fn mcp(&self) -> Arc<dyn McpProvider>;
}

// Re-export satellite modules that will be created in later tasks.
pub mod i18n;
pub mod planning;
pub mod office_hours;
pub mod game_design;
```

- [ ] Add `pub mod session_mode;` to `rust-ody/crates/tools-rs/src/builtin/mod.rs` after `pub mod read;`.

- [ ] Create stub files for the submodules so the module tree compiles:
  - `rust-ody/crates/tools-rs/src/builtin/session_mode/i18n.rs` (empty module with `pub fn t(...)`) — real content in Part 5, stub here.
  - `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs` (empty module)
  - `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs` (empty module)
  - `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs` (empty module)

  Each stub contains only `// populated in Part N` for now; this is a temporary scaffold allowed because the real tasks (Parts 2–4) immediately fill them.

- [ ] Add a unit test in `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` that exercises the mock trait implementations to prove the trait surface is object-safe:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct MockStore;
    #[async_trait::async_trait]
    impl OfficeHoursStateStore for MockStore {
        async fn append_profile(&self, _entry: BuilderProfileEntry) -> anyhow::Result<()> { Ok(()) }
        async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> { Ok(()) }
        async fn search_learnings(&self, _limit: usize, _cross_project: bool) -> anyhow::Result<Vec<LearningEntry>> { Ok(vec![]) }
    }

    #[test]
    fn office_hours_state_store_is_object_safe() {
        let _: Box<dyn OfficeHoursStateStore> = Box::new(MockStore);
    }
}
```

Run it:

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::tests
```

Expected: green.

- [ ] Run whole-workspace typecheck:

```bash
cd rust-ody && cargo check --workspace --all-targets
```

- [ ] Commit: `feat(tools-rs): define SessionModeProvider trait surface`.

---

## Task 4: Initialize `SessionModeManager` with the default behavior registry

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent.rs:406-412`
- Test: `rust-ody/crates/agent-rs/tests/session_mode_manager.rs` (verify enter plan/design/office-hours/game-design)

**Why:** `AgentBuilder::build` currently passes `HashMap::new()` as the behavior registry, so `SessionModeManager::enter` always fails with "No behavior registered for mode". The default registry from 4.3.7 already exists in `session_mode::behaviors::create_default_mode_behavior_registry`.

**Steps:**

- [ ] Add a failing test that builds an `Agent` and tries to enter plan mode:

```rust
#[tokio::test]
async fn agent_enters_plan_mode_with_default_registry() {
    let kaos = Arc::new(Kaos::new(detect_environment_from_node(), std::env::current_dir().unwrap()));
    let env = Arc::new(NoopEnv);
    let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
    agent.enter_session_mode(SessionModeKind::Plan, None).await.unwrap();
    assert!(agent.session_mode.lock().unwrap().is_active());
    assert_eq!(agent.session_mode.lock().unwrap().kind(), Some(SessionModeKind::Plan));
}
```

Run it before the fix:

```bash
cd rust-ody && cargo test -p agent-rs tests::agent_enters_plan_mode_with_default_registry
```

Expected failure: "No behavior registered for mode".

- [ ] Update `AgentBuilder::build` in `rust-ody/crates/agent-rs/src/agent.rs`:

Replace:

```rust
let session_mode =
    Mutex::new(SessionModeManager::new(ctx.clone(), HashMap::new()));
```

with:

```rust
let session_mode = Mutex::new(SessionModeManager::new(
    ctx.clone(),
    crate::session_mode::behaviors::create_default_mode_behavior_registry(),
));
```

- [ ] Re-run the test; it should now pass.

- [ ] Run whole-workspace typecheck:

```bash
cd rust-ody && cargo check --workspace --all-targets
```

- [ ] Commit: `fix(agent-rs): wire default session-mode behavior registry into Agent`.

---

## Local Self-Review

- [ ] 1. Spec-coverage table:
  - `ToolExecution.display` → Task 1 covered.
  - `selected_label` handoff propagation → Task 2 covered.
  - `SessionModeProvider` + satellite traits → Task 3 covered.
  - Default behavior registry → Task 4 covered.
- [ ] 2. Placeholder scan: no TODO/TBD; submodule stubs are explicitly temporary and filled in Parts 2–4.
- [ ] 3. No phantom tasks: every task produces code, tests, and a commit.
- [ ] 4. Dependency soundness: Task 1 → Task 2 → Task 3 → Task 4.
- [ ] 5. Caller & build soundness: Task 1 updates every `ToolExecution` construction site and ends with `cargo check --workspace --all-targets`; Task 2 updates every `handoff_to` caller.
- [ ] 6. Test-the-risk: Task 1 tests display forwarding; Task 2 tests label preservation; Task 4 tests plan-mode entry.
- [ ] 7. Type consistency: `ToolExecution.display` is `Option<Value>` in both `tools-rs` and forwarded to `RunnableToolExecution.display: Option<JsonValue>`; `HandoffOptions` is used consistently in `handoff_to`.
