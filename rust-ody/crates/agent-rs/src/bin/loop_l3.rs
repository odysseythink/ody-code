use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_rs::agent_loop::events::{LoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::run_turn::run_turn;
use agent_rs::agent_loop::tool_access::ToolAccesses;
use agent_rs::agent_loop::types::{
    ExecutableTool, ExecutableToolContext, ExecutableToolErrorResult, ExecutableToolOutput,
    ExecutableToolResult, ExecutableToolSuccessResult, LoopMessageBuilder, RunTurnInput,
    RunnableToolExecution, ToolExecution, TurnResult,
};
use anyhow::{Context, Error};
use async_trait::async_trait;
use kosong_rs::message::{ContentPart, Message, ToolCall};
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Deserialize)]
struct Fixture {
    #[serde(rename = "turnId")]
    turn_id: String,
    #[serde(rename = "maxSteps")]
    max_steps: Option<u32>,
    #[serde(rename = "maxRetryAttempts")]
    max_retry_attempts: Option<u32>,
    messages: Vec<Message>,
    responses: Vec<MockResponse>,
    #[serde(default)]
    tools: Vec<MockToolDef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MockResponse {
    #[serde(rename = "toolCalls", default)]
    tool_calls: Vec<ToolCall>,
    #[serde(
        rename = "finishReason",
        default,
        deserialize_with = "deserialize_finish_reason"
    )]
    finish_reason: Option<FinishReason>,
    #[serde(rename = "rawFinishReason", default)]
    raw_finish_reason: Option<String>,
    usage: TokenUsage,
}

fn deserialize_finish_reason<'de, D>(deserializer: D) -> Result<Option<FinishReason>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(s) => {
            let value = serde_json::Value::String(s);
            FinishReason::deserialize(value)
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MockToolDef {
    name: String,
    description: String,
    parameters: JsonValue,
    result: MockToolResult,
    #[serde(default)]
    accesses: Option<ToolAccesses>,
    #[serde(rename = "stopBatchAfterThis", default)]
    stop_batch_after_this: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct MockToolResult {
    output: MockOutput,
    #[serde(rename = "isError", default)]
    is_error: Option<bool>,
    #[serde(rename = "stopTurn", default)]
    stop_turn: Option<bool>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum MockOutput {
    Text(String),
    Parts(Vec<ContentPart>),
}

struct MockLlm {
    system_prompt: String,
    model_name: String,
    responses: Vec<MockResponse>,
    index: Mutex<usize>,
}

#[async_trait]
impl Llm for MockLlm {
    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, Error> {
        let idx = {
            let mut guard = self.index.lock().unwrap();
            let i = *guard;
            *guard = guard.wrapping_add(1);
            i
        };
        let response = self
            .responses
            .get(idx)
            .or_else(|| self.responses.last())
            .cloned()
            .unwrap_or_default();
        Ok(LlmChatResponse {
            tool_calls: response.tool_calls,
            provider_finish_reason: response.finish_reason,
            raw_finish_reason: response.raw_finish_reason,
            usage: response.usage,
            stream_timing: None,
        })
    }
}

struct MockTool {
    def: MockToolDef,
}

#[async_trait]
impl ExecutableTool for MockTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn parameters(&self) -> JsonValue {
        self.def.parameters.clone()
    }

    async fn resolve_execution(&self, _input: JsonValue) -> Result<ToolExecution, Error> {
        let output = match &self.def.result.output {
            MockOutput::Text(s) => ExecutableToolOutput::Text(s.clone()),
            MockOutput::Parts(p) => ExecutableToolOutput::Parts(p.clone()),
        };
        let result = if self.def.result.is_error == Some(true) {
            ExecutableToolResult::Error(ExecutableToolErrorResult {
                output,
                is_error: true,
                stop_turn: self.def.result.stop_turn,
                message: self.def.result.message.clone(),
            })
        } else {
            ExecutableToolResult::Success(ExecutableToolSuccessResult {
                output,
                is_error: self.def.result.is_error,
                stop_turn: self.def.result.stop_turn,
                message: self.def.result.message.clone(),
            })
        };
        Ok(ToolExecution::Runnable(RunnableToolExecution {
            is_error: None,
            accesses: self.def.accesses.clone(),
            display: None,
            description: None,
            stop_batch_after_this: self.def.stop_batch_after_this,
            approval_rule: String::new(),
            matches_rule: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let result = result.clone();
                Box::pin(async move { Ok(result) })
                    as std::pin::Pin<
                        Box<
                            dyn std::future::Future<Output = Result<ExecutableToolResult, Error>>
                                + Send,
                        >,
                    >
            }),
        }))
    }
}

#[derive(Clone)]
struct SnapshotDispatcher {
    recorded: Arc<Mutex<Vec<JsonValue>>>,
    live: Arc<Mutex<Vec<JsonValue>>>,
}

impl SnapshotDispatcher {
    fn new() -> Self {
        Self {
            recorded: Arc::new(Mutex::new(Vec::new())),
            live: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn take_snapshot(&self) -> (Vec<JsonValue>, Vec<JsonValue>) {
        (
            self.recorded.lock().unwrap().clone(),
            self.live.lock().unwrap().clone(),
        )
    }
}

#[async_trait]
impl LoopEventDispatcher for SnapshotDispatcher {
    async fn dispatch_recorded(&self, event: LoopRecordedEvent) -> Result<(), Error> {
        if let Ok(value) = serde_json::to_value(&event) {
            self.recorded.lock().unwrap().push(value);
        }
        Ok(())
    }

    fn dispatch_live(&self, event: LoopLiveOnlyEvent) {
        if let Ok(value) = serde_json::to_value(&event) {
            self.live.lock().unwrap().push(value);
        }
    }
}

#[derive(Debug, Serialize)]
struct Snapshot {
    #[serde(rename = "turnResult")]
    turn_result: TurnResult,
    #[serde(rename = "recordedEvents")]
    recorded_events: Vec<JsonValue>,
    #[serde(rename = "liveEvents")]
    live_events: Vec<JsonValue>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(PathBuf::from)
        .context("usage: loop_l3 <fixture.json>")?;

    let file = File::open(&fixture_path)
        .with_context(|| format!("failed to open {}", fixture_path.display()))?;
    let fixture: Fixture = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse {}", fixture_path.display()))?;

    let llm = Box::new(MockLlm {
        system_prompt: "fixture".into(),
        model_name: "mock".into(),
        responses: fixture.responses,
        index: Mutex::new(0),
    });

    let tools: Option<Vec<Arc<dyn ExecutableTool>>> = if fixture.tools.is_empty() {
        None
    } else {
        Some(
            fixture
                .tools
                .into_iter()
                .map(|def| Arc::new(MockTool { def }) as Arc<dyn ExecutableTool>)
                .collect(),
        )
    };

    let snapshot_dispatcher = SnapshotDispatcher::new();
    let dispatcher: Arc<dyn LoopEventDispatcher> = Arc::new(snapshot_dispatcher.clone());
    let messages = fixture.messages;
    let build_messages: LoopMessageBuilder = Arc::new(move || {
        let msgs = messages.clone();
        Box::pin(async move { Ok(msgs) })
    });

    let turn_result = run_turn(RunTurnInput {
        turn_id: fixture.turn_id,
        signal: AbortSignal::new(),
        llm,
        build_messages,
        dispatch_event: dispatcher,
        tools,
        hooks: None,
        max_steps: fixture.max_steps,
        max_retry_attempts: fixture.max_retry_attempts,
        record_step_usage: None,
    })
    .await?;

    let (recorded_events, live_events) = snapshot_dispatcher.take_snapshot();

    let snapshot = Snapshot {
        turn_result,
        recorded_events,
        live_events,
    };

    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}
