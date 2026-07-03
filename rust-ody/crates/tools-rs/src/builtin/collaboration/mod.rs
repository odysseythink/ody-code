use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::builtin::AbortSignal;
use crate::builtin::ExecutableToolResult;

pub mod agent;
pub mod ask_user;
pub mod skill;
pub use agent::{AgentTool, AgentToolOptions};
pub use ask_user::{AskUserQuestionOptions, AskUserQuestionTool};
pub use skill::{SkillTool, SkillToolOptions, MAX_SKILL_QUERY_DEPTH};

// ---------------------------------------------------------------------------
// Skill provider
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct SkillInfo {
    pub name: String,
    pub skill_type: Option<String>,
    pub disable_model_invocation: Option<bool>,
    pub hidden_in_modes: Option<Vec<String>>,
    pub content: String,
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillActivationOrigin {
    pub activation_id: String,
    pub skill_name: String,
    pub skill_args: Option<String>,
    pub trigger: String,
    pub skill_type: Option<String>,
    pub skill_path: Option<String>,
    pub skill_source: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SkillError {
    #[error("skill not found")]
    NotFound,
    #[error("model invocation disabled")]
    ModelInvocationDisabled,
    #[error("not an inline skill")]
    NotInline,
    #[error("skill hidden in current mode")]
    HiddenInMode,
}

pub trait SkillProvider: Send + Sync {
    fn get_skill(&self, name: &str) -> Option<SkillInfo>;
    fn record_activation(&self, origin: SkillActivationOrigin) -> Result<(), SkillError>;
    fn render_skill_prompt(&self, skill: &SkillInfo, args: &str) -> String;
    fn current_session_mode(&self) -> Option<String>;
    fn append_system_reminder(
        &self,
        content: String,
        origin: SkillActivationOrigin,
    ) -> Result<(), SkillError>;
}

// ---------------------------------------------------------------------------
// Question provider
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuestionItem {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestionRequest {
    pub turn_id: Option<i64>,
    pub tool_call_id: String,
    pub questions: Vec<QuestionItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestionAnswers {
    pub answers: std::collections::HashMap<String, serde_json::Value>,
    pub method: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuestionResult {
    Dismissed,
    Answers(QuestionAnswers),
}

#[derive(Debug, thiserror::Error)]
pub enum QuestionError {
    #[error("question RPC not implemented")]
    NotImplemented,
    #[error("question aborted")]
    Aborted,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[async_trait::async_trait]
pub trait QuestionProvider: Send + Sync {
    async fn request_question(
        &self,
        req: QuestionRequest,
        signal: &AbortSignal,
    ) -> Result<QuestionResult, QuestionError>;
}

// ---------------------------------------------------------------------------
// Subagent host
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct SubagentOptions {
    pub parent_tool_call_id: String,
    pub prompt: String,
    pub description: String,
    pub run_in_background: bool,
    pub signal: AbortSignal,
}

pub struct SubagentHandle {
    pub agent_id: String,
    pub profile_name: String,
    pub completion: SubagentCompletion,
}

impl std::fmt::Debug for SubagentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubagentHandle")
            .field("agent_id", &self.agent_id)
            .field("profile_name", &self.profile_name)
            .finish()
    }
}

impl Clone for SubagentHandle {
    fn clone(&self) -> Self {
        SubagentHandle {
            agent_id: self.agent_id.clone(),
            profile_name: self.profile_name.clone(),
            completion: Box::pin(std::future::pending::<Result<SubagentResult, SubagentError>>()),
        }
    }
}

impl PartialEq for SubagentHandle {
    fn eq(&self, other: &Self) -> bool {
        self.agent_id == other.agent_id && self.profile_name == other.profile_name
    }
}

pub type SubagentCompletion =
    Pin<Box<dyn Future<Output = Result<SubagentResult, SubagentError>> + Send>>;

#[derive(Debug, Clone, PartialEq)]
pub struct SubagentResult {
    pub result: String,
    pub usage: Option<SubagentUsage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubagentUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum SubagentError {
    #[error("subagent host unavailable")]
    Unavailable,
    #[error("invalid resume/subagent_type combination")]
    InvalidCombination,
    #[error("background agent unavailable")]
    BackgroundUnavailable,
    #[error("{0}")]
    Message(String),
}

#[async_trait::async_trait]
pub trait SubagentHost: Send + Sync {
    async fn spawn(
        &self,
        profile: &str,
        options: SubagentOptions,
    ) -> Result<SubagentHandle, SubagentError>;
    async fn resume(
        &self,
        agent_id: &str,
        options: SubagentOptions,
    ) -> Result<SubagentHandle, SubagentError>;
    fn get_profile_name(&self, agent_id: &str) -> Option<String>;
    fn background_task_timeout_ms(&self) -> u64;
    fn cancel_all(&self, reason: &str);
}

// ---------------------------------------------------------------------------
// Background registrar
// ---------------------------------------------------------------------------

pub type QuestionRunFn = Arc<
    dyn Fn(
            AbortSignal,
        )
            -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

pub type AgentCompletion =
    Pin<Box<dyn Future<Output = Result<SubagentResult, SubagentError>> + Send>>;

#[derive(Debug, Clone, PartialEq)]
pub struct QuestionTaskOptions {
    pub question_count: u32,
    pub tool_call_id: String,
}

pub struct AgentTaskOptions {
    pub timeout_ms: Option<u64>,
    pub agent_id: String,
    pub subagent_type: String,
    pub abort: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for AgentTaskOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentTaskOptions")
            .field("timeout_ms", &self.timeout_ms)
            .field("agent_id", &self.agent_id)
            .field("subagent_type", &self.subagent_type)
            .finish()
    }
}

impl Clone for AgentTaskOptions {
    fn clone(&self) -> Self {
        AgentTaskOptions {
            timeout_ms: self.timeout_ms,
            agent_id: self.agent_id.clone(),
            subagent_type: self.subagent_type.clone(),
            abort: Arc::clone(&self.abort),
        }
    }
}

impl PartialEq for AgentTaskOptions {
    fn eq(&self, other: &Self) -> bool {
        self.timeout_ms == other.timeout_ms
            && self.agent_id == other.agent_id
            && self.subagent_type == other.subagent_type
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum BackgroundError {
    #[error("background manager unavailable")]
    Unavailable,
    #[error("{0}")]
    Message(String),
}

#[async_trait::async_trait]
pub trait BackgroundRegistrar: Send + Sync {
    async fn register_question_task(
        &self,
        description: String,
        run: QuestionRunFn,
        options: QuestionTaskOptions,
    ) -> Result<String, BackgroundError>;

    async fn register_agent_task(
        &self,
        completion: AgentCompletion,
        description: String,
        options: AgentTaskOptions,
    ) -> Result<String, BackgroundError>;
}

// ---------------------------------------------------------------------------
// Mock implementations for object-safety testing
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSkillProvider;
    impl SkillProvider for MockSkillProvider {
        fn get_skill(&self, _name: &str) -> Option<SkillInfo> {
            None
        }
        fn record_activation(&self, _origin: SkillActivationOrigin) -> Result<(), SkillError> {
            Ok(())
        }
        fn render_skill_prompt(&self, _skill: &SkillInfo, _args: &str) -> String {
            String::new()
        }
        fn current_session_mode(&self) -> Option<String> {
            None
        }
        fn append_system_reminder(
            &self,
            _content: String,
            _origin: SkillActivationOrigin,
        ) -> Result<(), SkillError> {
            Ok(())
        }
    }

    struct MockQuestionProvider;
    #[async_trait::async_trait]
    impl QuestionProvider for MockQuestionProvider {
        async fn request_question(
            &self,
            _req: QuestionRequest,
            _signal: &AbortSignal,
        ) -> Result<QuestionResult, QuestionError> {
            Ok(QuestionResult::Dismissed)
        }
    }

    struct MockSubagentHost;
    #[async_trait::async_trait]
    impl SubagentHost for MockSubagentHost {
        async fn spawn(
            &self,
            _profile: &str,
            _options: SubagentOptions,
        ) -> Result<SubagentHandle, SubagentError> {
            Err(SubagentError::Unavailable)
        }
        async fn resume(
            &self,
            _agent_id: &str,
            _options: SubagentOptions,
        ) -> Result<SubagentHandle, SubagentError> {
            Err(SubagentError::Unavailable)
        }
        fn get_profile_name(&self, _agent_id: &str) -> Option<String> {
            None
        }
        fn background_task_timeout_ms(&self) -> u64 {
            600_000
        }
        fn cancel_all(&self, _reason: &str) {}
    }

    struct MockBackgroundRegistrar;
    #[async_trait::async_trait]
    impl BackgroundRegistrar for MockBackgroundRegistrar {
        async fn register_question_task(
            &self,
            _description: String,
            _run: QuestionRunFn,
            _options: QuestionTaskOptions,
        ) -> Result<String, BackgroundError> {
            Ok("question-12345678".into())
        }
        async fn register_agent_task(
            &self,
            _completion: AgentCompletion,
            _description: String,
            _options: AgentTaskOptions,
        ) -> Result<String, BackgroundError> {
            Ok("agent-12345678".into())
        }
    }

    // Verify traits are object-safe (can be boxed)
    #[test]
    fn collaboration_trait_object_safe() {
        let _skill: Box<dyn SkillProvider> = Box::new(MockSkillProvider);
        let _question: Box<dyn QuestionProvider> = Box::new(MockQuestionProvider);
        let _subagent: Box<dyn SubagentHost> = Box::new(MockSubagentHost);
        let _bg: Box<dyn BackgroundRegistrar> = Box::new(MockBackgroundRegistrar);

        // Verify associated types work
        let info = SkillInfo {
            name: "test".into(),
            skill_type: None,
            disable_model_invocation: None,
            hidden_in_modes: None,
            content: "content".into(),
            path: "/test".into(),
            source: "project".into(),
        };
        assert_eq!(info.name, "test");

        let options = SubagentOptions {
            parent_tool_call_id: "call_1".into(),
            prompt: "run".into(),
            description: "desc".into(),
            run_in_background: false,
            signal: AbortSignal::new(),
        };
        assert!(!options.run_in_background);

        let q_req = QuestionRequest {
            turn_id: Some(1),
            tool_call_id: "call_1".into(),
            questions: vec![],
        };
        assert_eq!(q_req.turn_id, Some(1));

        let task_opts = QuestionTaskOptions {
            question_count: 1,
            tool_call_id: "call_2".into(),
        };
        assert_eq!(task_opts.question_count, 1);

        let agent_opts = AgentTaskOptions {
            timeout_ms: Some(5000),
            agent_id: "agent_1".into(),
            subagent_type: "generic".into(),
            abort: Arc::new(|| {}),
        };
        assert_eq!(agent_opts.subagent_type, "generic");
    }
}
