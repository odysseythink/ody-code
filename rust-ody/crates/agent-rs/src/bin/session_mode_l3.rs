use std::sync::Mutex;

use agent_rs::injection::manager::InjectionManager;
use agent_rs::injection::types::*;
use agent_rs::records::nested::SessionModeKind;
use agent_rs::session_mode::behaviors::create_default_mode_behavior_registry;
use agent_rs::session_mode::manager::SessionModeManager;
use agent_rs::session_mode::types::*;
use serde::{Deserialize, Serialize};

/// Fixture step as deserialized from JSON.
#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
enum FixtureStep {
    #[serde(rename = "enter")]
    Enter { kind: String, id: Option<String> },
    #[serde(rename = "exit")]
    Exit { id: Option<String> },
    #[serde(rename = "cancel")]
    Cancel { id: Option<String> },
    #[serde(rename = "handoff")]
    Handoff { target: String },
    #[serde(rename = "inject")]
    Inject,
    #[serde(rename = "assert")]
    Assert {
        #[serde(rename = "isActive")]
        is_active: bool,
        kind: Option<String>,
    },
}

/// Normalized event for JSONL output.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum OutputEvent {
    #[serde(rename = "session_mode.enter")]
    SessionModeEnter { id: String, kind: String },
    #[serde(rename = "session_mode.exit")]
    SessionModeExit { id: Option<String> },
    #[serde(rename = "session_mode.cancel")]
    SessionModeCancel { id: Option<String> },
    #[serde(rename = "injection")]
    Injection { variant: String, contains: String },
}

fn kind_to_string(kind: SessionModeKind) -> String {
    match kind {
        SessionModeKind::Plan => "plan".into(),
        SessionModeKind::Design => "design".into(),
        SessionModeKind::OfficeHours => "office-hours".into(),
        SessionModeKind::GameDesign => "game-design".into(),
    }
}

fn string_to_kind(s: &str) -> Option<SessionModeKind> {
    match s {
        "plan" => Some(SessionModeKind::Plan),
        "design" => Some(SessionModeKind::Design),
        "office-hours" => Some(SessionModeKind::OfficeHours),
        "game-design" => Some(SessionModeKind::GameDesign),
        _ => None,
    }
}

/// A context that captures session-mode records and injection outputs for
/// snapshot comparison.
struct FixtureContext {
    records: Mutex<Vec<agent_rs::records::AgentRecord>>,
    model_alias: Mutex<Option<String>>,
    active_mode: Mutex<Option<SessionModeKind>>,
    injected_texts: Mutex<Vec<(String, String)>>, // (text, variant)
    replay_records: Mutex<Vec<agent_rs::replay::AgentReplayRecord>>,
}

impl FixtureContext {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            model_alias: Mutex::new(Some("default-model".into())),
            active_mode: Mutex::new(None),
            injected_texts: Mutex::new(Vec::new()),
            replay_records: Mutex::new(Vec::new()),
        }
    }

    fn take_records(&self) -> Vec<agent_rs::records::AgentRecord> {
        std::mem::take(&mut *self.records.lock().unwrap())
    }

    fn take_injections(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.injected_texts.lock().unwrap())
    }
}

#[async_trait::async_trait]
impl SessionModeContext for FixtureContext {
    fn log_record(&self, record: agent_rs::records::AgentRecord) {
        self.records.lock().unwrap().push(record);
    }
    fn restoring_time(&self) -> Option<i64> {
        None
    }
    fn update_model_alias(&self, alias: Option<String>) {
        *self.model_alias.lock().unwrap() = alias;
    }
    fn refresh_llm(&self) {}
    fn resolve_mode_model_alias(&self, model_key: &str) -> Option<String> {
        match model_key {
            "plan" => Some("plan-model-v1".into()),
            "design" => Some("design-model-v1".into()),
            "officeHours" => Some("hours-model".into()),
            "gameDesign" => Some("gd-model".into()),
            _ => None,
        }
    }
    fn default_model_alias(&self) -> Option<String> {
        Some("default-model".into())
    }
    fn set_context_mode(&self, mode: Option<SessionModeKind>) {
        *self.active_mode.lock().unwrap() = mode;
    }
    fn active_mode(&self) -> Option<SessionModeKind> {
        *self.active_mode.lock().unwrap()
    }
    fn has_open_steps(&self) -> bool {
        false
    }
    fn push_replay_record(&self, record: agent_rs::replay::AgentReplayRecord) {
        self.replay_records.lock().unwrap().push(record);
    }
    fn set_replay_mode(&self, _mode: Option<SessionModeKind>) {}
    fn emit_status_updated(&self) {}
    fn cwd(&self) -> String {
        "/tmp/fixture".into()
    }
    fn project_root(&self) -> Option<String> {
        Some("/tmp/fixture".into())
    }
    fn mkdir_p(&self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn file_exists(&self, _path: &str) -> bool {
        false
    }
    fn read_file(&self, _path: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
    fn write_file(&self, _path: &str, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl InjectionManagerContext for FixtureContext {
    fn is_session_mode_active(&self) -> bool {
        self.active_mode.lock().unwrap().is_some()
    }
    fn session_mode_kind(&self) -> Option<SessionModeKind> {
        *self.active_mode.lock().unwrap()
    }
    fn consume_pending_handoff_for_plan(&self) -> Option<PendingDesignHandoff> {
        None
    }
    fn consume_pending_handoff_for_normal(&self) -> Option<PendingPlanHandoff> {
        None
    }
    fn session_mode_file_path(&self) -> Option<String> {
        None
    }
    fn append_system_reminder(&self, text: &str, _kind: &str, variant: &str) {
        self.injected_texts
            .lock()
            .unwrap()
            .push((text.to_string(), variant.to_string()));
    }
    fn context_history_len(&self) -> usize {
        0
    }
    fn assistant_turn_count(&self) -> usize {
        0
    }
    fn is_tool_active(&self, _tool_name: &str) -> bool {
        false
    }
    fn get_unavailable_skills_reminder(&self, _mode: SessionModeKind) -> Option<String> {
        None
    }
    fn get_active_goal_text(&self) -> Option<String> {
        None
    }
    fn permission_mode(&self) -> Option<String> {
        None
    }
    fn is_flag_enabled(&self, _flag: &str) -> bool {
        false
    }
    fn agent_type(&self) -> &str {
        "main"
    }
    fn restoring_time(&self) -> Option<i64> {
        None
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let fixture_path = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: session_mode_l3 <fixture.json>"))?;

    let fixture_json = std::fs::read_to_string(fixture_path)?;
    let fixture: serde_json::Value = serde_json::from_str(&fixture_json)?;
    let steps: Vec<FixtureStep> =
        serde_json::from_value(fixture.get("steps").cloned().unwrap_or_default())?;

    let ctx = FixtureContext::new();
    let registry = create_default_mode_behavior_registry();
    let mut sm_mgr = SessionModeManager::new(ctx, registry);

    // InjectionManager is created separately; it only needs a &dyn InjectionManagerContext.
    let inj_mgr = InjectionManager::new(sm_mgr.context());

    for step in &steps {
        match step {
            FixtureStep::Enter { kind, id } => {
                let k = string_to_kind(kind)
                    .ok_or_else(|| anyhow::anyhow!("Unknown kind: {}", kind))?;
                sm_mgr.enter(k, id.clone(), None).await?;
            }
            FixtureStep::Exit { id } => {
                sm_mgr.exit(id.clone()).await?;
            }
            FixtureStep::Cancel { id } => {
                sm_mgr.cancel(id.clone()).await?;
            }
            FixtureStep::Handoff { target } => {
                sm_mgr.handoff_to(target, HandoffOptions::default()).await?;
            }
            FixtureStep::Inject => {
                inj_mgr.inject(sm_mgr.context()).await;
            }
            FixtureStep::Assert { is_active, kind } => {
                assert_eq!(sm_mgr.is_active(), *is_active);
                if let Some(expected_kind) = kind {
                    assert_eq!(sm_mgr.kind(), string_to_kind(expected_kind));
                }
            }
        }
    }

    // Drain records and build output events.
    // Order: session_mode records come from sm_mgr.context().records,
    // injection events come from sm_mgr.context().injected_texts.
    let raw_records = sm_mgr.context().take_records();
    let injections = sm_mgr.context().take_injections();

    let mut output_events: Vec<OutputEvent> = Vec::new();

    for record in &raw_records {
        match record {
            agent_rs::records::AgentRecord::SessionModeEnter { id, kind, .. } => {
                output_events.push(OutputEvent::SessionModeEnter {
                    id: id.clone(),
                    kind: kind.map_or_else(|| String::from("plan"), |k| kind_to_string(k)),
                });
            }
            agent_rs::records::AgentRecord::SessionModeExit { id, .. } => {
                output_events.push(OutputEvent::SessionModeExit { id: id.clone() });
            }
            agent_rs::records::AgentRecord::SessionModeCancel { id, .. } => {
                output_events.push(OutputEvent::SessionModeCancel { id: id.clone() });
            }
            _ => {}
        }
    }

    for (text, variant) in &injections {
        output_events.push(OutputEvent::Injection {
            variant: variant.clone(),
            contains: text.clone(),
        });
    }

    // Output events as JSONL to stdout.
    let stdout = std::io::stdout();
    for event in &output_events {
        serde_json::to_writer(stdout.lock(), event)?;
        println!();
    }

    Ok(())
}
