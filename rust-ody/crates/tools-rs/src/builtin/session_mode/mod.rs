use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Supported user languages, mirroring TS `SupportedLanguage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Zh,
}

impl Language {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().split('-').next()? {
            "zh" | "zh_cn" | "zh_tw" | "zh_hk" => Some(Language::Zh),
            _ => Some(Language::En),
        }
    }
}

/// Active session mode kind, mirroring TS `RuntimeMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionModeKind {
    Plan,
    Design,
    OfficeHours,
    GameDesign,
}

impl SessionModeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionModeKind::Plan => "plan",
            SessionModeKind::Design => "design",
            SessionModeKind::OfficeHours => "office-hours",
            SessionModeKind::GameDesign => "game-design",
        }
    }
}

/// Minimal filesystem / config surface needed by session-mode tools.
#[async_trait]
pub trait SessionModeContext: Send + Sync {
    fn cwd(&self) -> String;
    fn project_root(&self) -> Option<String>;
    async fn read_text(&self, path: &str) -> anyhow::Result<String>;
    async fn write_text(&self, path: &str, content: &str) -> anyhow::Result<()>;
    async fn stat(&self, path: &str) -> anyhow::Result<()>;
}

/// State-store entry shapes, mirroring TS `LearningEntry` / `BuilderProfileEntry`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningEntry {
    pub ts: String,
    pub skill: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub key: String,
    pub insight: String,
    pub confidence: f64,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderProfileEntry {
    pub date: String,
    pub mode: String,
    pub project_slug: String,
    pub signal_count: u64,
    pub signals: Vec<String>,
    pub design_doc: String,
    pub assignment: String,
    pub resources_shown: Vec<String>,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDesignProfileEntry {
    pub date: String,
    pub mode: String,
    pub project_slug: String,
    pub pillars: String,
    pub audience: String,
    pub platform: String,
    pub genre: String,
    pub signals: Vec<String>,
    pub design_doc: String,
}

#[async_trait]
pub trait OfficeHoursStateStore: Send + Sync {
    async fn append_profile(&self, entry: BuilderProfileEntry) -> anyhow::Result<()>;
    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()>;
    async fn search_learnings(
        &self,
        limit: usize,
        cross_project: bool,
    ) -> anyhow::Result<Vec<LearningEntry>>;
}

#[async_trait]
pub trait GameDesignStateStore: Send + Sync {
    async fn append_profile(&self, entry: GameDesignProfileEntry) -> anyhow::Result<()>;
    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()>;
    async fn search_learnings(
        &self,
        limit: usize,
        branch: Option<String>,
    ) -> anyhow::Result<Vec<LearningEntry>>;
}

/// Minimal telemetry surface.
pub trait TelemetryClient: Send + Sync {
    fn track(&self, event: &str, properties: HashMap<String, Value>);
}

/// Minimal MCP surface for artifact sync tools.
#[async_trait]
pub trait McpProvider: Send + Sync {
    async fn gbrain_available(&self) -> bool;
}

/// The main trait that session-mode tools consume.
#[async_trait]
pub trait SessionModeProvider: Send + Sync {
    fn is_session_mode_active(&self) -> bool;
    fn session_mode_kind(&self) -> Option<SessionModeKind>;
    fn session_mode_file_path(&self) -> Option<String>;
    async fn enter_session_mode(&self, kind: SessionModeKind) -> anyhow::Result<()>;
    async fn exit_session_mode(&self) -> anyhow::Result<()>;
    async fn handoff_to(&self, target: &str, selected_label: Option<String>) -> anyhow::Result<()>;
    fn user_language(&self) -> Language;
    fn set_user_language(&self, lang: Language);
    fn open_external_available(&self) -> bool;
    fn telemetry(&self) -> Arc<dyn TelemetryClient>;
    fn kaos(&self) -> Arc<dyn SessionModeContext>;
    fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore>;
    fn game_design_store(&self) -> Arc<dyn GameDesignStateStore>;
    fn mcp(&self) -> Arc<dyn McpProvider>;
}

// Re-export satellite modules that will be populated in later parts.
pub mod enter_design_mode;
pub mod enter_plan_mode;
pub mod exit_design_mode;
pub mod exit_plan_mode;
pub mod game_design;
pub mod i18n;
pub mod office_hours;
pub mod planning;
pub mod stores;

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::builtin::session_mode::stores::{
        InMemoryGameDesignStateStore, InMemoryOfficeHoursStateStore,
    };
    use std::sync::{Arc, Mutex};

    pub struct MockTelemetryClient {
        pub events: Mutex<Vec<(String, std::collections::HashMap<String, serde_json::Value>)>>,
    }

    impl MockTelemetryClient {
        pub fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl TelemetryClient for MockTelemetryClient {
        fn track(
            &self,
            event: &str,
            properties: std::collections::HashMap<String, serde_json::Value>,
        ) {
            self.events.lock().unwrap().push((event.into(), properties));
        }
    }

    pub struct MockMcpProvider;

    #[async_trait::async_trait]
    impl McpProvider for MockMcpProvider {
        async fn gbrain_available(&self) -> bool {
            false
        }
    }

    pub struct MockKaosContext {
        files: Mutex<std::collections::HashMap<String, String>>,
    }

    impl MockKaosContext {
        pub fn new() -> Self {
            Self {
                files: Mutex::new(std::collections::HashMap::new()),
            }
        }
        pub fn with_file(self, path: &str, content: &str) -> Self {
            self.files
                .lock()
                .unwrap()
                .insert(path.into(), content.into());
            self
        }
    }

    #[async_trait::async_trait]
    impl SessionModeContext for MockKaosContext {
        fn cwd(&self) -> String {
            "/".into()
        }
        fn project_root(&self) -> Option<String> {
            None
        }
        async fn read_text(&self, path: &str) -> anyhow::Result<String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("not found"))
        }
        async fn write_text(&self, _path: &str, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stat(&self, _path: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    pub struct MockSessionModeProvider {
        pub active: Mutex<bool>,
        pub kind: Mutex<Option<SessionModeKind>>,
        pub file_path: Mutex<Option<String>>,
        pub entered: Mutex<Vec<SessionModeKind>>,
        pub exited: Mutex<bool>,
        pub handed_off_to: Mutex<Vec<(String, Option<String>)>>,
        pub kaos: Arc<dyn SessionModeContext>,
        pub telemetry: Arc<dyn TelemetryClient>,
        pub office_hours_store: Arc<dyn OfficeHoursStateStore>,
        pub game_design_store: Arc<dyn GameDesignStateStore>,
        pub mcp: Arc<dyn McpProvider>,
    }

    impl MockSessionModeProvider {
        pub fn inactive() -> Self {
            Self {
                active: Mutex::new(false),
                kind: Mutex::new(None),
                file_path: Mutex::new(None),
                entered: Mutex::new(Vec::new()),
                exited: Mutex::new(false),
                handed_off_to: Mutex::new(Vec::new()),
                kaos: Arc::new(MockKaosContext::new()),
                telemetry: Arc::new(MockTelemetryClient::new()),
                office_hours_store: Arc::new(InMemoryOfficeHoursStateStore::new()),
                game_design_store: Arc::new(InMemoryGameDesignStateStore::new()),
                mcp: Arc::new(MockMcpProvider),
            }
        }

        pub fn active(kind: SessionModeKind) -> Self {
            let s = Self::inactive();
            *s.active.lock().unwrap() = true;
            *s.kind.lock().unwrap() = Some(kind);
            s
        }

        pub fn plan_mode_with_content(content: &str) -> Self {
            let mut s = Self::active(SessionModeKind::Plan);
            let path = "/plan.md".to_string();
            *s.file_path.lock().unwrap() = Some(path.clone());
            s.kaos = Arc::new(MockKaosContext::new().with_file(&path, content));
            s
        }

        pub fn design_mode_with_content(content: &str) -> Self {
            let mut s = Self::active(SessionModeKind::Design);
            let path = "/design.md".to_string();
            *s.file_path.lock().unwrap() = Some(path.clone());
            s.kaos = Arc::new(MockKaosContext::new().with_file(&path, content));
            s
        }
    }

    #[async_trait::async_trait]
    impl SessionModeProvider for MockSessionModeProvider {
        fn is_session_mode_active(&self) -> bool {
            *self.active.lock().unwrap()
        }
        fn session_mode_kind(&self) -> Option<SessionModeKind> {
            *self.kind.lock().unwrap()
        }
        fn session_mode_file_path(&self) -> Option<String> {
            self.file_path.lock().unwrap().clone()
        }

        async fn enter_session_mode(&self, kind: SessionModeKind) -> anyhow::Result<()> {
            self.entered.lock().unwrap().push(kind);
            *self.active.lock().unwrap() = true;
            *self.kind.lock().unwrap() = Some(kind);
            Ok(())
        }

        async fn exit_session_mode(&self) -> anyhow::Result<()> {
            *self.exited.lock().unwrap() = true;
            *self.active.lock().unwrap() = false;
            *self.kind.lock().unwrap() = None;
            Ok(())
        }

        async fn handoff_to(
            &self,
            target: &str,
            selected_label: Option<String>,
        ) -> anyhow::Result<()> {
            self.handed_off_to
                .lock()
                .unwrap()
                .push((target.into(), selected_label));
            *self.active.lock().unwrap() = false;
            *self.kind.lock().unwrap() = None;
            Ok(())
        }

        fn user_language(&self) -> Language {
            Language::En
        }
        fn set_user_language(&self, _lang: Language) {}
        fn open_external_available(&self) -> bool {
            false
        }
        fn telemetry(&self) -> Arc<dyn TelemetryClient> {
            self.telemetry.clone()
        }
        fn kaos(&self) -> Arc<dyn SessionModeContext> {
            self.kaos.clone()
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

    #[test]
    fn language_parses_zh_variants() {
        assert_eq!(Language::from_str("zh"), Some(Language::Zh));
        assert_eq!(Language::from_str("zh-CN"), Some(Language::Zh));
        assert_eq!(Language::from_str("zh_tw"), Some(Language::Zh));
        assert_eq!(Language::from_str("en"), Some(Language::En));
        assert_eq!(Language::from_str("EN-US"), Some(Language::En));
    }

    #[test]
    fn session_mode_kind_strings_match_ts() {
        assert_eq!(SessionModeKind::Plan.as_str(), "plan");
        assert_eq!(SessionModeKind::Design.as_str(), "design");
        assert_eq!(SessionModeKind::OfficeHours.as_str(), "office-hours");
        assert_eq!(SessionModeKind::GameDesign.as_str(), "game-design");
    }
}
