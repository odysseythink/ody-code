use kosong_rs::provider::ChatProvider;
use kosong_rs::{create_chat_provider, ProviderFactoryConfig};

use crate::config::ProviderConfig;
use crate::error::HostError;

pub fn create_host_provider(config: &ProviderConfig) -> Result<Box<dyn ChatProvider>, HostError> {
    create_chat_provider(ProviderFactoryConfig {
        provider_id: config.provider_id.clone(),
        model: config
            .default_model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".into()),
        api_key: Some(config.api_key.clone()).filter(|k| !k.is_empty()),
        base_url: config.base_url.clone(),
        default_headers: None,
    })
    .map_err(|e| HostError::config_invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_openai_provider() {
        let config = ProviderConfig {
            provider_id: "openai".into(),
            api_key: "sk".into(),
            base_url: Some("https://example.com/v1".into()),
            default_model: Some("gpt-4o-mini".into()),
        };
        let provider = create_host_provider(&config).unwrap();
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model_name(), "gpt-4o-mini");
    }

    #[test]
    fn rejects_unknown_provider() {
        let config = ProviderConfig {
            provider_id: "weird".into(),
            api_key: "x".into(),
            base_url: None,
            default_model: Some("m".into()),
        };
        if let Err(e) = create_host_provider(&config) {
            assert!(e.to_string().contains("weird"));
        } else {
            panic!("expected error for unknown provider");
        }
    }
}
