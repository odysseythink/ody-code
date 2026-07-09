use kosong_rs::provider::{ModelCapability, ProviderType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub r#type: ProviderType,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigData {
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_alias: Option<String>,
    pub model_capabilities: ModelCapability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    pub thinking_level: String,
    pub system_prompt: String,
}

// Re-export the records-layer update payload so config, records, and tests
// all refer to the same type.
pub use crate::records::nested::AgentConfigUpdateData;
