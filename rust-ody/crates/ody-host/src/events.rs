use serde::{Deserialize, Serialize};

use crate::error::RpcError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    #[serde(rename = "session.created")]
    SessionCreated {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
        #[serde(rename = "workDir")]
        work_dir: String,
    },
    #[serde(rename = "session.closed")]
    SessionClosed {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
    },
    #[serde(rename = "message")]
    Message {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
        role: String,
        content: String,
    },
    #[serde(rename = "tool.call")]
    ToolCall {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: serde_json::Value,
    },
    #[serde(rename = "tool.result")]
    ToolResult {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
        #[serde(rename = "toolName")]
        tool_name: String,
        result: serde_json::Value,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
        message: String,
    },
    #[serde(rename = "status")]
    Status {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
        status: String,
    },

    // TS-compatible turn events emitted during a prompt/steer round-trip.
    #[serde(rename = "turn.started")]
    TurnStarted {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "turnId")]
        turn_id: i64,
        origin: PromptOrigin,
    },
    #[serde(rename = "assistant.delta")]
    AssistantDelta {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "turnId")]
        turn_id: i64,
        delta: String,
    },
    #[serde(rename = "turn.ended")]
    TurnEnded {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "turnId")]
        turn_id: i64,
        reason: TurnEndReason,
        error: Option<String>,
    },
    #[serde(rename = "agent.status.updated")]
    AgentStatusUpdated {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        model: Option<String>,
        #[serde(rename = "thinkingLevel")]
        thinking_level: Option<String>,
        permission: Option<String>,
        #[serde(rename = "contextTokens")]
        context_tokens: Option<i64>,
        #[serde(rename = "maxContextTokens")]
        max_context_tokens: Option<i64>,
        #[serde(rename = "sessionMode")]
        session_mode: Option<String>,
        #[serde(rename = "sessionModeFilePath")]
        session_mode_file_path: Option<String>,
    },
    #[serde(rename = "thinking.delta")]
    ThinkingDelta {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "turnId")]
        turn_id: i64,
        delta: String,
    },
    #[serde(rename = "background.task.started")]
    BackgroundTaskStarted {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        info: serde_json::Value,
    },
    #[serde(rename = "background.task.terminated")]
    BackgroundTaskTerminated {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        info: serde_json::Value,
    },
    #[serde(rename = "cron.fired")]
    CronFired {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        origin: PromptOrigin,
        prompt: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PromptOrigin {
    User,
    SkillActivation {
        #[serde(rename = "activationId")]
        activation_id: String,
        #[serde(rename = "skillName")]
        skill_name: String,
    },
    Injection,
    CompactionSummary,
    SystemTrigger {
        name: String,
    },
    BackgroundTask {
        #[serde(rename = "taskId")]
        task_id: String,
        status: String,
        #[serde(rename = "notificationId")]
        notification_id: String,
    },
    CronJob {
        #[serde(rename = "jobId")]
        job_id: String,
        cron: String,
        recurring: bool,
        #[serde(rename = "coalescedCount")]
        coalesced_count: i64,
        stale: bool,
    },
    CronMissed {
        count: i64,
    },
    HookResult {
        event: String,
        blocked: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnEndReason {
    Completed,
    Cancelled,
    Failed,
}

#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    async fn request(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError>;
    fn emit(&self, event: AgentEvent);
}
