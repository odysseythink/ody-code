use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::agent_bridge::{
    HostAgentEnvironment, HostBuiltinToolsProvider, HostLlmFactory, HostProviderResolver,
};
use crate::config::ProviderConfig as HostProviderConfig;
use crate::events::EventSink;
use crate::llm::LlmProvider;
use crate::session::store::{
    IndexEntry, SessionError, SessionState, SessionStoreAdapter, SessionSummary,
};

fn new_id() -> String {
    Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
}

#[derive(Debug, Default, Clone)]
pub struct SessionFilter {
    pub work_dir: Option<String>,
    pub session_id: Option<String>,
}

pub struct SessionManager {
    store: SessionStoreAdapter,
    active: RwLock<HashMap<String, Arc<Session>>>,
    pub kaos: Arc<kaos_rs::kaos::Kaos>,
    pub event_sink: Arc<dyn EventSink>,
    pub provider_config: HostProviderConfig,
    pub llm_provider: Arc<dyn LlmProvider>,
}

pub struct Session {
    pub id: String,
    pub work_dir: std::path::PathBuf,
    pub dir: std::path::PathBuf,
    state: tokio::sync::Mutex<SessionState>,
    agent: tokio::sync::Mutex<Option<Arc<agent_rs::agent::Agent>>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("work_dir", &self.work_dir)
            .field("dir", &self.dir)
            .finish()
    }
}

impl Session {
    pub async fn model(&self) -> Option<String> {
        self.state.lock().await.model.clone()
    }

    pub async fn thinking(&self) -> Option<String> {
        self.state.lock().await.thinking.clone()
    }

    pub async fn permission(&self) -> Option<String> {
        self.state.lock().await.permission.clone()
    }

    pub async fn provider_id(&self) -> Option<String> {
        self.state.lock().await.provider_id.clone()
    }

    pub async fn set_model(&self, model: Option<String>) {
        self.state.lock().await.model = model;
    }

    pub async fn set_thinking(&self, thinking: Option<String>) {
        self.state.lock().await.thinking = thinking;
    }

    pub async fn set_permission(&self, permission: Option<String>) {
        self.state.lock().await.permission = permission;
    }

    pub async fn set_provider_id(&self, provider_id: Option<String>) {
        self.state.lock().await.provider_id = provider_id;
    }

    /// Lazily construct (or return cached) the Agent for this session.
    pub async fn agent(
        &self,
        kaos: Arc<kaos_rs::kaos::Kaos>,
        event_sink: Arc<dyn EventSink>,
        provider_config: &HostProviderConfig,
        llm_provider: Arc<dyn LlmProvider>,
    ) -> Result<Arc<agent_rs::agent::Agent>, SessionError> {
        let mut cached = self.agent.lock().await;
        if let Some(agent) = cached.as_ref() {
            return Ok(Arc::clone(agent));
        }

        let env = Arc::new(HostAgentEnvironment {
            session_id: self.id.clone(),
            agent_id: "main".into(),
            sink: event_sink,
        });
        let resolver = Arc::new(HostProviderResolver::new(provider_config.clone()));
        let records_path = self.dir.join("wire.jsonl");

        let agent = agent_rs::agent::AgentBuilder::new("main", Arc::clone(&kaos), env)
            .homedir(self.dir.clone())
            .provider_resolver(resolver)
            .llm_factory(Arc::new(HostLlmFactory))
            .build()
            .await
            .map_err(|e| SessionError::Io {
                source: std::io::Error::new(std::io::ErrorKind::Other, e),
                path: records_path,
            })?;

        // Create TurnFlow for this Agent and wire it up
        let turn_flow = Arc::new(agent_rs::turn::turn_flow::TurnFlow::new(
            Arc::clone(&agent) as Arc<dyn agent_rs::turn::types::TurnAgent>
        ));
        agent.set_turn_flow(Arc::clone(&turn_flow));

        // Sync session state (model/thinking/permission) to agent config
        let state = self.state.lock().await;
        if let Some(ref model) = state.model {
            let update = agent_rs::records::nested::AgentConfigUpdateData {
                model_alias: Some(model.clone()),
                ..Default::default()
            };
            agent.update_config(update);
        }
        if let Some(ref level) = state.thinking {
            let update = agent_rs::records::nested::AgentConfigUpdateData {
                thinking_level: Some(level.clone()),
                ..Default::default()
            };
            agent.update_config(update);
        }
        if let Some(ref mode_str) = state.permission {
            let mode = match mode_str.as_str() {
                "yolo" => agent_rs::records::nested::PermissionMode::Yolo,
                "auto" => agent_rs::records::nested::PermissionMode::Auto,
                _ => agent_rs::records::nested::PermissionMode::Manual,
            };
            agent.set_permission_mode(mode);
        }
        drop(state);

        // Wire up host builtin tools provider and agent-dependent services.
        let agent_weak = Arc::downgrade(&agent);

        let workspace = tools_rs::workspace::WorkspaceConfig {
            workspace_dir: self.work_dir.to_string_lossy().to_string(),
            additional_dirs: vec![],
        };
        let test_reviewer = Arc::new(crate::tools::LlmTestReviewer::new(
            Arc::clone(&llm_provider),
            provider_config
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
        ));
        let builtin_provider = Arc::new(HostBuiltinToolsProvider::new(
            Arc::clone(&kaos),
            workspace,
            Some(Arc::new(crate::tools::LocalFetchURLProvider::new(false))),
            None,
            Arc::new(crate::tools::HostE2ETestRunner::new(Arc::clone(&kaos))),
            test_reviewer,
            Arc::new(tools_rs::builtin::visual::MockDesignMockupHost::new(
                false,
                None,
                Ok(tools_rs::builtin::visual::OpenExternalResult {
                    opened: false,
                    error: Some("host does not support openExternal".to_string()),
                }),
            )),
            tools_rs::builtin::idea::MockIdeaReportContext::new(false, chrono::Utc::now()),
            false,
        ));
        builtin_provider.set_agent(agent_weak.clone());
        *agent.builtin_tools_provider.lock().unwrap() =
            Some(Arc::clone(&builtin_provider)
                as Arc<dyn agent_rs::tool::types::BuiltinToolsProvider>);

        *agent.background.lock().unwrap() = Some(Arc::new(
            agent_rs::background::manager::BackgroundManager::new(
                Arc::clone(&agent) as Arc<dyn agent_rs::turn::types::TurnAgent>,
                Arc::clone(&turn_flow),
                Some(
                    agent_rs::background::persistence::BackgroundTaskPersistence::new(
                        self.dir.clone(),
                    ),
                ),
            ),
        ));
        *agent.cron.lock().unwrap() = Some(agent_rs::cron::manager::CronManager::new(
            Arc::clone(&agent) as Arc<dyn agent_rs::turn::types::TurnAgent>,
            Arc::clone(&turn_flow),
            Some(self.dir.clone()),
            agent_rs::cron::manager::CronManagerOptions {
                clocks: None,
                poll_interval_ms: None,
            },
        ));

        // Re-sync builtins now that agent-dependent services are available.
        let provision_ctx = agent_rs::tool::types::BuiltinToolProvisionContext {
            agent_type: agent_rs::agent::AgentType::Main,
            model_capabilities: kosong_rs::provider::ModelCapability::unknown(),
            homedir: Some(self.dir.clone()),
            goal_command_enabled: false,
            rpc_open_external: false,
            rpc_request_question: false,
            background_available: true,
            cron_available: true,
            has_invocable_skills: false,
            subagent_host_available: false,
            web_searcher_available: false,
            url_fetcher_available: true,
        };
        agent
            .tools
            .lock()
            .unwrap()
            .sync_builtins(builtin_provider.as_ref(), provision_ctx);

        *cached = Some(Arc::clone(&agent));
        Ok(agent)
    }

    pub async fn persist_state(&self) -> Result<(), SessionError> {
        let mut state = self.state.lock().await.clone();
        state.agent_records_path = Some(self.dir.join("wire.jsonl"));
        crate::session::store::write_state_json(&self.dir, &state).map_err(|e| SessionError::Io {
            source: e,
            path: self.dir.clone(),
        })
    }
}

impl SessionManager {
    pub fn new(
        store: SessionStoreAdapter,
        kaos: Arc<kaos_rs::kaos::Kaos>,
        event_sink: Arc<dyn EventSink>,
        provider_config: HostProviderConfig,
        llm_provider: Arc<dyn LlmProvider>,
    ) -> Self {
        Self {
            store,
            active: RwLock::new(HashMap::new()),
            kaos,
            event_sink,
            provider_config,
            llm_provider,
        }
    }

    pub async fn create(
        &self,
        work_dir: &Path,
        title: Option<&str>,
    ) -> Result<SessionSummary, SessionError> {
        let id = new_id();
        self.create_with_id(&id, work_dir, title).await
    }

    pub async fn create_with_id(
        &self,
        id: &str,
        work_dir: &Path,
        title: Option<&str>,
    ) -> Result<SessionSummary, SessionError> {
        let dir = self.store.session_dir_for(id, work_dir)?;
        if dir.exists() {
            return Err(SessionError::AlreadyExists {
                session_id: id.to_string(),
            });
        }
        let index = self.store.read_index()?;
        if index.contains_key(id) {
            return Err(SessionError::AlreadyExists {
                session_id: id.to_string(),
            });
        }
        std::fs::create_dir_all(&dir).map_err(|e| SessionError::Io {
            source: e,
            path: dir.clone(),
        })?;
        let state = SessionState {
            title: title.map(|s| s.to_string()),
            last_prompt: None,
            custom: HashMap::new(),
            model: None,
            thinking: None,
            permission: None,
            provider_id: None,
            agent_records_path: None,
            resume_state: None,
        };
        crate::session::store::write_state_json(&dir, &state).map_err(|e| SessionError::Io {
            source: e,
            path: dir.clone(),
        })?;
        let normalized = crate::session::store::normalize_work_dir(work_dir);
        self.store.append_index(IndexEntry {
            session_id: id.to_string(),
            session_dir: dir.clone(),
            work_dir: normalized.clone(),
        })?;
        let summary = self
            .store
            .summary_from_dir(id.to_string(), &dir, &normalized)?;

        let session = Arc::new(Session {
            id: id.to_string(),
            work_dir: normalized,
            dir: dir.clone(),
            state: tokio::sync::Mutex::new(state),
            agent: tokio::sync::Mutex::new(None),
        });

        // Pre-create agent so createSession returns a live, usable Agent
        let _ = session
            .agent(
                Arc::clone(&self.kaos),
                Arc::clone(&self.event_sink),
                &self.provider_config,
                Arc::clone(&self.llm_provider),
            )
            .await?;
        self.active.write().await.insert(id.to_string(), session);
        Ok(summary)
    }

    pub async fn list(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, SessionError> {
        let index = self.store.read_index()?;
        let mut summaries = Vec::new();
        for (id, entry) in index {
            if let Some(wd) = &filter.work_dir {
                if entry.work_dir != crate::session::store::normalize_work_dir(Path::new(wd)) {
                    continue;
                }
            }
            if let Some(sid) = &filter.session_id {
                if &id != sid {
                    continue;
                }
            }
            if !entry.session_dir.exists() {
                continue;
            }
            summaries.push(
                self.store
                    .summary_from_dir(id, &entry.session_dir, &entry.work_dir)?,
            );
        }
        summaries.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        Ok(summaries)
    }

    pub async fn get(&self, id: String) -> Result<Arc<Session>, SessionError> {
        {
            let active = self.active.read().await;
            if let Some(s) = active.get(&id) {
                return Ok(Arc::clone(s));
            }
        }
        let index = self.store.read_index()?;
        let entry = index
            .get(&id)
            .cloned()
            .ok_or_else(|| SessionError::NotFound {
                session_id: id.clone(),
            })?;
        if !entry.session_dir.exists() {
            return Err(SessionError::NotFound { session_id: id });
        }
        let state = crate::session::store::read_state_json(&entry.session_dir)
            .map_err(|e| SessionError::Io {
                source: e,
                path: entry.session_dir.clone(),
            })?
            .unwrap_or_default();
        let session = Arc::new(Session {
            id: id.clone(),
            work_dir: entry.work_dir.clone(),
            dir: entry.session_dir.clone(),
            state: tokio::sync::Mutex::new(state),
            agent: tokio::sync::Mutex::new(None),
        });
        self.active.write().await.insert(id, Arc::clone(&session));
        Ok(session)
    }

    pub async fn close(&self, id: String) -> Result<(), SessionError> {
        self.active.write().await.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RpcError;
    use crate::events::AgentEvent;
    use std::path::Path;

    struct NoopSink;
    #[async_trait::async_trait]
    impl EventSink for NoopSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            Ok(vec![])
        }
        fn emit(&self, _event: AgentEvent) {}
    }

    fn make_manager() -> SessionManager {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            tmp.path(),
        ));
        let sink: Arc<dyn EventSink> = Arc::new(NoopSink);
        SessionManager::new(
            SessionStoreAdapter::new(tmp.path().to_path_buf()),
            kaos,
            sink,
            HostProviderConfig {
                provider_id: "mock".into(),
                api_key: "".into(),
                base_url: None,
                default_model: Some("mock".into()),
            },
            Arc::new(crate::llm::mock::MockProvider::default()),
        )
    }

    #[tokio::test]
    async fn create_then_list_returns_session() {
        let manager = make_manager();
        let summary = manager
            .create(Path::new("/tmp/wd"), Some("t"))
            .await
            .unwrap();
        let list = manager.list(SessionFilter::default()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, summary.id);
        assert_eq!(list[0].title, Some("t".to_string()));
    }

    #[tokio::test]
    async fn duplicate_id_fails() {
        let manager = make_manager();
        let summary = manager.create(Path::new("/tmp/wd"), None).await.unwrap();
        let err = manager
            .create_with_id(&summary.id, Path::new("/tmp/wd"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, SessionError::AlreadyExists { .. }));
    }

    #[tokio::test]
    async fn close_removes_active_but_keeps_disk() {
        let manager = make_manager();
        let summary = manager.create(Path::new("/tmp/wd"), None).await.unwrap();
        manager.close(summary.id.clone()).await.unwrap();
        assert!(summary.session_dir.exists());
        let reloaded = manager.get(summary.id.clone()).await.unwrap();
        assert_eq!(reloaded.id, summary.id);
    }

    #[tokio::test]
    async fn session_can_construct_agent() {
        let manager = make_manager();
        let summary = manager.create(Path::new("/tmp/wd"), None).await.unwrap();
        let session = manager.get(summary.id.clone()).await.unwrap();
        let agent = session
            .agent(
                Arc::clone(&manager.kaos),
                Arc::clone(&manager.event_sink),
                &manager.provider_config,
                Arc::clone(&manager.llm_provider),
            )
            .await
            .unwrap();
        assert!(agent
            .turn()
            .prompt(vec![], agent_rs::records::nested::PromptOrigin::User)
            .is_some());
    }

    #[tokio::test]
    async fn session_agent_loop_tools_contains_core_and_todo() {
        use crate::config::{HostConfig, LogLevel, ProviderConfig, TransportMode};
        use crate::events::AgentEvent;
        use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmProvider};
        use agent_rs::turn::TurnAgent;

        struct MockProvider;
        #[async_trait::async_trait]
        impl LlmProvider for MockProvider {
            async fn chat_stream(
                &self,
                _request: ChatRequest,
                _on_delta: &mut (dyn FnMut(ChatDelta) + Send),
            ) -> Result<FinishReason, crate::llm::LlmError> {
                Ok(FinishReason::Stop)
            }
        }

        struct MockSink;
        #[async_trait::async_trait]
        impl EventSink for MockSink {
            async fn request(
                &self,
                _method: &str,
                _payload: Vec<u8>,
            ) -> Result<Vec<u8>, crate::error::RpcError> {
                Ok(vec![])
            }
            fn emit(&self, _event: AgentEvent) {}
        }

        let config = HostConfig {
            home_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            config_path: None,
            transport: TransportMode::Stdio,
            log_level: LogLevel::Info,
            provider: ProviderConfig {
                provider_id: "mock".into(),
                api_key: "".into(),
                base_url: None,
                default_model: Some("mock".into()),
            },
            mock_provider: false,
        };
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            &config.home_dir,
        ));
        let store = crate::session::store::SessionStoreAdapter::new(config.home_dir.clone());
        let mgr = SessionManager::new(
            store,
            kaos,
            Arc::new(MockSink),
            config.provider,
            Arc::new(MockProvider),
        );
        let summary = mgr
            .create(std::path::Path::new("/tmp"), Some("test"))
            .await
            .unwrap();
        let session = mgr.get(summary.id).await.unwrap();
        let agent = session
            .agent(
                Arc::clone(&mgr.kaos),
                Arc::clone(&mgr.event_sink),
                &mgr.provider_config,
                Arc::clone(&mgr.llm_provider),
            )
            .await
            .unwrap();

        let names: Vec<String> = agent
            .tools()
            .loop_tools()
            .into_iter()
            .map(|t| t.name().to_string())
            .collect();
        assert!(names.contains(&"Read".to_string()));
        assert!(names.contains(&"TodoList".to_string()));
    }
}
