use agent_rs::injection::manager::InjectionManager;
use agent_rs::injection::types::*;
use agent_rs::records::nested::SessionModeKind;
use std::sync::Mutex;

struct MockInjCtx {
    is_active: bool,
    mode_kind: Option<SessionModeKind>,
    assistant_turns: Mutex<usize>,
    injected: Mutex<Vec<(String, String, String)>>, // (text, kind, variant)
    flag_enabled: bool,
    tool_active: Vec<String>,
}

impl MockInjCtx {
    fn new() -> Self {
        Self {
            is_active: false,
            mode_kind: None,
            assistant_turns: Mutex::new(0),
            injected: Mutex::new(Vec::new()),
            flag_enabled: false,
            tool_active: vec!["TodoList".to_string()],
        }
    }
}

#[async_trait::async_trait]
impl InjectionManagerContext for MockInjCtx {
    fn is_session_mode_active(&self) -> bool {
        self.is_active
    }
    fn session_mode_kind(&self) -> Option<SessionModeKind> {
        self.mode_kind
    }
    fn consume_pending_handoff_for_plan(&self) -> Option<PendingDesignHandoff> {
        None
    }
    fn consume_pending_handoff_for_normal(&self) -> Option<PendingPlanHandoff> {
        None
    }
    fn session_mode_file_path(&self) -> Option<String> {
        None
    }
    fn append_system_reminder(&self, text: &str, kind: &str, variant: &str) {
        self.injected.lock().unwrap().push((
            text.to_string(),
            kind.to_string(),
            variant.to_string(),
        ));
    }
    fn context_history_len(&self) -> usize {
        10
    }
    fn assistant_turn_count(&self) -> usize {
        *self.assistant_turns.lock().unwrap()
    }
    fn is_tool_active(&self, tool_name: &str) -> bool {
        self.tool_active.contains(&tool_name.to_string())
    }
    fn get_unavailable_skills_reminder(&self, _mode: SessionModeKind) -> Option<String> {
        None
    }
    fn get_active_goal_text(&self) -> Option<String> {
        None
    }
    fn permission_mode(&self) -> Option<String> {
        None
    }
    fn is_flag_enabled(&self, _flag: &str) -> bool {
        self.flag_enabled
    }
    fn agent_type(&self) -> &str {
        "main"
    }
    fn restoring_time(&self) -> Option<i64> {
        None
    }
}

#[tokio::test]
async fn injection_manager_inject_runs_all_injectors() {
    let ctx = MockInjCtx::new();
    let mgr = InjectionManager::new(&ctx);

    mgr.inject(&ctx).await;

    // Even without active session mode, PluginSessionStart and PermissionMode injectors run.
    // PluginSessionStart may inject (if it has content), TodoList may inject if turns%10==0.
    // Just verify no panic and the method completes.
    let injected = ctx.injected.lock().unwrap();
    // PluginSessionStart is a stub that returns None, so no injections expected here.
    // The test just verifies the pipeline doesn't crash.
    let _ = injected.len();
}

#[tokio::test]
async fn injection_manager_on_context_clear_calls_all_injectors() {
    let ctx = MockInjCtx::new();
    let mut mgr = InjectionManager::new(&ctx);

    // Should not panic
    mgr.on_context_clear();
}

#[tokio::test]
async fn injection_manager_on_context_compacted_calls_all_injectors() {
    let ctx = MockInjCtx::new();
    let mut mgr = InjectionManager::new(&ctx);

    mgr.on_context_compacted(5);
}

#[tokio::test]
async fn injection_manager_on_context_message_removed() {
    let ctx = MockInjCtx::new();
    let mut mgr = InjectionManager::new(&ctx);

    mgr.on_context_message_removed(3);
}

#[tokio::test]
async fn injection_manager_inject_with_active_plan_mode() {
    let ctx = MockInjCtx {
        is_active: true,
        mode_kind: Some(SessionModeKind::Plan),
        assistant_turns: Mutex::new(0),
        injected: Mutex::new(Vec::new()),
        flag_enabled: false,
        tool_active: vec![],
    };
    let mgr = InjectionManager::new(&ctx);

    mgr.inject(&ctx).await;

    let injected = ctx.injected.lock().unwrap();
    // Plan mode should produce an injection (entry reminder on first call)
    assert!(
        !injected.is_empty(),
        "Expected at least one injection for plan mode entry"
    );
    let plan_injections: Vec<_> = injected
        .iter()
        .filter(|(_, _, variant)| variant == &VARIANT_PLAN_MODE)
        .collect();
    assert!(!plan_injections.is_empty(), "Expected plan mode injection");
}
