use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use kosong_rs::message::ToolCall;
use kosong_rs::provider::AbortSignal;
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::agent_loop::tool_access::ToolAccesses;

pub type ExecutableToolOutput = crate::records::nested::ExecutableToolOutput;
pub type ExecutableToolSuccessResult = crate::records::nested::ExecutableToolSuccessResult;
pub type ExecutableToolErrorResult = crate::records::nested::ExecutableToolErrorResult;
pub type ExecutableToolResult = crate::records::nested::ExecutableToolResult;
pub type ToolUpdate = crate::records::nested::ToolUpdate;

pub struct ExecutableToolContext {
    pub turn_id: String,
    pub tool_call_id: String,
    pub metadata: Option<JsonValue>,
    pub signal: AbortSignal,
    pub on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
}

pub struct RunnableToolExecution {
    pub is_error: Option<bool>,
    pub accesses: Option<ToolAccesses>,
    pub display: Option<JsonValue>,
    pub description: Option<String>,
    pub stop_batch_after_this: Option<bool>,
    pub approval_rule: String,
    pub matches_rule: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    pub execute: Box<
        dyn Fn(
                ExecutableToolContext,
            )
                -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
            + Send
            + Sync,
    >,
}

pub enum ToolExecution {
    Runnable(RunnableToolExecution),
    Error(ExecutableToolErrorResult),
}

#[async_trait::async_trait]
pub trait ExecutableTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn resolve_execution(&self, input: JsonValue) -> Result<ToolExecution, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait UserToolExecutor: Send + Sync {
    async fn execute_user_tool(
        &self,
        name: &str,
        args: JsonValue,
        ctx: ExecutableToolContext,
    ) -> Result<ExecutableToolResult, anyhow::Error>;
}

pub struct UserToolExecutable {
    info: crate::tool::types::ExecutableTool,
    executor: Arc<dyn UserToolExecutor>,
}

impl UserToolExecutable {
    pub fn new(
        info: crate::tool::types::ExecutableTool,
        executor: Arc<dyn UserToolExecutor>,
    ) -> Self {
        Self { info, executor }
    }
}

#[async_trait::async_trait]
impl ExecutableTool for UserToolExecutable {
    fn name(&self) -> &str {
        &self.info.name
    }

    fn description(&self) -> &str {
        &self.info.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.info.parameters.clone()
    }

    async fn resolve_execution(&self, input: JsonValue) -> Result<ToolExecution, anyhow::Error> {
        let name = self.info.name.clone();
        let executor = Arc::clone(&self.executor);
        Ok(ToolExecution::Runnable(RunnableToolExecution {
            is_error: None,
            accesses: None,
            display: None,
            description: Some(name.clone()),
            stop_batch_after_this: None,
            approval_rule: name.clone(),
            matches_rule: None,
            execute: Box::new(move |ctx| {
                let name = name.clone();
                let input = input.clone();
                let executor = Arc::clone(&executor);
                Box::pin(async move { executor.execute_user_tool(&name, input, ctx).await })
            }),
        }))
    }
}

pub type LoopMessageBuilder = Arc<
    dyn Fn() -> Pin<
            Box<
                dyn Future<Output = Result<Vec<kosong_rs::message::Message>, anyhow::Error>> + Send,
            >,
        > + Send
        + Sync,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopStepStopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    Filtered,
    Paused,
    Unknown,
}

pub type LoopTerminalStepStopReason = LoopStepStopReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopTurnStopReason {
    EndTurn,
    MaxTokens,
    Filtered,
    Paused,
    Unknown,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnResult {
    pub stop_reason: LoopTurnStopReason,
    pub steps: u32,
    pub usage: TokenUsage,
}

pub struct LoopStepHookContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
}

pub struct ToolExecutionHookContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub tool_call: &'a ToolCall,
    pub tool: Option<&'a dyn ExecutableTool>,
    pub args: JsonValue,
}

pub struct ResolvedToolExecutionHookContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub tool_call: &'a ToolCall,
    pub tool: Option<&'a dyn ExecutableTool>,
    pub args: JsonValue,
    pub execution: &'a RunnableToolExecution,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizeToolExecutionResult {
    pub block: Option<bool>,
    pub reason: Option<String>,
    pub synthetic_result: Option<ExecutableToolResult>,
    pub execution_metadata: Option<JsonValue>,
}

#[derive(Debug, Clone, Default)]
pub struct PrepareToolExecutionResult {
    pub block: Option<bool>,
    pub reason: Option<String>,
    pub synthetic_result: Option<ExecutableToolResult>,
    pub execution_metadata: Option<JsonValue>,
    pub updated_args: Option<JsonValue>,
}

pub struct FinalizeToolResultContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub tool_call: &'a ToolCall,
    pub tool: Option<&'a dyn ExecutableTool>,
    pub args: JsonValue,
    pub result: ExecutableToolResult,
}

pub struct LoopAfterStepContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub usage: TokenUsage,
    pub stop_reason: LoopStepStopReason,
}

pub struct LoopStoppedStepContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub usage: TokenUsage,
    pub stop_reason: LoopTerminalStepStopReason,
}

#[derive(Debug, Clone, Default)]
pub struct BeforeStepResult {
    pub block: Option<bool>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AfterStepResult {
    pub stop_turn: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct RecordStepUsageResult {
    pub stop_turn: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ShouldContinueAfterStopResult {
    pub continue_: bool,
}

#[async_trait::async_trait]
pub trait BeforeStepHook: Send + Sync {
    async fn before_step(
        &self,
        ctx: LoopStepHookContext<'_>,
    ) -> Result<Option<BeforeStepResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait AfterStepHook: Send + Sync {
    async fn after_step(
        &self,
        ctx: LoopAfterStepContext<'_>,
    ) -> Result<Option<AfterStepResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait PrepareToolExecutionHook: Send + Sync {
    async fn prepare_tool_execution(
        &self,
        ctx: ToolExecutionHookContext<'_>,
    ) -> Result<Option<PrepareToolExecutionResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait AuthorizeToolExecutionHook: Send + Sync {
    async fn authorize_tool_execution(
        &self,
        ctx: ResolvedToolExecutionHookContext<'_>,
    ) -> Result<Option<AuthorizeToolExecutionResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait FinalizeToolResultHook: Send + Sync {
    async fn finalize_tool_result(
        &self,
        ctx: FinalizeToolResultContext<'_>,
    ) -> Result<Option<ExecutableToolResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait ShouldContinueAfterStopHook: Send + Sync {
    async fn should_continue_after_stop(
        &self,
        ctx: LoopStoppedStepContext<'_>,
    ) -> Result<Option<ShouldContinueAfterStopResult>, anyhow::Error>;
}

#[derive(Default)]
pub struct LoopHooks {
    pub before_step: Option<Box<dyn BeforeStepHook>>,
    pub after_step: Option<Box<dyn AfterStepHook>>,
    pub prepare_tool_execution: Option<Box<dyn PrepareToolExecutionHook>>,
    pub authorize_tool_execution: Option<Box<dyn AuthorizeToolExecutionHook>>,
    pub finalize_tool_result: Option<Box<dyn FinalizeToolResultHook>>,
    pub should_continue_after_stop: Option<Box<dyn ShouldContinueAfterStopHook>>,
}

pub struct RunTurnInput {
    pub turn_id: String,
    pub signal: AbortSignal,
    pub llm: Box<dyn crate::agent_loop::llm::Llm>,
    pub build_messages: LoopMessageBuilder,
    pub dispatch_event: Arc<dyn crate::agent_loop::events::LoopEventDispatcher>,
    pub tools: Option<Vec<Arc<dyn ExecutableTool>>>,
    pub hooks: Option<LoopHooks>,
    pub max_steps: Option<u32>,
    pub max_retry_attempts: Option<u32>,
    pub record_step_usage: Option<
        Arc<
            dyn Fn(
                    TokenUsage,
                ) -> Pin<
                    Box<
                        dyn Future<Output = Result<Option<RecordStepUsageResult>, anyhow::Error>>
                            + Send,
                    >,
                > + Send
                + Sync,
        >,
    >,
}

pub struct ExecuteLoopStepDeps<'a> {
    pub turn_id: String,
    pub signal: AbortSignal,
    pub build_messages: LoopMessageBuilder,
    pub dispatch_event: Arc<dyn crate::agent_loop::events::LoopEventDispatcher>,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub tools: Option<Vec<Arc<dyn ExecutableTool>>>,
    pub hooks: Option<&'a LoopHooks>,
    pub log: Option<Arc<dyn tracing::Subscriber + Send + Sync>>,
    pub current_step: u32,
    pub max_retry_attempts: Option<u32>,
    pub record_usage: Arc<
        dyn Fn(
                TokenUsage,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<Option<RecordStepUsageResult>, anyhow::Error>>
                        + Send,
                >,
            > + Send
            + Sync,
    >,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StepResult {
    pub usage: TokenUsage,
    pub stop_reason: LoopStepStopReason,
}

impl From<LoopStepStopReason> for LoopTurnStopReason {
    fn from(value: LoopStepStopReason) -> Self {
        match value {
            LoopStepStopReason::EndTurn => LoopTurnStopReason::EndTurn,
            LoopStepStopReason::MaxTokens => LoopTurnStopReason::MaxTokens,
            LoopStepStopReason::ToolUse => LoopTurnStopReason::Unknown,
            LoopStepStopReason::Filtered => LoopTurnStopReason::Filtered,
            LoopStepStopReason::Paused => LoopTurnStopReason::Paused,
            LoopStepStopReason::Unknown => LoopTurnStopReason::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct EchoExecutor;

    #[async_trait::async_trait]
    impl UserToolExecutor for EchoExecutor {
        async fn execute_user_tool(
            &self,
            name: &str,
            args: JsonValue,
            _ctx: ExecutableToolContext,
        ) -> Result<ExecutableToolResult, anyhow::Error> {
            Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                output: ExecutableToolOutput::Text(format!(
                    "{}:{}",
                    name,
                    args["x"].as_str().unwrap_or("")
                )),
                is_error: None,
                stop_turn: None,
                message: None,
            }))
        }
    }

    #[tokio::test]
    async fn user_tool_executable_routes_to_executor() {
        let info = crate::tool::types::ExecutableTool {
            name: "Echo".into(),
            description: "echo".into(),
            parameters: json!({"type": "object"}),
        };
        let tool = UserToolExecutable::new(info, Arc::new(EchoExecutor));
        let exec = tool.resolve_execution(json!({"x": "hi"})).await.unwrap();
        match exec {
            ToolExecution::Runnable(r) => {
                let ctx = ExecutableToolContext {
                    turn_id: "1".into(),
                    tool_call_id: "c1".into(),
                    signal: kosong_rs::provider::AbortSignal::new(),
                    metadata: None,
                    on_update: None,
                };
                let result = (r.execute)(ctx).await.unwrap();
                assert_eq!(
                    match result {
                        ExecutableToolResult::Success(s) => s.output.to_text(),
                        ExecutableToolResult::Error(e) => e.output.to_text(),
                    },
                    "Echo:hi"
                );
            }
            _ => panic!("expected Runnable"),
        }
    }
}
