use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Project,
    User,
    Extra,
    Builtin,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub skill_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_model_invocation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_in_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginContext {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub path: String,
    pub dir: String,
    pub content: String,
    pub metadata: SkillMetadata,
    pub source: SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<SkillPluginContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mermaid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d2: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub path: String,
    pub source: SkillSource,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub skill_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_model_invocation: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillRoot {
    pub path: String,
    pub source: SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<SkillPluginContext>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedSkill {
    pub path: String,
    #[serde(rename = "type")]
    pub skipped_type: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivatedEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub activation_id: String,
    pub skill_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_args: Option<String>,
    pub trigger: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_source: Option<SkillSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivateSkillPayload {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Skill \"{0}\" was not found")]
    NotFound(String),
    #[error("Skill \"{0}\" cannot be activated by the user")]
    UnsupportedType(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SkillPromptError {
    #[error("skill prompt failed: {0}")]
    PromptFailed(String),
}

pub fn normalize_skill_name(name: &str) -> String {
    name.to_lowercase()
}

pub fn is_inline_skill_type(skill_type: Option<&str>) -> bool {
    matches!(skill_type, None | Some("prompt") | Some("inline"))
}

pub fn is_user_activatable_skill_type(skill_type: Option<&str>) -> bool {
    is_inline_skill_type(skill_type) || skill_type == Some("flow")
}

pub fn is_knowledge_skill_type(skill_type: Option<&str>) -> bool {
    skill_type == Some("knowledge")
}

pub fn is_supported_skill_type(skill_type: Option<&str>) -> bool {
    is_user_activatable_skill_type(skill_type) || is_knowledge_skill_type(skill_type)
}

pub fn summarize_skill(skill: &SkillDefinition) -> SkillSummary {
    SkillSummary {
        name: skill.name.clone(),
        description: skill.description.clone(),
        path: skill.path.clone(),
        source: skill.source,
        skill_type: skill.metadata.skill_type.clone(),
        disable_model_invocation: skill.metadata.disable_model_invocation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_source_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&SkillSource::Builtin).unwrap(),
            "\"builtin\""
        );
    }

    #[test]
    fn skill_metadata_type_field_round_trips() {
        let meta = SkillMetadata {
            name: None,
            description: None,
            skill_type: Some("flow".into()),
            when_to_use: None,
            disable_model_invocation: None,
            hidden_in_modes: None,
            safe: None,
            arguments: None,
            triggers: None,
            extra: HashMap::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"type\":\"flow\""));
        let parsed: SkillMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.skill_type, Some("flow".into()));
    }

    #[test]
    fn summarize_skill_preserves_source_and_type() {
        let skill = SkillDefinition {
            name: "simplicity-first".into(),
            description: "".into(),
            path: "/skills/simplicity-first.md".into(),
            dir: "/skills".into(),
            content: "".into(),
            metadata: SkillMetadata {
                skill_type: Some("inline".into()),
                ..SkillMetadata::default()
            },
            source: SkillSource::Builtin,
            plugin: None,
            mermaid: None,
            d2: None,
        };
        let summary = summarize_skill(&skill);
        assert_eq!(summary.skill_type, Some("inline".into()));
        assert_eq!(summary.source, SkillSource::Builtin);
    }
}
