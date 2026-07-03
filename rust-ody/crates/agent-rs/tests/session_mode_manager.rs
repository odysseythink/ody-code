use agent_rs::agent::{AgentBuilder, AgentEnvironment};
use agent_rs::permission::types::ApprovalRequest;
use agent_rs::records::nested::{ApprovalResponse, SessionModeKind};
use agent_rs::session_mode::behaviors::create_default_mode_behavior_registry;
use agent_rs::session_mode::manager::SessionModeManager;
use agent_rs::session_mode::types::*;
use agent_rs::turn::types::{AgentEvent, HookResult, StopHookBlock};
use kaos_rs::environment::detect_environment_from_node;
use kaos_rs::kaos::Kaos;
use kosong_rs::message::ContentPart;
use kosong_rs::provider::AbortSignal;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

struct MockSmContext {
    model_alias: Mutex<Option<String>>,
    records: Mutex<Vec<agent_rs::records::AgentRecord>>,
    active_mode: Mutex<Option<SessionModeKind>>,
    replay_records: Mutex<Vec<agent_rs::replay::AgentReplayRecord>>,
    files: Mutex<HashMap<String, String>>,
}

impl MockSmContext {
    fn new() -> Self {
        Self {
            model_alias: Mutex::new(Some("default-model".into())),
            records: Mutex::new(Vec::new()),
            active_mode: Mutex::new(None),
            replay_records: Mutex::new(Vec::new()),
            files: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionModeContext for MockSmContext {
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
    fn has_open_steps(&self) -> bool {
        false
    }
    fn push_replay_record(&self, record: agent_rs::replay::AgentReplayRecord) {
        self.replay_records.lock().unwrap().push(record);
    }
    fn set_replay_mode(&self, _mode: Option<SessionModeKind>) {}
    fn emit_status_updated(&self) {}
    fn cwd(&self) -> String {
        "/tmp/test-sm".into()
    }
    fn project_root(&self) -> Option<String> {
        Some("/tmp/test-sm".into())
    }
    fn mkdir_p(&self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn file_exists(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }
    fn read_file(&self, path: &str) -> anyhow::Result<String> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("file not found: {}", path))
    }
    fn write_file(&self, path: &str, content: &str) -> anyhow::Result<()> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), content.to_string());
        Ok(())
    }
}

#[tokio::test]
async fn enter_plan_mode() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-1".into()), None)
        .await
        .unwrap();

    assert!(mgr.is_active());
    assert_eq!(mgr.kind(), Some(SessionModeKind::Plan));

    let records = mgr.context().records.lock().unwrap();
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

    mgr.enter(SessionModeKind::Plan, Some("plan-2".into()), None)
        .await
        .unwrap();
    assert!(mgr.is_active());

    mgr.exit(None).await.unwrap();

    assert!(!mgr.is_active());
    assert_eq!(mgr.kind(), None);

    let records = mgr.context().records.lock().unwrap();
    let exit_records: Vec<_> = records
        .iter()
        .filter(|r| matches!(r, agent_rs::records::AgentRecord::SessionModeExit { .. }))
        .collect();
    assert_eq!(exit_records.len(), 1);
}

#[tokio::test]
async fn cancel_plan_mode() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-3".into()), None)
        .await
        .unwrap();
    assert!(mgr.is_active());

    mgr.cancel(None).await.unwrap();

    assert!(!mgr.is_active());

    let records = mgr.context().records.lock().unwrap();
    let cancel_records: Vec<_> = records
        .iter()
        .filter(|r| matches!(r, agent_rs::records::AgentRecord::SessionModeCancel { .. }))
        .collect();
    assert_eq!(cancel_records.len(), 1);
}

#[tokio::test]
async fn enter_twice_throws() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("id-1".into()), None)
        .await
        .unwrap();
    let result = mgr
        .enter(SessionModeKind::Design, Some("id-2".into()), None)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn session_mode_file_path_resolves() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-file".into()), None)
        .await
        .unwrap();

    let path = mgr.session_mode_file_path();
    assert!(path.is_some());
    assert!(path.unwrap().contains("plans"));
}

#[tokio::test]
async fn design_handoff_preserves_selected_label() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Design, Some("design-1".into()), None)
        .await
        .unwrap();
    assert!(mgr.is_active());

    mgr.handoff_to(
        "plan",
        HandoffOptions {
            selected_label: Some("Approach A".into()),
        },
    )
    .await
    .unwrap();

    assert!(!mgr.is_active());
    let handoff = mgr.consume_pending_handoff_for_plan().expect("handoff");
    assert_eq!(handoff.selected_label, Some("Approach A".into()));
    assert!(handoff.path.contains("designs"));
}

#[tokio::test]
async fn plan_handoff_preserves_selected_label() {
    let ctx = MockSmContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut mgr = SessionModeManager::new(ctx, registry);

    mgr.enter(SessionModeKind::Plan, Some("plan-1".into()), None)
        .await
        .unwrap();
    assert!(mgr.is_active());

    let path = mgr.session_mode_file_path().clone().unwrap();
    mgr.context().write_file(&path, "# Plan content").unwrap();

    mgr.handoff_to(
        "normal",
        HandoffOptions {
            selected_label: Some("Approach B".into()),
        },
    )
    .await
    .unwrap();

    assert!(!mgr.is_active());
    let handoff = mgr.consume_pending_handoff_for_normal().expect("handoff");
    assert_eq!(handoff.selected_label, Some("Approach B".into()));
    assert_eq!(handoff.content, "# Plan content");
}

struct NoopEnv;
#[async_trait::async_trait]
impl AgentEnvironment for NoopEnv {
    fn emit_event(&self, _event: AgentEvent) {}
    async fn request_approval(
        &self,
        _req: &ApprovalRequest,
        _signal: AbortSignal,
    ) -> Result<ApprovalResponse, anyhow::Error> {
        Ok(ApprovalResponse {
            decision: "approved".into(),
            scope: None,
            feedback: None,
            selected_label: None,
        })
    }
    fn fire_hook_pre_tool_use(
        &self,
        _tool_name: &str,
        _tool_input: serde_json::Value,
        _tool_call_id: &str,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
        Box::pin(async { Ok(None) })
    }
    fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_user_prompt_submit(
        &self,
        _input: Vec<ContentPart>,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HookResult>, anyhow::Error>> + Send + '_>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn fire_hook_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<StopHookBlock>, anyhow::Error>> + Send + '_>>
    {
        Box::pin(async { Ok(None) })
    }
    fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
    fn trigger_hook(
        &self,
        _event: &str,
        _data: serde_json::Value,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
    fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
}

#[tokio::test]
async fn agent_enters_plan_mode_with_default_registry() {
    let kaos = Arc::new(Kaos::new(
        detect_environment_from_node(),
        std::env::current_dir().unwrap(),
    ));
    let env: Arc<dyn AgentEnvironment> = Arc::new(NoopEnv);
    let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();

    agent
        .enter_session_mode(SessionModeKind::Plan, None)
        .await
        .unwrap();
    assert!(agent.session_mode.lock().unwrap().is_active());
    assert_eq!(
        agent.session_mode.lock().unwrap().kind(),
        Some(SessionModeKind::Plan)
    );
}

fn kind_label(kind: SessionModeKind) -> &'static str {
    match kind {
        SessionModeKind::Plan => "plan",
        SessionModeKind::Design => "design",
        SessionModeKind::OfficeHours => "office-hours",
        SessionModeKind::GameDesign => "game-design",
    }
}

#[tokio::test]
async fn agent_enters_all_default_session_modes() {
    for kind in [
        SessionModeKind::Plan,
        SessionModeKind::Design,
        SessionModeKind::OfficeHours,
        SessionModeKind::GameDesign,
    ] {
        let kaos = Arc::new(Kaos::new(
            detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let env: Arc<dyn AgentEnvironment> = Arc::new(NoopEnv);
        let label = kind_label(kind);
        let agent = AgentBuilder::new(format!("test-{}", label), kaos, env)
            .build()
            .await
            .unwrap();

        agent
            .enter_session_mode(kind, Some(format!("{}-1", label)))
            .await
            .unwrap();
        assert!(agent.session_mode.lock().unwrap().is_active());
        assert_eq!(agent.session_mode.lock().unwrap().kind(), Some(kind));
    }
}
