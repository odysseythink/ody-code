use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::RpcError;
use crate::events::{AgentEvent, EventSink};
use crate::host::CoreHost;
use crate::transport::ByteDispatch;

pub struct RpcRouter {
    host: Arc<CoreHost>,
}

impl RpcRouter {
    pub fn new(host: Arc<CoreHost>) -> Self {
        Self { host }
    }

    pub async fn route(&self, request_bytes: &[u8]) -> Result<Vec<u8>, RpcError> {
        let wrapper: RpcRequestWrapper = serde_json::from_slice(request_bytes)?;
        let payload = wrapper
            .args
            .into_iter()
            .next()
            .unwrap_or(serde_json::Value::Null);
        let result = self.host.dispatch(&wrapper.method, payload).await;
        let response = match result {
            Ok(value) => RpcResponseWrapper {
                ok: true,
                value,
                error: None,
            },
            Err(e) => RpcResponseWrapper {
                ok: false,
                value: serde_json::Value::Null,
                error: Some(RpcErrorJson {
                    message: e.to_string(),
                    code: Some("internal".to_string()),
                }),
            },
        };
        Ok(serde_json::to_vec(&response)?)
    }

    pub fn into_byte_dispatch(self) -> Arc<ByteDispatch> {
        let router = Arc::new(self);
        Arc::new(move |bytes: &[u8]| {
            let router = Arc::clone(&router);
            let owned = bytes.to_vec();
            Box::pin(async move { router.route(&owned).await })
        })
    }
}

/// A cloneable wrapper around an EventSink, enabling shared access
/// to the transport's event sink from multiple components.
#[derive(Clone)]
pub struct TransportEventSink {
    inner: Arc<dyn EventSink>,
}

impl TransportEventSink {
    pub fn new(inner: Box<dyn EventSink>) -> Self {
        Self {
            inner: Arc::from(inner),
        }
    }
}

#[async_trait::async_trait]
impl EventSink for TransportEventSink {
    async fn request(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
        self.inner.request(method, payload).await
    }

    fn emit(&self, event: AgentEvent) {
        self.inner.emit(event);
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequestWrapper {
    method: String,
    args: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct RpcResponseWrapper {
    ok: bool,
    value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcErrorJson>,
}

#[derive(Debug, Serialize)]
struct RpcErrorJson {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HostConfig, ProviderConfig, TransportMode};
    use crate::events::AgentEvent;
    use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmProvider};

    struct MockProvider;
    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn chat_stream(
            &self,
            _request: ChatRequest,
            on_delta: &mut (dyn FnMut(ChatDelta) + Send),
        ) -> Result<FinishReason, crate::llm::LlmError> {
            on_delta(ChatDelta {
                index: 0,
                content: Some("ok".to_string()),
                tool_call: None,
            });
            Ok(FinishReason::Stop)
        }
    }

    struct MockSink;
    #[async_trait::async_trait]
    impl EventSink for MockSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            Err(RpcError::MethodNotFound("mock".to_string()))
        }
        fn emit(&self, _event: AgentEvent) {}
    }

    fn make_host() -> Arc<CoreHost> {
        let config = HostConfig {
            home_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            config_path: None,
            transport: TransportMode::Stdio,
            log_level: crate::config::LogLevel::Info,
            provider: ProviderConfig {
                provider_id: "mock".to_string(),
                api_key: "".to_string(),
                base_url: None,
                default_model: Some("mock".to_string()),
            },
            mock_provider: false,
        };
        Arc::new(CoreHost::new(config, Arc::new(MockSink), Arc::new(MockProvider)).unwrap())
    }

    #[tokio::test]
    async fn routes_get_core_info() {
        let router = RpcRouter::new(make_host());
        let request = br#"{"method":"getCoreInfo","args":[{}]}"#;
        let response = router.route(request).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(json["ok"], true);
        assert!(json["value"]["version"].is_string());
    }

    #[tokio::test]
    async fn returns_error_for_unknown_method() {
        let router = RpcRouter::new(make_host());
        let request = br#"{"method":"unknown","args":[{}]}"#;
        let response = router.route(request).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(json["ok"], false);
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown"));
    }
}
