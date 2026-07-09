use agent_rs::agent::{AgentBuilder, AgentEnvironment};
use agent_rs::permission::types::ApprovalRequest;
use agent_rs::records::nested::{ApprovalResponse, SessionModeKind};
use agent_rs::turn::types::{AgentEvent, HookResult, StopHookBlock};
use agent_rs::turn::TurnTools;
use kaos_rs::environment::detect_environment_from_node;
use kaos_rs::kaos::Kaos;
use kosong_rs::message::ContentPart;
use kosong_rs::provider::AbortSignal;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

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

async fn make_agent() -> Arc<agent_rs::agent::Agent> {
    let env: Arc<dyn AgentEnvironment> = Arc::new(NoopEnv);
    let kaos = Arc::new(Kaos::new(
        detect_environment_from_node(),
        std::env::current_dir().unwrap(),
    ));
    AgentBuilder::new("test", kaos, env).build().await.unwrap()
}

#[tokio::test]
async fn agent_exposes_planning_tools() {
    let agent = make_agent().await;
    let tools = agent.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name().to_string()).collect();
    assert!(names.contains(&"EnterPlanMode".into()));
    assert!(names.contains(&"ExitPlanMode".into()));
    assert!(names.contains(&"EnterDesignMode".into()));
    assert!(names.contains(&"ExitDesignMode".into()));
}

#[tokio::test]
async fn agent_exposes_office_hours_tools() {
    let agent = make_agent().await;
    let tools = agent.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name().to_string()).collect();
    assert!(names.contains(&"EnterOfficeHoursMode".into()));
    assert!(names.contains(&"ExitOfficeHoursMode".into()));
    assert!(names.contains(&"AppendBuilderProfile".into()));
    assert!(names.contains(&"AppendLearning".into()));
    assert!(names.contains(&"SearchLearnings".into()));
    assert!(names.contains(&"SetOfficeHoursLanguage".into()));
    assert!(names.contains(&"EnsureClaudeMdRouting".into()));
    assert!(names.contains(&"SyncOfficeHoursArtifact".into()));
}

#[tokio::test]
async fn agent_enters_and_exits_plan_mode() {
    let agent = make_agent().await;
    agent
        .enter_session_mode(SessionModeKind::Plan, None)
        .await
        .unwrap();
    assert!(agent.session_mode.lock().unwrap().is_active());
}

#[tokio::test]
async fn agent_enters_and_exits_office_hours_mode() {
    let agent = make_agent().await;
    agent
        .enter_session_mode(SessionModeKind::OfficeHours, None)
        .await
        .unwrap();
    assert!(agent.session_mode.lock().unwrap().is_active());
    assert_eq!(
        agent.session_mode.lock().unwrap().kind(),
        Some(SessionModeKind::OfficeHours)
    );
    agent.exit_session_mode().await.unwrap();
    assert!(!agent.session_mode.lock().unwrap().is_active());
}

#[tokio::test]
async fn agent_exposes_game_design_tools() {
    let agent = make_agent().await;
    let tools = agent.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name().to_string()).collect();
    assert!(names.contains(&"EnterGameDesignMode".into()));
    assert!(names.contains(&"ExitGameDesignMode".into()));
    assert!(names.contains(&"AppendGameDesignProfile".into()));
    assert!(names.contains(&"AppendGameDesignLearning".into()));
    assert!(names.contains(&"SearchGameDesignLearnings".into()));
    assert!(names.contains(&"SetGameDesignLanguage".into()));
    assert!(names.contains(&"EnsureGameDesignRouting".into()));
    assert!(names.contains(&"SyncGameDesignArtifact".into()));
}

#[tokio::test]
async fn agent_enters_and_exits_game_design_mode() {
    let agent = make_agent().await;
    agent
        .enter_session_mode(SessionModeKind::GameDesign, None)
        .await
        .unwrap();
    assert!(agent.session_mode.lock().unwrap().is_active());
    assert_eq!(
        agent.session_mode.lock().unwrap().kind(),
        Some(SessionModeKind::GameDesign)
    );
    agent.exit_session_mode().await.unwrap();
    assert!(!agent.session_mode.lock().unwrap().is_active());
}
