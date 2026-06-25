use std::path::Path;
use std::sync::Arc;

use crate::config::HostConfig;
use crate::events::{AgentEvent, EventSink};
use crate::llm::{ChatRequest, FinishReason, LlmProvider, Message, Role};
use crate::session::{SessionManager, SessionStoreAdapter};
use crate::tools::{ApprovalClient, ApprovalRequest, ApprovalResponse, ToolError, ToolRegistry};
use crate::tools::bash::BashTool;

pub struct CoreHost {
    pub config: HostConfig,
    pub session_manager: SessionManager,
    tool_registry: ToolRegistry,
    provider: Box<dyn LlmProvider>,
    sink: Box<dyn EventSink>,
}

pub struct CoreHostApprovalClient<'a> {
    pub sink: &'a dyn EventSink,
}

#[async_trait::async_trait]
impl ApprovalClient for CoreHostApprovalClient<'_> {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalResponse, ToolError> {
        let payload = serde_json::to_vec(&request)
            .map_err(|e| ToolError::ApprovalFailed { source: Box::new(e) })?;
        let response_bytes = self
            .sink
            .request("requestApproval", payload)
            .await
            .map_err(|e| ToolError::ApprovalFailed { source: Box::new(e) })?;
        let response = serde_json::from_slice::<ApprovalResponse>(&response_bytes)
            .map_err(|e| ToolError::ApprovalFailed { source: Box::new(e) })?;
        Ok(response)
    }
}

impl CoreHost {
    pub fn new(
        config: HostConfig,
        sink: Box<dyn EventSink>,
        provider: Box<dyn LlmProvider>,
    ) -> Result<Self, crate::error::HostError> {
        let store = SessionStoreAdapter::new(config.home_dir.clone());
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register(Arc::new(BashTool));

        Ok(Self {
            session_manager: SessionManager::new(store),
            tool_registry,
            provider,
            sink,
            config,
        })
    }

    pub async fn dispatch(&self, method: &str, payload: serde_json::Value) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        match method {
            "getCoreInfo" => Ok(self.get_core_info()),
            "createSession" => Ok(self.create_session(payload).await.map_err(|e| e.to_string())?),
            "resumeSession" => Ok(self.resume_session(payload).await.map_err(|e| e.to_string())?),
            "listSessions" => Ok(self.list_sessions(payload).await.map_err(|e| e.to_string())?),
            "closeSession" => Ok(self.close_session(payload).await.map_err(|e| e.to_string())?),
            "chat" => Ok(self.chat(payload).await.map_err(|e| e.to_string())?),
            "getConfig" => Ok(self.get_config().map_err(|e| e.to_string())?),
            "setConfig" => Ok(self.set_config(payload).await.map_err(|e| e.to_string())?),
            "getExperimentalFlags" => Ok(serde_json::json!({})),
            _ => Err(format!("unknown method: {method}").into()),
        }
    }

    fn get_core_info(&self) -> serde_json::Value {
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "name": "ody-host",
            "provider": self.config.provider.provider_id,
        })
    }

    async fn create_session(&self, payload: serde_json::Value) -> Result<serde_json::Value, crate::error::HostError> {
        let work_dir = payload
            .get("workDir")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let title = payload.get("title").and_then(|v| v.as_str());
        let summary = self.session_manager.create(Path::new(work_dir), title).await
            .map_err(|e| crate::error::HostError::config_invalid(e.to_string()))?;
        self.sink.emit(AgentEvent::SessionCreated {
            session_id: summary.id.clone(),
            work_dir: work_dir.to_string(),
        });
        Ok(serde_json::json!({
            "id": summary.id,
            "workDir": summary.work_dir,
            "title": summary.title,
            "createdAtMs": summary.created_at_ms,
            "updatedAtMs": summary.updated_at_ms,
        }))
    }

    async fn resume_session(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let id = payload
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("missing session id")?;
        let session = self.session_manager.get(id.to_string()).await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": session.id,
            "workDir": session.work_dir,
            "resumeState": {}
        }))
    }

    async fn list_sessions(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let summaries = self.session_manager.list(Default::default()).await
            .map_err(|e| e.to_string())?;
        let list: Vec<serde_json::Value> = summaries
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "workDir": s.work_dir,
                    "title": s.title,
                    "createdAtMs": s.created_at_ms,
                    "updatedAtMs": s.updated_at_ms,
                })
            })
            .collect();
        Ok(serde_json::json!(list))
    }

    async fn close_session(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let id = payload
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("missing session id")?;
        self.session_manager.close(id.to_string()).await
            .map_err(|e| e.to_string())?;
        self.sink.emit(AgentEvent::SessionClosed {
            session_id: id.to_string(),
        });
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn chat(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let _session_id = payload
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let prompt = payload
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or("missing prompt")?;

        let request = ChatRequest {
            model: self.config.provider.default_model.clone().unwrap_or_else(|| "gpt-4o-mini".to_string()),
            messages: vec![Message {
                role: Role::User,
                content: prompt.to_string(),
            }],
            tools: self.tool_registry.tool_definitions(),
            stream: true,
        };

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let reason = self.provider.chat_stream(request, &mut |delta| {
            if let Some(text) = delta.content {
                content.push_str(&text);
            }
            if let Some(tc) = delta.tool_call {
                tool_calls.push(tc);
            }
        }).await.map_err(|e| e.to_string())?;

        // Execute tool calls if any
        if !tool_calls.is_empty() {
            let approval_client = CoreHostApprovalClient { sink: self.sink.as_ref() };
            let mut results = Vec::new();
            for tc in &tool_calls {
                let result = self.tool_registry
                    .execute(&tc.name, tc.arguments.clone(), &approval_client)
                    .await
                    .unwrap_or(serde_json::json!({ "error": "tool execution failed" }));
                results.push(serde_json::json!({
                    "toolName": tc.name,
                    "result": result,
                }));
            }
            return Ok(serde_json::json!({
                "content": content,
                "finishReason": "tool_calls",
                "toolResults": results,
            }));
        }

        Ok(serde_json::json!({
            "content": content,
            "finishReason": match reason {
                FinishReason::Stop => "stop",
                FinishReason::ToolCalls => "tool_calls",
                FinishReason::Length => "length",
                FinishReason::ContentFilter => "content_filter",
                FinishReason::Other => "other",
            },
        }))
    }

    fn get_config(&self) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "providers": [{
                "id": self.config.provider.provider_id,
                "apiKey": self.config.provider.api_key,
                "baseUrl": self.config.provider.base_url,
                "defaultModel": self.config.provider.default_model,
            }],
            "homeDir": self.config.home_dir,
        }))
    }

    async fn set_config(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        // Prototype: return existing config unchanged
        self.get_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HostConfig, LogLevel, ProviderConfig, TransportMode};
    use crate::error::RpcError;
    use crate::events::AgentEvent;
    use crate::llm::{ChatDelta, LlmProvider};
    use crate::tools::{ApprovalDecision, ApprovalResponse};
    use std::sync::{Arc, Mutex};

    struct MockProvider;
    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn chat_stream(
            &self,
            _request: ChatRequest,
            on_delta: &mut (dyn FnMut(ChatDelta) + Send),
        ) -> Result<FinishReason, crate::llm::LlmError> {
            on_delta(ChatDelta { index: 0, content: Some("ok".to_string()), tool_call: None });
            Ok(FinishReason::Stop)
        }
    }

    struct MockSink(Arc<Mutex<Vec<AgentEvent>>>);

    #[async_trait::async_trait]
    impl EventSink for MockSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            let resp = ApprovalResponse { decision: ApprovalDecision::Cancelled };
            Ok(serde_json::to_vec(&resp).unwrap())
        }
        fn emit(&self, event: AgentEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn make_host() -> CoreHost {
        let config = HostConfig {
            home_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            config_path: None,
            transport: TransportMode::Stdio,
            log_level: LogLevel::Info,
            provider: ProviderConfig {
                provider_id: "mock".to_string(),
                api_key: "".to_string(),
                base_url: None,
                default_model: Some("mock".to_string()),
            },
        };
        CoreHost::new(config, Box::new(MockSink(Arc::new(Mutex::new(Vec::new())))), Box::new(MockProvider)).unwrap()
    }

    #[tokio::test]
    async fn get_core_info_returns_version() {
        let host = make_host();
        let result = host.dispatch("getCoreInfo", serde_json::json!({})).await.unwrap();
        assert_eq!(result["name"], "ody-host");
        assert!(result["version"].is_string());
    }

    #[tokio::test]
    async fn create_session_returns_summary() {
        let host = make_host();
        let result = host.dispatch("createSession", serde_json::json!({"workDir": "/tmp"})).await.unwrap();
        assert!(result["id"].is_string());
        assert!(result["workDir"].is_string());
    }

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let host = make_host();
        let err = host.dispatch("nosuch", serde_json::json!({})).await.unwrap_err();
        assert!(err.to_string().contains("unknown method"));
    }

    #[tokio::test]
    async fn chat_returns_content() {
        let host = make_host();
        let result = host.dispatch("chat", serde_json::json!({"sessionId": "s1", "prompt": "hi"})).await.unwrap();
        assert_eq!(result["content"], "ok");
        assert_eq!(result["finishReason"], "stop");
    }
}
