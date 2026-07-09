use serde::{Deserialize, Serialize};

use crate::provider::{ModelCapability, ProviderType};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub context: Option<i64>,
    pub output: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Modalities {
    pub input: Option<Vec<String>>,
    pub output: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModelEntry {
    pub id: Option<String>,
    pub name: Option<String>,
    pub family: Option<String>,
    pub limit: Option<Limit>,
    #[serde(rename = "tool_call")]
    pub tool_call: Option<bool>,
    pub reasoning: Option<bool>,
    pub interleaved: Option<serde_json::Value>,
    pub modalities: Option<Modalities>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProviderEntry {
    pub id: Option<String>,
    pub name: Option<String>,
    pub api: Option<String>,
    pub env: Option<Vec<String>>,
    pub npm: Option<String>,
    #[serde(rename = "type")]
    pub r#type: Option<ProviderType>,
    pub models: Option<std::collections::HashMap<String, CatalogModelEntry>>,
}

pub type Catalog = std::collections::HashMap<String, CatalogProviderEntry>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_key: Option<String>,
    pub capability: ModelCapability,
}

fn has_embedding_marker(value: Option<&str>) -> bool {
    let Some(v) = value else {
        return false;
    };
    let lower = v.to_lowercase();
    lower.contains("embedding")
        || regex::Regex::new(r"(?:^|[-_/])embed(?:$|[-_/])")
            .unwrap()
            .is_match(&lower)
}

fn is_usable_chat_model(model: &CatalogModelEntry) -> bool {
    if let Some(output) = &model.modalities {
        if let Some(out) = &output.output {
            if !out.contains(&"text".to_string()) {
                return false;
            }
        }
    }
    !has_embedding_marker(model.family.as_deref())
        && !has_embedding_marker(model.id.as_deref())
        && !has_embedding_marker(model.name.as_deref())
}

pub fn infer_wire_type(entry: &CatalogProviderEntry) -> Option<ProviderType> {
    if let Some(t) = entry.r#type {
        return Some(t);
    }
    let npm = entry.npm.as_deref().unwrap_or("").to_lowercase();
    let id = entry.id.as_deref().unwrap_or("").to_lowercase();
    if npm.contains("anthropic") || id.contains("anthropic") || id.contains("claude") {
        return Some(ProviderType::Anthropic);
    }
    if id.contains("vertex") {
        return Some(ProviderType::Vertexai);
    }
    if npm.contains("google") || id.contains("google") || id.contains("gemini") {
        return Some(ProviderType::GoogleGenai);
    }
    if npm.contains("openai") || id.contains("openai") {
        return Some(ProviderType::OpenAi);
    }
    if id.contains("deepseek") {
        return Some(ProviderType::Deepseek);
    }
    if id.contains("glm") || id.contains("zhipu") || id.contains("bigmodel") {
        return Some(ProviderType::Glm);
    }
    None
}

pub fn catalog_base_url(entry: &CatalogProviderEntry, wire: ProviderType) -> Option<String> {
    let api = entry.api.as_deref()?;
    if api.is_empty() {
        return None;
    }
    if wire == ProviderType::Anthropic {
        return Some(
            regex::Regex::new(r"/v1/?$")
                .unwrap()
                .replace(api, "")
                .to_string(),
        );
    }
    Some(api.to_string())
}

pub fn catalog_model_to_capability(model: &CatalogModelEntry) -> Option<CatalogModel> {
    let id = model.id.as_deref()?;
    if id.is_empty() {
        return None;
    }
    let context = model.limit.as_ref().and_then(|l| l.context)?;
    if context <= 0 {
        return None;
    }
    if !is_usable_chat_model(model) {
        return None;
    }
    let inputs = model
        .modalities
        .as_ref()
        .and_then(|m| m.input.clone())
        .unwrap_or_default();
    let output = model.limit.as_ref().and_then(|l| l.output);
    Some(CatalogModel {
        id: id.to_string(),
        name: model.name.clone().filter(|n| !n.is_empty()),
        max_output_size: output.filter(|n| *n > 0),
        reasoning_key: catalog_reasoning_key(model.interleaved.as_ref()),
        capability: ModelCapability {
            image_in: inputs.contains(&"image".to_string()),
            video_in: inputs.contains(&"video".to_string()),
            audio_in: inputs.contains(&"audio".to_string()),
            thinking: model.reasoning.unwrap_or(false),
            tool_use: model.tool_call.unwrap_or(true),
            max_context_tokens: context,
            max_output_tokens: output.unwrap_or(0),
        },
    })
}

fn catalog_reasoning_key(interleaved: Option<&serde_json::Value>) -> Option<String> {
    match interleaved {
        Some(v) if v.is_boolean() && v.as_bool() == Some(true) => Some("reasoning_content".into()),
        Some(v) if v.is_object() => v
            .get("field")
            .and_then(|f| f.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => None,
    }
}

pub fn catalog_provider_models(entry: &CatalogProviderEntry) -> Vec<CatalogModel> {
    entry
        .models
        .iter()
        .flat_map(|m| m.values())
        .filter_map(|m| catalog_model_to_capability(m))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_wire_type_uses_explicit_type() {
        let entry = CatalogProviderEntry {
            id: None,
            name: None,
            api: None,
            env: None,
            npm: None,
            r#type: Some(ProviderType::Deepseek),
            models: None,
        };
        assert_eq!(infer_wire_type(&entry), Some(ProviderType::Deepseek));
    }

    #[test]
    fn infer_wire_type_from_id() {
        let entry = provider_entry_with_id("claude-3-5-sonnet");
        assert_eq!(infer_wire_type(&entry), Some(ProviderType::Anthropic));
    }

    #[test]
    fn infer_wire_type_unknown() {
        let entry = provider_entry_with_id("foo");
        assert_eq!(infer_wire_type(&entry), None);
    }

    #[test]
    fn catalog_base_url_strips_v1_for_anthropic() {
        let entry = CatalogProviderEntry {
            api: Some("https://api.anthropic.com/v1".into()),
            ..Default::default()
        };
        assert_eq!(
            catalog_base_url(&entry, ProviderType::Anthropic),
            Some("https://api.anthropic.com".into())
        );
    }

    #[test]
    fn catalog_model_to_capability_skips_embedding() {
        let model = CatalogModelEntry {
            id: Some("text-embedding-3".into()),
            name: None,
            family: Some("embedding".into()),
            limit: Some(Limit {
                context: Some(8192),
                output: Some(1536),
            }),
            tool_call: None,
            reasoning: None,
            interleaved: None,
            modalities: Some(Modalities {
                input: Some(vec!["text".into()]),
                output: Some(vec!["text".into()]),
            }),
        };
        assert!(catalog_model_to_capability(&model).is_none());
    }

    #[test]
    fn catalog_model_to_capability_parses_modalities() {
        let model = CatalogModelEntry {
            id: Some("gpt-4o".into()),
            name: None,
            family: None,
            limit: Some(Limit {
                context: Some(128_000),
                output: Some(16_384),
            }),
            tool_call: Some(true),
            reasoning: None,
            interleaved: None,
            modalities: Some(Modalities {
                input: Some(vec!["text".into(), "image".into()]),
                output: Some(vec!["text".into()]),
            }),
        };
        let cap = catalog_model_to_capability(&model).unwrap();
        assert_eq!(cap.id, "gpt-4o");
        assert!(cap.capability.image_in);
        assert!(cap.capability.tool_use);
        assert_eq!(cap.capability.max_context_tokens, 128_000);
        assert_eq!(cap.capability.max_output_tokens, 16_384);
    }

    fn provider_entry_with_id(id: &str) -> CatalogProviderEntry {
        CatalogProviderEntry {
            id: Some(id.into()),
            name: None,
            api: None,
            env: None,
            npm: None,
            r#type: None,
            models: None,
        }
    }
}
