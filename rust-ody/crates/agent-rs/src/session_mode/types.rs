use crate::records::nested::SessionModeKind;
use async_trait::async_trait;
use std::collections::HashMap;

/// Context passed to `on_enter` — mirrors TS `ModeEnterContext`.
#[derive(Debug, Clone)]
pub struct ModeEnterContext {
    pub id: String,
    pub restore_target_alias: Option<String>,
}

/// Context passed to `on_exit` / `on_cancel` — mirrors TS `ModeExitContext`.
#[derive(Debug, Clone)]
pub struct ModeExitContext {
    pub id: Option<String>,
    pub session_mode_file_path: Option<String>,
}

/// Options passed to `handoff_to` — mirrors TS `HandoffOptions`.
#[derive(Debug, Clone, Default)]
pub struct HandoffOptions {
    pub selected_label: Option<String>,
}

/// Trait for one session-mode kind behavior.
/// Mirrors TS `SessionModeBehavior<TKind>`.
#[async_trait]
pub trait SessionModeKindBehavior: Send + Sync {
    /// Which session mode kind this behavior handles.
    fn kind(&self) -> SessionModeKind;

    /// Subdirectory under `.ody-code/` for this mode's output files.
    fn output_subdirectory(&self) -> &str;

    /// Config key for the mode-specific model alias (e.g. `"plan"`, `"design"`).
    fn mode_model_key(&self) -> &str;

    /// Optional handoff target: `Some("plan")` for design→plan, `Some("normal")` for plan→normal.
    fn handoff_target(&self) -> Option<&str>;

    /// Whether this mode supports design session checkpoints.
    fn supports_design_sessions(&self) -> bool;

    /// Called when this mode is entered.
    async fn on_enter(
        &self,
        ctx: &ModeEnterContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()>;

    /// Called when this mode is exited normally.
    async fn on_exit(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()>;

    /// Called when this mode is cancelled.
    async fn on_cancel(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()>;
}

/// Registry mapping each `SessionModeKind` to its behavior + injector factory.
/// Mirrors TS `ModeBehaviorRegistry`.
pub type ModeBehaviorRegistry = HashMap<SessionModeKind, Box<dyn SessionModeKindBehavior>>;

/// Minimal Agent surface required by `SessionModeManager` and behaviors.
/// Implemented by the real `Agent` in 4.3.9; tests provide a mock.
#[async_trait]
pub trait SessionModeContext: Send + Sync {
    // ── records ──
    fn log_record(&self, record: crate::records::AgentRecord);
    fn restoring_time(&self) -> Option<i64>;

    // ── config ──
    fn update_model_alias(&self, alias: Option<String>);
    fn refresh_llm(&self);
    fn resolve_mode_model_alias(&self, model_key: &str) -> Option<String>;
    fn default_model_alias(&self) -> Option<String>;

    // ── context partition ──
    fn set_context_mode(&self, mode: Option<SessionModeKind>);
    fn active_mode(&self) -> Option<SessionModeKind>;
    fn has_open_steps(&self) -> bool;

    // ── replay ──
    fn push_replay_record(&self, record: crate::replay::AgentReplayRecord);
    fn set_replay_mode(&self, mode: Option<SessionModeKind>);

    // ── status ──
    fn emit_status_updated(&self);

    // ── filesystem ──
    fn cwd(&self) -> String;
    fn project_root(&self) -> Option<String>;
    fn mkdir_p(&self, path: &str) -> anyhow::Result<()>;
    fn file_exists(&self, path: &str) -> bool;
    fn read_file(&self, path: &str) -> anyhow::Result<String>;
    fn write_file(&self, path: &str, content: &str) -> anyhow::Result<()>;
}
