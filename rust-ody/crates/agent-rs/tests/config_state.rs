use agent_rs::config::{
    AgentConfigContext, AgentConfigUpdateData, ConfigState, ProviderConfig,
    ResolvedRuntimeProvider, ThinkingConfig, ThinkingEffort,
};
use agent_rs::records::AgentRecord;
use agent_rs::tool::types::BuiltinToolProvisionContext;
use kosong_rs::provider::{ModelCapability, ProviderType};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct MockContext {
    records: Arc<Mutex<Vec<AgentRecord>>>,
    status_updates: Arc<Mutex<usize>>,
    tool_inits: Arc<Mutex<usize>>,
    chdirs: Arc<Mutex<Vec<String>>>,
    replays: Arc<Mutex<Vec<AgentConfigUpdateData>>>,
    cwd: String,
    default_model: Option<String>,
    thinking_config: Option<ThinkingConfig>,
}

impl AgentConfigContext for MockContext {
    fn log_record(&mut self, record: AgentRecord) {
        self.records.lock().unwrap().push(record);
    }

    fn emit_status_updated(&self) {
        *self.status_updates.lock().unwrap() += 1;
    }

    fn initialize_builtin_tools(&self, _ctx: BuiltinToolProvisionContext) {
        *self.tool_inits.lock().unwrap() += 1;
    }

    fn builtin_tool_provision_context(
        &self,
        _model_capabilities: ModelCapability,
    ) -> BuiltinToolProvisionContext {
        BuiltinToolProvisionContext::default()
    }

    fn get_cwd(&self) -> String {
        self.cwd.clone()
    }

    fn chdir(&self, cwd: &str) {
        self.chdirs.lock().unwrap().push(cwd.to_string());
    }

    fn default_model(&self) -> Option<String> {
        self.default_model.clone()
    }

    fn resolve_provider_config(&self, model_alias: &str) -> Option<ResolvedRuntimeProvider> {
        if model_alias == "kimi-k2" {
            Some(ResolvedRuntimeProvider {
                provider_name: "kimi".into(),
                provider: ProviderConfig {
                    r#type: ProviderType::Kimi,
                    model: "kimi-k2".into(),
                    api_key: Some("test".into()),
                    base_url: None,
                    default_headers: None,
                },
                model_capabilities: ModelCapability {
                    image_in: false,
                    video_in: false,
                    audio_in: false,
                    thinking: true,
                    tool_use: true,
                    max_context_tokens: 256_000,
                    max_output_tokens: 16_384,
                },
            })
        } else {
            None
        }
    }

    fn thinking_config(&self) -> Option<ThinkingConfig> {
        self.thinking_config.clone()
    }

    fn push_config_updated_replay(&self, config: &AgentConfigUpdateData) {
        self.replays.lock().unwrap().push(config.clone());
    }
}

#[test]
fn config_state_starts_with_cwd_and_default_model() {
    let ctx = MockContext {
        cwd: "/tmp".into(),
        default_model: Some("kimi-k2".into()),
        ..Default::default()
    };
    let state = ConfigState::new(ctx);
    assert_eq!(state.cwd(), "/tmp");
    assert_eq!(state.model_alias(), Some("kimi-k2"));
    assert!(state.has_model());
    assert!(state.has_provider());
    let data = state.data();
    assert_eq!(data.cwd, "/tmp");
    assert_eq!(data.model_alias, Some("kimi-k2".into()));
    assert!(data.model_capabilities.thinking);
}

#[test]
fn update_writes_record_and_changes_state() {
    let ctx = MockContext {
        cwd: "/tmp".into(),
        default_model: Some("kimi-k2".into()),
        thinking_config: Some(ThinkingConfig {
            mode: None,
            effort: Some("medium".into()),
        }),
        ..Default::default()
    };
    let mut state = ConfigState::new(ctx);
    state.update(AgentConfigUpdateData {
        cwd: Some("/home".into()),
        model_alias: None,
        profile_name: Some("code".into()),
        thinking_level: Some("on".into()),
        system_prompt: Some("be helpful".into()),
    });

    assert_eq!(state.cwd(), "/home");
    assert_eq!(state.profile_name(), Some("code"));
    assert_eq!(state.thinking_level(), ThinkingEffort::Medium);
    assert_eq!(state.system_prompt(), "be helpful");

    let records = state.context().records.lock().unwrap();
    assert_eq!(records.len(), 1);
    match &records[0] {
        AgentRecord::ConfigUpdate { update, .. } => {
            assert_eq!(update.cwd, Some("/home".into()));
            assert_eq!(update.profile_name, Some("code".into()));
        }
        _ => panic!("expected config.update record"),
    }
    drop(records);

    assert_eq!(*state.context().status_updates.lock().unwrap(), 1);
    assert_eq!(*state.context().tool_inits.lock().unwrap(), 1);
    assert_eq!(state.context().chdirs.lock().unwrap().len(), 1);
    assert_eq!(state.context().replays.lock().unwrap().len(), 1);
}

#[test]
fn update_without_changes_is_noop() {
    let ctx = MockContext {
        cwd: "/tmp".into(),
        default_model: Some("kimi-k2".into()),
        ..Default::default()
    };
    let mut state = ConfigState::new(ctx);
    state.update(AgentConfigUpdateData::default());
    assert!(state.context().records.lock().unwrap().is_empty());
    assert_eq!(*state.context().status_updates.lock().unwrap(), 0);
}

#[test]
fn model_alias_change_without_provider_drops_has_provider() {
    let ctx = MockContext {
        cwd: "/tmp".into(),
        default_model: None,
        ..Default::default()
    };
    let mut state = ConfigState::new(ctx);
    assert!(!state.has_provider());
    state.update(AgentConfigUpdateData {
        cwd: None,
        model_alias: Some("unknown-model".into()),
        profile_name: None,
        thinking_level: None,
        system_prompt: None,
    });
    assert!(!state.has_provider());
    let data = state.data();
    assert!(data.model_capabilities.is_unknown());
}

#[test]
#[should_panic(expected = "model not set")]
fn model_panics_when_unset() {
    let ctx = MockContext {
        cwd: "/tmp".into(),
        default_model: None,
        ..Default::default()
    };
    let state = ConfigState::new(ctx);
    let _ = state.model();
}
