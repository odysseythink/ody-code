use std::sync::Mutex;

use agent_rs::injection::dynamic_injector::DynamicInjector;
use agent_rs::injection::goal_injector::GoalInjector;
use agent_rs::injection::knowledge_microagent::KnowledgeMicroagentInjector;
use agent_rs::injection::permission_mode_injector::PermissionModeInjector;
use agent_rs::injection::plugin_session_start::PluginSessionStartInjector;
use agent_rs::injection::session_mode_injectors::PlanModeInjector;
use agent_rs::injection::todo_list_injector::TodoListReminderInjector;
use agent_rs::injection::types::*;
use agent_rs::records::nested::SessionModeKind;

/// Minimal mock context for injector testing.
struct MockInjectionCtx {
    is_active: bool,
    mode_kind: Option<SessionModeKind>,
    assistant_turns: Mutex<usize>,
    injected_texts: Mutex<Vec<String>>,
    handoff_plan: Mutex<Option<PendingDesignHandoff>>,
    handoff_normal: Mutex<Option<PendingPlanHandoff>>,
    unavailable_skills: Mutex<Option<String>>,
    permission_mode_val: Mutex<Option<String>>,
    goal_text: Mutex<Option<String>>,
    tool_active: Mutex<bool>,
    flag_enabled: Mutex<bool>,
}

impl MockInjectionCtx {
    fn set_permission_mode(&self, mode: Option<&str>) {
        *self.permission_mode_val.lock().unwrap() = mode.map(|s| s.to_string());
    }

    fn set_goal_text(&self, text: Option<&str>) {
        *self.goal_text.lock().unwrap() = text.map(|s| s.to_string());
    }

    fn set_tool_active(&self, active: bool) {
        *self.tool_active.lock().unwrap() = active;
    }

    fn set_flag_enabled(&self, enabled: bool) {
        *self.flag_enabled.lock().unwrap() = enabled;
    }

    fn set_assistant_turns(&self, turns: usize) {
        *self.assistant_turns.lock().unwrap() = turns;
    }
}

#[async_trait::async_trait]
impl InjectionManagerContext for MockInjectionCtx {
    fn is_session_mode_active(&self) -> bool {
        self.is_active
    }
    fn session_mode_kind(&self) -> Option<SessionModeKind> {
        self.mode_kind
    }
    fn consume_pending_handoff_for_plan(&self) -> Option<PendingDesignHandoff> {
        self.handoff_plan.lock().unwrap().take()
    }
    fn consume_pending_handoff_for_normal(&self) -> Option<PendingPlanHandoff> {
        self.handoff_normal.lock().unwrap().take()
    }
    fn session_mode_file_path(&self) -> Option<String> {
        None
    }
    fn append_system_reminder(&self, text: &str, _kind: &str, _variant: &str) {
        self.injected_texts.lock().unwrap().push(text.to_string());
    }
    fn context_history_len(&self) -> usize {
        10
    }
    fn assistant_turn_count(&self) -> usize {
        *self.assistant_turns.lock().unwrap()
    }
    fn is_tool_active(&self, _tool_name: &str) -> bool {
        *self.tool_active.lock().unwrap()
    }
    fn get_unavailable_skills_reminder(&self, _mode: SessionModeKind) -> Option<String> {
        self.unavailable_skills.lock().unwrap().clone()
    }
    fn get_active_goal_text(&self) -> Option<String> {
        self.goal_text.lock().unwrap().clone()
    }
    fn permission_mode(&self) -> Option<String> {
        self.permission_mode_val.lock().unwrap().clone()
    }
    fn is_flag_enabled(&self, _flag: &str) -> bool {
        *self.flag_enabled.lock().unwrap()
    }
    fn agent_type(&self) -> &str {
        "main"
    }
    fn restoring_time(&self) -> Option<i64> {
        None
    }
}

fn default_ctx() -> MockInjectionCtx {
    MockInjectionCtx {
        is_active: false,
        mode_kind: None,
        assistant_turns: Mutex::new(0),
        injected_texts: Mutex::new(Vec::new()),
        handoff_plan: Mutex::new(None),
        handoff_normal: Mutex::new(None),
        unavailable_skills: Mutex::new(None),
        permission_mode_val: Mutex::new(None),
        goal_text: Mutex::new(None),
        tool_active: Mutex::new(false),
        flag_enabled: Mutex::new(false),
    }
}

// ── Task 10: Non-mode injector tests ──

#[tokio::test]
async fn plugin_session_start_one_shot() {
    let ctx = default_ctx();
    let injector = PluginSessionStartInjector::new();

    // Stub returns None; one-shot gate is in place (pos.injected_at is None)
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
    // Initially not injected
    assert!(!injector.has_injected());

    // Simulate injection: mark pos as injected (as the InjectionManager would)
    injector.pos.lock().unwrap().mark_injected(10);
    assert!(injector.has_injected());

    // Second call: pos is set → gate check returns early → None
    let result2 = injector.get_injection(&ctx).await;
    assert!(result2.is_none());

    // on_context_clear resets position
    let mut injector_for_clear = PluginSessionStartInjector::new();
    injector_for_clear.pos.lock().unwrap().mark_injected(10);
    assert!(injector_for_clear.has_injected());
    injector_for_clear.on_context_clear();
    assert!(!injector_for_clear.has_injected());
}

#[tokio::test]
async fn goal_injector_no_goal_returns_none() {
    let ctx = default_ctx();
    let injector = GoalInjector::new();

    // No goal text set → returns None
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn goal_injector_returns_goal_text() {
    let ctx = default_ctx();
    ctx.set_goal_text(Some("Implement user authentication"));

    let injector = GoalInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "Implement user authentication");
}

#[tokio::test]
async fn goal_injector_one_shot_after_injected() {
    let ctx = default_ctx();
    ctx.set_goal_text(Some("Implement user authentication"));

    let injector = GoalInjector::new();
    // First injection returns goal text
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_some());

    // Mark as injected (simulate manager)
    injector.pos.lock().unwrap().mark_injected(10);
    assert!(injector.has_injected());

    // Second call: pos is set → one-shot gate → None
    let result2 = injector.get_injection(&ctx).await;
    assert!(result2.is_none());
}

#[tokio::test]
async fn todo_list_reminder_after_10_turns() {
    let ctx = default_ctx();
    ctx.set_tool_active(true);
    ctx.set_assistant_turns(10);

    let injector = TodoListReminderInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_some());
    assert!(result.unwrap().contains("TODO"));
}

#[tokio::test]
async fn todo_list_reminder_skips_when_tool_inactive() {
    let ctx = default_ctx();
    ctx.set_tool_active(false);
    ctx.set_assistant_turns(10);

    let injector = TodoListReminderInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn todo_list_reminder_skips_on_non_interval_turns() {
    let ctx = default_ctx();
    ctx.set_tool_active(true);
    ctx.set_assistant_turns(7); // Not a multiple of 10

    let injector = TodoListReminderInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn todo_list_reminder_skips_zero_turns() {
    let ctx = default_ctx();
    ctx.set_tool_active(true);
    ctx.set_assistant_turns(0); // Turns must be > 0

    let injector = TodoListReminderInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn permission_mode_injector_no_transition_returns_none() {
    let ctx = default_ctx();
    // Both previous (None) and current (None from mock) are the same
    let injector = PermissionModeInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn permission_mode_injector_transition_to_auto() {
    let ctx = default_ctx();
    ctx.set_permission_mode(Some("auto"));

    let injector = PermissionModeInjector::new();
    // initial previous_mode is None, current is "auto" → transition
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_some());
    assert!(result.unwrap().contains("Auto"));

    // Second call: previous_mode is now "auto", current is still "auto" → no transition
    let result2 = injector.get_injection(&ctx).await;
    assert!(result2.is_none());
}

#[tokio::test]
async fn permission_mode_injector_transition_to_yolo() {
    let ctx = default_ctx();
    ctx.set_permission_mode(Some("yolo"));

    let injector = PermissionModeInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_some());
    assert!(result.unwrap().contains("YOLO"));
}

#[tokio::test]
async fn permission_mode_injector_transition_unknown_mode_returns_none() {
    let ctx = default_ctx();
    ctx.set_permission_mode(Some("default"));

    let injector = PermissionModeInjector::new();
    let result = injector.get_injection(&ctx).await;
    // Unknown mode → no message, but previous_mode is updated
    assert!(result.is_none());
}

#[tokio::test]
async fn knowledge_microagent_only_in_normal_mode() {
    // Session mode is active → should not inject
    let ctx = MockInjectionCtx {
        is_active: true,
        mode_kind: Some(SessionModeKind::Plan),
        ..default_ctx()
    };
    ctx.set_flag_enabled(true);

    let injector = KnowledgeMicroagentInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn knowledge_microagent_with_flag_disabled() {
    // Normal mode (not active) but flag disabled → should not inject
    let ctx = default_ctx();
    ctx.set_flag_enabled(false);

    let injector = KnowledgeMicroagentInjector::new();
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn knowledge_microagent_normal_mode_flag_enabled_stub_returns_none() {
    // Normal mode, flag enabled → gate passes but stub returns None
    let ctx = default_ctx();
    ctx.set_flag_enabled(true);

    let injector = KnowledgeMicroagentInjector::new();
    let result = injector.get_injection(&ctx).await;
    // Gates pass but stub returns None
    assert!(result.is_none());
}

#[tokio::test]
async fn knowledge_microagent_one_shot_after_injected() {
    let ctx = default_ctx();
    ctx.set_flag_enabled(true);

    let injector = KnowledgeMicroagentInjector::new();
    // Mark as already injected
    injector.pos.lock().unwrap().mark_injected(10);
    assert!(injector.has_injected());

    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none());
}

// ── Task 9: Session-mode injector tests ──

/// Helper to create a context with a specific session mode active.
fn session_mode_ctx(mode_kind: SessionModeKind, turns: usize) -> MockInjectionCtx {
    MockInjectionCtx {
        is_active: true,
        mode_kind: Some(mode_kind),
        assistant_turns: Mutex::new(turns),
        ..default_ctx()
    }
}

#[tokio::test]
async fn plan_injector_entry_when_just_activated() {
    let ctx = session_mode_ctx(SessionModeKind::Plan, 0);
    let injector = PlanModeInjector::new();

    // First call: was_active=false (default), is_active=true → entry reminder
    let result = injector.get_injection(&ctx).await;
    assert!(result.is_some());
    assert!(result.unwrap().contains("Plan mode is active"));
}

#[tokio::test]
async fn plan_injector_exit_when_just_deactivated() {
    let ctx = MockInjectionCtx {
        is_active: false,
        mode_kind: None,
        ..default_ctx()
    };
    let injector = PlanModeInjector::new();
    // Mark was_active=true to simulate transition from active → inactive
    injector.set_was_active(true);

    let result = injector.get_injection(&ctx).await;
    assert!(result.is_some());
    assert!(result.unwrap().contains("Plan mode has ended"));
}

#[tokio::test]
async fn plan_injector_skips_on_off_turns() {
    let ctx = session_mode_ctx(SessionModeKind::Plan, 3); // Not a multiple of 2 or 5
    let injector = PlanModeInjector::new();
    // Mark was_active=true so we're in "staying active" state
    injector.set_was_active(true);

    let result = injector.get_injection(&ctx).await;
    assert!(result.is_none()); // Skips — not a full/sparse turn
}

#[test]
fn injector_on_context_clear_resets_position() {
    use agent_rs::injection::base_session_mode::BaseSessionModeInjector;

    let mut injector = PlanModeInjector::new();
    injector.pos_mut().mark_injected(5);
    injector.on_context_clear();
    assert!(!injector.has_injected());
}
