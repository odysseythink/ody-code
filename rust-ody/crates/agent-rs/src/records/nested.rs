use kosong_rs::message::{ContentPart, Message};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// PromptOrigin
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum PromptOrigin {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "skill_activation")]
    SkillActivation {
        #[serde(rename = "activationId")]
        activation_id: String,
        #[serde(rename = "skillName")]
        skill_name: String,
        #[serde(rename = "skillArgs", skip_serializing_if = "Option::is_none")]
        skill_args: Option<String>,
        trigger: String,
        #[serde(rename = "skillType", skip_serializing_if = "Option::is_none")]
        skill_type: Option<String>,
        #[serde(rename = "skillPath", skip_serializing_if = "Option::is_none")]
        skill_path: Option<String>,
    },
    #[serde(rename = "injection")]
    Injection { variant: String },
    #[serde(rename = "compaction_summary")]
    CompactionSummary,
    #[serde(rename = "system_trigger")]
    SystemTrigger { name: String },
    #[serde(rename = "background_task")]
    BackgroundTask {
        #[serde(rename = "taskId")]
        task_id: String,
        status: String,
        #[serde(rename = "notificationId")]
        notification_id: String,
    },
    #[serde(rename = "cron_job")]
    CronJob {
        #[serde(rename = "jobId")]
        job_id: String,
        cron: String,
        recurring: bool,
        #[serde(rename = "coalescedCount")]
        coalesced_count: i64,
        stale: bool,
    },
    #[serde(rename = "cron_missed")]
    CronMissed { count: i64 },
    #[serde(rename = "hook_result")]
    HookResult {
        event: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked: Option<bool>,
    },
}

// ---------------------------------------------------------------------------
// ContextMessage
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextMessage {
    #[serde(flatten)]
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<PromptOrigin>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

// ---------------------------------------------------------------------------
// LoopRecordedEvent
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LoopRecordedEvent {
    #[serde(rename = "step.begin")]
    StepBegin {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
    },
    #[serde(rename = "step.end")]
    StepEnd {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        #[serde(rename = "finishReason", skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        #[serde(
            rename = "llmFirstTokenLatencyMs",
            skip_serializing_if = "Option::is_none"
        )]
        llm_first_token_latency_ms: Option<i64>,
        #[serde(
            rename = "llmStreamDurationMs",
            skip_serializing_if = "Option::is_none"
        )]
        llm_stream_duration_ms: Option<i64>,
        #[serde(
            rename = "providerFinishReason",
            skip_serializing_if = "Option::is_none"
        )]
        provider_finish_reason: Option<String>,
        #[serde(rename = "rawFinishReason", skip_serializing_if = "Option::is_none")]
        raw_finish_reason: Option<String>,
    },
    #[serde(rename = "content.part")]
    ContentPartEvent {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
        #[serde(rename = "stepUuid")]
        step_uuid: String,
        part: ContentPart,
    },
    #[serde(rename = "tool.call")]
    ToolCallEvent {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
        #[serde(rename = "stepUuid")]
        step_uuid: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        name: String,
        args: JsonValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<JsonValue>,
    },
    #[serde(rename = "tool.result")]
    ToolResultEvent {
        #[serde(rename = "parentUuid")]
        parent_uuid: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        result: ExecutableToolResult,
    },
}

// ---------------------------------------------------------------------------
// Tool result / update
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExecutableToolOutput {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl ExecutableToolOutput {
    /// Extract text output, returning empty string if it's Parts format.
    pub fn to_text(&self) -> String {
        match self {
            ExecutableToolOutput::Text(s) => s.clone(),
            ExecutableToolOutput::Parts(_) => String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableToolSuccessResult {
    pub output: ExecutableToolOutput,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(rename = "stopTurn", skip_serializing_if = "Option::is_none")]
    pub stop_turn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableToolErrorResult {
    pub output: ExecutableToolOutput,
    #[serde(rename = "isError")]
    pub is_error: bool,
    #[serde(rename = "stopTurn", skip_serializing_if = "Option::is_none")]
    pub stop_turn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExecutableToolResult {
    Success(ExecutableToolSuccessResult),
    Error(ExecutableToolErrorResult),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUpdate {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(rename = "customKind", skip_serializing_if = "Option::is_none")]
    pub custom_kind: Option<String>,
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<JsonValue>,
}

// ---------------------------------------------------------------------------
// Config / Permission / SessionMode / Tools / Usage / Compaction
// ---------------------------------------------------------------------------
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfigUpdateData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(rename = "modelAlias", skip_serializing_if = "Option::is_none")]
    pub model_alias: Option<String>,
    #[serde(rename = "profileName", skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    #[serde(rename = "thinkingLevel", skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Manual,
    Yolo,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    #[serde(rename = "selectedLabel", skip_serializing_if = "Option::is_none")]
    pub selected_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionApprovalResultRecord {
    #[serde(rename = "turnId")]
    pub turn_id: i64,
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    pub action: String,
    #[serde(
        rename = "sessionApprovalRule",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_approval_rule: Option<String>,
    pub result: ApprovalResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionModeKind {
    Plan,
    Design,
    #[serde(rename = "office-hours")]
    OfficeHours,
    #[serde(rename = "game-design")]
    GameDesign,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserToolRegistration {
    pub name: String,
    pub description: String,
    pub parameters: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageRecordScope {
    Session,
    Turn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionBeginData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    pub source: CompactionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionSource {
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionResult {
    pub summary: String,
    pub compacted_count: i64,
    pub tokens_before: i64,
    pub tokens_after: i64,
}

// ---------------------------------------------------------------------------
// ToolStoreUpdate
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolStoreUpdate {
    pub key: String,
    pub value: JsonValue,
}

// ---------------------------------------------------------------------------
// Goal
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[default]
    Active,
    Paused,
    Blocked,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalActor {
    User,
    Model,
    Runtime,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GoalBudgetLimits {
    #[serde(rename = "tokenBudget", skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    #[serde(rename = "turnBudget", skip_serializing_if = "Option::is_none")]
    pub turn_budget: Option<i64>,
    #[serde(rename = "wallClockBudgetMs", skip_serializing_if = "Option::is_none")]
    pub wall_clock_budget_ms: Option<i64>,
}
