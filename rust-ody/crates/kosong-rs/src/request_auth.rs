use std::collections::HashMap;

use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::provider::ProviderRequestAuth;

pub fn require_provider_api_key(
    provider_name: &str,
    auth: Option<&ProviderRequestAuth>,
    default_api_key: Option<&str>,
) -> Result<String, ChatProviderError> {
    let api_key = auth
        .as_ref()
        .and_then(|a| a.api_key.clone())
        .or_else(|| default_api_key.map(|s| s.to_string()));
    match api_key {
        Some(key) if !key.is_empty() => Ok(key),
        _ => Err(ChatProviderError::MissingApiKey(APIMissingApiKeyError {
            provider: provider_name.to_string(),
        })),
    }
}

pub fn merge_request_headers(
    default_headers: Option<&HashMap<String, String>>,
    request_headers: Option<&HashMap<String, String>>,
) -> Option<HashMap<String, String>> {
    let mut merged = HashMap::new();
    if let Some(default) = default_headers {
        merged.extend(default.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    if let Some(request) = request_headers {
        merged.extend(request.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

pub struct AuthBackedClientState<TClient: Clone> {
    pub cached_client: Option<TClient>,
    pub client_factory: Option<Box<dyn Fn(&ProviderRequestAuth) -> TClient>>,
}

pub fn resolve_auth_backed_client<TClient: Clone>(
    state: &AuthBackedClientState<TClient>,
    auth: Option<&ProviderRequestAuth>,
    build: impl FnOnce(Option<&ProviderRequestAuth>) -> TClient,
) -> TClient {
    if let Some(factory) = &state.client_factory {
        return factory(auth.unwrap_or(&ProviderRequestAuth {
            api_key: None,
            headers: None,
        }));
    }
    if auth.is_none() {
        if let Some(cached) = &state.cached_client {
            return cached.clone();
        }
    }
    build(auth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ChatProviderError;
    use crate::provider::ProviderRequestAuth;

    #[test]
    fn require_api_key_returns_key() {
        let auth = ProviderRequestAuth {
            api_key: Some("sk".into()),
            headers: None,
        };
        assert_eq!(
            require_provider_api_key("p", Some(&auth), None).unwrap(),
            "sk"
        );
    }

    #[test]
    fn require_api_key_prefers_request_over_default() {
        let auth = ProviderRequestAuth {
            api_key: Some("req".into()),
            headers: None,
        };
        assert_eq!(
            require_provider_api_key("p", Some(&auth), Some("def")).unwrap(),
            "req"
        );
    }

    #[test]
    fn require_api_key_falls_back_to_default() {
        assert_eq!(
            require_provider_api_key("p", None, Some("def")).unwrap(),
            "def"
        );
    }

    #[test]
    fn require_api_key_rejects_missing() {
        let err = require_provider_api_key("openai", None, None).unwrap_err();
        assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
        assert!(err.to_string().contains("apiKey is required"));
    }

    #[test]
    fn merge_headers_combines_maps() {
        let mut default = HashMap::new();
        default.insert("a".into(), "1".into());
        let mut request = HashMap::new();
        request.insert("b".into(), "2".into());
        let merged = merge_request_headers(Some(&default), Some(&request));
        assert_eq!(merged.as_ref().unwrap()["a"], "1");
        assert_eq!(merged.as_ref().unwrap()["b"], "2");
    }

    #[test]
    fn merge_headers_request_overrides_default() {
        let mut default = HashMap::new();
        default.insert("a".into(), "1".into());
        let mut request = HashMap::new();
        request.insert("a".into(), "2".into());
        let merged = merge_request_headers(Some(&default), Some(&request));
        assert_eq!(merged.as_ref().unwrap()["a"], "2");
    }

    #[test]
    fn resolve_uses_factory_when_present() {
        let state = AuthBackedClientState::<String> {
            cached_client: Some("cached".into()),
            client_factory: Some(Box::new(|auth| {
                format!("factory:{}", auth.api_key.as_deref().unwrap_or(""))
            })),
        };
        let auth = ProviderRequestAuth {
            api_key: Some("k".into()),
            headers: None,
        };
        let client = resolve_auth_backed_client(&state, Some(&auth), |_auth| "built".into());
        assert_eq!(client, "factory:k");
    }

    #[test]
    fn resolve_reuses_cached_when_no_auth() {
        let state = AuthBackedClientState::<String> {
            cached_client: Some("cached".into()),
            client_factory: None,
        };
        let client = resolve_auth_backed_client(&state, None, |_auth| panic!("should not build"));
        assert_eq!(client, "cached");
    }

    #[test]
    fn resolve_builds_when_auth_present() {
        let state = AuthBackedClientState::<String> {
            cached_client: Some("cached".into()),
            client_factory: None,
        };
        let auth = ProviderRequestAuth {
            api_key: Some("k".into()),
            headers: None,
        };
        let client = resolve_auth_backed_client(&state, Some(&auth), |auth| {
            format!("built:{}", auth.unwrap().api_key.as_deref().unwrap_or(""))
        });
        assert_eq!(client, "built:k");
    }
}
