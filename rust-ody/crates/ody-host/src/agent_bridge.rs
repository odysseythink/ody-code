use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use kosong_rs::provider::{AbortSignal, ChatProvider, ModelCapability};

use agent_rs::agent::{AgentEnvironment, LlmFactory, ProviderResolver};
use agent_rs::config::state::ResolvedRuntimeProvider;
use agent_rs::config::types::ProviderConfig as AgentProviderConfig;
use agent_rs::permission::types::ApprovalRequest as AgentApprovalRequest;
use agent_rs::records::nested::{
    ApprovalResponse as AgentApprovalResponse, PromptOrigin as AgentPromptOrigin,
};
use agent_rs::turn::kosong_llm::{KosongLLM, KosongLLMConfig};
use agent_rs::turn::types::{AgentEvent as AgentRsEvent, HookResult, StopHookBlock};

use crate::config::ProviderConfig as HostProviderConfig;
use crate::error::RpcError;
use crate::events::{
    AgentEvent as HostEvent, EventSink, PromptOrigin as HostPromptOrigin, TurnEndReason,
};

use agent_rs::agent::Agent;
use agent_rs::agent_loop::types::ExecutableTool as LoopExecutableTool;
use agent_rs::tool::bridge::ToolBridge;
use agent_rs::tool::types::{BuiltinToolProvisionContext, BuiltinToolsProvider};

/// Resolves host provider configuration to agent-rs ResolvedRuntimeProvider.
pub struct HostProviderResolver {
    host_provider: HostProviderConfig,
}

impl HostProviderResolver {
    pub fn new(host_provider: HostProviderConfig) -> Self {
        Self { host_provider }
    }

    fn parse_alias(&self, raw: &str) -> (String, String) {
        let (provider_id, model) = if let Some(idx) = raw.find('/') {
            (raw[..idx].to_string(), raw[idx + 1..].to_string())
        } else if let Some(idx) = raw.find(':') {
            (raw[..idx].to_string(), raw[idx + 1..].to_string())
        } else {
            (self.host_provider.provider_id.clone(), raw.to_string())
        };
        let model = if model.trim().is_empty() {
            self.host_provider
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".into())
        } else {
            model
        };
        (provider_id, model)
    }

    fn provider_type_from_id(id: &str) -> Option<kosong_rs::provider::ProviderType> {
        serde_json::from_value(serde_json::Value::String(id.to_string())).ok()
    }
}

impl ProviderResolver for HostProviderResolver {
    fn default_model(&self) -> Option<String> {
        self.host_provider.default_model.clone()
    }

    fn resolve(&self, model_alias: &str) -> Option<ResolvedRuntimeProvider> {
        let (provider_id, model) = self.parse_alias(model_alias);
        let provider_type = Self::provider_type_from_id(&provider_id)?;
        let capability = kosong_rs::resolve_model_capability(&provider_id, &model)
            .unwrap_or_else(kosong_rs::provider::ModelCapability::unknown);
        Some(ResolvedRuntimeProvider {
            provider_name: provider_id,
            provider: AgentProviderConfig {
                r#type: provider_type,
                model,
                api_key: Some(self.host_provider.api_key.clone()).filter(|k| !k.is_empty()),
                base_url: self.host_provider.base_url.clone(),
                default_headers: None,
            },
            model_capabilities: capability,
        })
    }

    fn thinking_config(&self) -> Option<agent_rs::config::thinking::ThinkingConfig> {
        None
    }
}

/// Wraps a host-created ChatProvider into a KosongLLM via agent-rs.
pub struct HostLlmFactory;

impl LlmFactory for HostLlmFactory {
    fn create(
        &self,
        provider: Box<dyn ChatProvider>,
        model_name: String,
        system_prompt: String,
        capability: Option<ModelCapability>,
    ) -> Arc<dyn agent_rs::agent_loop::llm::Llm> {
        Arc::new(KosongLLM::new(KosongLLMConfig {
            provider,
            model_name,
            system_prompt,
            capability,
            completion_budget_config: None,
        }))
    }
}

/// Bridges agent-rs events/approval/hooks/telemetry/log to the host EventSink.
#[derive(Clone)]
pub struct HostAgentEnvironment {
    pub session_id: String,
    pub agent_id: String,
    pub sink: Arc<dyn EventSink>,
}

#[async_trait]
impl AgentEnvironment for HostAgentEnvironment {
    fn emit_event(&self, event: AgentRsEvent) {
        self.sink.emit(map_agent_event(
            self.session_id.clone(),
            self.agent_id.clone(),
            event,
        ));
    }

    async fn request_approval(
        &self,
        req: &AgentApprovalRequest,
        _signal: AbortSignal,
    ) -> Result<AgentApprovalResponse, anyhow::Error> {
        let payload =
            serde_json::to_vec(req).map_err(|e| anyhow::anyhow!("serialize approval: {e}"))?;
        let bytes = self
            .sink
            .request("requestApproval", payload)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let decision: serde_json::Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| serde_json::json!({ "decision": "cancelled" }));
        Ok(AgentApprovalResponse {
            decision: decision
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("cancelled")
                .to_string(),
            scope: decision
                .get("scope")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            feedback: decision
                .get("feedback")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            selected_label: decision
                .get("selectedLabel")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    fn fire_hook_pre_tool_use(
        &self,
        tool_name: &str,
        tool_input: serde_json::Value,
        tool_call_id: &str,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        let tool_name = tool_name.to_string();
        let tool_call_id = tool_call_id.to_string();
        Box::pin(async move {
            let payload = serde_json::json!({
                "toolName": tool_name,
                "toolInput": tool_input,
                "toolCallId": tool_call_id,
            });
            let _ = sink
                .request("fireHook.preToolUse", serde_json::to_vec(&payload)?)
                .await;
            Ok(None)
        })
    }

    fn fire_hook_permission_request(&self, tool_name: &str, data: serde_json::Value) {
        let sink = Arc::clone(&self.sink);
        let tool_name = tool_name.to_string();
        tokio::spawn(async move {
            let _ = sink
                .request(
                    "fireHook.permissionRequest",
                    serde_json::to_vec(&serde_json::json!({
                        "toolName": tool_name,
                        "data": data,
                    }))
                    .unwrap_or_default(),
                )
                .await;
        });
    }

    fn fire_hook_permission_result(&self, tool_name: &str, data: serde_json::Value) {
        let sink = Arc::clone(&self.sink);
        let tool_name = tool_name.to_string();
        tokio::spawn(async move {
            let _ = sink
                .request(
                    "fireHook.permissionResult",
                    serde_json::to_vec(&serde_json::json!({
                        "toolName": tool_name,
                        "data": data,
                    }))
                    .unwrap_or_default(),
                )
                .await;
        });
    }

    fn fire_hook_user_prompt_submit(
        &self,
        input: Vec<kosong_rs::message::ContentPart>,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HookResult>, anyhow::Error>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        Box::pin(async move {
            let payload = serde_json::json!({ "input": input });
            let _ = sink
                .request("fireHook.userPromptSubmit", serde_json::to_vec(&payload)?)
                .await;
            Ok(vec![])
        })
    }

    fn fire_hook_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<StopHookBlock>, anyhow::Error>> + Send + '_>>
    {
        let sink = Arc::clone(&self.sink);
        Box::pin(async move {
            let payload = serde_json::json!({});
            let _ = sink
                .request("fireHook.stop", serde_json::to_vec(&payload)?)
                .await;
            Ok(None)
        })
    }

    fn fire_and_forget_hook(&self, event: &str, data: serde_json::Value) {
        let sink = Arc::clone(&self.sink);
        let event = event.to_string();
        tokio::spawn(async move {
            let _ = sink
                .request(
                    "fireAndForgetHook",
                    serde_json::to_vec(&serde_json::json!({
                        "event": event,
                        "data": data,
                    }))
                    .unwrap_or_default(),
                )
                .await;
        });
    }

    fn trigger_hook(
        &self,
        event: &str,
        data: serde_json::Value,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        let event = event.to_string();
        Box::pin(async move {
            let payload = serde_json::json!({ "event": event, "data": data });
            let _ = sink
                .request("triggerHook", serde_json::to_vec(&payload)?)
                .await;
            Ok(())
        })
    }

    fn track_telemetry(&self, event: &str, properties: serde_json::Value) {
        let sink = Arc::clone(&self.sink);
        let event = event.to_string();
        tokio::spawn(async move {
            let _ = sink
                .request(
                    "telemetry.track",
                    serde_json::to_vec(&serde_json::json!({
                        "event": event,
                        "properties": properties,
                    }))
                    .unwrap_or_default(),
                )
                .await;
        });
    }

    fn log_debug(&self, msg: &str, data: serde_json::Value) {
        tracing::debug!(target: "agent", message = msg, data = %data);
    }
    fn log_warn(&self, msg: &str, data: serde_json::Value) {
        tracing::warn!(target: "agent", message = msg, data = %data);
    }
    fn log_error(&self, msg: &str, data: serde_json::Value) {
        tracing::error!(target: "agent", message = msg, data = %data);
    }
}

// ---------------------------------------------------------------------------
// Event mapping helpers
// ---------------------------------------------------------------------------

fn map_origin(origin: AgentPromptOrigin) -> HostPromptOrigin {
    match origin {
        AgentPromptOrigin::User => HostPromptOrigin::User,
        AgentPromptOrigin::SkillActivation {
            activation_id,
            skill_name,
            ..
        } => HostPromptOrigin::SkillActivation {
            activation_id,
            skill_name,
        },
        AgentPromptOrigin::Injection { .. } => HostPromptOrigin::Injection,
        AgentPromptOrigin::CompactionSummary => HostPromptOrigin::CompactionSummary,
        AgentPromptOrigin::SystemTrigger { name } => HostPromptOrigin::SystemTrigger { name },
        AgentPromptOrigin::BackgroundTask {
            task_id,
            status,
            notification_id,
        } => HostPromptOrigin::BackgroundTask {
            task_id,
            status,
            notification_id,
        },
        AgentPromptOrigin::CronJob {
            job_id,
            cron,
            recurring,
            coalesced_count,
            stale,
        } => HostPromptOrigin::CronJob {
            job_id,
            cron,
            recurring,
            coalesced_count,
            stale,
        },
        AgentPromptOrigin::CronMissed { count } => HostPromptOrigin::CronMissed { count },
        AgentPromptOrigin::HookResult { event, blocked } => {
            HostPromptOrigin::HookResult { event, blocked }
        }
    }
}

fn map_agent_event(session_id: String, agent_id: String, event: AgentRsEvent) -> HostEvent {
    use agent_rs::turn::types::AgentEvent::*;
    match event {
        TurnStarted { turn_id, origin } => HostEvent::TurnStarted {
            session_id,
            agent_id,
            turn_id,
            origin: map_origin(origin),
        },
        TurnEnded(te) => HostEvent::TurnEnded {
            session_id,
            agent_id,
            turn_id: te.turn_id,
            reason: match te.reason {
                agent_rs::turn::types::TurnEndedReason::Completed => TurnEndReason::Completed,
                agent_rs::turn::types::TurnEndedReason::Cancelled => TurnEndReason::Cancelled,
                agent_rs::turn::types::TurnEndedReason::Failed => TurnEndReason::Failed,
            },
            error: te.error.map(|e| e.message),
        },
        AssistantDelta { turn_id, delta } => HostEvent::AssistantDelta {
            session_id,
            agent_id,
            turn_id,
            delta,
        },
        ThinkingDelta { turn_id, delta } => HostEvent::ThinkingDelta {
            session_id,
            agent_id,
            turn_id,
            delta,
        },
        ToolCallStarted {
            turn_id: _,
            tool_call_id: _,
            name,
            args,
            ..
        } => HostEvent::ToolCall {
            session_id,
            agent_id: Some(agent_id),
            tool_name: name,
            args,
        },
        ToolResult {
            turn_id: _,
            tool_call_id,
            output,
            is_error: _,
        } => {
            let result = match output {
                agent_rs::records::nested::ExecutableToolOutput::Text(t) => {
                    serde_json::json!(t)
                }
                agent_rs::records::nested::ExecutableToolOutput::Parts(parts) => {
                    serde_json::to_value(parts).unwrap_or_default()
                }
            };
            HostEvent::ToolResult {
                session_id,
                agent_id: Some(agent_id),
                tool_name: tool_call_id,
                result,
            }
        }
        BackgroundTaskStarted { info } => HostEvent::BackgroundTaskStarted {
            session_id,
            agent_id,
            info: serde_json::to_value(info).unwrap_or_default(),
        },
        BackgroundTaskTerminated { info } => HostEvent::BackgroundTaskTerminated {
            session_id,
            agent_id,
            info: serde_json::to_value(info).unwrap_or_default(),
        },
        CronFired { origin, prompt } => HostEvent::CronFired {
            session_id,
            agent_id,
            origin: map_origin(origin),
            prompt,
        },
        _ => HostEvent::Status {
            session_id,
            agent_id: Some(agent_id),
            status: serde_json::to_string(&event).unwrap_or_else(|_| "agent-rs-event".into()),
        },
    }
}

// ---------------------------------------------------------------------------
// Host adapters: bridge agent-rs state to tools-rs builtin tool interfaces
// ---------------------------------------------------------------------------

pub struct AgentToolStore {
    agent: Weak<Agent>,
}

impl AgentToolStore {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }
}

impl tools_rs::store::ToolStore for AgentToolStore {
    fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.agent
            .upgrade()
            .and_then(|a| a.tools.lock().unwrap().store_data().get(key).cloned())
    }

    fn set(&self, key: &str, value: serde_json::Value) {
        if let Some(a) = self.agent.upgrade() {
            a.tools.lock().unwrap().update_store(key, value);
        }
    }
}

// ---- HostBackgroundManager: bridges agent-rs BackgroundManager to tools-rs trait ----

pub struct HostBackgroundManager {
    agent: Weak<Agent>,
}

impl HostBackgroundManager {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }

    fn manager(&self) -> Option<Arc<agent_rs::background::manager::BackgroundManager>> {
        self.agent.upgrade()?.background.lock().unwrap().clone()
    }
}

fn map_background_status(
    s: agent_rs::background::types::BackgroundTaskStatus,
) -> tools_rs::builtin::background::BackgroundTaskStatus {
    use agent_rs::background::types::BackgroundTaskStatus as A;
    use tools_rs::builtin::background::BackgroundTaskStatus as T;
    match s {
        A::Running => T::Running,
        A::Completed => T::Completed,
        A::Failed => T::Failed,
        A::TimedOut => T::TimedOut,
        A::Killed => T::Killed,
        A::Lost => T::Lost,
    }
}

fn map_background_info(
    info: agent_rs::background::types::BackgroundTaskInfo,
) -> tools_rs::builtin::background::BackgroundTaskInfoData {
    tools_rs::builtin::background::BackgroundTaskInfoData {
        task_id: info.id.0,
        description: info.description,
        status: map_background_status(info.status),
        started_at: info.started_at.timestamp_millis() as u64,
        ended_at: info.finished_at.map(|t| t.timestamp_millis() as u64),
        stop_reason: info.stop_reason,
        terminal_notification_suppressed: info.terminal_notification_suppressed.unwrap_or(false),
    }
}

impl tools_rs::builtin::background::BackgroundManager for HostBackgroundManager {
    fn list(
        &self,
        active_only: bool,
        limit: Option<usize>,
    ) -> Vec<tools_rs::builtin::background::BackgroundTaskInfoData> {
        self.manager()
            .map(|m| {
                m.list(active_only, limit)
                    .into_iter()
                    .map(map_background_info)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_task(
        &self,
        task_id: &str,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskInfoData> {
        self.manager()
            .and_then(|m| m.get_task(task_id).map(map_background_info))
    }

    fn get_output_snapshot(
        &self,
        task_id: &str,
        max_preview_bytes: usize,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskOutputSnapshot> {
        let info = self.manager().and_then(|m| m.get_task(task_id))?;
        let output = info.output_snapshot.unwrap_or_default();
        let bytes = output.as_bytes();
        let truncated = bytes.len() > max_preview_bytes;
        let preview = if truncated {
            String::from_utf8_lossy(&bytes[..max_preview_bytes]).to_string()
        } else {
            output.clone()
        };
        Some(
            tools_rs::builtin::background::BackgroundTaskOutputSnapshot {
                output_path: None,
                output_size_bytes: bytes.len() as u64,
                preview_bytes: max_preview_bytes,
                truncated,
                full_output_available: !truncated,
                preview,
            },
        )
    }

    fn stop(
        &self,
        task_id: &str,
        reason: Option<String>,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskStopResult> {
        let mgr = self.manager()?;
        let rt = tokio::runtime::Handle::current();
        let info = rt.block_on(mgr.stop(task_id, reason))?;
        Some(tools_rs::builtin::background::BackgroundTaskStopResult {
            task_id: info.id.0,
            status: map_background_status(info.status),
        })
    }

    fn wait(
        &self,
        task_id: &str,
        timeout_ms: Option<u64>,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskInfoData> {
        let mgr = self.manager()?;
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(30_000));
        let rt = tokio::runtime::Handle::current();
        let info = rt.block_on(mgr.wait(task_id, timeout))?;
        Some(map_background_info(info))
    }

    fn suppress_terminal_notification(&self, _task_id: &str) {
        // agent-rs BackgroundManager does not expose a public suppress interface; no-op.
    }
}

// ---- HostCronManager: bridges agent-rs CronManager to tools-rs trait ----

pub struct HostCronManager {
    agent: Weak<Agent>,
}

impl HostCronManager {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }

    fn manager(&self) -> Option<Arc<agent_rs::cron::manager::CronManager>> {
        self.agent.upgrade()?.cron.lock().unwrap().clone()
    }
}

fn map_cron_task(t: agent_rs::cron::task::CronTask) -> tools_rs::builtin::cron::CronTask {
    tools_rs::builtin::cron::CronTask {
        id: t.id,
        cron: t.cron,
        prompt: t.prompt,
        created_at: t.created_at as u64,
        recurring: t.recurring.unwrap_or(true),
        last_fired_at: t.last_fired_at.map(|v| v as u64),
    }
}

fn map_cron_init(
    init: tools_rs::builtin::cron::SessionCronTaskInit,
) -> agent_rs::cron::task::CronTaskInit {
    agent_rs::cron::task::CronTaskInit {
        cron: init.cron,
        prompt: init.prompt,
        recurring: Some(init.recurring),
    }
}

impl tools_rs::builtin::cron::CronManager for HostCronManager {
    fn add_task(
        &self,
        init: tools_rs::builtin::cron::SessionCronTaskInit,
    ) -> tools_rs::builtin::cron::CronTask {
        let mgr = self.manager().expect("cron manager not set");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        map_cron_task(mgr.add_task(map_cron_init(init), now))
    }

    fn remove_tasks(&self, ids: &[String]) -> Vec<String> {
        self.manager()
            .map(|m| m.remove_tasks(ids))
            .unwrap_or_default()
    }

    fn list_tasks(&self) -> Vec<tools_rs::builtin::cron::CronTask> {
        self.manager()
            .map(|m| {
                m.store
                    .lock()
                    .unwrap()
                    .list()
                    .into_iter()
                    .map(map_cron_task)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_task(&self, id: &str) -> Option<tools_rs::builtin::cron::CronTask> {
        self.manager().and_then(|m| {
            m.store
                .lock()
                .unwrap()
                .get(id)
                .map(|t| map_cron_task(t.clone()))
        })
    }

    fn get_next_fire_for_task(&self, task_id: &str) -> Option<u64> {
        self.manager()
            .and_then(|m| m.next_fire_for_task(task_id).map(|v| v as u64))
    }

    fn is_stale(&self, task: &tools_rs::builtin::cron::CronTask) -> bool {
        self.manager()
            .map(|m| {
                m.is_stale(&agent_rs::cron::task::CronTask {
                    id: task.id.clone(),
                    cron: task.cron.clone(),
                    prompt: task.prompt.clone(),
                    created_at: task.created_at as i64,
                    recurring: Some(task.recurring),
                    last_fired_at: task.last_fired_at.map(|v| v as i64),
                })
            })
            .unwrap_or(false)
    }

    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

// ---- HostCheckpointCoordinator: stub that reports success ----

pub struct HostCheckpointCoordinator {
    agent: Weak<Agent>,
}

impl HostCheckpointCoordinator {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }
}

impl tools_rs::builtin::checkpoint::CheckpointCoordinator for HostCheckpointCoordinator {
    fn checkpoint_now(&self) -> Result<(), tools_rs::builtin::checkpoint::CheckpointError> {
        let _ = self.agent;
        Ok(())
    }
}

// ---- HostGoalStore: in-memory stub; errors when goal command is disabled ----

pub struct HostGoalStore {
    enabled: bool,
    state: std::sync::Mutex<Option<tools_rs::builtin::goal::GoalSnapshot>>,
}

impl HostGoalStore {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            state: std::sync::Mutex::new(None),
        }
    }
}

impl tools_rs::builtin::goal::GoalStore for HostGoalStore {
    fn create_goal(
        &self,
        input: tools_rs::builtin::goal::CreateGoalInput,
    ) -> Result<tools_rs::builtin::goal::GoalSnapshot, tools_rs::builtin::goal::GoalStoreError>
    {
        if !self.enabled {
            return Err(tools_rs::builtin::goal::GoalStoreError::Other(
                "goal command is not enabled".into(),
            ));
        }
        let mut state = self.state.lock().unwrap();
        let objective = input.objective.trim().to_string();
        if objective.is_empty() {
            return Err(tools_rs::builtin::goal::GoalStoreError::ObjectiveEmpty);
        }
        if state.is_some() && input.replace != Some(true) {
            return Err(tools_rs::builtin::goal::GoalStoreError::AlreadyExists);
        }
        let snapshot = tools_rs::builtin::goal::GoalSnapshot {
            goal_id: "host-goal-1".into(),
            objective,
            completion_criterion: input.completion_criterion.filter(|s| !s.trim().is_empty()),
            status: tools_rs::builtin::goal::GoalStatus::Active,
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
            started_by: tools_rs::builtin::goal::GoalActor::Model,
            updated_by: tools_rs::builtin::goal::GoalActor::Model,
            turns_used: 0,
            tokens_used: 0,
            wall_clock_ms: 0,
            budget: tools_rs::builtin::goal::GoalBudgetReport {
                token_budget: None,
                turn_budget: None,
                wall_clock_budget_ms: None,
                remaining_tokens: None,
                remaining_turns: None,
                remaining_wall_clock_ms: None,
                token_budget_reached: false,
                turn_budget_reached: false,
                wall_clock_budget_reached: false,
                over_budget: false,
            },
            terminal_reason: None,
        };
        *state = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn get_goal(&self) -> tools_rs::builtin::goal::GoalToolResult {
        let state = self.state.lock().unwrap();
        tools_rs::builtin::goal::GoalToolResult {
            goal: state.clone(),
        }
    }

    fn set_budget_limits(
        &self,
        _limits: tools_rs::builtin::goal::GoalBudgetLimits,
        _actor: tools_rs::builtin::goal::GoalActor,
    ) -> Result<tools_rs::builtin::goal::GoalSnapshot, tools_rs::builtin::goal::GoalStoreError>
    {
        self.state
            .lock()
            .unwrap()
            .clone()
            .ok_or(tools_rs::builtin::goal::GoalStoreError::NotFound)
    }

    fn resume_goal(
        &self,
        _actor: tools_rs::builtin::goal::GoalActor,
    ) -> Result<tools_rs::builtin::goal::GoalSnapshot, tools_rs::builtin::goal::GoalStoreError>
    {
        self.state
            .lock()
            .unwrap()
            .clone()
            .ok_or(tools_rs::builtin::goal::GoalStoreError::NotFound)
    }

    fn mark_complete(
        &self,
        _actor: tools_rs::builtin::goal::GoalActor,
    ) -> Result<
        Option<tools_rs::builtin::goal::GoalSnapshot>,
        tools_rs::builtin::goal::GoalStoreError,
    > {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut g) = *state {
            g.status = tools_rs::builtin::goal::GoalStatus::Complete;
            Ok(Some(g.clone()))
        } else {
            Err(tools_rs::builtin::goal::GoalStoreError::NotFound)
        }
    }

    fn mark_blocked(
        &self,
        _actor: tools_rs::builtin::goal::GoalActor,
    ) -> Result<
        Option<tools_rs::builtin::goal::GoalSnapshot>,
        tools_rs::builtin::goal::GoalStoreError,
    > {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut g) = *state {
            g.status = tools_rs::builtin::goal::GoalStatus::Blocked;
            Ok(Some(g.clone()))
        } else {
            Err(tools_rs::builtin::goal::GoalStoreError::NotFound)
        }
    }

    fn pause_goal(
        &self,
        _actor: tools_rs::builtin::goal::GoalActor,
    ) -> Result<tools_rs::builtin::goal::GoalSnapshot, tools_rs::builtin::goal::GoalStoreError>
    {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut g) = *state {
            g.status = tools_rs::builtin::goal::GoalStatus::Paused;
            Ok(g.clone())
        } else {
            Err(tools_rs::builtin::goal::GoalStoreError::NotFound)
        }
    }
}

// ---------------------------------------------------------------------------
// HostBuiltinToolsProvider: builds tools-rs builtins conditioned on Agent state
// ---------------------------------------------------------------------------

// Sized wrappers around trait objects so they can be passed to generic tool constructors.

struct DynTestReviewer(Arc<dyn tools_rs::builtin::test_review::TestReviewer>);

#[async_trait::async_trait]
impl tools_rs::builtin::test_review::TestReviewer for DynTestReviewer {
    async fn review_tests(
        &self,
        content: &str,
        reviewer_alias: &str,
        signal: &tools_rs::builtin::AbortSignal,
    ) -> Result<
        tools_rs::builtin::test_review::AdvancedSessionReviewResult,
        tools_rs::builtin::test_review::TestReviewError,
    > {
        self.0.review_tests(content, reviewer_alias, signal).await
    }
}

struct DynE2ETestRunner(Arc<dyn tools_rs::builtin::e2e::E2ETestRunner>);

#[async_trait::async_trait]
impl tools_rs::builtin::e2e::E2ETestRunner for DynE2ETestRunner {
    async fn detect_generator(
        &self,
        project_root: &str,
    ) -> Result<(), tools_rs::builtin::e2e::E2ETestRunnerError> {
        self.0.detect_generator(project_root).await
    }
    async fn analyze_impact(
        &self,
        changed_files: &[String],
        config: &tools_rs::builtin::e2e::E2EConfig,
        project_root: &str,
    ) -> Result<tools_rs::builtin::e2e::E2EImpact, tools_rs::builtin::e2e::E2ETestRunnerError> {
        self.0
            .analyze_impact(changed_files, config, project_root)
            .await
    }
    async fn generate_tests(
        &self,
        tool: &tools_rs::builtin::e2e::AffectedTool,
        changed_files: &[String],
        project_root: &str,
        generated_test_dir: &str,
    ) -> Result<Vec<String>, tools_rs::builtin::e2e::E2ETestRunnerError> {
        self.0
            .generate_tests(tool, changed_files, project_root, generated_test_dir)
            .await
    }
    async fn run_e2e_tests(
        &self,
        test_files: &[String],
        project_root: &str,
        signal: &tools_rs::builtin::AbortSignal,
    ) -> Result<tools_rs::builtin::e2e::E2EResult, tools_rs::builtin::e2e::E2ETestRunnerError> {
        self.0.run_e2e_tests(test_files, project_root, signal).await
    }
}

struct DynBackgroundManager(Arc<dyn tools_rs::builtin::background::BackgroundManager>);

impl tools_rs::builtin::background::BackgroundManager for DynBackgroundManager {
    fn list(
        &self,
        active_only: bool,
        limit: Option<usize>,
    ) -> Vec<tools_rs::builtin::background::BackgroundTaskInfoData> {
        self.0.list(active_only, limit)
    }
    fn get_task(
        &self,
        task_id: &str,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskInfoData> {
        self.0.get_task(task_id)
    }
    fn get_output_snapshot(
        &self,
        task_id: &str,
        max_preview_bytes: usize,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskOutputSnapshot> {
        self.0.get_output_snapshot(task_id, max_preview_bytes)
    }
    fn stop(
        &self,
        task_id: &str,
        reason: Option<String>,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskStopResult> {
        self.0.stop(task_id, reason)
    }
    fn wait(
        &self,
        task_id: &str,
        timeout_ms: Option<u64>,
    ) -> Option<tools_rs::builtin::background::BackgroundTaskInfoData> {
        self.0.wait(task_id, timeout_ms)
    }
    fn suppress_terminal_notification(&self, task_id: &str) {
        self.0.suppress_terminal_notification(task_id)
    }
}

struct DynCronManager(Arc<dyn tools_rs::builtin::cron::CronManager>);

impl tools_rs::builtin::cron::CronManager for DynCronManager {
    fn add_task(
        &self,
        init: tools_rs::builtin::cron::SessionCronTaskInit,
    ) -> tools_rs::builtin::cron::CronTask {
        self.0.add_task(init)
    }
    fn remove_tasks(&self, ids: &[String]) -> Vec<String> {
        self.0.remove_tasks(ids)
    }
    fn list_tasks(&self) -> Vec<tools_rs::builtin::cron::CronTask> {
        self.0.list_tasks()
    }
    fn get_task(&self, id: &str) -> Option<tools_rs::builtin::cron::CronTask> {
        self.0.get_task(id)
    }
    fn get_next_fire_for_task(&self, task_id: &str) -> Option<u64> {
        self.0.get_next_fire_for_task(task_id)
    }
    fn is_stale(&self, task: &tools_rs::builtin::cron::CronTask) -> bool {
        self.0.is_stale(task)
    }
    fn now_ms(&self) -> u64 {
        self.0.now_ms()
    }
}

pub struct HostBuiltinToolsProvider {
    agent: std::sync::Mutex<Option<Weak<Agent>>>,
    kaos: Arc<kaos_rs::kaos::Kaos>,
    workspace: tools_rs::workspace::WorkspaceConfig,
    url_fetcher: Option<Arc<dyn crate::tools::providers::UrlFetcher>>,
    web_searcher: Option<Arc<dyn crate::tools::providers::WebSearchProvider>>,
    e2e_runner: Arc<dyn tools_rs::builtin::e2e::E2ETestRunner>,
    test_reviewer: Arc<dyn tools_rs::builtin::test_review::TestReviewer>,
    design_host: Arc<dyn tools_rs::builtin::visual::DesignMockupHost>,
    idea_context: tools_rs::builtin::idea::MockIdeaReportContext,
    goal_command_enabled: bool,
}

impl HostBuiltinToolsProvider {
    pub fn new(
        kaos: Arc<kaos_rs::kaos::Kaos>,
        workspace: tools_rs::workspace::WorkspaceConfig,
        url_fetcher: Option<Arc<dyn crate::tools::providers::UrlFetcher>>,
        web_searcher: Option<Arc<dyn crate::tools::providers::WebSearchProvider>>,
        e2e_runner: Arc<dyn tools_rs::builtin::e2e::E2ETestRunner>,
        test_reviewer: Arc<dyn tools_rs::builtin::test_review::TestReviewer>,
        design_host: Arc<dyn tools_rs::builtin::visual::DesignMockupHost>,
        idea_context: tools_rs::builtin::idea::MockIdeaReportContext,
        goal_command_enabled: bool,
    ) -> Self {
        Self {
            agent: std::sync::Mutex::new(None),
            kaos,
            workspace,
            url_fetcher,
            web_searcher,
            e2e_runner,
            test_reviewer,
            design_host,
            idea_context,
            goal_command_enabled,
        }
    }

    pub fn set_agent(&self, agent: Weak<Agent>) {
        *self.agent.lock().unwrap() = Some(agent);
    }

    fn bridge(&self, tool: Arc<dyn tools_rs::builtin::BuiltinTool>) -> Arc<dyn LoopExecutableTool> {
        Arc::new(ToolBridge::new(tool))
    }
}

impl BuiltinToolsProvider for HostBuiltinToolsProvider {
    fn provide(&self, ctx: BuiltinToolProvisionContext) -> Vec<Arc<dyn LoopExecutableTool>> {
        use agent_rs::tool::collaboration::{
            AgentBackgroundRegistrar, AgentQuestionProvider, AgentSkillProvider,
        };

        let mut tools: Vec<Arc<dyn LoopExecutableTool>> = Vec::new();
        let kaos = (*self.kaos).clone();

        // Core filesystem / shell / search tools
        tools.push(self.bridge(Arc::new(tools_rs::builtin::read::ReadTool::new(
            kaos.clone(),
            self.workspace.clone(),
        ))));
        tools.push(
            self.bridge(Arc::new(tools_rs::builtin::write::WriteTool::new(
                kaos.clone(),
                self.workspace.clone(),
            ))),
        );
        tools.push(self.bridge(Arc::new(tools_rs::builtin::edit::EditTool::new(
            kaos.clone(),
            self.workspace.clone(),
        ))));
        tools.push(self.bridge(Arc::new(tools_rs::builtin::glob::GlobTool::new(
            kaos.clone(),
            self.workspace.clone(),
        ))));
        tools.push(self.bridge(Arc::new(tools_rs::builtin::grep::GrepTool::new(
            kaos.clone(),
            self.workspace.clone(),
        ))));
        tools.push(self.bridge(Arc::new(tools_rs::builtin::bash::BashTool::new(
            kaos.clone(),
            self.workspace.clone(),
        ))));

        // Web tools (conditional on host-provided fetcher/searcher)
        if ctx.url_fetcher_available {
            if let Some(ref fetcher) = self.url_fetcher {
                tools.push(
                    self.bridge(Arc::new(crate::tools::FetchURLTool::new(Arc::clone(
                        fetcher,
                    )))),
                );
            }
        }
        if ctx.web_searcher_available {
            if let Some(ref searcher) = self.web_searcher {
                tools.push(
                    self.bridge(Arc::new(crate::tools::WebSearchTool::new(Arc::clone(
                        searcher,
                    )))),
                );
            }
        }

        // Quality / idea / visual / e2e / test-review tools
        tools.push(self.bridge(Arc::new(
            tools_rs::builtin::quality::HarvestOdyMarkersTool::new(
                kaos.clone(),
                self.workspace.clone(),
                tools_rs::builtin::grep::GrepTool::new(kaos.clone(), self.workspace.clone()),
                Arc::new(tools_rs::builtin::quality::NoopTelemetryClient),
            ),
        )));
        tools.push(
            self.bridge(Arc::new(tools_rs::builtin::idea::SaveIdeaReportTool::new(
                kaos.clone(),
                self.workspace.clone(),
                self.idea_context.clone(),
            ))),
        );
        tools.push(self.bridge(Arc::new(
            tools_rs::builtin::visual::ShowDesignMockupTool::new(
                kaos.clone(),
                Arc::clone(&self.design_host),
            ),
        )));
        tools.push(self.bridge(Arc::new(
            tools_rs::builtin::test_review::ReviewTestsTool::new(
                kaos.clone(),
                Arc::new(DynTestReviewer(Arc::clone(&self.test_reviewer))),
            ),
        )));
        tools.push(
            self.bridge(Arc::new(tools_rs::builtin::e2e::RunE2ETestsTool::new(
                kaos.clone(),
                tools_rs::builtin::e2e::E2EConfig::default(),
                Arc::new(DynE2ETestRunner(Arc::clone(&self.e2e_runner))),
            ))),
        );

        // Tools that need a live Agent
        let agent_opt = self
            .agent
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|w| w.upgrade());
        let Some(agent) = agent_opt else {
            return tools;
        };

        // TodoList (shares ToolManager store)
        let tool_store = Arc::new(AgentToolStore::new(agent.self_weak.clone()));
        tools.push(
            self.bridge(Arc::new(tools_rs::builtin::todo_list::TodoListTool::new(
                tool_store,
            ))),
        );

        // Checkpoint
        tools.push(self.bridge(Arc::new(
            tools_rs::builtin::checkpoint::CheckpointTool::new(Arc::new(
                HostCheckpointCoordinator::new(agent.self_weak.clone()),
            )),
        )));

        // Collaboration: skill
        if ctx.has_invocable_skills {
            if let Some(registry) = agent.skill_registry.lock().unwrap().clone() {
                let skill_provider =
                    Arc::new(AgentSkillProvider::new(agent.self_weak.clone(), registry));
                tools.push(self.bridge(Arc::new(
                    tools_rs::builtin::collaboration::SkillTool::new(
                        skill_provider,
                        tools_rs::builtin::collaboration::SkillToolOptions::default(),
                    ),
                )));
            }
        }

        // Collaboration: question
        if ctx.rpc_request_question {
            if let Some(callback) = agent.question_callback.lock().unwrap().clone() {
                let question_provider = Arc::new(AgentQuestionProvider::new(callback));
                tools.push(self.bridge(Arc::new(
                    tools_rs::builtin::collaboration::AskUserQuestionTool::new(
                        question_provider,
                        Arc::new(AgentBackgroundRegistrar::new(None)),
                        tools_rs::builtin::collaboration::AskUserQuestionOptions::default(),
                    ),
                )));
            }
        }

        // Collaboration: subagent
        if ctx.subagent_host_available {
            if let Some(host) = agent.subagent_host.lock().unwrap().clone() {
                let registrar = Arc::new(AgentBackgroundRegistrar::new(
                    agent.background.lock().unwrap().clone(),
                ));
                tools.push(self.bridge(Arc::new(
                    tools_rs::builtin::collaboration::AgentTool::new(
                        host,
                        Some(registrar),
                        tools_rs::builtin::collaboration::AgentToolOptions::default(),
                    ),
                )));
            }
        }

        // Background tools
        if ctx.background_available {
            let bg_mgr = Arc::new(DynBackgroundManager(Arc::new(HostBackgroundManager::new(
                agent.self_weak.clone(),
            ))
                as Arc<dyn tools_rs::builtin::background::BackgroundManager>));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::background::task_list::TaskListTool::new(Arc::clone(&bg_mgr)),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::background::task_output::TaskOutputTool::new(Arc::clone(
                    &bg_mgr,
                )),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::background::task_stop::TaskStopTool::new(Arc::clone(&bg_mgr)),
            )));
        }

        // Cron tools
        if ctx.cron_available {
            let cron_mgr = Arc::new(DynCronManager(Arc::new(HostCronManager::new(
                agent.self_weak.clone(),
            ))
                as Arc<dyn tools_rs::builtin::cron::CronManager>));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::cron::cron_create::CronCreateTool::new(Arc::clone(&cron_mgr)),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::cron::cron_delete::CronDeleteTool::new(Arc::clone(&cron_mgr)),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::cron::cron_list::CronListTool::new(Arc::clone(&cron_mgr)),
            )));
        }

        // Session-mode tools
        if let Some(sm_provider) = agent.session_mode_provider.lock().unwrap().clone() {
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::enter_plan_mode::EnterPlanModeTool::new(
                    Arc::clone(&sm_provider),
                ),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::exit_plan_mode::ExitPlanModeTool::new(Arc::clone(
                    &sm_provider,
                )),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::enter_design_mode::EnterDesignModeTool::new(
                    Arc::clone(&sm_provider),
                ),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::exit_design_mode::ExitDesignModeTool::new(
                    Arc::clone(&sm_provider),
                ),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::office_hours::EnterOfficeHoursModeTool::new(
                    Arc::clone(&sm_provider),
                ),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::office_hours::ExitOfficeHoursModeTool::new(
                    Arc::clone(&sm_provider),
                ),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::game_design::EnterGameDesignModeTool::new(
                    Arc::clone(&sm_provider),
                ),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::session_mode::game_design::ExitGameDesignModeTool::new(
                    Arc::clone(&sm_provider),
                ),
            )));
            // Note: a generic SetLanguageTool does not exist in tools-rs; skipped.
        }

        // Goal tools
        if ctx.goal_command_enabled {
            let goal_store: Arc<dyn tools_rs::builtin::goal::GoalStore> =
                Arc::new(HostGoalStore::new(true));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::goal::create_goal::CreateGoalTool::new(Arc::clone(&goal_store)),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::goal::get_goal::GetGoalTool::new(Arc::clone(&goal_store)),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::goal::set_goal_budget::SetGoalBudgetTool::new(Arc::clone(
                    &goal_store,
                )),
            )));
            tools.push(self.bridge(Arc::new(
                tools_rs::builtin::goal::update_goal::UpdateGoalTool::new(
                    Arc::clone(&goal_store),
                    None,
                ),
            )));
        }

        // RequestCodeReviewTool deferred to 4.5.0

        tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use crate::error::RpcError;
    use crate::events::AgentEvent as HostEvent;
    use crate::events::EventSink;
    use std::sync::{Arc, Mutex};

    struct CollectSink(Arc<Mutex<Vec<HostEvent>>>);

    #[async_trait::async_trait]
    impl EventSink for CollectSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            Ok(vec![])
        }
        fn emit(&self, event: HostEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn resolver_splits_model_alias() {
        let resolver = HostProviderResolver::new(ProviderConfig {
            provider_id: "kimi".into(),
            api_key: "ak".into(),
            base_url: None,
            default_model: Some("moonshot-v1".into()),
        });
        let resolved = resolver.resolve("openai/gpt-4o-mini").unwrap();
        assert_eq!(resolved.provider_name, "openai");
        assert_eq!(resolved.provider.model, "gpt-4o-mini");
        assert_eq!(resolved.provider.api_key, Some("ak".into()));
    }

    #[test]
    fn resolver_falls_back_to_host_provider() {
        let resolver = HostProviderResolver::new(ProviderConfig {
            provider_id: "openai".into(),
            api_key: "ak".into(),
            base_url: Some("https://example.com/v1".into()),
            default_model: Some("gpt-4o-mini".into()),
        });
        let resolved = resolver.resolve("gpt-4o").unwrap();
        assert_eq!(resolved.provider_name, "openai");
        assert_eq!(resolved.provider.model, "gpt-4o");
    }

    #[tokio::test]
    async fn environment_wraps_turn_started() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let env = HostAgentEnvironment {
            session_id: "sess-1".into(),
            agent_id: "main".into(),
            sink: Arc::new(CollectSink(Arc::clone(&events))),
        };
        env.emit_event(AgentRsEvent::TurnStarted {
            turn_id: 7,
            origin: AgentPromptOrigin::User,
        });
        let ev = events.lock().unwrap().pop().unwrap();
        match ev {
            HostEvent::TurnStarted {
                session_id,
                agent_id,
                turn_id,
                ..
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(agent_id, "main");
                assert_eq!(turn_id, 7);
            }
            _ => panic!("expected TurnStarted, got {:?}", ev),
        }
    }
}

#[cfg(test)]
mod adapter_tests {
    use super::*;
    use std::sync::Arc;

    fn _tool_store_implements_trait() {
        fn check<T: tools_rs::store::ToolStore>(_: T) {}
        check(AgentToolStore::new(Weak::new()));
    }

    fn _background_manager_implements_trait() {
        fn check<T: tools_rs::builtin::background::BackgroundManager>(_: T) {}
        check(HostBackgroundManager::new(Weak::new()));
    }

    fn _cron_manager_implements_trait() {
        fn check<T: tools_rs::builtin::cron::CronManager>(_: T) {}
        check(HostCronManager::new(Weak::new()));
    }

    fn _checkpoint_coordinator_implements_trait() {
        fn check<T: tools_rs::builtin::checkpoint::CheckpointCoordinator>(_: T) {}
        check(HostCheckpointCoordinator::new(Weak::new()));
    }

    fn _goal_store_implements_trait() {
        fn check<T: tools_rs::builtin::goal::GoalStore>(_: T) {}
        check(HostGoalStore::new(false));
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use agent_rs::agent::{AgentBuilder, AgentEnvironment, AgentType};
    use agent_rs::agent_loop::types::ExecutableTool;
    use agent_rs::tool::types::{BuiltinToolProvisionContext, BuiltinToolsProvider};
    use kaos_rs::kaos::Kaos;
    use std::sync::Arc;

    struct NoopEnv;
    #[async_trait::async_trait]
    impl AgentEnvironment for NoopEnv {
        fn emit_event(&self, _event: agent_rs::turn::types::AgentEvent) {}
        async fn request_approval(
            &self,
            _req: &agent_rs::permission::types::ApprovalRequest,
            _signal: kosong_rs::provider::AbortSignal,
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
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_,
            >,
        > {
            Box::pin(async { Ok(None) })
        }
        fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_user_prompt_submit(
            &self,
            _input: Vec<kosong_rs::message::ContentPart>,
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Vec<agent_rs::turn::types::HookResult>, anyhow::Error>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(vec![]) })
        }
        fn fire_hook_stop_hook(
            &self,
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            Option<agent_rs::turn::types::StopHookBlock>,
                            anyhow::Error,
                        >,
                    > + Send
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
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>,
        > {
            Box::pin(async { Ok(()) })
        }
        fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
        fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
    }

    fn basic_ctx() -> BuiltinToolProvisionContext {
        BuiltinToolProvisionContext {
            agent_type: AgentType::Main,
            model_capabilities: kosong_rs::provider::ModelCapability::unknown(),
            homedir: None,
            goal_command_enabled: false,
            rpc_open_external: false,
            rpc_request_question: false,
            background_available: false,
            cron_available: false,
            has_invocable_skills: false,
            subagent_host_available: false,
            web_searcher_available: false,
            url_fetcher_available: false,
        }
    }

    #[tokio::test]
    async fn provider_returns_core_tools_without_agent_upgrade() {
        let kaos = Arc::new(Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let provider = HostBuiltinToolsProvider::new(
            Arc::clone(&kaos),
            tools_rs::workspace::WorkspaceConfig {
                workspace_dir: std::env::current_dir()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                additional_dirs: vec![],
            },
            None,
            None,
            Arc::new(crate::tools::HostE2ETestRunner::new(Arc::clone(&kaos))),
            Arc::new(crate::tools::LlmTestReviewer::new(
                Arc::new(crate::llm::mock::MockProvider::default()),
                "mock",
            )),
            Arc::new(tools_rs::builtin::visual::MockDesignMockupHost::new(
                false,
                None,
                Ok(tools_rs::builtin::visual::OpenExternalResult {
                    opened: false,
                    error: None,
                }),
            )),
            tools_rs::builtin::idea::MockIdeaReportContext::new(false, chrono::Utc::now()),
            false,
        );
        let tools = provider.provide(basic_ctx());
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Write"));
        assert!(names.contains(&"Bash"));
        assert!(!names.contains(&"TaskList"));
        assert!(!names.contains(&"CronCreate"));
    }
}
