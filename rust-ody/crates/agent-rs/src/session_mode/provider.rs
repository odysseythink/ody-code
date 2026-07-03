use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use crate::agent::Agent;
use crate::records::nested::SessionModeKind;

use crate::session_mode::types::HandoffOptions;
use tools_rs::builtin::session_mode::{
    GameDesignStateStore, Language, McpProvider, OfficeHoursStateStore, SessionModeContext,
    SessionModeKind as ToolSessionModeKind, SessionModeProvider, TelemetryClient,
};

pub struct AgentSessionModeProvider {
    agent: Weak<Agent>,
    language: Mutex<Language>,
    office_hours_store: Arc<dyn OfficeHoursStateStore>,
    game_design_store: Arc<dyn GameDesignStateStore>,
    telemetry: Arc<dyn TelemetryClient>,
    mcp: Arc<dyn McpProvider>,
}

impl AgentSessionModeProvider {
    pub fn new(
        agent: Weak<Agent>,
        office_hours_store: Arc<dyn OfficeHoursStateStore>,
        game_design_store: Arc<dyn GameDesignStateStore>,
        telemetry: Arc<dyn TelemetryClient>,
        mcp: Arc<dyn McpProvider>,
    ) -> Self {
        Self {
            agent,
            language: Mutex::new(Language::En),
            office_hours_store,
            game_design_store,
            telemetry,
            mcp,
        }
    }

    fn upgrade(&self) -> Option<Arc<Agent>> {
        self.agent.upgrade()
    }
}

#[async_trait]
impl SessionModeProvider for AgentSessionModeProvider {
    fn is_session_mode_active(&self) -> bool {
        self.upgrade()
            .map(|a| a.session_mode.lock().unwrap().is_active())
            .unwrap_or(false)
    }

    fn session_mode_kind(&self) -> Option<ToolSessionModeKind> {
        self.upgrade()
            .and_then(|a| a.session_mode.lock().unwrap().kind())
            .map(to_tool_kind)
    }

    fn session_mode_file_path(&self) -> Option<String> {
        self.upgrade()
            .and_then(|a| a.session_mode.lock().unwrap().session_mode_file_path())
    }

    async fn enter_session_mode(&self, kind: ToolSessionModeKind) -> anyhow::Result<()> {
        if let Some(agent) = self.upgrade() {
            agent.enter_session_mode(to_agent_kind(kind), None).await?;
        }
        Ok(())
    }

    async fn exit_session_mode(&self) -> anyhow::Result<()> {
        if let Some(agent) = self.upgrade() {
            agent.exit_session_mode().await?;
        }
        Ok(())
    }

    async fn handoff_to(&self, target: &str, selected_label: Option<String>) -> anyhow::Result<()> {
        if let Some(agent) = self.upgrade() {
            let target = target.to_string();
            tokio::task::spawn_blocking(move || {
                let mut sm = agent.session_mode.lock().unwrap();
                let rt = tokio::runtime::Handle::current();
                rt.block_on(sm.handoff_to(&target, HandoffOptions { selected_label }))
            })
            .await??;
        }
        Ok(())
    }

    fn user_language(&self) -> Language {
        *self.language.lock().unwrap()
    }

    fn set_user_language(&self, lang: Language) {
        *self.language.lock().unwrap() = lang;
    }

    fn open_external_available(&self) -> bool {
        // The host sets `agent.rpc` after construction; default to false until wired.
        false
    }

    fn telemetry(&self) -> Arc<dyn TelemetryClient> {
        self.telemetry.clone()
    }

    fn kaos(&self) -> Arc<dyn SessionModeContext> {
        self.upgrade()
            .map(|a| {
                Arc::new(KaosSessionModeContext {
                    kaos: a.kaos.clone(),
                }) as Arc<dyn SessionModeContext>
            })
            .unwrap_or_else(|| Arc::new(EmptySessionModeContext))
    }

    fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore> {
        self.office_hours_store.clone()
    }

    fn game_design_store(&self) -> Arc<dyn GameDesignStateStore> {
        self.game_design_store.clone()
    }

    fn mcp(&self) -> Arc<dyn McpProvider> {
        self.mcp.clone()
    }
}

pub struct AgentTelemetryClient {
    agent: Weak<Agent>,
}

impl AgentTelemetryClient {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }
}

impl TelemetryClient for AgentTelemetryClient {
    fn track(&self, event: &str, properties: HashMap<String, serde_json::Value>) {
        if let Some(agent) = self.agent.upgrade() {
            agent
                .environment
                .track_telemetry(event, serde_json::to_value(properties).unwrap_or_default());
        }
    }
}

pub struct AgentMcpProvider {
    agent: Weak<Agent>,
}

impl AgentMcpProvider {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }
}

#[async_trait]
impl McpProvider for AgentMcpProvider {
    async fn gbrain_available(&self) -> bool {
        // MCP host is not yet ported; always report unavailable for now.
        let _ = self.agent;
        false
    }
}

struct KaosSessionModeContext {
    kaos: Arc<kaos_rs::kaos::Kaos>,
}

#[async_trait]
impl SessionModeContext for KaosSessionModeContext {
    fn cwd(&self) -> String {
        self.kaos.getcwd()
    }

    fn project_root(&self) -> Option<String> {
        None
    }

    async fn read_text(&self, path: &str) -> anyhow::Result<String> {
        Ok(self.kaos.read_text(path, None, None).await?)
    }

    async fn write_text(&self, path: &str, content: &str) -> anyhow::Result<()> {
        self.kaos.write_text(path, content, None, None).await?;
        Ok(())
    }

    async fn stat(&self, path: &str) -> anyhow::Result<()> {
        self.kaos.stat(path, true).await?;
        Ok(())
    }
}

fn to_tool_kind(kind: SessionModeKind) -> ToolSessionModeKind {
    match kind {
        SessionModeKind::Plan => ToolSessionModeKind::Plan,
        SessionModeKind::Design => ToolSessionModeKind::Design,
        SessionModeKind::OfficeHours => ToolSessionModeKind::OfficeHours,
        SessionModeKind::GameDesign => ToolSessionModeKind::GameDesign,
    }
}

fn to_agent_kind(kind: ToolSessionModeKind) -> SessionModeKind {
    match kind {
        ToolSessionModeKind::Plan => SessionModeKind::Plan,
        ToolSessionModeKind::Design => SessionModeKind::Design,
        ToolSessionModeKind::OfficeHours => SessionModeKind::OfficeHours,
        ToolSessionModeKind::GameDesign => SessionModeKind::GameDesign,
    }
}

struct EmptySessionModeContext;

#[async_trait]
impl SessionModeContext for EmptySessionModeContext {
    fn cwd(&self) -> String {
        "/".into()
    }
    fn project_root(&self) -> Option<String> {
        None
    }
    async fn read_text(&self, _path: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
    async fn write_text(&self, _path: &str, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn stat(&self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentBuilder, AgentEnvironment};
    use crate::permission::types::ApprovalRequest;
    use crate::turn::types::{AgentEvent, HookResult, StopHookBlock};
    use kosong_rs::message::ContentPart;
    use kosong_rs::provider::AbortSignal;
    use std::future::Future;
    use std::pin::Pin;

    struct NoopEnv;
    #[async_trait::async_trait]
    impl AgentEnvironment for NoopEnv {
        fn emit_event(&self, _event: AgentEvent) {}
        async fn request_approval(
            &self,
            _req: &ApprovalRequest,
            _signal: AbortSignal,
        ) -> Result<crate::records::nested::ApprovalResponse, anyhow::Error> {
            Ok(crate::records::nested::ApprovalResponse {
                decision: "approved".into(),
                scope: None,
                feedback: None,
                selected_label: None,
            })
        }
        fn fire_hook_pre_tool_use(
            &self,
            _tool_name: &str,
            _tool_input: serde_json::Value,
            _tool_call_id: &str,
            _signal: AbortSignal,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>>
        {
            Box::pin(async { Ok(None) })
        }
        fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_user_prompt_submit(
            &self,
            _input: Vec<ContentPart>,
            _signal: AbortSignal,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<HookResult>, anyhow::Error>> + Send + '_>>
        {
            Box::pin(async { Ok(vec![]) })
        }
        fn fire_hook_stop_hook(
            &self,
            _signal: AbortSignal,
        ) -> Pin<Box<dyn Future<Output = Result<Option<StopHookBlock>, anyhow::Error>> + Send + '_>>
        {
            Box::pin(async { Ok(None) })
        }
        fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
        fn trigger_hook(
            &self,
            _event: &str,
            _data: serde_json::Value,
            _signal: AbortSignal,
        ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }
        fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
        fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
    }

    struct DummyTelemetryClient;
    impl TelemetryClient for DummyTelemetryClient {
        fn track(&self, _event: &str, _properties: HashMap<String, serde_json::Value>) {}
    }

    struct DummyMcpProvider;
    #[async_trait::async_trait]
    impl McpProvider for DummyMcpProvider {
        async fn gbrain_available(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn adapter_enters_plan_mode() {
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let env: Arc<dyn AgentEnvironment> = Arc::new(NoopEnv);
        let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
        let provider = AgentSessionModeProvider::new(
            Arc::downgrade(&agent),
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryOfficeHoursStateStore::new()),
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryGameDesignStateStore::new()),
            Arc::new(DummyTelemetryClient),
            Arc::new(DummyMcpProvider),
        );
        provider
            .enter_session_mode(ToolSessionModeKind::Plan)
            .await
            .unwrap();
        assert!(provider.is_session_mode_active());
        assert_eq!(
            provider.session_mode_kind(),
            Some(ToolSessionModeKind::Plan)
        );
    }

    #[tokio::test]
    async fn adapter_enters_game_design_mode_and_returns_store() {
        use tools_rs::builtin::session_mode::GameDesignProfileEntry;

        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let env: Arc<dyn AgentEnvironment> = Arc::new(NoopEnv);
        let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
        let game_design_store =
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryGameDesignStateStore::new());
        let provider = AgentSessionModeProvider::new(
            Arc::downgrade(&agent),
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryOfficeHoursStateStore::new()),
            game_design_store.clone(),
            Arc::new(DummyTelemetryClient),
            Arc::new(DummyMcpProvider),
        );
        provider
            .enter_session_mode(ToolSessionModeKind::GameDesign)
            .await
            .unwrap();
        assert!(provider.is_session_mode_active());
        assert_eq!(
            provider.session_mode_kind(),
            Some(ToolSessionModeKind::GameDesign)
        );

        // Verify the returned store is wired by appending through the provider.
        provider
            .game_design_store()
            .append_profile(GameDesignProfileEntry {
                date: "2024-01-01".into(),
                mode: "startup".into(),
                project_slug: "test-game".into(),
                pillars: "a, b, c".into(),
                audience: "players".into(),
                platform: "PC".into(),
                genre: "RPG".into(),
                signals: vec![],
                design_doc: "/design.md".into(),
            })
            .await
            .unwrap();
        assert_eq!(game_design_store.profiles().len(), 1);
        assert_eq!(game_design_store.profiles()[0].project_slug, "test-game");
    }

    #[tokio::test]
    async fn adapter_language_round_trips() {
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let env: Arc<dyn AgentEnvironment> = Arc::new(NoopEnv);
        let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
        let provider = AgentSessionModeProvider::new(
            Arc::downgrade(&agent),
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryOfficeHoursStateStore::new()),
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryGameDesignStateStore::new()),
            Arc::new(DummyTelemetryClient),
            Arc::new(DummyMcpProvider),
        );
        assert_eq!(provider.user_language(), Language::En);
        provider.set_user_language(Language::Zh);
        assert_eq!(provider.user_language(), Language::Zh);
    }
}
