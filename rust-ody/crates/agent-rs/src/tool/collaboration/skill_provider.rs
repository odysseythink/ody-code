use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use kosong_rs::message::ContentPart;

use crate::agent::AgentContext;
use crate::records::nested::PromptOrigin;
use crate::skill::{
    manager::SkillActivationOrigin as AgentSkillActivationOrigin,
    registry::SkillRegistry,
    types::{SkillActivatedEvent, SkillDefinition, SkillSource},
    SkillActivationContext,
};
use tools_rs::builtin::collaboration::{
    SkillActivationOrigin as ToolsSkillActivationOrigin, SkillError, SkillInfo, SkillProvider,
};

fn map_skill_source(s: &str) -> SkillSource {
    match s.to_ascii_lowercase().as_str() {
        "user" => SkillSource::User,
        "extra" => SkillSource::Extra,
        "builtin" => SkillSource::Builtin,
        _ => SkillSource::Project,
    }
}

fn map_tools_origin_to_agent_origin(
    origin: &ToolsSkillActivationOrigin,
) -> AgentSkillActivationOrigin {
    AgentSkillActivationOrigin {
        activation_id: origin.activation_id.clone(),
        skill_name: origin.skill_name.clone(),
        skill_args: origin.skill_args.clone(),
        trigger: origin.trigger.clone(),
        skill_type: origin.skill_type.clone(),
        skill_path: origin.skill_path.clone(),
        skill_source: origin.skill_source.as_deref().map(map_skill_source),
    }
}

pub struct AgentSkillProvider {
    context: Mutex<AgentContext>,
    registry: Arc<dyn SkillRegistry>,
}

impl AgentSkillProvider {
    pub fn new(agent: Weak<crate::agent::Agent>, registry: Arc<dyn SkillRegistry>) -> Self {
        Self {
            context: Mutex::new(AgentContext { agent }),
            registry,
        }
    }
}

impl SkillProvider for AgentSkillProvider {
    fn get_skill(&self, name: &str) -> Option<SkillInfo> {
        self.registry.get_skill(name).map(|s| SkillInfo {
            name: s.name.clone(),
            skill_type: s.metadata.skill_type.clone(),
            disable_model_invocation: s.metadata.disable_model_invocation,
            hidden_in_modes: s.metadata.hidden_in_modes.clone(),
            content: s.content.clone(),
            path: s.path.clone(),
            source: match s.source {
                SkillSource::Project => "project".into(),
                SkillSource::User => "user".into(),
                SkillSource::Extra => "extra".into(),
                SkillSource::Builtin => "builtin".into(),
            },
        })
    }

    fn record_activation(&self, origin: ToolsSkillActivationOrigin) -> Result<(), SkillError> {
        let mut ctx = self.context.lock().unwrap();
        let agent_origin = map_tools_origin_to_agent_origin(&origin);
        ctx.emit_skill_activated(SkillActivatedEvent {
            event_type: "skill.activated".into(),
            activation_id: agent_origin.activation_id,
            skill_name: agent_origin.skill_name,
            skill_args: agent_origin.skill_args,
            trigger: agent_origin.trigger,
            skill_path: agent_origin.skill_path,
            skill_source: agent_origin.skill_source,
        });
        let mut props = HashMap::new();
        props.insert("skill_name".into(), origin.skill_name.clone());
        props.insert("trigger".into(), origin.trigger.clone());
        ctx.telemetry_track("skill_invoked", props);
        Ok(())
    }

    fn render_skill_prompt(&self, skill: &SkillInfo, args: &str) -> String {
        let def = SkillDefinition {
            name: skill.name.clone(),
            description: String::new(),
            path: skill.path.clone(),
            dir: String::new(),
            content: skill.content.clone(),
            metadata: crate::skill::SkillMetadata {
                skill_type: skill.skill_type.clone(),
                disable_model_invocation: skill.disable_model_invocation,
                hidden_in_modes: skill.hidden_in_modes.clone(),
                ..Default::default()
            },
            source: map_skill_source(&skill.source),
            plugin: None,
            mermaid: None,
            d2: None,
        };
        self.registry.render_skill_prompt(&def, args)
    }

    fn current_session_mode(&self) -> Option<String> {
        None
    }

    fn append_system_reminder(
        &self,
        content: String,
        origin: ToolsSkillActivationOrigin,
    ) -> Result<(), SkillError> {
        let mut ctx = self.context.lock().unwrap();
        let agent_origin = map_tools_origin_to_agent_origin(&origin);
        let _ = ctx.prompt(
            vec![ContentPart::Text { text: content }],
            PromptOrigin::SkillActivation {
                activation_id: agent_origin.activation_id,
                skill_name: agent_origin.skill_name,
                skill_args: agent_origin.skill_args,
                trigger: agent_origin.trigger,
                skill_type: agent_origin.skill_type,
                skill_path: agent_origin.skill_path,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentBuilder, AgentEnvironment};
    use crate::skill::{InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillSource};
    use kaos_rs::environment::detect_environment_from_node;
    use kaos_rs::kaos::Kaos;
    use kosong_rs::provider::AbortSignal;
    use std::pin::Pin;
    use std::sync::Arc;

    struct NoopEnv;
    #[async_trait::async_trait]
    impl AgentEnvironment for NoopEnv {
        fn emit_event(&self, _event: crate::turn::types::AgentEvent) {}
        async fn request_approval(
            &self,
            _req: &crate::permission::types::ApprovalRequest,
            _signal: AbortSignal,
        ) -> Result<crate::records::nested::ApprovalResponse, anyhow::Error> {
            Ok(crate::records::nested::ApprovalResponse {
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
            _signal: AbortSignal,
        ) -> Pin<
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
            _input: Vec<ContentPart>,
            _signal: AbortSignal,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Vec<crate::turn::types::HookResult>, anyhow::Error>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(vec![]) })
        }
        fn fire_hook_stop_hook(
            &self,
            _signal: AbortSignal,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Option<crate::turn::types::StopHookBlock>, anyhow::Error>,
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
            _signal: AbortSignal,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
        fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
        fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
    }

    fn sample_skill() -> SkillDefinition {
        SkillDefinition {
            name: "refactor".into(),
            description: "".into(),
            path: "/skills/refactor.md".into(),
            dir: "/skills".into(),
            content: "Refactor this code.".into(),
            metadata: SkillMetadata {
                skill_type: Some("prompt".into()),
                ..SkillMetadata::default()
            },
            source: SkillSource::Project,
            plugin: None,
            mermaid: None,
            d2: None,
        }
    }

    fn build_agent() -> Arc<crate::agent::Agent> {
        let kaos = Arc::new(Kaos::new(
            detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            AgentBuilder::new("test", kaos, Arc::new(NoopEnv))
                .build()
                .await
                .unwrap()
        })
    }

    #[test]
    fn provider_maps_registry_skill_to_tools_rs_info() {
        let agent = build_agent();
        let mut registry = InMemorySkillRegistry::new();
        registry.register(sample_skill());
        let registry_arc: Arc<dyn SkillRegistry> = Arc::new(registry);
        let provider = AgentSkillProvider::new(Arc::downgrade(&agent), registry_arc);

        let info = provider.get_skill("refactor").expect("skill should exist");
        assert_eq!(info.name, "refactor");
        assert_eq!(info.skill_type.as_deref(), Some("prompt"));
        assert_eq!(info.content, "Refactor this code.");
        assert_eq!(info.source, "project");
    }

    #[test]
    fn record_activation_does_not_panic() {
        let agent = build_agent();
        let mut registry = InMemorySkillRegistry::new();
        registry.register(sample_skill());
        let registry_arc: Arc<dyn SkillRegistry> = Arc::new(registry);
        let provider = AgentSkillProvider::new(Arc::downgrade(&agent), registry_arc);

        provider
            .record_activation(ToolsSkillActivationOrigin {
                activation_id: "a1".into(),
                skill_name: "refactor".into(),
                skill_args: Some("foo.rs".into()),
                trigger: "model-tool".into(),
                skill_type: Some("prompt".into()),
                skill_path: Some("/skills/refactor.md".into()),
                skill_source: Some("project".into()),
            })
            .unwrap();

        // AgentContext 当前为 stub 实现；这里至少验证调用不 panic。
        assert!(true);
    }
}
