use std::pin::Pin;

use futures_util::Stream;
use kosong_rs::message::ContentPart;
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::records::nested::*;

pub type RecordStream<'a> = Pin<Box<dyn Stream<Item = anyhow::Result<AgentRecord>> + Send + 'a>>;
pub type RawRecordStream<'a> = Pin<Box<dyn Stream<Item = anyhow::Result<JsonValue>> + Send + 'a>>;

#[async_trait::async_trait]
pub trait AgentRecordPersistence: Send + Sync {
    async fn read(&self) -> anyhow::Result<RecordStream<'_>>;
    async fn read_raw(&self) -> anyhow::Result<RawRecordStream<'_>>;
    fn append(&mut self, record: AgentRecord);
    fn rewrite(&mut self, records: &[AgentRecord]);
    async fn flush(&mut self) -> anyhow::Result<()>;
    async fn close(&mut self) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentRecord {
    #[serde(rename = "metadata")]
    Metadata {
        time: Option<i64>,
        protocol_version: String,
        created_at: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        app_version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resumed: Option<bool>,
    },
    #[serde(rename = "turn.prompt")]
    TurnPrompt {
        time: Option<i64>,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    },
    #[serde(rename = "turn.steer")]
    TurnSteer {
        time: Option<i64>,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    },
    #[serde(rename = "turn.cancel")]
    TurnCancel {
        time: Option<i64>,
        #[serde(rename = "turnId", skip_serializing_if = "Option::is_none")]
        turn_id: Option<i64>,
    },
    #[serde(rename = "config.update")]
    ConfigUpdate {
        time: Option<i64>,
        #[serde(flatten)]
        update: AgentConfigUpdateData,
    },
    #[serde(rename = "permission.set_mode")]
    PermissionSetMode {
        time: Option<i64>,
        mode: PermissionMode,
    },
    #[serde(rename = "permission.record_approval_result")]
    PermissionRecordApprovalResult {
        time: Option<i64>,
        #[serde(flatten)]
        record: PermissionApprovalResultRecord,
    },
    #[serde(rename = "full_compaction.begin")]
    FullCompactionBegin {
        time: Option<i64>,
        #[serde(flatten)]
        data: CompactionBeginData,
    },
    #[serde(rename = "session_mode.enter")]
    SessionModeEnter {
        time: Option<i64>,
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<SessionModeKind>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    #[serde(rename = "session_mode.cancel")]
    SessionModeCancel {
        time: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    #[serde(rename = "session_mode.exit")]
    SessionModeExit {
        time: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    #[serde(rename = "tools.register_user_tool")]
    ToolsRegisterUserTool {
        time: Option<i64>,
        #[serde(flatten)]
        registration: UserToolRegistration,
    },
    #[serde(rename = "tools.unregister_user_tool")]
    ToolsUnregisterUserTool { time: Option<i64>, name: String },
    #[serde(rename = "tools.set_active_tools")]
    ToolsSetActiveTools {
        time: Option<i64>,
        names: Vec<String>,
    },
    #[serde(rename = "usage.record")]
    UsageRecord {
        time: Option<i64>,
        model: String,
        usage: TokenUsage,
        #[serde(rename = "usageScope", skip_serializing_if = "Option::is_none")]
        usage_scope: Option<UsageRecordScope>,
    },
    #[serde(rename = "full_compaction.cancel")]
    FullCompactionCancel { time: Option<i64> },
    #[serde(rename = "full_compaction.complete")]
    FullCompactionComplete { time: Option<i64> },
    #[serde(rename = "micro_compaction.apply")]
    MicroCompactionApply { time: Option<i64>, cutoff: i64 },
    #[serde(rename = "context.append_message")]
    ContextAppendMessage {
        time: Option<i64>,
        message: ContextMessage,
    },
    #[serde(rename = "context.append_loop_event")]
    ContextAppendLoopEvent {
        time: Option<i64>,
        event: LoopRecordedEvent,
    },
    #[serde(rename = "context.clear")]
    ContextClear { time: Option<i64> },
    #[serde(rename = "context.apply_compaction")]
    ContextApplyCompaction {
        time: Option<i64>,
        #[serde(flatten)]
        result: CompactionResult,
    },
    #[serde(rename = "context.undo")]
    ContextUndo { time: Option<i64>, count: i64 },
    #[serde(rename = "tools.update_store")]
    ToolsUpdateStore {
        time: Option<i64>,
        #[serde(flatten)]
        update: ToolStoreUpdate,
    },
    #[serde(rename = "goal.create")]
    GoalCreate {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        objective: String,
        status: GoalStatus,
        actor: GoalActor,
        #[serde(rename = "budgetLimits")]
        budget_limits: GoalBudgetLimits,
    },
    #[serde(rename = "goal.update")]
    GoalUpdate {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        status: GoalStatus,
        actor: GoalActor,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(rename = "turnsUsed", skip_serializing_if = "Option::is_none")]
        turns_used: Option<i64>,
        #[serde(rename = "tokensUsed", skip_serializing_if = "Option::is_none")]
        tokens_used: Option<i64>,
        #[serde(rename = "wallClockMs", skip_serializing_if = "Option::is_none")]
        wall_clock_ms: Option<i64>,
    },
    #[serde(rename = "goal.account_usage")]
    GoalAccountUsage {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        #[serde(rename = "usageKind")]
        usage_kind: String,
        delta: i64,
        #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
        #[serde(rename = "agentType", skip_serializing_if = "Option::is_none")]
        agent_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(rename = "tokensUsed")]
        tokens_used: i64,
        #[serde(rename = "wallClockMs")]
        wall_clock_ms: i64,
    },
    #[serde(rename = "goal.continuation")]
    GoalContinuation {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        #[serde(rename = "turnsUsed")]
        turns_used: i64,
    },
    #[serde(rename = "goal.clear")]
    GoalClear {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        actor: GoalActor,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl AgentRecord {
    pub fn record_type(&self) -> &'static str {
        match self {
            AgentRecord::Metadata { .. } => "metadata",
            AgentRecord::TurnPrompt { .. } => "turn.prompt",
            AgentRecord::TurnSteer { .. } => "turn.steer",
            AgentRecord::TurnCancel { .. } => "turn.cancel",
            AgentRecord::ConfigUpdate { .. } => "config.update",
            AgentRecord::PermissionSetMode { .. } => "permission.set_mode",
            AgentRecord::PermissionRecordApprovalResult { .. } => {
                "permission.record_approval_result"
            }
            AgentRecord::FullCompactionBegin { .. } => "full_compaction.begin",
            AgentRecord::FullCompactionCancel { .. } => "full_compaction.cancel",
            AgentRecord::FullCompactionComplete { .. } => "full_compaction.complete",
            AgentRecord::MicroCompactionApply { .. } => "micro_compaction.apply",
            AgentRecord::SessionModeEnter { .. } => "session_mode.enter",
            AgentRecord::SessionModeCancel { .. } => "session_mode.cancel",
            AgentRecord::SessionModeExit { .. } => "session_mode.exit",
            AgentRecord::ContextAppendMessage { .. } => "context.append_message",
            AgentRecord::ContextAppendLoopEvent { .. } => "context.append_loop_event",
            AgentRecord::ContextClear { .. } => "context.clear",
            AgentRecord::ContextApplyCompaction { .. } => "context.apply_compaction",
            AgentRecord::ContextUndo { .. } => "context.undo",
            AgentRecord::ToolsRegisterUserTool { .. } => "tools.register_user_tool",
            AgentRecord::ToolsUnregisterUserTool { .. } => "tools.unregister_user_tool",
            AgentRecord::ToolsSetActiveTools { .. } => "tools.set_active_tools",
            AgentRecord::ToolsUpdateStore { .. } => "tools.update_store",
            AgentRecord::UsageRecord { .. } => "usage.record",
            AgentRecord::GoalCreate { .. } => "goal.create",
            AgentRecord::GoalUpdate { .. } => "goal.update",
            AgentRecord::GoalAccountUsage { .. } => "goal.account_usage",
            AgentRecord::GoalContinuation { .. } => "goal.continuation",
            AgentRecord::GoalClear { .. } => "goal.clear",
        }
    }

    pub fn time(&self) -> Option<i64> {
        match self {
            AgentRecord::Metadata { time, .. } => *time,
            AgentRecord::TurnPrompt { time, .. } => *time,
            AgentRecord::TurnSteer { time, .. } => *time,
            AgentRecord::TurnCancel { time, .. } => *time,
            AgentRecord::ConfigUpdate { time, .. } => *time,
            AgentRecord::PermissionSetMode { time, .. } => *time,
            AgentRecord::PermissionRecordApprovalResult { time, .. } => *time,
            AgentRecord::FullCompactionBegin { time, .. } => *time,
            AgentRecord::FullCompactionCancel { time } => *time,
            AgentRecord::FullCompactionComplete { time } => *time,
            AgentRecord::MicroCompactionApply { time, .. } => *time,
            AgentRecord::SessionModeEnter { time, .. } => *time,
            AgentRecord::SessionModeCancel { time, .. } => *time,
            AgentRecord::SessionModeExit { time, .. } => *time,
            AgentRecord::ContextAppendMessage { time, .. } => *time,
            AgentRecord::ContextAppendLoopEvent { time, .. } => *time,
            AgentRecord::ContextClear { time } => *time,
            AgentRecord::ContextApplyCompaction { time, .. } => *time,
            AgentRecord::ContextUndo { time, .. } => *time,
            AgentRecord::ToolsRegisterUserTool { time, .. } => *time,
            AgentRecord::ToolsUnregisterUserTool { time, .. } => *time,
            AgentRecord::ToolsSetActiveTools { time, .. } => *time,
            AgentRecord::ToolsUpdateStore { time, .. } => *time,
            AgentRecord::UsageRecord { time, .. } => *time,
            AgentRecord::GoalCreate { time, .. } => *time,
            AgentRecord::GoalUpdate { time, .. } => *time,
            AgentRecord::GoalAccountUsage { time, .. } => *time,
            AgentRecord::GoalContinuation { time, .. } => *time,
            AgentRecord::GoalClear { time, .. } => *time,
        }
    }

    pub fn with_time(self, new_time: i64) -> Self {
        match self {
            AgentRecord::Metadata {
                protocol_version,
                created_at,
                app_version,
                resumed,
                ..
            } => AgentRecord::Metadata {
                time: Some(new_time),
                protocol_version,
                created_at,
                app_version,
                resumed,
            },
            AgentRecord::TurnPrompt { input, origin, .. } => AgentRecord::TurnPrompt {
                time: Some(new_time),
                input,
                origin,
            },
            AgentRecord::TurnSteer { input, origin, .. } => AgentRecord::TurnSteer {
                time: Some(new_time),
                input,
                origin,
            },
            AgentRecord::TurnCancel { turn_id, .. } => AgentRecord::TurnCancel {
                time: Some(new_time),
                turn_id,
            },
            AgentRecord::ConfigUpdate { update, .. } => AgentRecord::ConfigUpdate {
                time: Some(new_time),
                update,
            },
            AgentRecord::PermissionSetMode { mode, .. } => AgentRecord::PermissionSetMode {
                time: Some(new_time),
                mode,
            },
            AgentRecord::PermissionRecordApprovalResult { record, .. } => {
                AgentRecord::PermissionRecordApprovalResult {
                    time: Some(new_time),
                    record,
                }
            }
            AgentRecord::FullCompactionBegin { data, .. } => AgentRecord::FullCompactionBegin {
                time: Some(new_time),
                data,
            },
            AgentRecord::FullCompactionCancel { .. } => AgentRecord::FullCompactionCancel {
                time: Some(new_time),
            },
            AgentRecord::FullCompactionComplete { .. } => AgentRecord::FullCompactionComplete {
                time: Some(new_time),
            },
            AgentRecord::MicroCompactionApply { cutoff, .. } => AgentRecord::MicroCompactionApply {
                time: Some(new_time),
                cutoff,
            },
            AgentRecord::SessionModeEnter { id, kind, path, .. } => AgentRecord::SessionModeEnter {
                time: Some(new_time),
                id,
                kind,
                path,
            },
            AgentRecord::SessionModeCancel { id, .. } => AgentRecord::SessionModeCancel {
                time: Some(new_time),
                id,
            },
            AgentRecord::SessionModeExit { id, .. } => AgentRecord::SessionModeExit {
                time: Some(new_time),
                id,
            },
            AgentRecord::ContextAppendMessage { message, .. } => {
                AgentRecord::ContextAppendMessage {
                    time: Some(new_time),
                    message,
                }
            }
            AgentRecord::ContextAppendLoopEvent { event, .. } => {
                AgentRecord::ContextAppendLoopEvent {
                    time: Some(new_time),
                    event,
                }
            }
            AgentRecord::ContextClear { .. } => AgentRecord::ContextClear {
                time: Some(new_time),
            },
            AgentRecord::ContextApplyCompaction { result, .. } => {
                AgentRecord::ContextApplyCompaction {
                    time: Some(new_time),
                    result,
                }
            }
            AgentRecord::ContextUndo { count, .. } => AgentRecord::ContextUndo {
                time: Some(new_time),
                count,
            },
            AgentRecord::ToolsRegisterUserTool { registration, .. } => {
                AgentRecord::ToolsRegisterUserTool {
                    time: Some(new_time),
                    registration,
                }
            }
            AgentRecord::ToolsUnregisterUserTool { name, .. } => {
                AgentRecord::ToolsUnregisterUserTool {
                    time: Some(new_time),
                    name,
                }
            }
            AgentRecord::ToolsSetActiveTools { names, .. } => AgentRecord::ToolsSetActiveTools {
                time: Some(new_time),
                names,
            },
            AgentRecord::UsageRecord {
                model,
                usage,
                usage_scope,
                ..
            } => AgentRecord::UsageRecord {
                time: Some(new_time),
                model,
                usage,
                usage_scope,
            },
            AgentRecord::ToolsUpdateStore { update, .. } => AgentRecord::ToolsUpdateStore {
                time: Some(new_time),
                update,
            },
            AgentRecord::GoalCreate {
                goal_id,
                objective,
                status,
                actor,
                budget_limits,
                ..
            } => AgentRecord::GoalCreate {
                time: Some(new_time),
                goal_id,
                objective,
                status,
                actor,
                budget_limits,
            },
            AgentRecord::GoalUpdate {
                goal_id,
                status,
                actor,
                reason,
                turns_used,
                tokens_used,
                wall_clock_ms,
                ..
            } => AgentRecord::GoalUpdate {
                time: Some(new_time),
                goal_id,
                status,
                actor,
                reason,
                turns_used,
                tokens_used,
                wall_clock_ms,
            },
            AgentRecord::GoalAccountUsage {
                goal_id,
                usage_kind,
                delta,
                agent_id,
                agent_type,
                source,
                tokens_used,
                wall_clock_ms,
                ..
            } => AgentRecord::GoalAccountUsage {
                time: Some(new_time),
                goal_id,
                usage_kind,
                delta,
                agent_id,
                agent_type,
                source,
                tokens_used,
                wall_clock_ms,
            },
            AgentRecord::GoalContinuation {
                goal_id,
                turns_used,
                ..
            } => AgentRecord::GoalContinuation {
                time: Some(new_time),
                goal_id,
                turns_used,
            },
            AgentRecord::GoalClear {
                goal_id,
                actor,
                reason,
                ..
            } => AgentRecord::GoalClear {
                time: Some(new_time),
                goal_id,
                actor,
                reason,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use kosong_rs::message::ContentPart;
    use kosong_rs::usage::TokenUsage;

    use super::*;

    #[test]
    fn agent_record_round_trip_metadata() {
        let record = AgentRecord::Metadata {
            time: Some(1_700_000_000_000),
            protocol_version: "1.3".into(),
            created_at: 1_700_000_000_000,
            app_version: Some("0.0.0".into()),
            resumed: Some(false),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"metadata\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_turn_prompt() {
        let record = AgentRecord::TurnPrompt {
            time: Some(1_700_000_000_001),
            input: vec![ContentPart::Text {
                text: "hello".into(),
            }],
            origin: PromptOrigin::User,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"turn.prompt\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_context_append_loop_event_tool_result() {
        let record = AgentRecord::ContextAppendLoopEvent {
            time: Some(1_700_000_000_002),
            event: LoopRecordedEvent::ToolResultEvent {
                parent_uuid: "p1".into(),
                tool_call_id: "tc1".into(),
                result: ExecutableToolResult::Success(ExecutableToolSuccessResult {
                    output: ExecutableToolOutput::Text("ok".into()),
                    is_error: None,
                    stop_turn: None,
                    message: None,
                }),
            },
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"tool.result\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_usage_record() {
        let record = AgentRecord::UsageRecord {
            time: Some(1_700_000_000_003),
            model: "kimi-k2".into(),
            usage: TokenUsage {
                input_other: 10,
                output: 5,
                input_cache_read: 1,
                input_cache_creation: 0,
            },
            usage_scope: Some(UsageRecordScope::Turn),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"usage.record\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_goal_create() {
        let record = AgentRecord::GoalCreate {
            time: Some(1_700_000_000_004),
            goal_id: "g1".into(),
            objective: "ship it".into(),
            status: GoalStatus::Active,
            actor: GoalActor::User,
            budget_limits: GoalBudgetLimits {
                token_budget: Some(1_000_000),
                turn_budget: Some(100),
                wall_clock_budget_ms: None,
            },
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"goal.create\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_deserializes_ts_jsonl() {
        let line = r#"{"type":"metadata","protocol_version":"1.3","created_at":1700000000000,"app_version":"0.0.0"}"#;
        let record: AgentRecord = serde_json::from_str(line).unwrap();
        match record {
            AgentRecord::Metadata {
                protocol_version, ..
            } => assert_eq!(protocol_version, "1.3"),
            _ => panic!("expected metadata"),
        }
    }
}
