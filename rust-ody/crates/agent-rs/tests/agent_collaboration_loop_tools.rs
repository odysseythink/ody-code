use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use agent_rs::agent::{AgentBuilder, AgentEnvironment};
use agent_rs::agent_loop::types::ExecutableTool as LoopExecutableTool;
use agent_rs::skill::{InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillSource};
use agent_rs::tool::collaboration::QuestionCallback;
use agent_rs::tool::types::{BuiltinToolProvisionContext, BuiltinToolsProvider, ExecutableTool};
use agent_rs::turn::types::TurnAgent;
use kaos_rs::environment::detect_environment_from_node;
use kaos_rs::kaos::Kaos;
use kosong_rs::message::ContentPart;
use kosong_rs::provider::AbortSignal;

struct NoopEnv;
#[async_trait::async_trait]
impl AgentEnvironment for NoopEnv {
    fn emit_event(&self, _event: agent_rs::turn::types::AgentEvent) {}
    async fn request_approval(
        &self,
        _req: &agent_rs::permission::types::ApprovalRequest,
        _signal: AbortSignal,
    ) -> Result<agent_rs::records::nested::ApprovalResponse, anyhow::Error> {
        Ok(agent_rs::records::nested::ApprovalResponse {
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
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<agent_rs::turn::types::HookResult>, anyhow::Error>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async { Ok(vec![]) })
    }
    fn fire_hook_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Option<agent_rs::turn::types::StopHookBlock>, anyhow::Error>>
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
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
    fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
}

fn sample_skill() -> SkillDefinition {
    SkillDefinition {
        name: "refactor".into(),
        description: "".into(),
        path: "/skills/refactor.md".into(),
        dir: "/skills".into(),
        content: "Refactor this code.".into(),
        metadata: SkillMetadata {
            skill_type: Some("prompt".into()),
            ..SkillMetadata::default()
        },
        source: SkillSource::Project,
        plugin: None,
        mermaid: None,
        d2: None,
    }
}

struct CollaborationProvider;

impl BuiltinToolsProvider for CollaborationProvider {
    fn provide(&self, _ctx: BuiltinToolProvisionContext) -> Vec<Arc<dyn LoopExecutableTool>> {
        vec![
            Arc::new(ExecutableTool {
                name: "Skill".into(),
                description: "Invoke a skill".into(),
                parameters: serde_json::json!({"type": "object"}),
            }),
            Arc::new(ExecutableTool {
                name: "AskUserQuestion".into(),
                description: "Ask the user a question".into(),
                parameters: serde_json::json!({"type": "object"}),
            }),
        ]
    }
}

#[tokio::test]
async fn loop_tools_includes_collaboration_tools_when_configured() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill());

    let callback: QuestionCallback = Arc::new(|_req, _signal| {
        Box::pin(async move { Ok(tools_rs::builtin::collaboration::QuestionResult::Dismissed) })
    });

    let kaos = Arc::new(Kaos::new(
        detect_environment_from_node(),
        std::env::current_dir().unwrap(),
    ));
    let agent = AgentBuilder::new("test", kaos, Arc::new(NoopEnv))
        .skills_registry(Box::new(registry))
        .question_callback(callback)
        .builtin_tools_provider(Arc::new(CollaborationProvider))
        .build()
        .await
        .unwrap();

    agent
        .tools
        .lock()
        .unwrap()
        .set_active_tools(&["Skill".into(), "AskUserQuestion".into()]);

    let tools = agent.tools().loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
    assert!(names.contains(&"Skill"), "got {:?}", names);
    assert!(names.contains(&"AskUserQuestion"), "got {:?}", names);
    // AgentTool requires subagent_host; should not appear when not configured
    assert!(!names.contains(&"Agent"));
}
