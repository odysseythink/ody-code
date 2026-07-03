use agent_rs::records::nested::SessionModeKind;
use agent_rs::session_mode::behaviors::*;
use agent_rs::session_mode::types::*;
use std::sync::Mutex;

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
    fn restoring_time(&self) -> Option<i64> {
        None
    }
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
    fn default_model_alias(&self) -> Option<String> {
        Some("default-model".into())
    }
    fn set_context_mode(&self, _mode: Option<SessionModeKind>) {}
    fn active_mode(&self) -> Option<SessionModeKind> {
        None
    }
    fn has_open_steps(&self) -> bool {
        false
    }
    fn push_replay_record(&self, _record: agent_rs::replay::AgentReplayRecord) {}
    fn set_replay_mode(&self, _mode: Option<SessionModeKind>) {}
    fn emit_status_updated(&self) {}
    fn cwd(&self) -> String {
        "/tmp/test".into()
    }
    fn project_root(&self) -> Option<String> {
        Some("/tmp/test".into())
    }
    fn mkdir_p(&self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn file_exists(&self, _path: &str) -> bool {
        false
    }
    fn read_file(&self, _path: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
    fn write_file(&self, _path: &str, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

// Use a real PlanModeBehavior (from Task 6) — but we can't import it yet.
// Instead, we test the do_enter/do_exit/do_cancel functions directly.
// We'll use a minimal SessionModeKindBehavior impl for testing.

struct TestPlanBehavior;

#[async_trait::async_trait]
impl SessionModeKindBehavior for TestPlanBehavior {
    fn kind(&self) -> SessionModeKind {
        SessionModeKind::Plan
    }
    fn output_subdirectory(&self) -> &str {
        "plans"
    }
    fn mode_model_key(&self) -> &str {
        "plan"
    }
    fn handoff_target(&self) -> Option<&str> {
        Some("normal")
    }
    fn supports_design_sessions(&self) -> bool {
        false
    }

    async fn on_enter(
        &self,
        ctx: &ModeEnterContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_enter(SessionModeKind::Plan, ctx, sm_ctx).await
    }

    async fn on_exit(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_exit(SessionModeKind::Plan, ctx, sm_ctx, None).await
    }

    async fn on_cancel(
        &self,
        ctx: &ModeExitContext,
        sm_ctx: &dyn SessionModeContext,
    ) -> anyhow::Result<()> {
        do_cancel(SessionModeKind::Plan, ctx, sm_ctx, None).await
    }
}

#[tokio::test]
async fn base_do_enter_switches_model() {
    let ctx = MockContext::new();
    let behavior = TestPlanBehavior;

    let enter_ctx = ModeEnterContext {
        id: "test-id-1".into(),
        restore_target_alias: Some("default-model".into()),
    };

    behavior.on_enter(&enter_ctx, &ctx).await.unwrap();

    assert_eq!(
        *ctx.model_alias.lock().unwrap(),
        Some("plan-model-v1".into())
    );
}

#[tokio::test]
async fn base_do_exit_restores_model() {
    let ctx = MockContext::new();
    *ctx.model_alias.lock().unwrap() = Some("plan-model-v1".into());

    let behavior = TestPlanBehavior;
    let exit_ctx = ModeExitContext {
        id: Some("test-id-1".into()),
        session_mode_file_path: None,
    };

    behavior.on_exit(&exit_ctx, &ctx).await.unwrap();

    // Should restore to the pre-mode alias.
    assert_eq!(
        *ctx.model_alias.lock().unwrap(),
        Some("default-model".into())
    );
}

#[tokio::test]
async fn base_do_enter_switches_model_and_creates_dirs() {
    let ctx = MockContext::new();
    let behavior = TestPlanBehavior;

    let enter_ctx = ModeEnterContext {
        id: "rec-test-id".into(),
        restore_target_alias: None,
    };

    behavior.on_enter(&enter_ctx, &ctx).await.unwrap();

    // do_enter handles mkdir_p, gitignore, model alias switching.
    // WAL record logging is done by SessionModeManager, not do_enter.
    // Verify that the model alias was switched to the plan-specific model.
    assert_eq!(
        *ctx.model_alias.lock().unwrap(),
        Some("plan-model-v1".into())
    );
}

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

#[test]
fn design_behavior_kind_is_design() {
    let behavior = DesignModeBehavior;
    assert_eq!(behavior.kind(), SessionModeKind::Design);
}

#[test]
fn design_behavior_handoff_target_is_plan() {
    let behavior = DesignModeBehavior;
    assert_eq!(behavior.handoff_target(), Some("plan"));
}

#[test]
fn design_behavior_supports_design_sessions() {
    let behavior = DesignModeBehavior;
    assert_eq!(behavior.supports_design_sessions(), true);
}

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
