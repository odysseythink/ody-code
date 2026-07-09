use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::tool_accesses::ToolAccesses;

#[derive(Debug, Clone)]
pub struct AbortSignal {
    pub(crate) flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl PartialEq for AbortSignal {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.flag, &other.flag)
    }
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    pub fn from_inner(inner: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { flag: inner }
    }
    pub fn abort(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn aborted(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ExecutableToolContext {
    pub turn_id: String,
    pub tool_call_id: String,
    pub signal: AbortSignal,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ExecutableToolOutput {
    Text(String),
    Parts(Vec<Value>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableToolResult {
    pub output: ExecutableToolOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_turn: Option<bool>,
}

impl ExecutableToolResult {
    pub fn ok_text(output: String) -> Self {
        Self {
            output: ExecutableToolOutput::Text(output),
            message: None,
            is_error: false,
            stop_turn: None,
        }
    }
    pub fn error_text(output: String, message: String) -> Self {
        Self {
            output: ExecutableToolOutput::Text(output),
            message: Some(message),
            is_error: true,
            stop_turn: None,
        }
    }
    /// Extract text output, returning empty string if it's Parts format.
    pub fn to_text(&self) -> String {
        match &self.output {
            ExecutableToolOutput::Text(s) => s.clone(),
            ExecutableToolOutput::Parts(_) => String::new(),
        }
    }
}

pub type ExecuteFn = Box<
    dyn Fn(ExecutableToolContext) -> Pin<Box<dyn Future<Output = ExecutableToolResult> + Send>>
        + Send
        + Sync,
>;

pub struct ToolExecution {
    pub accesses: ToolAccesses,
    pub description: String,
    pub approval_rule: String,
    pub matches_rule: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    pub display: Option<serde_json::Value>,
    pub execute: ExecuteFn,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error(transparent)]
    PathSecurity(#[from] crate::policies::path_access::PathSecurityError),
}

pub trait BuiltinTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError>;
}

// Re-export tool modules
pub mod background;
pub mod bash;
pub mod collaboration;
pub use collaboration::agent::{AgentTool, AgentToolOptions};
pub use collaboration::ask_user::{AskUserQuestionOptions, AskUserQuestionTool};
pub use collaboration::skill::{SkillTool, SkillToolOptions};
pub mod checkpoint;
pub mod cron;
pub mod e2e;
pub mod edit;
pub mod glob;
pub mod goal;
pub mod grep;
pub mod idea;
pub mod line_endings;
pub mod media;
pub mod quality;
pub mod read;
pub mod session_mode;
pub mod test_review;
pub mod todo_list;
pub mod visual;
pub mod write;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_signal_starts_unaborted() {
        let s = AbortSignal::new();
        assert!(!s.aborted());
    }

    #[test]
    fn abort_signal_reflects_abort_call() {
        let s = AbortSignal::new();
        s.abort();
        assert!(s.aborted());
    }

    #[test]
    fn executable_tool_result_serializes_text_success() {
        let r = ExecutableToolResult::ok_text("hello".into());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["output"], "hello");
        assert!(!json.as_object().unwrap().contains_key("isError"));
    }

    #[test]
    fn executable_tool_result_serializes_error() {
        let r = ExecutableToolResult::error_text("err".into(), "brief".into());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["isError"], true);
        assert_eq!(json["message"], "brief");
    }

    #[test]
    fn context_carries_turn_and_tool_call_id() {
        let ctx = ExecutableToolContext {
            turn_id: "42".into(),
            tool_call_id: "call_abc".into(),
            signal: AbortSignal::new(),
            metadata: None,
        };
        assert_eq!(ctx.turn_id, "42");
        assert_eq!(ctx.tool_call_id, "call_abc");
        assert!(!ctx.signal.aborted());
    }
}
