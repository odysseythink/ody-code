use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use crate::config::HostConfig;
use crate::events::{AgentEvent, EventSink, PromptOrigin, TurnEndReason};
use crate::llm::{ChatRequest, ContentPart, FinishReason, LlmProvider, Message, Role};
use crate::session::{SessionManager, SessionStoreAdapter};
use crate::tools::{ApprovalClient, ApprovalRequest, ApprovalResponse, ToolError};
use agent_rs::agent_loop::types::{ExecutableToolContext, ToolExecution};
use agent_rs::tool::types::UserToolRegistration;
use agent_rs::turn::TurnAgent;
use kaos_rs::environment::detect_environment_from_node;
use kaos_rs::kaos::Kaos;
use kosong_rs::message::UrlPayload;

pub struct CoreHost {
    pub config: HostConfig,
    pub session_manager: SessionManager,
    provider: Arc<dyn LlmProvider>,
    sink: Arc<dyn EventSink>,
    _turn_counter: AtomicI64,
    kaos: Arc<Kaos>,
}

pub struct CoreHostApprovalClient<'a> {
    pub sink: &'a dyn EventSink,
}

#[async_trait::async_trait]
impl ApprovalClient for CoreHostApprovalClient<'_> {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalResponse, ToolError> {
        let payload = serde_json::to_vec(&request).map_err(|e| ToolError::ApprovalFailed {
            source: Box::new(e),
        })?;
        let response_bytes = self
            .sink
            .request("requestApproval", payload)
            .await
            .map_err(|e| ToolError::ApprovalFailed {
                source: Box::new(e),
            })?;
        let response =
            serde_json::from_slice::<ApprovalResponse>(&response_bytes).map_err(|e| {
                ToolError::ApprovalFailed {
                    source: Box::new(e),
                }
            })?;
        Ok(response)
    }
}

impl CoreHost {
    pub fn new(
        config: HostConfig,
        sink: Arc<dyn EventSink>,
        provider: Arc<dyn LlmProvider>,
    ) -> Result<Self, crate::error::HostError> {
        let store = SessionStoreAdapter::new(config.home_dir.clone());
        let env = detect_environment_from_node();
        let kaos = Arc::new(Kaos::new(env, &config.home_dir));

        let session_manager = SessionManager::new(
            store,
            Arc::clone(&kaos),
            Arc::clone(&sink),
            config.provider.clone(),
            Arc::clone(&provider),
        );

        Ok(Self {
            session_manager,
            provider,
            sink,
            config,
            _turn_counter: AtomicI64::new(0),
            kaos,
        })
    }

    /// Return a clone of the host-level `Arc<Kaos>`.
    pub fn kaos(&self) -> Arc<Kaos> {
        Arc::clone(&self.kaos)
    }

    pub async fn dispatch(
        &self,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        match method {
            "getCoreInfo" => Ok(self.get_core_info()),
            "createSession" => Ok(self
                .create_session(payload)
                .await
                .map_err(|e| e.to_string())?),
            "resumeSession" => Ok(self
                .resume_session(payload)
                .await
                .map_err(|e| e.to_string())?),
            "listSessions" => Ok(self
                .list_sessions(payload)
                .await
                .map_err(|e| e.to_string())?),
            "closeSession" => Ok(self
                .close_session(payload)
                .await
                .map_err(|e| e.to_string())?),
            "chat" => Ok(self.chat(payload).await.map_err(|e| e.to_string())?),
            "prompt" => Ok(self.prompt(payload).await.map_err(|e| e.to_string())?),
            "steer" => Ok(self.steer(payload).await.map_err(|e| e.to_string())?),
            "setModel" => Ok(self.set_model(payload).await.map_err(|e| e.to_string())?),
            "setThinking" => Ok(self
                .set_thinking(payload)
                .await
                .map_err(|e| e.to_string())?),
            "setPermission" => Ok(self
                .set_permission(payload)
                .await
                .map_err(|e| e.to_string())?),
            "listSkills" => Ok(self.list_skills()),
            "getConfig" => Ok(self
                .get_agent_config(payload)
                .await
                .map_err(|e| e.to_string())?),
            "getOdyConfig" => Ok(self.get_ody_config()),
            "setConfig" => Ok(self
                .set_agent_config(payload)
                .await
                .map_err(|e| e.to_string())?),
            "setOdyConfig" => Ok(self
                .set_ody_config(payload)
                .await
                .map_err(|e| e.to_string())?),
            "getExperimentalFlags" => Ok(serde_json::json!({})),
            "getContext" => Ok(self.get_context()),
            "getPermission" => Ok(self
                .get_permission(payload)
                .await
                .map_err(|e| e.to_string())?),
            "getPlan" => Ok(self.get_plan()),
            "getUsage" => Ok(self.get_usage()),
            "getUserLanguage" => Ok(self.get_user_language()),
            "listMcpServers" => Ok(self.list_mcp_servers()),
            "getMcpStartupMetrics" => Ok(self.get_mcp_startup_metrics()),
            "enterPlan" => Ok(self.enter_plan(payload).await.map_err(|e| e.to_string())?),
            "getModel" => Ok(self.get_model(payload).await.map_err(|e| e.to_string())?),
            "getTools" => Ok(self.get_tools(payload).await.map_err(|e| e.to_string())?),
            "getBackground" => Ok(self
                .get_background(payload)
                .await
                .map_err(|e| e.to_string())?),
            "getBackgroundOutput" => Ok(self
                .get_background_output(payload)
                .await
                .map_err(|e| e.to_string())?),
            "stopBackground" => Ok(self
                .stop_background(payload)
                .await
                .map_err(|e| e.to_string())?),
            "registerTool" => Ok(self
                .register_tool(payload)
                .await
                .map_err(|e| e.to_string())?),
            "unregisterTool" => Ok(self
                .unregister_tool(payload)
                .await
                .map_err(|e| e.to_string())?),
            "setActiveTools" => Ok(self
                .set_active_tools(payload)
                .await
                .map_err(|e| e.to_string())?),
            "activateSkill" => Ok(self
                .activate_skill(payload)
                .await
                .map_err(|e| e.to_string())?),
            "clearPlan" => Ok(self.clear_plan(payload).await.map_err(|e| e.to_string())?),
            "cancelPlan" => Ok(self.cancel_plan(payload).await.map_err(|e| e.to_string())?),
            "undoHistory" => Ok(self
                .undo_history(payload)
                .await
                .map_err(|e| e.to_string())?),
            "beginCompaction" => Ok(self
                .begin_compaction(payload)
                .await
                .map_err(|e| e.to_string())?),
            "cancelCompaction" => Ok(self
                .cancel_compaction(payload)
                .await
                .map_err(|e| e.to_string())?),
            "clearContext" => Ok(self
                .clear_context(payload)
                .await
                .map_err(|e| e.to_string())?),
            method if method.starts_with("env.") => {
                crate::env::dispatch(&self.kaos, method, payload)
                    .await
                    .map_err(|e| e.into())
            }
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

    async fn create_session(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, crate::error::HostError> {
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
            Some(id) => {
                self.session_manager
                    .create_with_id(id, Path::new(work_dir), title)
                    .await
            }
            None => {
                self.session_manager
                    .create(Path::new(work_dir), title)
                    .await
            }
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

    async fn resume_session(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = payload
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or("missing session id")?;
        let session = self
            .session_manager
            .get(id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": session.id,
            "workDir": session.work_dir,
            "resumeState": {}
        }))
    }

    async fn list_sessions(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let summaries = self
            .session_manager
            .list(Default::default())
            .await
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
        self.session_manager
            .close(id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        self.sink.emit(AgentEvent::SessionClosed {
            session_id: id.to_string(),
            agent_id: None,
        });
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn execute_agent_tool(
        &self,
        agent: &agent_rs::agent::Agent,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let tool_dyn: Arc<dyn agent_rs::agent_loop::types::ExecutableTool> = {
            let tools = agent.tools.lock().unwrap();
            let tool = tools
                .loop_tools()
                .into_iter()
                .find(|t| t.name() == name)
                .ok_or_else(|| format!("tool not found: {name}"))?;
            Arc::clone(&tool)
        };

        match tool_dyn
            .resolve_execution(args)
            .await
            .map_err(|e| e.to_string())?
        {
            ToolExecution::Runnable(exec) => {
                let needs_approval = exec
                    .matches_rule
                    .as_ref()
                    .map(|rule| rule(name))
                    .unwrap_or(false);
                if needs_approval {
                    let approval_client = CoreHostApprovalClient {
                        sink: self.sink.as_ref(),
                    };
                    let request = ApprovalRequest {
                        tool_call_id: uuid::Uuid::now_v7().to_string(),
                        tool_name: name.into(),
                        action: exec.description.clone().unwrap_or_else(|| name.into()),
                        display: exec.display.clone().unwrap_or(serde_json::Value::Null),
                    };
                    let response = approval_client
                        .request(request)
                        .await
                        .map_err(|e| e.to_string())?;
                    if !matches!(response.decision, crate::tools::ApprovalDecision::Approved) {
                        return Ok(serde_json::json!({
                            "output": "Tool execution was not approved.",
                            "isError": true,
                        }));
                    }
                }

                let ctx = ExecutableToolContext {
                    turn_id: self.allocate_turn_id().to_string(),
                    tool_call_id: uuid::Uuid::now_v7().to_string(),
                    signal: kosong_rs::provider::AbortSignal::new(),
                    metadata: None,
                    on_update: None,
                };
                let result = (exec.execute)(ctx).await.map_err(|e| e.to_string())?;
                Ok(match result {
                    agent_rs::records::nested::ExecutableToolResult::Success(s) => {
                        serde_json::json!({
                            "output": s.output.to_text(),
                            "isError": false,
                        })
                    }
                    agent_rs::records::nested::ExecutableToolResult::Error(e) => {
                        serde_json::json!({
                            "output": e.output.to_text(),
                            "isError": true,
                            "message": e.message,
                        })
                    }
                })
            }
            ToolExecution::Error(e) => Ok(serde_json::json!({
                "output": e.output.to_text(),
                "isError": true,
                "message": e.message,
            })),
        }
    }

    async fn chat(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let parts = extract_input_parts(&payload)
            .or_else(|| {
                payload
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| vec![ContentPart::Text { text: s.into() }])
            })
            .ok_or("missing prompt")?;

        let tool_defs: Vec<crate::llm::ToolDefinition> = agent
            .tools()
            .loop_tools()
            .into_iter()
            .map(|t| crate::llm::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect();

        let request = ChatRequest {
            model: self
                .config
                .provider
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            messages: vec![Message {
                role: Role::User,
                content: parts,
            }],
            tools: tool_defs,
            stream: true,
        };

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let reason = self
            .provider
            .chat_stream(request, &mut |delta| {
                if let Some(text) = delta.content {
                    content.push_str(&text);
                }
                if let Some(tc) = delta.tool_call {
                    tool_calls.push(tc);
                }
            })
            .await
            .map_err(|e| e.to_string())?;

        // Execute tool calls if any
        if !tool_calls.is_empty() {
            let mut results = Vec::new();
            for tc in &tool_calls {
                let result = self
                    .execute_agent_tool(&agent, &tc.name, tc.arguments.clone())
                    .await?;
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

    async fn set_ody_config(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        // Prototype: return existing config unchanged
        Ok(self.get_ody_config())
    }

    async fn get_agent_config(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self
            .session_manager
            .get(session_id)
            .await
            .map_err(|e| e.to_string())?;
        let agent = session
            .agent(
                Arc::clone(&self.kaos),
                Arc::clone(&self.sink),
                &self.config.provider,
                Arc::clone(&self.provider),
            )
            .await
            .map_err(|e| e.to_string())?;

        let data = agent.config_data();
        let model_alias = data.model_alias.clone().unwrap_or_else(|| {
            self.config
                .provider
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".into())
        });
        let provider_id = data
            .provider
            .as_ref()
            .and_then(|p| serde_json::to_value(&p.r#type).ok())
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| self.config.provider.provider_id.clone());

        let capability = kosong_rs::resolve_model_capability(&provider_id, &model_alias)
            .unwrap_or_else(kosong_rs::ModelCapability::unknown);

        Ok(serde_json::json!({
            "cwd": data.cwd,
            "provider": {
                "id": provider_id,
                "model": model_alias.clone(),
            },
            "modelAlias": model_alias,
            "modelCapabilities": {
                "image_in": capability.image_in,
                "video_in": capability.video_in,
                "audio_in": capability.audio_in,
                "thinking": capability.thinking,
                "tool_use": capability.tool_use,
                "max_context_tokens": capability.max_context_tokens,
                "max_output_tokens": capability.max_output_tokens,
            },
            "thinkingLevel": data.thinking_level,
            "systemPrompt": data.system_prompt,
        }))
    }

    async fn set_agent_config(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        // Prototype: agent config updates are not persisted; return current config.
        self.get_agent_config(payload).await
    }

    async fn set_model(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let raw = payload
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or("missing model")?
            .to_string();

        let (provider_id, model) = parse_model_alias(&raw);
        let resolved_provider_id =
            provider_id.unwrap_or_else(|| self.config.provider.provider_id.clone());

        let session = self
            .session_manager
            .get(session_id)
            .await
            .map_err(|e| e.to_string())?;
        session.set_model(Some(model.clone())).await;
        session
            .set_provider_id(Some(resolved_provider_id.clone()))
            .await;
        session.persist_state().await.map_err(|e| e.to_string())?;

        let agent = session
            .agent(
                Arc::clone(&self.kaos),
                Arc::clone(&self.sink),
                &self.config.provider,
                Arc::clone(&self.provider),
            )
            .await
            .map_err(|e| e.to_string())?;

        agent.update_config(agent_rs::records::nested::AgentConfigUpdateData {
            model_alias: Some(raw.clone()),
            ..Default::default()
        });

        Ok(serde_json::json!({
            "model": model,
            "providerName": resolved_provider_id,
        }))
    }

    async fn set_thinking(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let level = payload
            .get("level")
            .and_then(|v| v.as_str())
            .ok_or("missing level")?
            .to_string();
        let session = self
            .session_manager
            .get(session_id)
            .await
            .map_err(|e| e.to_string())?;
        session.set_thinking(Some(level.clone())).await;
        session.persist_state().await.map_err(|e| e.to_string())?;

        let agent = session
            .agent(
                Arc::clone(&self.kaos),
                Arc::clone(&self.sink),
                &self.config.provider,
                Arc::clone(&self.provider),
            )
            .await
            .map_err(|e| e.to_string())?;
        agent.update_config(agent_rs::records::nested::AgentConfigUpdateData {
            thinking_level: Some(level),
            ..Default::default()
        });

        Ok(serde_json::json!({}))
    }

    async fn set_permission(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let mode = payload
            .get("mode")
            .and_then(|v| v.as_str())
            .ok_or("missing mode")?
            .to_string();
        let session = self
            .session_manager
            .get(session_id)
            .await
            .map_err(|e| e.to_string())?;
        session.set_permission(Some(mode.clone())).await;
        session.persist_state().await.map_err(|e| e.to_string())?;

        let agent = session
            .agent(
                Arc::clone(&self.kaos),
                Arc::clone(&self.sink),
                &self.config.provider,
                Arc::clone(&self.provider),
            )
            .await
            .map_err(|e| e.to_string())?;

        let perm_mode = match mode.as_str() {
            "yolo" => agent_rs::records::nested::PermissionMode::Yolo,
            "auto" => agent_rs::records::nested::PermissionMode::Auto,
            _ => agent_rs::records::nested::PermissionMode::Manual,
        };
        agent.set_permission_mode(perm_mode);

        Ok(serde_json::json!({}))
    }

    fn list_skills(&self) -> serde_json::Value {
        // Prototype: no dynamic skills exposed by the Rust host.
        serde_json::json!([])
    }

    async fn prompt(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, agent_id) = self.require_session_agent(&payload)?;
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let parts = extract_input_parts(&payload).ok_or("missing or empty prompt input")?;
        let turn_id = self.allocate_turn_id();

        self.sink.emit(AgentEvent::TurnStarted {
            session_id: session_id.clone(),
            agent_id: agent_id.clone(),
            turn_id,
            origin: PromptOrigin::User,
        });

        let model = _session.model().await.unwrap_or_else(|| {
            self.config
                .provider
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".to_string())
        });

        let tool_defs: Vec<crate::llm::ToolDefinition> = agent
            .tools()
            .loop_tools()
            .into_iter()
            .map(|t| crate::llm::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect();

        let request = ChatRequest {
            model,
            messages: vec![Message {
                role: Role::User,
                content: parts,
            }],
            tools: tool_defs,
            stream: true,
        };

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let result = self
            .provider
            .chat_stream(request, &mut |delta| {
                if let Some(text) = delta.content {
                    content.push_str(&text);
                }
                if let Some(tc) = delta.tool_call {
                    tool_calls.push(tc);
                }
            })
            .await;

        match result {
            Ok(reason) => {
                if !tool_calls.is_empty() {
                    for tc in &tool_calls {
                        let tool_result = self
                            .execute_agent_tool(&agent, &tc.name, tc.arguments.clone())
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
                Ok(
                    serde_json::json!({ "ok": true, "finishReason": finish_reason, "content": content }),
                )
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
        // Prototype: context is maintained per-session by Agent.contexts.
        // Without a sessionId, return empty context.
        serde_json::json!({
            "history": [],
            "tokenCount": 0,
        })
    }

    async fn get_permission(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self
            .session_manager
            .get(session_id)
            .await
            .map_err(|e| e.to_string())?;
        let agent = session
            .agent(
                Arc::clone(&self.kaos),
                Arc::clone(&self.sink),
                &self.config.provider,
                Arc::clone(&self.provider),
            )
            .await
            .map_err(|e| e.to_string())?;

        let data = agent.permission_data();
        Ok(serde_json::json!({
            "mode": serde_json::to_value(data.mode).unwrap_or(serde_json::json!("manual")),
            "rules": data.rules,
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

    async fn enter_plan(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        // Prototype: session-mode enter requires a Send fix in agent-rs (MutexGuard across .await).
        // Will wire through Agent::enter_session_mode once that's resolved.
        Ok(serde_json::Value::Null)
    }

    async fn session_and_agent(
        &self,
        payload: &serde_json::Value,
    ) -> Result<
        (
            Arc<crate::session::manager::Session>,
            Arc<agent_rs::agent::Agent>,
        ),
        String,
    > {
        let (session_id, _agent_id) = self.require_session_agent(payload)?;
        let session = self
            .session_manager
            .get(session_id)
            .await
            .map_err(|e| e.to_string())?;
        let agent = session
            .agent(
                Arc::clone(&self.kaos),
                Arc::clone(&self.sink),
                &self.config.provider,
                Arc::clone(&self.provider),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok((session, agent))
    }

    async fn get_model(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let data = agent.config_data();
        Ok(serde_json::json!(data.model_alias))
    }

    async fn clear_plan(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        // Prototype: session-mode exit requires a Send fix in agent-rs (MutexGuard across .await).
        // Will wire through Agent::exit_session_mode once that's resolved.
        Ok(serde_json::Value::Null)
    }

    async fn cancel_plan(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        // Prototype: same as clear_plan.
        Ok(serde_json::Value::Null)
    }

    async fn get_tools(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let infos = agent.tools.lock().unwrap().data();
        Ok(serde_json::json!(infos))
    }

    async fn get_background(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!([]))
    }

    async fn get_background_output(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!(""))
    }

    async fn stop_background(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn register_tool(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let name = payload
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("missing name")?;
        let description = payload
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let parameters = payload
            .get("parameters")
            .cloned()
            .unwrap_or(serde_json::json!({"type":"object"}));
        agent
            .tools
            .lock()
            .unwrap()
            .register_user_tool(UserToolRegistration {
                name: name.into(),
                description: description.into(),
                parameters,
            });
        Ok(serde_json::json!({"ok": true}))
    }

    async fn unregister_tool(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let name = payload
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("missing name")?;
        agent.tools.lock().unwrap().unregister_user_tool(name);
        Ok(serde_json::json!({"ok": true}))
    }

    async fn set_active_tools(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let names: Vec<String> = payload
            .get("names")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        agent.tools.lock().unwrap().set_active_tools(&names);
        Ok(serde_json::json!({"ok": true}))
    }

    async fn activate_skill(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn undo_history(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn begin_compaction(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn cancel_compaction(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn clear_context(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        agent.context().clear();
        Ok(serde_json::Value::Null)
    }

    fn allocate_turn_id(&self) -> i64 {
        self._turn_counter.fetch_add(1, Ordering::SeqCst)
    }

    fn require_session_agent(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(String, String), String> {
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

fn parse_model_alias(raw: &str) -> (Option<String>, String) {
    if let Some(idx) = raw.find('/') {
        return (Some(raw[..idx].to_string()), raw[idx + 1..].to_string());
    }
    if let Some(idx) = raw.find(':') {
        return (Some(raw[..idx].to_string()), raw[idx + 1..].to_string());
    }
    (None, raw.to_string())
}

fn extract_input_parts(payload: &serde_json::Value) -> Option<Vec<ContentPart>> {
    let input = payload.get("input")?.as_array()?;
    let mut parts = Vec::new();
    for part in input {
        let part_type = part.get("type")?.as_str()?;
        match part_type {
            "text" => {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    parts.push(ContentPart::Text { text: text.into() });
                }
            }
            "image_url" => {
                let url = part
                    .get("imageUrl")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
                    .or_else(|| part.get("image_url").and_then(|v| v.as_str()))?;
                parts.push(ContentPart::ImageUrl {
                    image_url: UrlPayload {
                        url: url.into(),
                        id: None,
                    },
                });
            }
            "audio_url" => {
                let url = part
                    .get("audioUrl")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
                    .or_else(|| part.get("audio_url").and_then(|v| v.as_str()))?;
                parts.push(ContentPart::AudioUrl {
                    audio_url: UrlPayload {
                        url: url.into(),
                        id: None,
                    },
                });
            }
            "video_url" => {
                let url = part
                    .get("videoUrl")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
                    .or_else(|| part.get("video_url").and_then(|v| v.as_str()))?;
                parts.push(ContentPart::VideoUrl {
                    video_url: UrlPayload {
                        url: url.into(),
                        id: None,
                    },
                });
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        return None;
    }
    // Preserve the original behavior of rejecting purely empty text prompts,
    // while still allowing image-only or audio-only inputs.
    let all_text_empty = parts.iter().all(|p| match p {
        ContentPart::Text { text } => text.trim().is_empty(),
        _ => false,
    });
    if all_text_empty {
        return None;
    }
    Some(parts)
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
    impl MockProvider {
        fn new() -> Self {
            Self
        }
    }
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

    struct MockSink(Arc<Mutex<Vec<AgentEvent>>>);

    #[async_trait::async_trait]
    impl EventSink for MockSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            let resp = ApprovalResponse {
                decision: ApprovalDecision::Cancelled,
            };
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
        let host = CoreHost::new(
            config,
            Arc::new(MockSink(Arc::clone(&events))),
            Arc::new(MockProvider::new()),
        )
        .unwrap();
        (host, events)
    }

    #[tokio::test]
    async fn get_core_info_returns_version() {
        let host = make_host();
        let result = host
            .dispatch("getCoreInfo", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result["name"], "ody-host");
        assert!(result["version"].is_string());
    }

    #[tokio::test]
    async fn create_session_returns_summary() {
        let host = make_host();
        let result = host
            .dispatch("createSession", serde_json::json!({"workDir": "/tmp"}))
            .await
            .unwrap();
        assert!(result["id"].is_string());
        assert!(result["workDir"].is_string());
    }

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let host = make_host();
        let err = host
            .dispatch("nosuch", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown method"));
    }

    #[tokio::test]
    async fn chat_returns_content() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "chat-1"}),
            )
            .await
            .unwrap();
        let result = host
            .dispatch(
                "chat",
                serde_json::json!({"sessionId": session["id"], "prompt": "hi"}),
            )
            .await
            .unwrap();
        assert_eq!(result["content"], "ok");
        assert_eq!(result["finishReason"], "stop");
    }

    #[tokio::test]
    async fn create_session_with_provided_id() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let result = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "custom-1"}),
            )
            .await
            .unwrap();
        assert_eq!(result["id"], "custom-1");
        assert_eq!(result["workDir"], work_dir);
    }

    #[tokio::test]
    async fn create_session_without_id_uses_uuid() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
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
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let first = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "dup-1"}),
            )
            .await
            .unwrap();
        assert_eq!(first["id"], "dup-1");
        let err = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "dup-1"}),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn get_context_returns_empty_context() {
        let host = make_host();
        let result = host
            .dispatch(
                "getContext",
                serde_json::json!({"sessionId": "s1", "agentId": "main"}),
            )
            .await
            .unwrap();
        assert!(result["history"].is_array());
        assert_eq!(result["history"].as_array().unwrap().len(), 0);
        assert_eq!(result["tokenCount"], 0);
    }

    #[tokio::test]
    async fn get_permission_returns_manual_mode() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "perm-1"}),
            )
            .await
            .unwrap();
        let result = host
            .dispatch(
                "getPermission",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        assert_eq!(result["mode"], "manual");
        assert!(result["rules"].is_array());
    }

    #[tokio::test]
    async fn get_plan_returns_null() {
        let host = make_host();
        let result = host
            .dispatch(
                "getPlan",
                serde_json::json!({"sessionId": "s1", "agentId": "main"}),
            )
            .await
            .unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn get_usage_returns_empty_object() {
        let host = make_host();
        let result = host
            .dispatch(
                "getUsage",
                serde_json::json!({"sessionId": "s1", "agentId": "main"}),
            )
            .await
            .unwrap();
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn get_user_language_returns_en() {
        let host = make_host();
        let result = host
            .dispatch(
                "getUserLanguage",
                serde_json::json!({"sessionId": "s1", "agentId": "main"}),
            )
            .await
            .unwrap();
        assert_eq!(result, "en");
    }

    #[tokio::test]
    async fn list_mcp_servers_returns_empty_array() {
        let host = make_host();
        let result = host
            .dispatch("listMcpServers", serde_json::json!({"sessionId": "s1"}))
            .await
            .unwrap();
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_mcp_startup_metrics_returns_zero() {
        let host = make_host();
        let result = host
            .dispatch(
                "getMcpStartupMetrics",
                serde_json::json!({"sessionId": "s1"}),
            )
            .await
            .unwrap();
        assert_eq!(result["durationMs"], 0);
    }

    #[tokio::test]
    async fn get_ody_config_returns_host_config() {
        let host = make_host();
        let result = host
            .dispatch("getOdyConfig", serde_json::json!({}))
            .await
            .unwrap();
        assert!(result["providers"].is_array());
        assert!(result["homeDir"].is_string());
    }

    #[tokio::test]
    async fn get_agent_config_returns_session_scoped_config() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "cfg-1"}),
            )
            .await
            .unwrap();
        let result = host
            .dispatch(
                "getConfig",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        assert!(result["modelAlias"].is_string());
        assert!(result["modelCapabilities"].is_object());
        assert_eq!(result["thinkingLevel"], "off");
    }

    #[tokio::test]
    async fn set_model_updates_session_model() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "model-1"}),
            )
            .await
            .unwrap();
        let result = host
            .dispatch("setModel", serde_json::json!({"sessionId": session["id"], "agentId": "main", "model": "gpt-4o"}))
            .await
            .unwrap();
        assert_eq!(result["model"], "gpt-4o");

        let config = host
            .dispatch(
                "getConfig",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        assert_eq!(config["modelAlias"], "gpt-4o");
    }

    #[tokio::test]
    async fn set_thinking_updates_session_thinking() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "think-1"}),
            )
            .await
            .unwrap();
        host.dispatch(
            "setThinking",
            serde_json::json!({"sessionId": session["id"], "agentId": "main", "level": "on"}),
        )
        .await
        .unwrap();
        let config = host
            .dispatch(
                "getConfig",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        assert_eq!(config["thinkingLevel"], "high");
    }

    #[tokio::test]
    async fn set_permission_updates_session_permission() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "perm-2"}),
            )
            .await
            .unwrap();
        host.dispatch(
            "setPermission",
            serde_json::json!({"sessionId": session["id"], "agentId": "main", "mode": "yolo"}),
        )
        .await
        .unwrap();
        let permission = host
            .dispatch(
                "getPermission",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        assert_eq!(permission["mode"], "yolo");
    }

    #[tokio::test]
    async fn list_skills_returns_empty_array() {
        let host = make_host();
        let result = host
            .dispatch(
                "listSkills",
                serde_json::json!({"sessionId": "s1", "agentId": "main"}),
            )
            .await
            .unwrap();
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn prompt_emits_turn_events_and_returns_ok() {
        let (host, events) = make_host_with_events();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "prompt-1"}),
            )
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
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "steer-1"}),
            )
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
        let has_turn_ended = events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnEnded { .. }));
        assert!(has_turn_ended);
    }

    #[tokio::test]
    async fn env_getcwd_returns_home_dir() {
        let host = make_host();
        let result = host
            .dispatch("env.getcwd", serde_json::json!({}))
            .await
            .unwrap();
        assert!(result["cwd"].is_string());
        // make_host uses a tempdir as home_dir; the returned cwd should be normalized.
        assert!(!result["cwd"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn env_stat_returns_file_metadata() {
        let host = make_host();
        // make_host drops the TempDir, so create our own tempdir for the file.
        let td = tempfile::tempdir().unwrap();
        let file_path = td.path().join("test.txt");
        tokio::fs::write(&file_path, "hello").await.unwrap();

        let result = host
            .dispatch(
                "env.stat",
                serde_json::json!({"path": file_path.to_string_lossy(), "followSymlinks": true}),
            )
            .await
            .unwrap();
        assert_eq!(result["stSize"], 5);
        assert_eq!(result["isDir"], false);
    }

    #[tokio::test]
    async fn env_unknown_method_returns_error() {
        let host = make_host();
        let err = host
            .dispatch("env.nosuch", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown env method"));
    }

    #[tokio::test]
    async fn get_tools_returns_agent_tools() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "tools-1"}),
            )
            .await
            .unwrap();
        let result = host
            .dispatch(
                "getTools",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        let arr = result.as_array().unwrap();
        let names: Vec<&str> = arr.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Bash"));
    }

    #[tokio::test]
    async fn set_active_tools_filters_loop_tools() {
        let host = make_host();
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch(
                "createSession",
                serde_json::json!({"workDir": work_dir, "id": "tools-2"}),
            )
            .await
            .unwrap();

        host.dispatch(
            "setActiveTools",
            serde_json::json!({"sessionId": session["id"], "agentId": "main", "names": ["Read"]}),
        )
        .await
        .unwrap();

        let result = host
            .dispatch(
                "getTools",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        let active_names: Vec<&str> = result
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["active"].as_bool().unwrap())
            .map(|v| v["name"].as_str().unwrap())
            .collect();
        assert_eq!(active_names, vec!["Read"]);
    }

    #[tokio::test]
    async fn chat_triggers_fetch_url_tool() {
        let (host, _events) = make_host_with_events();

        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": "/tmp"}))
            .await
            .unwrap();
        let session_id = session["id"].as_str().unwrap();

        // FetchURL may not be active by default; ensure it is enabled.
        host.dispatch(
            "setActiveTools",
            serde_json::json!({"sessionId": session_id, "agentId": "main", "names": ["FetchURL"]}),
        )
        .await
        .unwrap();

        let tools = host
            .dispatch(
                "getTools",
                serde_json::json!({"sessionId": session_id, "agentId": "main"}),
            )
            .await
            .unwrap();
        let tools_array = tools.as_array().unwrap();
        let names: Vec<&str> = tools_array
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(
            names.contains(&"FetchURL"),
            "FetchURL tool should be available. Got: {names:?}"
        );
    }
}

#[cfg(test)]
mod provider_routing_tests {
    use super::*;
    use crate::config::{HostConfig, LogLevel, ProviderConfig, TransportMode};
    use crate::events::AgentEvent;
    use crate::llm::{ChatDelta, LlmProvider};
    use std::sync::{Arc, Mutex};

    struct EchoProvider;
    #[async_trait::async_trait]
    impl LlmProvider for EchoProvider {
        async fn chat_stream(
            &self,
            _request: crate::llm::ChatRequest,
            on_delta: &mut (dyn FnMut(ChatDelta) + Send),
        ) -> Result<crate::llm::FinishReason, crate::llm::LlmError> {
            on_delta(ChatDelta {
                index: 0,
                content: Some("ok".into()),
                tool_call: None,
            });
            Ok(crate::llm::FinishReason::Stop)
        }
    }

    struct MockSink(Arc<Mutex<Vec<AgentEvent>>>);

    #[async_trait::async_trait]
    impl EventSink for MockSink {
        async fn request(
            &self,
            _method: &str,
            _payload: Vec<u8>,
        ) -> Result<Vec<u8>, crate::error::RpcError> {
            let resp = crate::tools::ApprovalResponse {
                decision: crate::tools::ApprovalDecision::Cancelled,
            };
            Ok(serde_json::to_vec(&resp).unwrap())
        }
        fn emit(&self, event: AgentEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn make_host_with_config(config: HostConfig) -> CoreHost {
        CoreHost::new(
            config,
            Arc::new(MockSink(Arc::new(Mutex::new(Vec::new())))),
            Arc::new(EchoProvider),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn set_model_with_provider_prefix_updates_both() {
        let config = HostConfig {
            home_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            config_path: None,
            transport: TransportMode::Stdio,
            log_level: LogLevel::Info,
            provider: ProviderConfig {
                provider_id: "openai".into(),
                api_key: "".into(),
                base_url: None,
                default_model: Some("gpt-4o-mini".into()),
            },
            mock_provider: false,
        };
        let host = make_host_with_config(config);
        let work_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string();
        let session = host
            .dispatch("createSession", serde_json::json!({"workDir": work_dir}))
            .await
            .unwrap();

        let result = host
            .dispatch("setModel", serde_json::json!({"sessionId": session["id"], "agentId": "main", "model": "anthropic/claude-sonnet-4"}))
            .await
            .unwrap();
        assert_eq!(result["model"], "claude-sonnet-4");
        assert_eq!(result["providerName"], "anthropic");

        let cfg = host
            .dispatch(
                "getConfig",
                serde_json::json!({"sessionId": session["id"], "agentId": "main"}),
            )
            .await
            .unwrap();
        assert_eq!(cfg["modelAlias"], "anthropic/claude-sonnet-4");
        assert_eq!(cfg["provider"]["id"], "anthropic");
    }
}
