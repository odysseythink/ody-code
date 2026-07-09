use std::sync::Arc;

use serde_json::Value;

use crate::builtin::collaboration::{SkillActivationOrigin, SkillProvider};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::policies::rule_match::matches_glob_rule_subject;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;

pub const MAX_SKILL_QUERY_DEPTH: u32 = 3;

#[derive(Debug, Clone, Default)]
pub struct SkillToolOptions {
    pub query_depth: Option<u32>,
    pub initial_query_depth: Option<u32>,
    pub session_mode: Option<String>,
}

impl SkillToolOptions {
    fn current_depth(&self) -> u32 {
        self.initial_query_depth.or(self.query_depth).unwrap_or(0)
    }
}

pub struct SkillTool {
    provider: Arc<dyn SkillProvider>,
    options: SkillToolOptions,
}

impl SkillTool {
    pub fn new(provider: Arc<dyn SkillProvider>, options: SkillToolOptions) -> Self {
        Self { provider, options }
    }

    pub fn with_query_depth(provider: Arc<dyn SkillProvider>, depth: u32) -> Self {
        Self {
            provider,
            options: SkillToolOptions {
                query_depth: Some(depth),
                ..Default::default()
            },
        }
    }

    pub fn with_session_mode(provider: Arc<dyn SkillProvider>, mode: String) -> Self {
        Self {
            provider,
            options: SkillToolOptions {
                session_mode: Some(mode),
                ..Default::default()
            },
        }
    }
}

impl BuiltinTool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Invoke a registered skill from the current skill listing. BLOCKING REQUIREMENT: when a skill from the listing matches the user's request, you MUST call this tool (not free-form text). Do NOT call the same skill repeatedly inside one turn — recursive depth is capped at 3."
    }

    fn parameters(&self) -> Value {
        InputSchema::object(vec![
            (
                "skill",
                InputSchema::string().description("The name of the skill to invoke."),
            ),
            (
                "args",
                InputSchema::string()
                    .optional()
                    .description("Optional arguments to pass to the skill."),
            ),
        ])
        .build()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let skill_name = args
            .get("skill")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArgs("skill is required".into()))?
            .to_string();
        let skill_args = args
            .get("args")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let provider = Arc::clone(&self.provider);
        let options = self.options.clone();
        let subject = skill_name.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::none(),
            description: format!("Invoke skill {}", skill_name),
            approval_rule: "Skill".into(),
            matches_rule: Some(Box::new(move |rule_args| {
                matches_glob_rule_subject(rule_args, &subject)
            })),
            display: None,
            execute: Box::new(move |ctx| {
                let provider = Arc::clone(&provider);
                let skill_name = skill_name.clone();
                let skill_args = skill_args.clone();
                let options = options.clone();
                Box::pin(async move {
                    execute_skill(provider, skill_name, skill_args, options, ctx).await
                })
            }),
        })
    }
}

fn is_inline_skill_type(skill_type: Option<&str>) -> bool {
    matches!(skill_type, None | Some("prompt") | Some("inline"))
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

async fn execute_skill(
    provider: Arc<dyn SkillProvider>,
    skill_name: String,
    skill_args: String,
    options: SkillToolOptions,
    _ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    let current_depth = options.current_depth();
    if current_depth >= MAX_SKILL_QUERY_DEPTH {
        return ExecutableToolResult::error_text(
            format!(
                "Nested skill invocation \"{}\" exceeded the maximum depth of {} — refusing to recurse further.",
                skill_name, MAX_SKILL_QUERY_DEPTH
            ),
            "Nested skill too deep".into(),
        );
    }

    let skill = match provider.get_skill(&skill_name) {
        Some(s) => s,
        None => {
            return ExecutableToolResult::error_text(
                format!(
                    "Skill \"{}\" not found in the current skill listing.",
                    skill_name
                ),
                "Skill not found".into(),
            )
        }
    };

    if skill.disable_model_invocation == Some(true) {
        return ExecutableToolResult::error_text(
            format!(
                "Skill \"{}\" can only be triggered by the user (model invocation is disabled).",
                skill.name
            ),
            "Model invocation disabled".into(),
        );
    }

    if !is_inline_skill_type(skill.skill_type.as_deref()) {
        return ExecutableToolResult::error_text(
            format!(
                "Skill \"{}\" is not an inline skill and cannot be invoked by the model in v1.",
                skill.name
            ),
            "Not an inline skill".into(),
        );
    }

    let session_mode = options.session_mode.as_deref().unwrap_or("normal");
    if session_mode != "normal" {
        if let Some(hidden) = &skill.hidden_in_modes {
            if hidden.iter().any(|m| m == session_mode) {
                return ExecutableToolResult::error_text(
                    format!(
                        "Skill \"{}\" is not available in {} mode.",
                        skill.name, session_mode
                    ),
                    "Skill hidden in mode".into(),
                );
            }
        }
    }

    let origin = SkillActivationOrigin {
        activation_id: uuid::Uuid::new_v4().to_string(),
        skill_name: skill.name.clone(),
        skill_args: if skill_args.is_empty() {
            None
        } else {
            Some(skill_args.clone())
        },
        trigger: if current_depth > 0 {
            "nested-skill".into()
        } else {
            "model-tool".into()
        },
        skill_type: skill.skill_type.clone(),
        skill_path: Some(skill.path.clone()),
        skill_source: Some(skill.source.clone()),
    };

    if let Err(e) = provider.record_activation(origin.clone()) {
        return ExecutableToolResult::error_text(
            format!("Failed to record skill activation: {:?}", e),
            "Activation failed".into(),
        );
    }

    let skill_content = provider.render_skill_prompt(&skill, &skill_args);
    let reminder = format!(
        "<kimi-skill-loaded name=\"{}\" args=\"{}\">\n{}\n</kimi-skill-loaded>",
        escape_xml(&skill.name),
        escape_xml(&skill_args),
        skill_content
    );

    if let Err(e) = provider.append_system_reminder(reminder, origin) {
        return ExecutableToolResult::error_text(
            format!("Failed to append system reminder: {:?}", e),
            "Reminder failed".into(),
        );
    }

    ExecutableToolResult::ok_text(format!(
        "Skill \"{}\" loaded inline. Follow its instructions.",
        skill.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::collaboration::{
        SkillActivationOrigin, SkillError, SkillInfo, SkillProvider,
    };
    use crate::builtin::AbortSignal;
    use serde_json::json;
    use std::sync::Mutex;

    struct TestProvider {
        skills: Vec<SkillInfo>,
        reminders: Mutex<Vec<(String, SkillActivationOrigin)>>,
        activations: Mutex<Vec<SkillActivationOrigin>>,
    }

    impl TestProvider {
        fn new(skills: Vec<SkillInfo>) -> Self {
            Self {
                skills,
                reminders: Mutex::new(Vec::new()),
                activations: Mutex::new(Vec::new()),
            }
        }
    }

    impl SkillProvider for TestProvider {
        fn get_skill(&self, name: &str) -> Option<SkillInfo> {
            self.skills.iter().find(|s| s.name == name).cloned()
        }

        fn record_activation(&self, origin: SkillActivationOrigin) -> Result<(), SkillError> {
            self.activations.lock().unwrap().push(origin);
            Ok(())
        }

        fn render_skill_prompt(&self, skill: &SkillInfo, args: &str) -> String {
            format!("{} rendered with {}", skill.content, args)
        }

        fn current_session_mode(&self) -> Option<String> {
            None
        }

        fn append_system_reminder(
            &self,
            content: String,
            origin: SkillActivationOrigin,
        ) -> Result<(), SkillError> {
            self.reminders.lock().unwrap().push((content, origin));
            Ok(())
        }
    }

    fn ctx() -> ExecutableToolContext {
        ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: AbortSignal::new(),
            metadata: None,
        }
    }

    async fn run_skill(tool: &SkillTool, args: serde_json::Value) -> ExecutableToolResult {
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ctx()).await
    }

    #[tokio::test]
    async fn success_loads_inline_skill_and_appends_reminder() {
        let provider = Arc::new(TestProvider::new(vec![SkillInfo {
            name: "refactor".into(),
            skill_type: Some("prompt".into()),
            disable_model_invocation: Some(false),
            hidden_in_modes: None,
            content: "Refactor this code.".into(),
            path: "/skills/refactor.md".into(),
            source: "project".into(),
        }]));
        let tool = SkillTool::new(provider, SkillToolOptions::default());
        let result = run_skill(&tool, json!({"skill": "refactor", "args": "foo.rs"})).await;
        assert!(!result.is_error, "{:?}", result);
        assert_eq!(
            result.to_text(),
            r#"Skill "refactor" loaded inline. Follow its instructions."#
        );
    }

    #[tokio::test]
    async fn missing_skill_returns_error() {
        let provider = Arc::new(TestProvider::new(vec![]));
        let tool = SkillTool::new(provider, SkillToolOptions::default());
        let result = run_skill(&tool, json!({"skill": "missing"})).await;
        assert!(result.is_error);
        assert!(result
            .to_text()
            .contains("not found in the current skill listing"));
    }

    #[tokio::test]
    async fn disabled_model_invocation_returns_error() {
        let provider = Arc::new(TestProvider::new(vec![SkillInfo {
            name: "secret".into(),
            skill_type: Some("prompt".into()),
            disable_model_invocation: Some(true),
            hidden_in_modes: None,
            content: "secret".into(),
            path: "/skills/secret.md".into(),
            source: "project".into(),
        }]));
        let tool = SkillTool::new(provider, SkillToolOptions::default());
        let result = run_skill(&tool, json!({"skill": "secret"})).await;
        assert!(result.is_error);
        assert!(result
            .to_text()
            .contains("can only be triggered by the user"));
    }

    #[tokio::test]
    async fn non_inline_skill_returns_error() {
        let provider = Arc::new(TestProvider::new(vec![SkillInfo {
            name: "flow".into(),
            skill_type: Some("flow".into()),
            disable_model_invocation: Some(false),
            hidden_in_modes: None,
            content: "flow".into(),
            path: "/skills/flow.md".into(),
            source: "project".into(),
        }]));
        let tool = SkillTool::new(provider, SkillToolOptions::default());
        let result = run_skill(&tool, json!({"skill": "flow"})).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("is not an inline skill"));
    }

    #[tokio::test]
    async fn hidden_in_mode_returns_error() {
        let provider = Arc::new(TestProvider::new(vec![SkillInfo {
            name: "plan".into(),
            skill_type: Some("prompt".into()),
            disable_model_invocation: Some(false),
            hidden_in_modes: Some(vec!["debug".into()]),
            content: "plan".into(),
            path: "/skills/plan.md".into(),
            source: "project".into(),
        }]));
        let tool = SkillTool::with_session_mode(provider, "debug".into());
        let result = run_skill(&tool, json!({"skill": "plan"})).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("is not available in debug mode"));
    }

    #[tokio::test]
    async fn recursion_cap_returns_error() {
        let provider = Arc::new(TestProvider::new(vec![SkillInfo {
            name: "recursive".into(),
            skill_type: Some("prompt".into()),
            disable_model_invocation: Some(false),
            hidden_in_modes: None,
            content: "recurse".into(),
            path: "/skills/recursive.md".into(),
            source: "project".into(),
        }]));
        let tool = SkillTool::with_query_depth(provider, MAX_SKILL_QUERY_DEPTH);
        let result = run_skill(&tool, json!({"skill": "recursive"})).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("maximum depth"));
    }

    #[test]
    fn matches_rule_compares_skill_name() {
        let tool = SkillTool::with_query_depth(Arc::new(TestProvider::new(vec![])), 0);
        let exec = tool
            .resolve_execution(json!({"skill": "my-skill"}))
            .unwrap();
        let matches = exec.matches_rule.expect("skill should have matches_rule");
        assert!(matches("my-skill"));
        assert!(matches("my-*"));
        assert!(!matches("other"));
    }
}
