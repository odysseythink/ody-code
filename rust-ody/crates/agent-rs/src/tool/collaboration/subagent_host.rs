use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};

use tools_rs::builtin::collaboration::{
    SubagentError, SubagentHandle, SubagentHost, SubagentOptions, SubagentResult,
};
use tools_rs::builtin::AbortSignal as ToolsAbortSignal;

use crate::agent::{Agent, AgentType};
use crate::agent_loop::events::DefaultLoopEventDispatcher;
use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use crate::agent_loop::run_turn::run_turn;
use crate::agent_loop::types::{LoopMessageBuilder, RunTurnInput};
use crate::context::types::PromptOrigin;
use crate::turn::types::TurnAgent;
use kosong_rs::message::ContentPart;
use kosong_rs::provider::AbortSignal as KosongAbortSignal;

pub type SubagentRunFn = Arc<
    dyn Fn(
            Weak<Agent>,
            String,
            ToolsAbortSignal,
        ) -> Pin<Box<dyn Future<Output = Result<SubagentResult, SubagentError>> + Send>>
        + Send
        + Sync,
>;

struct SharedLlm(Arc<dyn Llm>);

#[async_trait::async_trait]
impl Llm for SharedLlm {
    fn system_prompt(&self) -> &str {
        self.0.system_prompt()
    }
    fn model_name(&self) -> &str {
        self.0.model_name()
    }
    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        self.0.chat(params).await
    }
}

pub struct AgentSubagentHost {
    parent: Weak<Agent>,
    run_fn: SubagentRunFn,
}

impl AgentSubagentHost {
    pub fn new(parent: Weak<Agent>) -> Self {
        Self {
            parent,
            run_fn: Arc::new(move |parent, prompt, signal| {
                Box::pin(default_run_child_turn(parent, prompt, signal))
            }),
        }
    }

    pub fn with_run_fn(run_fn: SubagentRunFn) -> Self {
        Self {
            parent: Weak::new(),
            run_fn,
        }
    }
}

#[async_trait::async_trait]
impl SubagentHost for AgentSubagentHost {
    async fn spawn(
        &self,
        profile: &str,
        options: SubagentOptions,
    ) -> Result<SubagentHandle, SubagentError> {
        let agent_id = format!("subagent-{}", uuid::Uuid::new_v4());
        let completion = (self.run_fn)(
            self.parent.clone(),
            options.prompt.clone(),
            options.signal.clone(),
        );
        Ok(SubagentHandle {
            agent_id,
            profile_name: profile.into(),
            completion,
        })
    }

    async fn resume(
        &self,
        agent_id: &str,
        options: SubagentOptions,
    ) -> Result<SubagentHandle, SubagentError> {
        let completion = (self.run_fn)(
            self.parent.clone(),
            options.prompt.clone(),
            options.signal.clone(),
        );
        Ok(SubagentHandle {
            agent_id: agent_id.into(),
            profile_name: "coder".into(),
            completion,
        })
    }

    fn get_profile_name(&self, _agent_id: &str) -> Option<String> {
        None
    }
    fn background_task_timeout_ms(&self) -> u64 {
        600_000
    }
    fn cancel_all(&self, _reason: &str) {}
}

async fn default_run_child_turn(
    parent: Weak<Agent>,
    prompt: String,
    signal: ToolsAbortSignal,
) -> Result<SubagentResult, SubagentError> {
    let parent = parent.upgrade().ok_or_else(|| SubagentError::Unavailable)?;
    let child = crate::agent::AgentBuilder::new(
        format!("subagent-{}", uuid::Uuid::new_v4()),
        parent.kaos.clone(),
        parent.environment.clone(),
    )
    .agent_type(AgentType::Sub)
    .provider_resolver(parent.provider_resolver.clone())
    .llm_factory(parent.llm_factory.clone())
    .build()
    .await
    .map_err(|e| SubagentError::Message(format!("failed to build subagent: {}", e)))?;

    {
        let mode = child.active_mode();
        let ctx = child.contexts.get(&mode).expect("active context");
        let mut mem = ctx.lock().unwrap();
        mem.append_user_message(vec![ContentPart::Text { text: prompt }], PromptOrigin::User);
    }

    let kosong_signal = KosongAbortSignal::new();
    let kosong_signal_for_task = kosong_signal.clone();
    let abort_forwarder = {
        let mut watch = signal.clone();
        tokio::spawn(async move {
            while !watch.aborted() {
                tokio::task::yield_now().await;
            }
            kosong_signal_for_task.abort();
        })
    };

    let dispatcher: Arc<dyn crate::agent_loop::events::LoopEventDispatcher> =
        Arc::new(DefaultLoopEventDispatcher::new(|_| async { Ok(()) }, None));
    let child_for_messages = child.clone();
    let build_messages: LoopMessageBuilder = Arc::new(move || {
        let child = child_for_messages.clone();
        Box::pin(async move { Ok(child.context().messages()) })
    });

    let result = run_turn(RunTurnInput {
        turn_id: uuid::Uuid::new_v4().to_string(),
        signal: kosong_signal,
        llm: Box::new(SharedLlm(child.llm_resolver().llm())),
        build_messages,
        dispatch_event: dispatcher,
        tools: Some(child.tools().loop_tools()),
        hooks: None,
        max_steps: Some(10),
        max_retry_attempts: Some(3),
        record_step_usage: None,
    })
    .await;

    abort_forwarder.abort();

    match result {
        Ok(_turn) => Ok(SubagentResult {
            result: "Subagent turn completed.".into(),
            usage: None,
        }),
        Err(e) => Err(SubagentError::Message(format!(
            "subagent turn failed: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tools_rs::builtin::collaboration::SubagentUsage;

    #[tokio::test]
    async fn injected_run_fn_determines_completion() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = Arc::clone(&calls);
        let run_fn: SubagentRunFn = Arc::new(move |_parent, _prompt, _signal| {
            calls_clone.lock().unwrap().push(());
            Box::pin(async move {
                Ok(SubagentResult {
                    result: "done".into(),
                    usage: Some(SubagentUsage {
                        input: 1,
                        output: 2,
                        cache_read: None,
                        cache_write: None,
                    }),
                })
            })
        });

        let host = AgentSubagentHost::with_run_fn(run_fn);
        let handle = host
            .spawn(
                "coder",
                SubagentOptions {
                    parent_tool_call_id: "call_a".into(),
                    prompt: "do it".into(),
                    description: "test".into(),
                    run_in_background: false,
                    signal: ToolsAbortSignal::new(),
                },
            )
            .await
            .unwrap();

        assert!(!handle.agent_id.is_empty());
        assert_eq!(handle.profile_name, "coder");
        let result = handle.completion.await.unwrap();
        assert_eq!(result.result, "done");
        assert_eq!(calls.lock().unwrap().len(), 1);
    }
}
