use agent_rs::config::{
    AgentConfigContext, AgentConfigUpdateData, ConfigState, ProviderConfig,
    ResolvedRuntimeProvider, ThinkingConfig,
};
use agent_rs::records::AgentRecord;
use agent_rs::tool::types::BuiltinToolProvisionContext;
use kosong_rs::provider::{ModelCapability, ProviderType};
use std::env;
use std::fs;
use std::path::PathBuf;

struct FixtureContext;

impl AgentConfigContext for FixtureContext {
    fn log_record(&mut self, _record: AgentRecord) {}
    fn emit_status_updated(&self) {}
    fn initialize_builtin_tools(&self, _ctx: BuiltinToolProvisionContext) {}
    fn builtin_tool_provision_context(
        &self,
        _model_capabilities: ModelCapability,
    ) -> BuiltinToolProvisionContext {
        BuiltinToolProvisionContext::default()
    }
    fn get_cwd(&self) -> String {
        "/fixture/cwd".into()
    }
    fn chdir(&self, _cwd: &str) {}
    fn default_model(&self) -> Option<String> {
        Some("kimi-k2".into())
    }
    fn resolve_provider_config(&self, _model_alias: &str) -> Option<ResolvedRuntimeProvider> {
        Some(ResolvedRuntimeProvider {
            provider_name: "kimi".into(),
            provider: ProviderConfig {
                r#type: ProviderType::Kimi,
                model: "kimi-k2".into(),
                api_key: None,
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
    }
    fn thinking_config(&self) -> Option<ThinkingConfig> {
        Some(ThinkingConfig {
            mode: None,
            effort: Some("high".into()),
        })
    }
    fn push_config_updated_replay(&self, _config: &AgentConfigUpdateData) {}
}

fn main() {
    let mut state = ConfigState::new(FixtureContext);
    state.update(AgentConfigUpdateData {
        cwd: None,
        model_alias: None,
        profile_name: Some("fixture".into()),
        thinking_level: Some("on".into()),
        system_prompt: Some("fixture system prompt".into()),
    });

    let data = state.data();
    let json = serde_json::to_string_pretty(&data).unwrap();

    let out_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap()
        .join("tests/fixtures");
    fs::create_dir_all(&out_dir).unwrap();
    fs::write(out_dir.join("config-rust.json"), json).unwrap();
}
