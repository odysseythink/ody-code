use crate::agent_loop::types::RunnableToolExecution;
use crate::records::nested::{ApprovalResponse, PermissionMode};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// PermissionRule
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleScope {
    #[serde(rename = "turn-override")]
    TurnOverride,
    #[serde(rename = "session-runtime")]
    SessionRuntime,
    #[serde(rename = "project")]
    Project,
    #[serde(rename = "user")]
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRule {
    pub decision: PermissionRuleDecision,
    pub scope: PermissionRuleScope,
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// PermissionRuleMatch
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleMatchStrategy {
    #[serde(rename = "tool_name_only")]
    ToolNameOnly,
    #[serde(rename = "matches_rule")]
    MatchesRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRuleMatch {
    pub rule: PermissionRule,
    pub strategy: PermissionRuleMatchStrategy,
    #[serde(rename = "hasRuleArgs")]
    pub has_rule_args: bool,
}

// ---------------------------------------------------------------------------
// PermissionData
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionData {
    pub mode: PermissionMode,
    pub rules: Vec<PermissionRule>,
}

// ---------------------------------------------------------------------------
// ApprovalRequest
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<JsonValue>,
}

// ---------------------------------------------------------------------------
// PermissionDecision & reason
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Approve,
    Deny,
    Ask,
}

pub type PermissionReasonValue = JsonValue;
pub type PermissionDecisionReason = HashMap<String, PermissionReasonValue>;

// ---------------------------------------------------------------------------
// PermissionPolicyContext — mirrors TS ResolvedToolExecutionHookContext
// ---------------------------------------------------------------------------
pub struct PermissionPolicyContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: kosong_rs::provider::AbortSignal,
    pub tool_call: &'a kosong_rs::message::ToolCall,
    pub tool: Option<&'a dyn crate::agent_loop::types::ExecutableTool>,
    pub args: JsonValue,
    pub execution: &'a RunnableToolExecution,
}

// ---------------------------------------------------------------------------
// PermissionPolicyResult — the tagged union
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum PermissionPolicyResult {
    #[serde(rename = "approve")]
    Approve {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<PermissionDecisionReason>,
        #[serde(rename = "executionMetadata", skip_serializing_if = "Option::is_none")]
        execution_metadata: Option<JsonValue>,
    },
    #[serde(rename = "deny")]
    Deny {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "ask")]
    Ask {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<PermissionDecisionReason>,
        #[serde(skip)]
        resolve_approval: Option<fn(&ApprovalResponse) -> Option<Box<PermissionPolicyResolution>>>,
        #[serde(skip)]
        resolve_error: Option<fn(&anyhow::Error) -> Option<Box<PermissionPolicyResolution>>>,
    },
}

impl PartialEq for PermissionPolicyResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                PermissionPolicyResult::Approve {
                    reason: r1,
                    execution_metadata: e1,
                },
                PermissionPolicyResult::Approve {
                    reason: r2,
                    execution_metadata: e2,
                },
            ) => r1 == r2 && e1 == e2,
            (
                PermissionPolicyResult::Deny {
                    reason: r1,
                    message: m1,
                },
                PermissionPolicyResult::Deny {
                    reason: r2,
                    message: m2,
                },
            ) => r1 == r2 && m1 == m2,
            (
                PermissionPolicyResult::Ask { reason: r1, .. },
                PermissionPolicyResult::Ask { reason: r2, .. },
            ) => r1 == r2,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// PermissionPolicyResolution — Approve | Deny | Ask | Result
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub enum PermissionPolicyResolution {
    Approve {
        reason: Option<PermissionDecisionReason>,
        execution_metadata: Option<JsonValue>,
    },
    Deny {
        reason: Option<PermissionDecisionReason>,
        message: Option<String>,
    },
    Ask {
        reason: Option<PermissionDecisionReason>,
        resolve_approval: Option<fn(&ApprovalResponse) -> Option<Box<PermissionPolicyResolution>>>,
        resolve_error: Option<fn(&anyhow::Error) -> Option<Box<PermissionPolicyResolution>>>,
    },
    Result {
        // Wraps a PrepareToolExecutionResult
        inner: crate::agent_loop::types::PrepareToolExecutionResult,
    },
}

impl From<PermissionPolicyResult> for PermissionPolicyResolution {
    fn from(result: PermissionPolicyResult) -> Self {
        match result {
            PermissionPolicyResult::Approve {
                reason,
                execution_metadata,
            } => PermissionPolicyResolution::Approve {
                reason,
                execution_metadata,
            },
            PermissionPolicyResult::Deny { reason, message } => {
                PermissionPolicyResolution::Deny { reason, message }
            }
            PermissionPolicyResult::Ask {
                reason,
                resolve_approval,
                resolve_error,
            } => PermissionPolicyResolution::Ask {
                reason,
                resolve_approval,
                resolve_error,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// PermissionPolicy trait
// ---------------------------------------------------------------------------
#[async_trait::async_trait]
pub trait PermissionPolicy: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate(&self, context: &PermissionPolicyContext<'_>)
        -> Option<PermissionPolicyResolution>;
}
