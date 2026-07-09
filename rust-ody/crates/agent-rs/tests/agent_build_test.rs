use std::sync::{Arc, Mutex};

use agent_rs::agent::{Agent, AgentBuilder, AgentEnvironment, AgentType};
use agent_rs::context::types::PromptOrigin;
use agent_rs::permission::types::ApprovalRequest;
use agent_rs::records::nested::AgentConfigUpdateData;
use agent_rs::records::nested::ApprovalResponse;
use agent_rs::records::AgentRecord;
use agent_rs::turn::types::{AgentEvent, HookResult, StopHookBlock};
use kaos_rs::environment::detect_environment_from_node;
use kaos_rs::kaos::Kaos;
use kosong_rs::message::ContentPart;
use kosong_rs::provider::AbortSignal;

struct CollectEnv {
    events: Mutex<Vec<AgentEvent>>,
}

#[async_trait::async_trait]
impl AgentEnvironment for CollectEnv {
    fn emit_event(&self, e: AgentEvent) {
        self.events.lock().unwrap().push(e);
    }

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
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>,
    > {
        Box::pin(async { Ok(None) })
    }

    fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}

    fn fire_hook_user_prompt_submit(
        &self,
        _input: Vec<ContentPart>,
        _signal: AbortSignal,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<HookResult>, anyhow::Error>> + Send + '_>,
    > {
        Box::pin(async { Ok(vec![]) })
    }

    fn fire_hook_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<StopHookBlock>, anyhow::Error>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async { Ok(None) })
    }

    fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
    fn trigger_hook(
        &self,
        _event: &str,
        _data: serde_json::Value,
        _signal: AbortSignal,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>>
    {
        Box::pin(async { Ok(()) })
    }

    fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
    fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
}

#[tokio::test]
async fn agent_builder_creates_agent_and_records_persist() {
    let env: Arc<dyn AgentEnvironment> = Arc::new(CollectEnv {
        events: Mutex::new(Vec::new()),
    });
    let homedir = std::path::Path::new("/tmp/agent-build-test");
    let kaos = Arc::new(Kaos::new(detect_environment_from_node(), homedir));
    let agent = AgentBuilder::new("main".to_string(), kaos, env)
        .build()
        .await
        .expect("build agent");

    assert_eq!(agent.agent_type_name(), "main");
    assert_eq!(agent.id, "main");

    // Verify we can set context mode
    agent.set_context_mode(None);
    assert_eq!(agent.active_mode(), None);

    // Verify records can be written
    {
        let mut records = agent.records.lock().unwrap();
        records.log_record(AgentRecord::ContextClear { time: None });
    }
}
