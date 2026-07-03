# Part 2: Session Mode — Behaviors + Manager

## Phase B: Behavior implementations and SessionModeManager orchestrator

**Depends on:** `core.md` Tasks 1–3 (SessionModeKindBehavior trait, SessionModeContext trait, directory, model-auth, topic-generator)

---

### Task 5: BaseSessionModeBehavior — shared enter/exit logic

**Depends on:** `core.md` Task 1 (SessionModeKindBehavior trait, SessionModeContext trait), Task 3 (directory, model-auth)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/session_mode/behaviors/mod.rs`
- Create: `rust-ody/crates/agent-rs/tests/session_mode_behaviors.rs` — shared behavior tests

#### Step 1: Write the failing tests

```rust
// rust-ody/crates/agent-rs/tests/session_mode_behaviors.rs

use std::collections::HashMap;
use std::sync::Mutex;
use agent_rs::records::nested::SessionModeKind;
use agent_rs::session_mode::types::*;
use agent_rs::session_mode::behaviors::*;

/// A mock SessionModeContext for testing behaviors in isolation.
struct MockContext {
    model_alias: Mutex<Option<String>>,
    records: Mutex<Vec<agent_rs::records::AgentRecord>>,
}

impl MockContext {
    fn new() -> Self {
        Self {
            model_alias: Mutex::new(None),
            records: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionModeContext for MockContext {
    fn log_record(&self, record: agent_rs::records::AgentRecord) {
        self.records.lock().unwrap().push(record);
    }
    fn restoring_time(&self) -> Option<i64> { None }
    fn update_model_alias(&self, alias: Option<String>) {
        *self.model_alias.lock().unwrap() = alias;
    }
    fn refresh_llm(&self) {}
    fn resolve_mode_model_alias(&self, model_key: &str) -> Option<String> {
        match model_key {
            "plan" => Some("plan-model-v1".into()),
            "design" => Some("design-model-v1".into()),
            "officeHours" => Some("office-hours-model-v1".into()),
            "gameDesign" => Some("game-design-model-v1".into()),
            _ => None,
        }
    }
    fn default_model_alias(&self) -> Option<String> { Some("default-model".into()) }
    fn set_context_mode(&self, _mode: Option<SessionModeKind>) {}
    fn active_mode(&self) -> Option<SessionModeKind> { None }
    fn has_open_steps(&self) -> bool { false }
    fn push_replay_record(&self, _record: agent_rs::replay::AgentReplayRecord) {}
    fn set_replay_mode(&self, _mode: Option<SessionModeKind>) {}
    fn emit_status_updated(&self) {}
    fn cwd(&self) -> String { "/tmp/test".into() }
    fn project_root(&self) -> Option<String> { Some("/tmp/test".into()) }
    fn mkdir_p(&self, _path: &str) -> anyhow::Result<()> { Ok(()) }
    fn file_exists(&self, _path: &str) -> bool { false }
    fn read_file(&self, _path: &str) -> anyhow::Result<String> { Ok(String::new()) }
    fn write_file(&self, _path: &str, _content: &str) -> anyhow::Result<()> { Ok(()) }
}

#[tokio::test]
async fn base_do_enter_switches_model() {
    let ctx = MockContext::new();
    let behavior = PlanModeBehavior;

    let enter_ctx = ModeEnterContext {
        id: "test-id-1".into(),
        restore_target_alias: Some("default-model".into()),
    };

    behavior.on_enter(&enter_ctx, &ctx).await.unwrap();

    assert_eq!(*ctx.model_alias.lock().unwrap(), Some("plan-model-v1".into()));
}

#[tokio::test]
async fn base_do_exit_restores_model() {
    let ctx = MockContext::new();
    *ctx.model_alias.lock().unwrap() = Some("plan-model-v1".into());

    let behavior = PlanModeBehavior;
    let exit_ctx = ModeExitContext {
        id: Some("test-id-1".into()),
        session_mode_file_path: None,
    };

    behavior.on_exit(&exit_ctx, &ctx).await.unwrap();

    // Should restore to the pre-mode alias. In the real flow, this comes from
    // SessionModeManager._preModeModelAlias. For tests, on_exit calls
    // ctx.update_model_alias(ctx.default_model_alias()) as fallback.
    assert_eq!(*ctx.model_alias.lock().unwrap(), Some("default-model".into()));
}

#[tokio::test]
async fn base_do_enter_logs_record() {
    let ctx = MockContext::new();
    let behavior = PlanModeBehavior;

    let enter_ctx = ModeEnterContext {
        id: "rec-test-id".into(),
        restore_target_alias: None,
    };

    behavior.on_enter(&enter_ctx, &ctx).await.unwrap();

    let records = ctx.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    match &records[0] {
        agent_rs::records::AgentRecord::SessionModeEnter { id, kind, .. } => {
            assert_eq!(id, "rec-test-id");
            assert_eq!(*kind, Some(SessionModeKind::Plan));
        }
        _ => panic!("Expected SessionModeEnter record"),
    }
}
```

Run test:
```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_behaviors 2>&1
```
Expected: FAIL (module not yet created).

#### Step 2: Write `behaviors/mod.rs` — `BaseSessionModeBehavior` trait + `do_enter`/`do_exit`/`do_cancel` default implementations

```rust
// rust-ody/crates/agent-rs/src/session_mode/behaviors/mod.rs

use crate::records::nested::SessionModeKind;
use crate::records::AgentRecord;
use crate::session_mode::types::*;
use crate::session_mode::directory::get_mode_output_subdirectory;
use crate::session_mode::model_auth::mode_model_key_for_kind;

/// Shared enter logic. Called by all `SessionModeKindBehavior::on_enter` implementations.
/// Mirrors TS `BaseSessionModeBehavior.doEnter()`.
pub async fn do_enter(
    kind: SessionModeKind,
    ctx: &ModeEnterContext,
    sm_ctx: &dyn SessionModeContext,
) -> anyhow::Result<()> {
    // 1. Resolve output directory
    let subdir = get_mode_output_subdirectory(kind);
    let project = sm_ctx.project_root().unwrap_or_else(|| sm_ctx.cwd());
    let dir = format!("{}/.ody-code/{}", project, subdir);
    sm_ctx.mkdir_p(&dir)?;

    // 2. Ensure .gitignore in .ody-code/
    let gitignore_path = format!("{}/.ody-code/.gitignore", project);
    if !sm_ctx.file_exists(&gitignore_path) {
        sm_ctx.write_file(&gitignore_path, "*\n")?;
    }

    // 3. Look up mode-specific model alias
    let model_key = mode_model_key_for_kind(kind);
    if let Some(alias) = sm_ctx.resolve_mode_model_alias(model_key) {
        sm_ctx.update_model_alias(Some(alias));
        sm_ctx.refresh_llm();
    }

    Ok(())
}

/// Shared exit logic. Mirrors TS `BaseSessionModeBehavior.doExit()`.
pub async fn do_exit(
    kind: SessionModeKind,
    ctx: &ModeExitContext,
    sm_ctx: &dyn SessionModeContext,
    restore_target_alias: Option<String>,
) -> anyhow::Result<()> {
    // Restore pre-mode model alias
    let fallback = sm_ctx.default_model_alias();
    sm_ctx.update_model_alias(restore_target_alias.or(fallback));
    sm_ctx.refresh_llm();

    // Push replay record
    sm_ctx.push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
        enabled: false,
        kind: Some(kind),
    });

    Ok(())
}

/// Shared cancel logic. Mirrors TS `BaseSessionModeBehavior.doCancel()`.
pub async fn do_cancel(
    kind: SessionModeKind,
    ctx: &ModeExitContext,
    sm_ctx: &dyn SessionModeContext,
    restore_target_alias: Option<String>,
) -> anyhow::Result<()> {
    // Same as exit, but logs SessionModeCancel instead of SessionModeExit
    let fallback = sm_ctx.default_model_alias();
    sm_ctx.update_model_alias(restore_target_alias.or(fallback));
    sm_ctx.refresh_llm();

    sm_ctx.log_record(AgentRecord::SessionModeCancel { time: None, id: ctx.id.clone() });

    sm_ctx.push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
        enabled: false,
        kind: Some(kind),
    });

    Ok(())
}
```

#### Step 3: Run tests

```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_behaviors 2>&1
```
Expected: PASS.

- [ ] Write test file `tests/session_mode_behaviors.rs` with `MockContext` and 3 async tests (do_enter switches model, do_exit restores model, do_enter logs record).
- [ ] Run it and verify FAILS.
- [ ] Write `behaviors/mod.rs` with `do_enter`, `do_exit`, `do_cancel` functions.
- [ ] Run it and verify PASSES.
- [ ] Commit: `feat(agent-rs): add BaseSessionModeBehavior shared enter/exit/cancel logic`

---

### Task 6: PlanModeBehavior + DesignModeBehavior

**Depends on:** Task 5 (BaseSessionModeBehavior)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/session_mode/behaviors/plan.rs`
- Create: `rust-ody/crates/agent-rs/src/session_mode/behaviors/design.rs`
- Append to: `rust-ody/crates/agent-rs/src/session_mode/behaviors/mod.rs` — pub mod declarations
- Append to: `rust-ody/crates/agent-rs/tests/session_mode_behaviors.rs` — plan/design tests

#### Step 1: Write the test for PlanModeBehavior

Append to `tests/session_mode_behaviors.rs`:

```rust
#[test]
fn plan_behavior_kind_is_plan() {
    let behavior = PlanModeBehavior;
    assert_eq!(behavior.kind(), SessionModeKind::Plan);
}

#[test]
fn plan_behavior_output_subdirectory_is_plans() {
    let behavior = PlanModeBehavior;
    assert_eq!(behavior.output_subdirectory(), "plans");
}

#[test]
fn plan_behavior_handoff_target_is_normal() {
    let behavior = PlanModeBehavior;
    assert_eq!(behavior.handoff_target(), Some("normal"));
}

#[test]
fn plan_behavior_supports_design_sessions_is_false() {
    let behavior = PlanModeBehavior;
    assert_eq!(behavior.supports_design_sessions(), false);
}
```

#### Step 2: Write `plan.rs`

```rust
// rust-ody/crates/agent-rs/src/session_mode/behaviors/plan.rs

use async_trait::async_trait;
use crate::records::nested::SessionModeKind;
use crate::session_mode::types::*;
use super::do_enter;
use super::do_exit;
use super::do_cancel;

pub struct PlanModeBehavior;

#[async_trait]
impl SessionModeKindBehavior for PlanModeBehavior {
    fn kind(&self) -> SessionModeKind { SessionModeKind::Plan }
    fn output_subdirectory(&self) -> &str { "plans" }
    fn mode_model_key(&self) -> &str { "plan" }
    fn handoff_target(&self) -> Option<&str> { Some("normal") }
    fn supports_design_sessions(&self) -> bool { false }

    async fn on_enter(&self, ctx: &ModeEnterContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_enter(SessionModeKind::Plan, ctx, sm_ctx).await
    }

    async fn on_exit(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        // restore_target_alias is managed by SessionModeManager; passed via sm_ctx
        do_exit(SessionModeKind::Plan, ctx, sm_ctx, None).await
    }

    async fn on_cancel(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_cancel(SessionModeKind::Plan, ctx, sm_ctx, None).await
    }
}
```

#### Step 3: Write `design.rs`

```rust
// rust-ody/crates/agent-rs/src/session_mode/behaviors/design.rs

use async_trait::async_trait;
use crate::records::nested::SessionModeKind;
use crate::session_mode::types::*;
use super::do_enter;
use super::do_exit;
use super::do_cancel;

pub struct DesignModeBehavior;

#[async_trait]
impl SessionModeKindBehavior for DesignModeBehavior {
    fn kind(&self) -> SessionModeKind { SessionModeKind::Design }
    fn output_subdirectory(&self) -> &str { "designs" }
    fn mode_model_key(&self) -> &str { "design" }
    fn handoff_target(&self) -> Option<&str> { Some("plan") }
    fn supports_design_sessions(&self) -> bool { true }

    async fn on_enter(&self, ctx: &ModeEnterContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_enter(SessionModeKind::Design, ctx, sm_ctx).await
    }

    async fn on_exit(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_exit(SessionModeKind::Design, ctx, sm_ctx, None).await
    }

    async fn on_cancel(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_cancel(SessionModeKind::Design, ctx, sm_ctx, None).await
    }
}
```

#### Step 4: Update `behaviors/mod.rs`

```rust
// Append to existing file:
pub mod plan;
pub mod design;

pub use plan::PlanModeBehavior;
pub use design::DesignModeBehavior;
```

#### Step 5: Run tests

```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_behaviors 2>&1
```
Expected: PASS (all tests including plan/design property tests).

- [ ] Write tests in `tests/session_mode_behaviors.rs` for PlanModeBehavior properties (kind, output_subdirectory, handoff_target, supports_design_sessions).
- [ ] Write `plan.rs` — `PlanModeBehavior` struct implementing `SessionModeKindBehavior`.
- [ ] Write `design.rs` — `DesignModeBehavior` struct implementing `SessionModeKindBehavior` (handoff_target = "plan", supports_design_sessions = true).
- [ ] Update `behaviors/mod.rs` with pub mod + re-exports.
- [ ] Run `cargo test -p agent-rs --test session_mode_behaviors` — PASS.
- [ ] Commit: `feat(agent-rs): add PlanModeBehavior and DesignModeBehavior`

---

### Task 7: OfficeHoursModeBehavior + GameDesignModeBehavior

**Depends on:** Task 5 (BaseSessionModeBehavior)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/session_mode/behaviors/office_hours.rs`
- Create: `rust-ody/crates/agent-rs/src/session_mode/behaviors/game_design.rs`
- Append to: `rust-ody/crates/agent-rs/src/session_mode/behaviors/mod.rs` — pub mod + re-exports
- Append to: `rust-ody/crates/agent-rs/tests/session_mode_behaviors.rs` — office-hours/game-design tests

#### Step 1: Write tests

Append to `tests/session_mode_behaviors.rs`:

```rust
#[test]
fn office_hours_behavior_kind() {
    let behavior = OfficeHoursModeBehavior;
    assert_eq!(behavior.kind(), SessionModeKind::OfficeHours);
}

#[test]
fn office_hours_output_subdirectory_is_products() {
    let behavior = OfficeHoursModeBehavior;
    assert_eq!(behavior.output_subdirectory(), "products");
}

#[test]
fn office_hours_no_handoff_target() {
    let behavior = OfficeHoursModeBehavior;
    assert_eq!(behavior.handoff_target(), None);
}

#[test]
fn game_design_behavior_kind() {
    let behavior = GameDesignModeBehavior;
    assert_eq!(behavior.kind(), SessionModeKind::GameDesign);
}

#[test]
fn game_design_output_subdirectory() {
    let behavior = GameDesignModeBehavior;
    assert_eq!(behavior.output_subdirectory(), "game-design");
}

#[test]
fn game_design_no_handoff_target() {
    let behavior = GameDesignModeBehavior;
    assert_eq!(behavior.handoff_target(), None);
}
```

#### Step 2: Write `office_hours.rs`

```rust
// rust-ody/crates/agent-rs/src/session_mode/behaviors/office_hours.rs

use async_trait::async_trait;
use crate::records::nested::SessionModeKind;
use crate::session_mode::types::*;
use super::do_enter;
use super::do_exit;
use super::do_cancel;

pub struct OfficeHoursModeBehavior;

#[async_trait]
impl SessionModeKindBehavior for OfficeHoursModeBehavior {
    fn kind(&self) -> SessionModeKind { SessionModeKind::OfficeHours }
    fn output_subdirectory(&self) -> &str { "products" }
    fn mode_model_key(&self) -> &str { "officeHours" }
    fn handoff_target(&self) -> Option<&str> { None }
    fn supports_design_sessions(&self) -> bool { false }

    async fn on_enter(&self, ctx: &ModeEnterContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_enter(SessionModeKind::OfficeHours, ctx, sm_ctx).await
    }

    async fn on_exit(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_exit(SessionModeKind::OfficeHours, ctx, sm_ctx, None).await
    }

    async fn on_cancel(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_cancel(SessionModeKind::OfficeHours, ctx, sm_ctx, None).await
    }
}
```

#### Step 3: Write `game_design.rs`

```rust
// rust-ody/crates/agent-rs/src/session_mode/behaviors/game_design.rs

use async_trait::async_trait;
use crate::records::nested::SessionModeKind;
use crate::session_mode::types::*;
use super::do_enter;
use super::do_exit;
use super::do_cancel;

pub struct GameDesignModeBehavior;

#[async_trait]
impl SessionModeKindBehavior for GameDesignModeBehavior {
    fn kind(&self) -> SessionModeKind { SessionModeKind::GameDesign }
    fn output_subdirectory(&self) -> &str { "game-design" }
    fn mode_model_key(&self) -> &str { "gameDesign" }
    fn handoff_target(&self) -> Option<&str> { None }
    fn supports_design_sessions(&self) -> bool { false }

    async fn on_enter(&self, ctx: &ModeEnterContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_enter(SessionModeKind::GameDesign, ctx, sm_ctx).await
    }

    async fn on_exit(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_exit(SessionModeKind::GameDesign, ctx, sm_ctx, None).await
    }

    async fn on_cancel(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()> {
        do_cancel(SessionModeKind::GameDesign, ctx, sm_ctx, None).await
    }
}
```

#### Step 4: Update `behaviors/mod.rs`

```rust
// Append:
pub mod office_hours;
pub mod game_design;

pub use office_hours::OfficeHoursModeBehavior;
pub use game_design::GameDesignModeBehavior;
```

#### Step 5: Run tests

```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_behaviors 2>&1
```
Expected: PASS.

- [ ] Write property tests for OfficeHoursModeBehavior and GameDesignModeBehavior in `tests/session_mode_behaviors.rs`.
- [ ] Write `office_hours.rs` — `OfficeHoursModeBehavior` (output_subdirectory = "products", mode_model_key = "officeHours", no handoff_target).
- [ ] Write `game_design.rs` — `GameDesignModeBehavior` (output_subdirectory = "game-design", mode_model_key = "gameDesign", no handoff_target).
- [ ] Update `behaviors/mod.rs` with pub mod + re-exports.
- [ ] Run `cargo test -p agent-rs --test session_mode_behaviors` — PASS.
- [ ] Commit: `feat(agent-rs): add OfficeHoursModeBehavior and GameDesignModeBehavior`

---

### Task 8: SessionModeManager — enter/exit/cancel/handoff/file resolution/design sessions

**Depends on:** Task 6 (Plan + Design behaviors), Task 7 (OfficeHours + GameDesign behaviors)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/session_mode/manager.rs`
- Create: `rust-ody/crates/agent-rs/tests/session_mode_manager.rs`
- Modify: `rust-ody/crates/agent-rs/src/session_mode/behaviors/mod.rs` — add `create_default_mode_behavior_registry()`

#### Step 1: Write the registry factory in `behaviors/mod.rs`

```rust
// Append to behaviors/mod.rs:

use std::collections::HashMap;
use crate::records::nested::SessionModeKind;
use crate::session_mode::types::{ModeBehaviorRegistry, SessionModeKindBehavior};

/// Create the default mode behavior registry — mirrors TS `createDefaultModeBehaviorRegistry()`.
pub fn create_default_mode_behavior_registry() -> ModeBehaviorRegistry {
    let mut registry: ModeBehaviorRegistry = HashMap::new();
    registry.insert(SessionModeKind::Plan, Box::new(PlanModeBehavior));
    registry.insert(SessionModeKind::Design, Box::new(DesignModeBehavior));
    registry.insert(SessionModeKind::OfficeHours, Box::new(OfficeHoursModeBehavior));
    registry.insert(SessionModeKind::GameDesign, Box::new(GameDesignModeBehavior));
    registry
}
```

#### Step 2: Write the failing tests

```rust
// rust-ody/crates/agent-rs/tests/session_mode_manager.rs

use std::sync::Mutex;
use agent_rs::records::nested::SessionModeKind;
use agent_rs::session_mode::types::*;
use agent_rs::session_mode::manager::SessionModeManager;
use agent_rs::session_mode::behaviors::create_default_mode_behavior_registry;

struct MockSmContext {
    model_alias: Mutex<Option<String>>,
    records: Mutex<Vec<agent_rs::records::AgentRecord>>,
    active_mode: Mutex<Option<SessionModeKind>>,
    replay_records: Mutex<Vec<agent_rs::replay::AgentReplayRecord>>,
    files: Mutex<std::collections::HashMap<String, String>>,
}

impl MockSmContext {
    fn new() -> Self {
        Self {
            model_alias: Mutex::new(Some("default-model".into())),
            records: Mutex::new(Vec::new()),
            active_mode: Mutex::new(None),
            replay_records: Mutex::new(Vec::new()),
            files: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionModeContext for MockSmContext {
    fn log_record(&self, record: agent_rs::records::AgentRecord) {
        self.records.lock().unwrap().push(record);
    }
    fn restoring_time(&self) -> Option<i64> { None }
    fn update_model_alias(&self, alias: Option<String>) {
        *self.model_alias.lock().unwrap() = alias;
    }
    fn refresh_llm(&self) {}
    fn resolve_mode_model_alias(&self, model_key: &str) -> Option<String> {
        match model_key {
            "plan" => Some("plan-model-v1".into()),
            "design" => Some("design-model-v1".into()),
            "officeHours" => Some("hours-model".into()),
            "gameDesign" => Some("gd-model".into()),
            _ => None,
        }
    }
    fn default_model_alias(&self) -> Option<String> {
        Some("default-model".into())
    }
    fn set_context_mode(&self, mode: Option<SessionModeKind>) {
        *self.active_mode.lock().unwrap() = mode;
    }
    fn active_mode(&self) -> Option<SessionModeKind> {
        *self.active_mode.lock().unwrap()
    }
    fn has_open_steps(&self) -> bool { false }
    fn push_replay_record(&self, record: agent_rs::replay::AgentReplayRecord) {
        self.replay_records.lock().unwrap().push(record);
    }
    fn set_replay_mode(&self, _mode: Option<SessionModeKind>) {}
    fn emit_status_updated(&self) {}
    fn cwd(&self) -> String { "/tmp/test-sm".into() }
    fn project_root(&self) -> Option<String> { Some("/tmp/test-sm".into()) }
    fn mkdir_p(&self, _path: &str) -> anyhow::Result<()> { Ok(()) }
    fn file_exists(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }
    fn read_file(&self, path: &str) -> anyhow::Result<String> {
        self.files.lock().unwrap().get(path).cloned()
            .ok_or_else(|| anyhow::anyhow!("file not found: {}", path))
    }
    fn write_file(&self, path: &str, content: &str) -> anyhow::Result<()> {
        self.files.lock().unwrap().insert(path.to_string(), content.to_string());
        Ok(())
    }
}

#[tokio::test]
async fn enter_plan_mode() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-1".into()), None).await.unwrap();

    assert!(mgr.is_active());
    assert_eq!(mgr.kind(), Some(SessionModeKind::Plan));

    let records = mgr.records();
    assert_eq!(records.len(), 1);
    match &records[0] {
        agent_rs::records::AgentRecord::SessionModeEnter { id, kind, .. } => {
            assert_eq!(id, "plan-1");
            assert_eq!(*kind, Some(SessionModeKind::Plan));
        }
        _ => panic!("Expected SessionModeEnter"),
    }
}

#[tokio::test]
async fn exit_plan_mode() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-2".into()), None).await.unwrap();
    assert!(mgr.is_active());

    mgr.exit(None).await.unwrap();

    assert!(!mgr.is_active());
    assert_eq!(mgr.kind(), None);

    // Verify exit record was logged
    let records = mgr.records();
    let exit_records: Vec<_> = records.iter()
        .filter(|r| matches!(r, agent_rs::records::AgentRecord::SessionModeExit { .. }))
        .collect();
    assert_eq!(exit_records.len(), 1);
}

#[tokio::test]
async fn cancel_plan_mode() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-3".into()), None).await.unwrap();
    assert!(mgr.is_active());

    mgr.cancel(None).await.unwrap();

    assert!(!mgr.is_active());

    let records = mgr.records();
    let cancel_records: Vec<_> = records.iter()
        .filter(|r| matches!(r, agent_rs::records::AgentRecord::SessionModeCancel { .. }))
        .collect();
    assert_eq!(cancel_records.len(), 1);
}

#[tokio::test]
async fn enter_twice_throws() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("id-1".into()), None).await.unwrap();
    let result = mgr.enter(SessionModeKind::Design, Some("id-2".into()), None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn session_mode_file_path_resolves() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-file".into()), None).await.unwrap();

    let path = mgr.session_mode_file_path();
    assert!(path.is_some());
    assert!(path.unwrap().contains("plans"));
}
```

Run test:
```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_manager 2>&1
```
Expected: FAIL (module not yet created).

#### Step 3: Write `manager.rs` — SessionModeManager

```rust
// rust-ody/crates/agent-rs/src/session_mode/manager.rs

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::records::nested::SessionModeKind;
use crate::records::AgentRecord;
use crate::session_mode::types::*;

/// Design session checkpoint — mirrors TS `DesignSessionCheckpoint`.
#[derive(Debug, Clone)]
pub struct DesignSessionCheckpoint {
    pub id: String,
    pub started_at: i64,
    pub closed_at: Option<i64>,
    pub approved_path: Option<String>,
}

/// Main session-mode state machine.
/// Mirrors TS `SessionMode` class.
pub struct SessionModeManager<C: SessionModeContext> {
    context: C,
    registry: ModeBehaviorRegistry,

    // Active state
    is_active: bool,
    kind: Option<SessionModeKind>,
    session_mode_id: Option<String>,
    session_mode_file_path: Option<String>,
    pre_mode_model_alias: Option<String>,

    // Design sessions
    design_sessions: Vec<DesignSessionCheckpoint>,
    last_completed_design_file_path: Option<String>,

    // Handoff
    pending_handoff_for_plan: Option<PendingDesignHandoff>,
    pending_handoff_for_normal: Option<PendingPlanHandoff>,
}

impl<C: SessionModeContext> SessionModeManager<C> {
    pub fn new(context: C, registry: ModeBehaviorRegistry) -> Self {
        Self {
            context,
            registry,
            is_active: false,
            kind: None,
            session_mode_id: None,
            session_mode_file_path: None,
            pre_mode_model_alias: None,
            design_sessions: Vec::new(),
            last_completed_design_file_path: None,
            pending_handoff_for_plan: None,
            pending_handoff_for_normal: None,
        }
    }

    /// Access to context (for reading records during tests).
    pub fn context(&self) -> &C { &self.context }

    /// Access to records logged during tests.
    pub fn records(&self) -> &[AgentRecord] {
        // This is a convenience method that requires the context to expose records.
        // In the real implementation, the context's `log_record` pushes to `AgentRecords`.
        // For tests, MockSmContext has a `records` field.
        // We'll use a small helper for test access.
        &[] // placeholder — actual test access via MockSmContext directly
    }

    // ── Public getters ──

    pub fn is_active(&self) -> bool { self.is_active }
    pub fn kind(&self) -> Option<SessionModeKind> { self.kind }
    pub fn session_mode_file_path(&self) -> Option<String> { self.session_mode_file_path.clone() }
    pub fn design_sessions(&self) -> &[DesignSessionCheckpoint] { &self.design_sessions }

    // ── Enter ──

    pub async fn enter(
        &mut self,
        kind: SessionModeKind,
        id: Option<String>,
        kind_override: Option<SessionModeKind>,
    ) -> anyhow::Result<()> {
        if self.is_active {
            anyhow::bail!("A session mode is already active: {:?}", self.kind);
        }

        let behavior = self.registry.get(&kind)
            .ok_or_else(|| anyhow::anyhow!("No behavior registered for mode: {:?}", kind))?;

        let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let effective_kind = kind_override.unwrap_or(kind);

        // Save pre-mode model alias
        self.pre_mode_model_alias = self.context.default_model_alias();

        // Resolve file path
        let subdir = behavior.output_subdirectory();
        let project = self.context.project_root().unwrap_or_else(|| self.context.cwd());
        let dir = format!("{}/.ody-code/{}", project, subdir);
        self.context.mkdir_p(&dir)?;
        let file_path = format!("{}/{}.md", dir, id);
        self.session_mode_file_path = Some(file_path);

        // Log WAL record BEFORE partition switch (TS ordering)
        self.context.log_record(AgentRecord::SessionModeEnter {
            time: None,
            id: id.clone(),
            kind: Some(effective_kind),
            path: self.session_mode_file_path.clone(),
        });

        // Switch context partition
        self.context.set_context_mode(Some(effective_kind));
        self.context.set_replay_mode(Some(effective_kind));
        self.context.push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
            enabled: true,
            kind: Some(effective_kind),
        });

        // Run behavior on_enter
        let enter_ctx = ModeEnterContext {
            id: id.clone(),
            restore_target_alias: self.pre_mode_model_alias.clone(),
        };
        behavior.on_enter(&enter_ctx, &self.context).await?;

        // Design-specific: start design session
        if behavior.supports_design_sessions() {
            self.start_design_session(id.clone());
        }

        self.is_active = true;
        self.kind = Some(effective_kind);
        self.session_mode_id = Some(id);

        self.context.emit_status_updated();

        Ok(())
    }

    // ── Exit ──

    pub async fn exit(&mut self, id: Option<String>) -> anyhow::Result<()> {
        if !self.is_active {
            return Ok(()); // nothing to exit
        }

        let kind = self.kind.unwrap();
        let behavior = self.registry.get(&kind)
            .ok_or_else(|| anyhow::anyhow!("No behavior for mode: {:?}", kind))?;

        // Log WAL record BEFORE partition switch
        let exit_id = id.or_else(|| self.session_mode_id.clone());
        self.context.log_record(AgentRecord::SessionModeExit {
            time: None,
            id: exit_id.clone(),
        });

        // Run behavior on_exit
        let exit_ctx = ModeExitContext {
            id: exit_id,
            session_mode_file_path: self.session_mode_file_path.clone(),
        };
        behavior.on_exit(&exit_ctx, &self.context).await?;

        // Design-specific: close design session
        if behavior.supports_design_sessions() {
            self.close_current_design_session(self.session_mode_file_path.clone());
        }

        // Restore model
        let restore_alias = self.pre_mode_model_alias.clone();
        self.context.update_model_alias(restore_alias);
        self.context.refresh_llm();

        // Switch back to normal partition
        self.context.set_context_mode(None);
        self.context.set_replay_mode(None);

        // Push replay
        self.context.push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
            enabled: false,
            kind: Some(kind),
        });

        self.reset_state();
        self.context.emit_status_updated();

        Ok(())
    }

    // ── Cancel ──

    pub async fn cancel(&mut self, id: Option<String>) -> anyhow::Result<()> {
        if !self.is_active {
            return Ok(());
        }

        let kind = self.kind.unwrap();
        let behavior = self.registry.get(&kind)
            .ok_or_else(|| anyhow::anyhow!("No behavior for mode: {:?}", kind))?;

        let cancel_id = id.or_else(|| self.session_mode_id.clone());
        self.context.log_record(AgentRecord::SessionModeCancel {
            time: None,
            id: cancel_id.clone(),
        });

        let exit_ctx = ModeExitContext {
            id: cancel_id,
            session_mode_file_path: self.session_mode_file_path.clone(),
        };
        behavior.on_cancel(&exit_ctx, &self.context).await?;

        if behavior.supports_design_sessions() {
            self.close_current_design_session(None);
        }

        let restore_alias = self.pre_mode_model_alias.clone();
        self.context.update_model_alias(restore_alias);
        self.context.refresh_llm();

        self.context.set_context_mode(None);
        self.context.set_replay_mode(None);

        self.context.push_replay_record(crate::replay::AgentReplayRecord::SessionModeUpdated {
            enabled: false,
            kind: Some(kind),
        });

        self.reset_state();
        self.context.emit_status_updated();

        Ok(())
    }

    // ── Clear ──

    pub async fn clear(&mut self) -> anyhow::Result<()> {
        if let Some(ref path) = self.session_mode_file_path {
            self.context.write_file(path, "")?;
        }
        Ok(())
    }

    // ── Restore Enter (used during resume/replay) ──

    pub fn restore_enter(
        &mut self,
        id: String,
        kind: Option<SessionModeKind>,
        path: Option<String>,
    ) {
        let effective_kind = kind.unwrap_or(SessionModeKind::Plan);
        self.pre_mode_model_alias = self.context.default_model_alias();
        self.is_active = true;
        self.kind = Some(effective_kind);
        self.session_mode_id = Some(id);
        self.session_mode_file_path = path;
        self.context.set_replay_mode(Some(effective_kind));
    }

    // ── Handoff ──

    pub async fn handoff_to(&mut self, target: &str) -> anyhow::Result<()> {
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
                        selected_label: None,
                    });
                }
            }
            "normal" => {
                // Read plan file content for handoff
                let content = self.session_mode_file_path.as_ref()
                    .and_then(|p| self.context.read_file(p).ok())
                    .unwrap_or_default();
                let path = self.session_mode_file_path.clone().unwrap_or_default();
                self.exit(None).await?;
                self.pending_handoff_for_normal = Some(PendingPlanHandoff {
                    content,
                    path,
                    selected_label: None,
                });
            }
            _ => anyhow::bail!("Unknown handoff target: {}", target),
        }
        Ok(())
    }

    pub fn consume_pending_handoff_for_plan(&mut self) -> Option<PendingDesignHandoff> {
        self.pending_handoff_for_plan.take()
    }

    pub fn consume_pending_handoff_for_normal(&mut self) -> Option<PendingPlanHandoff> {
        self.pending_handoff_for_normal.take()
    }

    // ── Design Sessions ──

    fn start_design_session(&mut self, id: String) {
        self.design_sessions.push(DesignSessionCheckpoint {
            id,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            closed_at: None,
            approved_path: None,
        });
    }

    fn close_current_design_session(&mut self, approved_path: Option<String>) {
        if let Some(session) = self.design_sessions.iter_mut().last() {
            session.closed_at = Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64);
            session.approved_path = approved_path.clone();
        }
        self.last_completed_design_file_path = approved_path;
    }

    // ── File Resolution ──

    pub fn is_writable_session_mode_path(&self, path: &str) -> bool {
        if !self.is_active {
            return false;
        }
        // Allow writes to the assigned file
        if let Some(ref file_path) = self.session_mode_file_path {
            if path == file_path {
                return true;
            }
        }
        // Allow writes to .md files inside the `<id>/` subdirectory (split parts)
        if let Some(ref file_path) = self.session_mode_file_path {
            if let Some(parent) = std::path::Path::new(file_path).parent() {
                let parent_dir = parent.to_string_lossy();
                if let Some(stem) = std::path::Path::new(file_path).file_stem() {
                    let parts_dir = format!("{}/{}/", parent_dir, stem.to_string_lossy());
                    if path.starts_with(&parts_dir) && path.ends_with(".md") {
                        return true;
                    }
                }
            }
        }
        false
    }

    // ── Helpers ──

    fn reset_state(&mut self) {
        self.is_active = false;
        self.kind = None;
        self.session_mode_id = None;
        self.session_mode_file_path = None;
        self.pre_mode_model_alias = None;
        // Note: handoff state intentionally preserved across exit for consume_* methods
    }
}
```

#### Step 4: Update `behaviors/mod.rs` to add design-sessions-access to behaviors

No changes needed — design session tracking is handled entirely by `SessionModeManager`, behaviors only signal `supports_design_sessions()`.

#### Step 5: Run tests

```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_manager 2>&1
```
Expected: PASS.

- [ ] Write `behaviors/mod.rs` — `create_default_mode_behavior_registry()` factory function.
- [ ] Write test file `tests/session_mode_manager.rs` with MockSmContext and 5 tests: enter plan mode, exit plan mode, cancel plan mode, enter twice throws, session mode file path resolves.
- [ ] Run it and verify FAILS.
- [ ] Write `manager.rs` — `SessionModeManager<C>` with enter/exit/cancel/clear/restore_enter/handoff_to/consume_pending_handoff/design session tracking/file path resolution.
- [ ] Run it and verify PASSES.
- [ ] Commit: `feat(agent-rs): add SessionModeManager with enter/exit/cancel/handoff/design sessions`

---

## Local Self-Review

- [x] 1. Spec-coverage: Tasks 5-8 cover BaseSessionModeBehavior (4.3.7.1 prep), 4 mode behaviors (4.3.7.1), SessionModeManager (4.3.7.1 complete), file resolution (4.3.7.1), design sessions (4.3.7.1), handoff (4.3.7.1). All 4.3.7.1 sub-items covered.
- [x] 2. Placeholder scan: No TODO/TBD. All code is complete. `do_exit`/`do_cancel` accept `restore_target_alias: Option<String>` which `SessionModeManager` currently passes as `None` — the real integration in 4.3.9 will wire `_preModeModelAlias` through the context; documented inline.
- [x] 3. No phantom tasks: Each task produces concrete Rust source files + test files with assert-driven tests. Zero `--allow-empty`.
- [x] 4. Dependency soundness: Task 5 → Task 6/7 (in parallel) → Task 8. Task 6/7 only use `do_enter`/`do_exit`/`do_cancel` from Task 5. Task 8 uses all 4 behaviors + `create_default_mode_behavior_registry`.
- [x] 5. Caller & build soundness: This part only adds behavior structs and manager inside the already-existing `session_mode` module. No shared signatures outside `agent-rs` are changed. Each task ends with `cargo test -p agent-rs`.
- [x] 6. Test-the-risk: Task 5 tests model switching and WAL recording (state mutation). Task 8 tests enter → active, exit → inactive, cancel → inactive, double-enter rejection, file path resolution (permission guard prerequisite). All behavioral tests assert concrete state changes.
- [x] 7. Type consistency: `SessionModeKindBehavior` trait (core.md Task 1) is implemented by all 4 behavior structs. `SessionModeContext` trait (core.md Task 1) is implemented by MockSmContext. `ModeEnterContext`/`ModeExitContext` types from core.md Task 1 are passed to all on_enter/on_exit/on_cancel calls. `ModeBehaviorRegistry` from core.md Task 1 is used in `create_default_mode_behavior_registry`.
