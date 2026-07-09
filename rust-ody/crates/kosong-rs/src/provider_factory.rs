use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderFactoryConfig {
    pub provider_id: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderFactoryError {
    UnknownProvider(String),
    MissingModel,
    MissingApiKey { provider: String },
}

impl std::fmt::Display for ProviderFactoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderFactoryError::UnknownProvider(id) => write!(f, "unknown provider: {id}"),
            ProviderFactoryError::MissingModel => write!(f, "model is required"),
            ProviderFactoryError::MissingApiKey { provider } => {
                write!(f, "apiKey is required for provider: {provider}")
            }
        }
    }
}

impl std::error::Error for ProviderFactoryError {}

impl ProviderFactoryConfig {
    pub fn require_api_key(&self) -> Result<String, ProviderFactoryError> {
        match self.api_key.as_ref().filter(|k| !k.is_empty()) {
            Some(key) => Ok(key.clone()),
            None => Err(ProviderFactoryError::MissingApiKey {
                provider: self.provider_id.clone(),
            }),
        }
    }
}

use crate::provider::ChatProvider;
use crate::providers::glm::{GLMChatProvider, GLMOptions};
use crate::{
    AnthropicChatProvider, AnthropicOptions, DeepSeekChatProvider, DeepSeekOptions,
    GoogleGenAIChatProvider, KimiChatProvider, KimiOptions, MockProvider, OpenAILegacyChatProvider,
    OpenAILegacyOptions, OpenAIResponsesChatProvider, OpenAIResponsesOptions,
};

pub fn create_chat_provider(
    config: ProviderFactoryConfig,
) -> Result<Box<dyn ChatProvider>, ProviderFactoryError> {
    if config.model.is_empty() {
        return Err(ProviderFactoryError::MissingModel);
    }

    match config.provider_id.as_str() {
        "mock" => Ok(Box::new(MockProvider::new("mock", config.model))),
        "openai" => Ok(Box::new(OpenAILegacyChatProvider::new(
            OpenAILegacyOptions {
                api_key: config.api_key,
                base_url: config.base_url,
                model: config.model,
                stream: Some(true),
                max_tokens: None,
                reasoning_key: None,
                default_headers: config.default_headers,
                tool_message_conversion: None,
                http_client: None,
            },
        ))),
        "openai_responses" => Ok(Box::new(OpenAIResponsesChatProvider::new(
            OpenAIResponsesOptions {
                api_key: config.api_key,
                base_url: config.base_url,
                model: config.model,
                max_output_tokens: None,
                default_headers: config.default_headers,
                tool_message_conversion: None,
                http_client: None,
            },
        ))),
        "kimi" => Ok(Box::new(KimiChatProvider::new(KimiOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            stream: Some(true),
            default_headers: config.default_headers,
            generation_kwargs: None,
            http_client: None,
            reasoning_key: None,
        }))),
        "anthropic" => Ok(Box::new(AnthropicChatProvider::new(AnthropicOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            default_max_tokens: None,
            beta_features: None,
            default_headers: config.default_headers,
            metadata: None,
            stream: Some(true),
            adaptive_thinking: None,
        }))),
        "deepseek" => Ok(Box::new(DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            http_client: None,
            default_headers: config.default_headers,
            tool_message_conversion: None,
        }))),
        "glm" => Ok(Box::new(GLMChatProvider::new(GLMOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            stream: Some(true),
            max_tokens: None,
            default_headers: config.default_headers,
            http_client: None,
        }))),
        "google-genai" => {
            let mut provider = GoogleGenAIChatProvider::new(config.model).with_stream(true);
            if let Some(key) = config.api_key {
                provider = provider.with_api_key(key);
            }
            if let Some(url) = config.base_url {
                provider = provider.with_base_url(url);
            }
            Ok(Box::new(provider))
        }
        "vertexai" => {
            let mut provider = GoogleGenAIChatProvider::new(config.model)
                .with_stream(true)
                .with_vertexai("", "");
            if let Some(key) = config.api_key {
                provider = provider.with_api_key(key);
            }
            if let Some(url) = config.base_url {
                provider = provider.with_base_url(url);
            }
            Ok(Box::new(provider))
        }
        other => Err(ProviderFactoryError::UnknownProvider(other.into())),
    }
}

use crate::capability_registry::{
    get_anthropic_model_capability, get_deepseek_model_capability, get_glm_model_capability,
    get_google_genai_model_capability, get_kimi_model_capability,
    get_openai_legacy_model_capability, get_openai_responses_model_capability,
};
use crate::provider::ModelCapability;

pub fn resolve_model_capability(provider_id: &str, model: &str) -> Option<ModelCapability> {
    match provider_id {
        "openai" => Some(get_openai_legacy_model_capability(model)),
        "openai_responses" => Some(get_openai_responses_model_capability(model)),
        "kimi" => Some(get_kimi_model_capability(model)),
        "anthropic" => Some(get_anthropic_model_capability(model)),
        "deepseek" => Some(get_deepseek_model_capability(model)),
        "glm" => Some(get_glm_model_capability(model)),
        "google-genai" | "vertexai" => Some(get_google_genai_model_capability(model)),
        _ => Some(ModelCapability::unknown()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_provider_error_includes_id() {
        let err = ProviderFactoryError::UnknownProvider("weird".into());
        assert!(err.to_string().contains("weird"));
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn resolve_openai_known_model() {
        let cap = resolve_model_capability("openai", "gpt-4o").unwrap();
        assert!(cap.image_in);
        assert!(cap.tool_use);
        assert!(!cap.is_unknown());
    }

    #[test]
    fn resolve_unknown_model_returns_unknown() {
        let cap = resolve_model_capability("openai", "not-a-real-model").unwrap();
        assert!(cap.is_unknown());
    }

    #[test]
    fn resolve_unsupported_provider_returns_unknown() {
        let cap = resolve_model_capability("weird", "m1").unwrap();
        assert!(cap.is_unknown());
    }
}

#[cfg(test)]
mod create_tests {
    use super::*;

    #[tokio::test]
    async fn factory_creates_mock_provider() {
        let provider = create_chat_provider(ProviderFactoryConfig {
            provider_id: "mock".into(),
            model: "m1".into(),
            api_key: None,
            base_url: None,
            default_headers: None,
        })
        .unwrap();
        assert_eq!(provider.name(), "mock");
        assert_eq!(provider.model_name(), "m1");
    }

    #[tokio::test]
    async fn factory_creates_openai_provider() {
        let provider = create_chat_provider(ProviderFactoryConfig {
            provider_id: "openai".into(),
            model: "gpt-4o-mini".into(),
            api_key: Some("sk-test".into()),
            base_url: Some("https://example.com/v1".into()),
            default_headers: None,
        })
        .unwrap();
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model_name(), "gpt-4o-mini");
    }

    #[tokio::test]
    async fn factory_rejects_unknown_provider() {
        let result = create_chat_provider(ProviderFactoryConfig {
            provider_id: "weird".into(),
            model: "x".into(),
            api_key: None,
            base_url: None,
            default_headers: None,
        });
        assert!(matches!(
            result,
            Err(ProviderFactoryError::UnknownProvider(id)) if id == "weird"
        ));
    }
}
