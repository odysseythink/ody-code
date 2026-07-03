use crate::records::nested::SessionModeKind;
use async_trait::async_trait;

use super::dynamic_injector::DynamicInjector;

/// Injection variant constants — mirror TS injection variant strings.
pub const VARIANT_PLUGIN_SESSION_START: &str = "plugin_session_start";
pub const VARIANT_TODO_LIST_REMINDER: &str = "todo_list_reminder";
pub const VARIANT_PLAN_MODE: &str = "plan_mode";
pub const VARIANT_DESIGN_MODE: &str = "design_mode";
pub const VARIANT_OFFICE_HOURS: &str = "office_hours";
pub const VARIANT_GAME_DESIGN: &str = "game_design";
pub const VARIANT_PERMISSION_MODE: &str = "permission_mode";
pub const VARIANT_KNOWLEDGE_MICROAGENT: &str = "knowledge_microagent";
pub const VARIANT_GOAL: &str = "goal";

/// Pending handoff from design mode to plan mode.
#[derive(Debug, Clone)]
pub struct PendingDesignHandoff {
    pub path: String,
    pub filename: String,
    pub selected_label: Option<String>,
}

/// Pending handoff from plan mode to normal mode.
#[derive(Debug, Clone)]
pub struct PendingPlanHandoff {
    pub content: String,
    pub path: String,
    pub selected_label: Option<String>,
}

/// Minimal Agent surface required by `InjectionManager` and its injectors.
/// Mirrors TS injection's access to `agent.*` subsystems.
#[async_trait]
pub trait InjectionManagerContext: Send + Sync {
    // ── session mode ──
    fn is_session_mode_active(&self) -> bool;
    fn session_mode_kind(&self) -> Option<SessionModeKind>;
    fn consume_pending_handoff_for_plan(&self) -> Option<PendingDesignHandoff>;
    fn consume_pending_handoff_for_normal(&self) -> Option<PendingPlanHandoff>;
    fn session_mode_file_path(&self) -> Option<String>;

    // ── context ──
    fn append_system_reminder(&self, text: &str, kind: &str, variant: &str);
    fn context_history_len(&self) -> usize;
    fn assistant_turn_count(&self) -> usize;

    // ── tools ──
    fn is_tool_active(&self, tool_name: &str) -> bool;

    // ── skills ──
    fn get_unavailable_skills_reminder(&self, mode: SessionModeKind) -> Option<String>;

    // ── goals ──
    fn get_active_goal_text(&self) -> Option<String>;

    // ── permission ──
    fn permission_mode(&self) -> Option<String>;

    // ── config/flags ──
    fn is_flag_enabled(&self, flag: &str) -> bool;
    fn agent_type(&self) -> &str;

    // ── records ──
    fn restoring_time(&self) -> Option<i64>;
}

/// Trait for injectors that serve a specific session mode.
/// Extends the base DynamicInjector with mode-specific awareness.
pub trait SessionModeInjector: DynamicInjector + Send + Sync {
    fn injection_variant(&self) -> &str;
}
