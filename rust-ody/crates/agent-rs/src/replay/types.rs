use serde::{Deserialize, Serialize};

use crate::records::nested::{ContextMessage, SessionModeKind};

/// Mirrors TS `AgentReplayRecord`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentReplayRecord {
    #[serde(rename = "message")]
    Message {
        message: ContextMessage,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<SessionModeKind>,
    },
    #[serde(rename = "session_mode_updated")]
    SessionModeUpdated {
        enabled: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<SessionModeKind>,
    },
    #[serde(rename = "config_updated")]
    ConfigUpdated { config: serde_json::Value },
    #[serde(rename = "permission_updated")]
    PermissionUpdated { mode: String },
    #[serde(rename = "approval_result")]
    ApprovalResult { record: serde_json::Value },
}

/// Mirrors TS `ReplayBuilder`.
#[derive(Debug, Default)]
pub struct ReplayBuilder {
    records: Vec<AgentReplayRecord>,
    current_mode: Option<SessionModeKind>,
}

impl ReplayBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set current runtime mode. Called by `agent.setContextMode()`.
    pub fn set_mode(&mut self, mode: Option<SessionModeKind>) {
        self.current_mode = mode;
    }

    /// Push a context message record. Only stores during replay (caller checks `restoring`).
    /// Tags messages with the current runtime mode for per-partition filtering.
    pub fn push_message(&mut self, message: &ContextMessage) {
        self.records.push(AgentReplayRecord::Message {
            message: message.clone(),
            mode: self.current_mode,
        });
    }

    /// Push a session-mode enter/exit record.
    pub fn push_session_mode_updated(&mut self, enabled: bool, kind: Option<SessionModeKind>) {
        self.records
            .push(AgentReplayRecord::SessionModeUpdated { enabled, kind });
    }

    /// Push a config update record.
    pub fn push_config_updated(&mut self, config: serde_json::Value) {
        self.records
            .push(AgentReplayRecord::ConfigUpdated { config });
    }

    /// Push a permission mode change record.
    pub fn push_permission_updated(&mut self, mode: &str) {
        self.records.push(AgentReplayRecord::PermissionUpdated {
            mode: mode.to_string(),
        });
    }

    /// Push an approval result record.
    pub fn push_approval_result(&mut self, record: serde_json::Value) {
        self.records
            .push(AgentReplayRecord::ApprovalResult { record });
    }

    /// Remove messages matching the given slice.
    /// Accepts `&[ContextMessage]` because `ContextMessage` does not derive `Hash`.
    pub fn remove_last_messages(&mut self, messages: &[ContextMessage]) {
        self.records.retain(|r| match r {
            AgentReplayRecord::Message { message, .. } => !messages.contains(message),
            _ => true,
        });
    }

    /// Return all stored records.
    pub fn build_result(&self) -> Vec<AgentReplayRecord> {
        self.records.clone()
    }

    /// Return records filtered by a specific runtime mode.
    /// `None` means "normal mode" (no session mode active).
    pub fn build_result_for_mode(&self, mode: Option<SessionModeKind>) -> Vec<AgentReplayRecord> {
        self.records
            .iter()
            .filter(|r| match r {
                AgentReplayRecord::Message { mode: msg_mode, .. } => *msg_mode == mode,
                _ => true,
            })
            .cloned()
            .collect()
    }
}
