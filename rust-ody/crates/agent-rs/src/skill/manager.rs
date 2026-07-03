use std::collections::HashMap;

use kosong_rs::message::ContentPart;

use crate::records::nested::PromptOrigin;

use super::registry::SkillRegistry;
use super::types::{
    is_user_activatable_skill_type, ActivateSkillPayload, SkillActivatedEvent, SkillError,
    SkillPromptError, SkillSource,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SkillActivationOrigin {
    pub activation_id: String,
    pub skill_name: String,
    pub skill_args: Option<String>,
    pub trigger: String,
    pub skill_type: Option<String>,
    pub skill_path: Option<String>,
    pub skill_source: Option<SkillSource>,
}

/// Minimal Agent surface required by `SkillManager`.
pub trait SkillActivationContext: Send + Sync {
    fn emit_skill_activated(&mut self, event: SkillActivatedEvent);
    fn telemetry_track(&mut self, event_name: &str, properties: HashMap<String, String>);
    fn prompt(
        &mut self,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    ) -> Result<(), SkillPromptError>;
    fn new_activation_id(&self) -> String;
}

pub struct SkillManager<C: SkillActivationContext, R: SkillRegistry> {
    context: C,
    registry: R,
}

impl<C: SkillActivationContext, R: SkillRegistry> SkillManager<C, R> {
    pub fn new(context: C, registry: R) -> Self {
        Self { context, registry }
    }

    pub fn activate(&mut self, payload: ActivateSkillPayload) -> Result<(), SkillError> {
        let skill = self
            .registry
            .get_skill(&payload.name)
            .ok_or_else(|| SkillError::NotFound(payload.name.clone()))?;

        if !is_user_activatable_skill_type(skill.metadata.skill_type.as_deref()) {
            return Err(SkillError::UnsupportedType(skill.name.clone()));
        }

        let skill_content = self
            .registry
            .render_skill_prompt(skill, payload.args.as_deref().unwrap_or(""));
        let args_attr = payload
            .args
            .as_ref()
            .map(|a| format!(" args=\"{}\"", escape_xml(a)))
            .unwrap_or_default();
        let wrapped_text = format!(
            "<system-reminder>\n<kimi-skill-loaded name=\"{}\"{}>\n{}\n</kimi-skill-loaded>\n</system-reminder>",
            escape_xml(&skill.name),
            args_attr,
            skill_content
        );
        let wrapped = vec![ContentPart::Text { text: wrapped_text }];

        let origin = SkillActivationOrigin {
            activation_id: self.context.new_activation_id(),
            skill_name: skill.name.clone(),
            skill_args: payload.args,
            trigger: "user-slash".to_string(),
            skill_type: skill.metadata.skill_type.clone(),
            skill_path: Some(skill.path.clone()),
            skill_source: Some(skill.source),
        };
        self.record_activation(origin, Some(wrapped));
        Ok(())
    }

    pub fn record_activation(
        &mut self,
        origin: SkillActivationOrigin,
        input: Option<Vec<ContentPart>>,
    ) {
        self.context.emit_skill_activated(SkillActivatedEvent {
            event_type: "skill.activated".to_string(),
            activation_id: origin.activation_id.clone(),
            skill_name: origin.skill_name.clone(),
            skill_args: origin.skill_args.clone(),
            trigger: origin.trigger.clone(),
            skill_path: origin.skill_path.clone(),
            skill_source: origin.skill_source,
        });

        let mut props = HashMap::new();
        props.insert("skill_name".to_string(), origin.skill_name.clone());
        props.insert("trigger".to_string(), origin.trigger.clone());
        self.context.telemetry_track("skill_invoked", props);

        if origin.skill_type.as_deref() == Some("flow") {
            let mut flow_props = HashMap::new();
            flow_props.insert("flow_name".to_string(), origin.skill_name.clone());
            self.context.telemetry_track("flow_invoked", flow_props);
        }

        if let Some(input) = input {
            let prompt_origin = PromptOrigin::SkillActivation {
                activation_id: origin.activation_id,
                skill_name: origin.skill_name,
                skill_args: origin.skill_args,
                trigger: origin.trigger,
                skill_type: origin.skill_type,
                skill_path: origin.skill_path,
            };
            self.context
                .prompt(input, prompt_origin)
                .expect("prompt should succeed");
        }
    }

    pub fn into_inner(self) -> (C, R) {
        (self.context, self.registry)
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
