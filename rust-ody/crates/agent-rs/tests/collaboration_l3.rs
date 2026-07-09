use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use agent_rs::agent::{AgentBuilder, AgentEnvironment};
use agent_rs::agent_loop::events::DefaultLoopEventDispatcher;
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::run_turn::run_turn;
use agent_rs::agent_loop::types::{LoopTurnStopReason, RunTurnInput};
use agent_rs::skill::{
    registry::SkillRegistry, InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillSource,
};
use agent_rs::tool::collaboration::{AgentSubagentHost, QuestionCallback, SubagentRunFn};
use agent_rs::turn::types::TurnAgent;
use kaos_rs::environment::detect_environment_from_node;
use kaos_rs::kaos::Kaos;
use kosong_rs::message::{ContentPart, ToolCall};
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;

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

struct CountingRegistry {
    skills: HashMap<String, SkillDefinition>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl CountingRegistry {
    fn new(skills: Vec<SkillDefinition>) -> Self {
        let map = skills.into_iter().map(|s| (s.name.clone(), s)).collect();
        Self {
            skills: map,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Clone for CountingRegistry {
    fn clone(&self) -> Self {
        Self {
            skills: self.skills.clone(),
            calls: Arc::clone(&self.calls),
        }
    }
}

impl SkillRegistry for CountingRegistry {
    fn get_skill(&self, name: &str) -> Option<&SkillDefinition> {
        self.calls.lock().unwrap().push(name.to_string());
        self.skills.get(name)
    }
    fn list_skills(&self) -> Vec<&SkillDefinition> {
        let mut v: Vec<_> = self.skills.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
    fn list_invocable_skills(&self, session_mode: Option<&str>) -> Vec<&SkillDefinition> {
        self.list_skills()
            .into_iter()
            .filter(|skill| {
                if skill.metadata.disable_model_invocation == Some(true) {
                    return false;
                }
                let inline = matches!(
                    skill.metadata.skill_type.as_deref(),
                    None | Some("prompt") | Some("inline")
                );
                if !inline {
                    return false;
                }
                if let Some(mode) = session_mode {
                    if let Some(hidden) = &skill.metadata.hidden_in_modes {
                        if hidden.iter().any(|m| m.eq_ignore_ascii_case(mode)) {
                            return false;
                        }
                    }
                }
                true
            })
            .collect()
    }
    fn render_skill_prompt(&self, skill: &SkillDefinition, _raw_args: &str) -> String {
        skill.content.clone()
    }
}

struct SequencedLlm {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl Llm for SequencedLlm {
    fn system_prompt(&self) -> &str {
        ""
    }
    fn model_name(&self) -> &str {
        "sequenced"
    }
    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let step = {
            let mut s = self.step.lock().unwrap();
            *s += 1;
            *s
        };
        let tool_call = |name: &str, args: serde_json::Value| ToolCall {
            call_type: "tool_call".into(),
            id: format!("call_{}", name.to_lowercase()),
            name: name.into(),
            arguments: Some(serde_json::to_string(&args).unwrap()),
            extras: None,
            stream_index: None,
        };
        let (tool_calls, finish) = match step {
            1 => (
                vec![tool_call("Skill", serde_json::json!({"skill":"refactor"}))],
                FinishReason::ToolCalls,
            ),
            2 => (
                vec![tool_call(
                    "AskUserQuestion",
                    serde_json::json!({"questions":[{"question":"Pick a color?","header":"Style","options":[{"label":"Red","description":"warm"},{"label":"Blue","description":"cool"}],"multi_select":false}]}),
                )],
                FinishReason::ToolCalls,
            ),
            3 => (
                vec![tool_call(
                    "Agent",
                    serde_json::json!({"prompt":"do it","description":"sub"}),
                )],
                FinishReason::ToolCalls,
            ),
            _ => (vec![], FinishReason::Completed),
        };
        Ok(LlmChatResponse {
            tool_calls,
            provider_finish_reason: Some(finish),
            raw_finish_reason: Some(format!("{:?}", finish)),
            usage: TokenUsage::default(),
            stream_timing: None,
        })
    }
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

#[tokio::test]
async fn collaboration_tools_execute_in_turn() {
    let counting_registry = Arc::new(CountingRegistry::new(vec![sample_skill()]));

    let question_calls = Arc::new(Mutex::new(Vec::new()));
    let question_calls_clone = Arc::clone(&question_calls);
    let callback: QuestionCallback = Arc::new(move |req, _signal| {
        question_calls_clone
            .lock()
            .unwrap()
            .push(req.tool_call_id.clone());
        let mut answers = HashMap::new();
        answers.insert("Pick a color?".into(), serde_json::json!("Red"));
        Box::pin(async move {
            Ok(tools_rs::builtin::collaboration::QuestionResult::Answers(
                tools_rs::builtin::collaboration::QuestionAnswers {
                    answers,
                    method: Some("enter".into()),
                },
            ))
        })
    });

    let spawn_calls = Arc::new(Mutex::new(Vec::new()));
    let spawn_calls_clone = Arc::clone(&spawn_calls);
    let run_fn: SubagentRunFn = Arc::new(move |_parent, prompt, _signal| {
        spawn_calls_clone.lock().unwrap().push(prompt.clone());
        Box::pin(async move {
            Ok(tools_rs::builtin::collaboration::SubagentResult {
                result: "subagent done".into(),
                usage: None,
            })
        })
    });
    let host = AgentSubagentHost::with_run_fn(run_fn);

    let kaos = Arc::new(Kaos::new(
        detect_environment_from_node(),
        std::env::current_dir().unwrap(),
    ));
    let agent = AgentBuilder::new("l3-test", kaos, Arc::new(NoopEnv))
        .skills_registry(Box::new((*counting_registry).clone()))
        .question_callback(callback)
        .subagent_host(Arc::new(host))
        .build()
        .await
        .unwrap();

    {
        let mode = agent.active_mode();
        let ctx = agent.contexts.get(&mode).expect("context");
        let mut mem = ctx.lock().unwrap();
        mem.append_user_message(
            vec![ContentPart::Text {
                text: "use all collaboration tools".into(),
            }],
            agent_rs::context::types::PromptOrigin::User,
        );
    }

    let dispatcher = DefaultLoopEventDispatcher::new(|_| async { Ok(()) }, None);
    let result = run_turn(RunTurnInput {
        turn_id: "turn-1".into(),
        signal: AbortSignal::new(),
        llm: Box::new(SequencedLlm {
            step: Mutex::new(0),
        }),
        build_messages: {
            let agent = agent.clone();
            Arc::new(move || {
                let agent = agent.clone();
                Box::pin(async move { Ok(agent.context().messages()) })
            })
        },
        dispatch_event: Arc::new(dispatcher),
        tools: Some(agent.tools().loop_tools()),
        hooks: None,
        max_steps: Some(10),
        max_retry_attempts: Some(3),
        record_step_usage: None,
    })
    .await
    .unwrap();

    assert_eq!(result.stop_reason, LoopTurnStopReason::EndTurn);
    assert!(counting_registry
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|n| n == "refactor"));
    assert!(!question_calls.lock().unwrap().is_empty());
    assert!(spawn_calls.lock().unwrap().iter().any(|p| p == "do it"));
}
