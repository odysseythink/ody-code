use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use crate::config::HostConfig;
use crate::events::{AgentEvent, EventSink, PromptOrigin, TurnEndReason};
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
    turn_counter: AtomicI64,
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
            turn_counter: AtomicI64::new(0),
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
            "prompt" => Ok(self.prompt(payload).await.map_err(|e| e.to_string())?),
            "steer" => Ok(self.steer(payload).await.map_err(|e| e.to_string())?),
            "setModel" => Ok(self.set_model(payload).await.map_err(|e| e.to_string())?),
            "setThinking" => Ok(self.set_thinking(payload).await.map_err(|e| e.to_string())?),
            "setPermission" => Ok(self.set_permission(payload).await.map_err(|e| e.to_string())?),
            "listSkills" => Ok(self.list_skills()),
            "getConfig" => Ok(self.get_agent_config(payload).await.map_err(|e| e.to_string())?),
            "getOdyConfig" => Ok(self.get_ody_config()),
            "setConfig" => Ok(self.set_agent_config(payload).await.map_err(|e| e.to_string())?),
            "setOdyConfig" => Ok(self.set_ody_config(payload).await.map_err(|e| e.to_string())?),
            "getExperimentalFlags" => Ok(serde_json::json!({})),
            "getContext" => Ok(self.get_context()),
            "getPermission" => Ok(self.get_permission(payload).await.map_err(|e| e.to_string())?),
            "getPlan" => Ok(self.get_plan()),
            "getUsage" => Ok(self.get_usage()),
            "getUserLanguage" => Ok(self.get_user_language()),
            "listMcpServers" => Ok(self.list_mcp_servers()),
            "getMcpStartupMetrics" => Ok(self.get_mcp_startup_metrics()),
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
        let id = payload
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        let summary = match id {
            Some(id) => self.session_manager.create_with_id(id, Path::new(work_dir), title).await,
            None => self.session_manager.create(Path::new(work_dir), title).await,
        }
        .map_err(|e| crate::error::HostError::config_invalid(e.to_string()))?;
        self.sink.emit(AgentEvent::SessionCreated {
            session_id: summary.id.clone(),
            agent_id: None,
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
            .get("sessionId")
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
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or("missing session id")?;
        self.session_manager.close(id.to_string()).await
            .map_err(|e| e.to_string())?;
        self.sink.emit(AgentEvent::SessionClosed {
            session_id: id.to_string(),
            agent_id: None,
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

    fn get_ody_config(&self) -> serde_json::Value {
        serde_json::json!({
            "providers": [{
                "id": self.config.provider.provider_id,
                "apiKey": self.config.provider.api_key,
                "baseUrl": self.config.provider.base_url,
                "defaultModel": self.config.provider.default_model,
            }],
            "homeDir": self.config.home_dir,
        })
    }

    async fn set_ody_config(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        // Prototype: return existing config unchanged
        Ok(self.get_ody_config())
    }

    async fn get_agent_config(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        let model = session.model().await;
        let thinking = session.thinking().await.unwrap_or_else(|| "off".to_string());
        let default_model = self.config.provider.default_model.clone().unwrap_or_else(|| "gpt-4o-mini".to_string());
        Ok(serde_json::json!({
            "cwd": session.work_dir,
            "provider": {
                "id": self.config.provider.provider_id,
                "model": model.clone().unwrap_or_else(|| default_model.clone()),
            },
            "modelAlias": model.unwrap_or(default_model),
            "modelCapabilities": {
                "image_in": false,
                "video_in": false,
                "audio_in": false,
                "thinking": false,
                "tool_use": true,
                "max_context_tokens": 0,
                "max_output_tokens": 0,
            },
            "thinkingLevel": thinking,
            "systemPrompt": "",
        }))
    }

    async fn set_agent_config(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        // Prototype: agent config updates are not persisted; return current config.
        self.get_agent_config(payload).await
    }

    async fn set_model(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let model = payload
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or("missing model")?
            .to_string();
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        session.set_model(Some(model.clone())).await;
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "model": model, "providerName": self.config.provider.provider_id }))
    }

    async fn set_thinking(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let level = payload
            .get("level")
            .and_then(|v| v.as_str())
            .ok_or("missing level")?
            .to_string();
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        session.set_thinking(Some(level.clone())).await;
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({}))
    }

    async fn set_permission(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let mode = payload
            .get("mode")
            .and_then(|v| v.as_str())
            .ok_or("missing mode")?
            .to_string();
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        session.set_permission(Some(mode.clone())).await;
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({}))
    }

    fn list_skills(&self) -> serde_json::Value {
        // Prototype: no dynamic skills exposed by the Rust host.
        serde_json::json!([])
    }

    async fn prompt(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, agent_id) = self.require_session_agent(&payload)?;
        let text = extract_text_input(&payload).ok_or("missing or empty prompt input")?;
        let turn_id = self.allocate_turn_id();

        self.sink.emit(AgentEvent::TurnStarted {
            session_id: session_id.clone(),
            agent_id: agent_id.clone(),
            turn_id,
            origin: PromptOrigin::User,
        });

        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let model = session.model().await.unwrap_or_else(|| {
            self.config.provider.default_model.clone().unwrap_or_else(|| "gpt-4o-mini".to_string())
        });

        let request = ChatRequest {
            model,
            messages: vec![Message {
                role: Role::User,
                content: text,
            }],
            tools: self.tool_registry.tool_definitions(),
            stream: true,
        };

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let result = self.provider.chat_stream(request, &mut |delta| {
            if let Some(text) = delta.content {
                content.push_str(&text);
            }
            if let Some(tc) = delta.tool_call {
                tool_calls.push(tc);
            }
        }).await;

        match result {
            Ok(reason) => {
                if !tool_calls.is_empty() {
                    let approval_client = CoreHostApprovalClient { sink: self.sink.as_ref() };
                    for tc in &tool_calls {
                        let tool_result = self.tool_registry
                            .execute(&tc.name, tc.arguments.clone(), &approval_client)
                            .await
                            .unwrap_or(serde_json::json!({ "error": "tool execution failed" }));
                        self.sink.emit(AgentEvent::ToolResult {
                            session_id: session_id.clone(),
                            agent_id: Some(agent_id.clone()),
                            tool_name: tc.name.clone(),
                            result: tool_result,
                        });
                    }
                }

                if !content.is_empty() {
                    self.sink.emit(AgentEvent::AssistantDelta {
                        session_id: session_id.clone(),
                        agent_id: agent_id.clone(),
                        turn_id,
                        delta: content.clone(),
                    });
                }

                self.sink.emit(AgentEvent::TurnEnded {
                    session_id,
                    agent_id,
                    turn_id,
                    reason: TurnEndReason::Completed,
                    error: None,
                });

                let finish_reason = match reason {
                    FinishReason::Stop => "stop",
                    FinishReason::ToolCalls => "tool_calls",
                    FinishReason::Length => "length",
                    FinishReason::ContentFilter => "content_filter",
                    FinishReason::Other => "other",
                };
                Ok(serde_json::json!({ "ok": true, "finishReason": finish_reason, "content": content }))
            }
            Err(e) => {
                let message = e.to_string();
                self.sink.emit(AgentEvent::TurnEnded {
                    session_id,
                    agent_id,
                    turn_id,
                    reason: TurnEndReason::Failed,
                    error: Some(message.clone()),
                });
                Err(message)
            }
        }
    }

    async fn steer(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        // Prototype: steer is treated the same as a prompt.
        self.prompt(payload).await
    }

    fn get_context(&self) -> serde_json::Value {
        // Prototype: the Rust host does not yet maintain agent context history.
        serde_json::json!({
            "history": [],
            "tokenCount": 0,
        })
    }

    async fn get_permission(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        let mode = session.permission().await.unwrap_or_else(|| "manual".to_string());
        Ok(serde_json::json!({
            "mode": mode,
            "rules": [],
        }))
    }

    fn get_plan(&self) -> serde_json::Value {
        // Prototype: plan/design mode is not yet implemented in the Rust host.
        serde_json::Value::Null
    }

    fn get_usage(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn get_user_language(&self) -> serde_json::Value {
        serde_json::json!("en")
    }

    fn list_mcp_servers(&self) -> serde_json::Value {
        // Prototype: no MCP servers configured in the Rust host.
        serde_json::json!([])
    }

    fn get_mcp_startup_metrics(&self) -> serde_json::Value {
        serde_json::json!({ "durationMs": 0 })
    }

    fn allocate_turn_id(&self) -> i64 {
        self.turn_counter.fetch_add(1, Ordering::SeqCst)
    }

    fn require_session_agent(&self, payload: &serde_json::Value) -> Result<(String, String), String> {
        let session_id = payload
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or("missing sessionId")?
            .to_string();
        let agent_id = payload
            .get("agentId")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();
        Ok((session_id, agent_id))
    }
}

fn extract_text_input(payload: &serde_json::Value) -> Option<String> {
    let input = payload.get("input")?.as_array()?;
    let mut text = String::new();
    for part in input {
        if part.get("type")?.as_str()? == "text" {
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                text.push_str(t);
            }
        }
    }
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
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
        make_host_with_events().0
    }

    fn make_host_with_events() -> (CoreHost, Arc<Mutex<Vec<AgentEvent>>>) {
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
            mock_provider: false,
        };
        let events = Arc::new(Mutex::new(Vec::new()));
        let host = CoreHost::new(config, Box::new(MockSink(Arc::clone(&events))), Box::new(MockProvider)).unwrap();
        (host, events)
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

    #[tokio::test]
    async fn create_session_with_provided_id() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let result = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "custom-1"}))
            .await
            .unwrap();
        assert_eq!(result["id"], "custom-1");
        assert_eq!(result["workDir"], work_dir);
    }

    #[tokio::test]
    async fn create_session_without_id_uses_uuid() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let a = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir}))
            .await
            .unwrap();
        let b = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir}))
            .await
            .unwrap();
        assert!(a["id"].as_str().unwrap().len() > 10);
        assert!(b["id"].as_str().unwrap().len() > 10);
        assert_ne!(a["id"], b["id"]);
    }

    #[tokio::test]
    async fn create_session_duplicate_id_fails() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let first = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "dup-1"}))
            .await
            .unwrap();
        assert_eq!(first["id"], "dup-1");
        let err = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "dup-1"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn get_context_returns_empty_context() {
        let host = make_host();
        let result = host.dispatch("getContext", serde_json::json!({"sessionId": "s1", "agentId": "main"})).await.unwrap();
        assert!(result["history"].is_array());
        assert_eq!(result["history"].as_array().unwrap().len(), 0);
        assert_eq!(result["tokenCount"], 0);
    }

    #[tokio::test]
    async fn get_permission_returns_manual_mode() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "perm-1"}))
            .await
            .unwrap();
        let result = host
            .dispatch("getPermission", serde_json::json!({"sessionId": session["id"], "agentId": "main"}))
            .await
            .unwrap();
        assert_eq!(result["mode"], "manual");
        assert!(result["rules"].is_array());
    }

    #[tokio::test]
    async fn get_plan_returns_null() {
        let host = make_host();
        let result = host.dispatch("getPlan", serde_json::json!({"sessionId": "s1", "agentId": "main"})).await.unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn get_usage_returns_empty_object() {
        let host = make_host();
        let result = host.dispatch("getUsage", serde_json::json!({"sessionId": "s1", "agentId": "main"})).await.unwrap();
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn get_user_language_returns_en() {
        let host = make_host();
        let result = host.dispatch("getUserLanguage", serde_json::json!({"sessionId": "s1", "agentId": "main"})).await.unwrap();
        assert_eq!(result, "en");
    }

    #[tokio::test]
    async fn list_mcp_servers_returns_empty_array() {
        let host = make_host();
        let result = host.dispatch("listMcpServers", serde_json::json!({"sessionId": "s1"})).await.unwrap();
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_mcp_startup_metrics_returns_zero() {
        let host = make_host();
        let result = host.dispatch("getMcpStartupMetrics", serde_json::json!({"sessionId": "s1"})).await.unwrap();
        assert_eq!(result["durationMs"], 0);
    }

    #[tokio::test]
    async fn get_ody_config_returns_host_config() {
        let host = make_host();
        let result = host.dispatch("getOdyConfig", serde_json::json!({})).await.unwrap();
        assert!(result["providers"].is_array());
        assert!(result["homeDir"].is_string());
    }

    #[tokio::test]
    async fn get_agent_config_returns_session_scoped_config() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "cfg-1"}))
            .await
            .unwrap();
        let result = host
            .dispatch("getConfig", serde_json::json!({"sessionId": session["id"], "agentId": "main"}))
            .await
            .unwrap();
        assert!(result["modelAlias"].is_string());
        assert!(result["modelCapabilities"].is_object());
        assert_eq!(result["thinkingLevel"], "off");
    }

    #[tokio::test]
    async fn set_model_updates_session_model() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "model-1"}))
            .await
            .unwrap();
        let result = host
            .dispatch("setModel", serde_json::json!({"sessionId": session["id"], "agentId": "main", "model": "gpt-4o"}))
            .await
            .unwrap();
        assert_eq!(result["model"], "gpt-4o");

        let config = host
            .dispatch("getConfig", serde_json::json!({"sessionId": session["id"], "agentId": "main"}))
            .await
            .unwrap();
        assert_eq!(config["modelAlias"], "gpt-4o");
    }

    #[tokio::test]
    async fn set_thinking_updates_session_thinking() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "think-1"}))
            .await
            .unwrap();
        host
            .dispatch("setThinking", serde_json::json!({"sessionId": session["id"], "agentId": "main", "level": "on"}))
            .await
            .unwrap();
        let config = host
            .dispatch("getConfig", serde_json::json!({"sessionId": session["id"], "agentId": "main"}))
            .await
            .unwrap();
        assert_eq!(config["thinkingLevel"], "on");
    }

    #[tokio::test]
    async fn set_permission_updates_session_permission() {
        let host = make_host();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "perm-2"}))
            .await
            .unwrap();
        host
            .dispatch("setPermission", serde_json::json!({"sessionId": session["id"], "agentId": "main", "mode": "yolo"}))
            .await
            .unwrap();
        let permission = host
            .dispatch("getPermission", serde_json::json!({"sessionId": session["id"], "agentId": "main"}))
            .await
            .unwrap();
        assert_eq!(permission["mode"], "yolo");
    }

    #[tokio::test]
    async fn list_skills_returns_empty_array() {
        let host = make_host();
        let result = host
            .dispatch("listSkills", serde_json::json!({"sessionId": "s1", "agentId": "main"}))
            .await
            .unwrap();
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn prompt_emits_turn_events_and_returns_ok() {
        let (host, events) = make_host_with_events();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "prompt-1"}))
            .await
            .unwrap();
        let result = host
            .dispatch(
                "prompt",
                serde_json::json!({
                    "sessionId": session["id"],
                    "agentId": "main",
                    "input": [{"type": "text", "text": "hello"}]
                }),
            )
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["content"], "ok");

        let event_types: Vec<String> = events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AgentEvent::TurnStarted { .. } => Some("turn.started".to_string()),
                AgentEvent::AssistantDelta { .. } => Some("assistant.delta".to_string()),
                AgentEvent::TurnEnded { .. } => Some("turn.ended".to_string()),
                _ => None,
            })
            .collect();
        assert!(event_types.contains(&"turn.started".to_string()));
        assert!(event_types.contains(&"assistant.delta".to_string()));
        assert!(event_types.contains(&"turn.ended".to_string()));
    }

    #[tokio::test]
    async fn steer_emits_turn_events() {
        let (host, events) = make_host_with_events();
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "steer-1"}))
            .await
            .unwrap();
        let result = host
            .dispatch(
                "steer",
                serde_json::json!({
                    "sessionId": session["id"],
                    "agentId": "main",
                    "input": [{"type": "text", "text": "steer me"}]
                }),
            )
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        let has_turn_ended = events.lock().unwrap().iter().any(|e| matches!(e, AgentEvent::TurnEnded { .. }));
        assert!(has_turn_ended);
    }
}
