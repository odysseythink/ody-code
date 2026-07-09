use kosong_rs::provider::{ChatProvider, ModelCapability};

use crate::records::nested::AgentConfigUpdateData;
use crate::records::AgentRecord;
use crate::tool::types::BuiltinToolProvisionContext;

use super::thinking::{resolve_thinking_effort, ThinkingConfig, ThinkingEffort};
use super::types::{AgentConfigData, ProviderConfig};

/// Runtime provider resolution result, aligned with TS `ResolvedRuntimeProvider`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRuntimeProvider {
    pub provider_name: String,
    pub provider: ProviderConfig,
    pub model_capabilities: ModelCapability,
}

/// Minimal Agent surface required by `ConfigState`. Implemented by the real
/// `Agent` in 4.3.9; tests provide a mock.
pub trait AgentConfigContext: Send + Sync {
    fn log_record(&mut self, record: AgentRecord);
    fn emit_status_updated(&self);
    fn initialize_builtin_tools(&self, ctx: BuiltinToolProvisionContext);
    fn builtin_tool_provision_context(
        &self,
        model_capabilities: kosong_rs::provider::ModelCapability,
    ) -> BuiltinToolProvisionContext;

    fn get_cwd(&self) -> String;
    fn chdir(&self, cwd: &str);

    fn default_model(&self) -> Option<String>;
    fn resolve_provider_config(&self, model_alias: &str) -> Option<ResolvedRuntimeProvider>;
    fn thinking_config(&self) -> Option<ThinkingConfig>;

    /// Push a `config_updated` replay entry (ReplayBuilder lives in 4.3.7).
    fn push_config_updated_replay(&self, config: &AgentConfigUpdateData);
}

pub struct ConfigState<C: AgentConfigContext> {
    context: C,
    cwd: String,
    model_alias: Option<String>,
    profile_name: Option<String>,
    thinking_level: ThinkingEffort,
    system_prompt: String,
}

impl<C: AgentConfigContext> ConfigState<C> {
    pub fn new(context: C) -> Self {
        let cwd = context.get_cwd();
        let model_alias = context.default_model();
        Self {
            context,
            cwd,
            model_alias,
            profile_name: None,
            thinking_level: ThinkingEffort::Off,
            system_prompt: String::new(),
        }
    }

    pub fn update(&mut self, changed: AgentConfigUpdateData) {
        if changed.cwd.is_none()
            && changed.model_alias.is_none()
            && changed.profile_name.is_none()
            && changed.thinking_level.is_none()
            && changed.system_prompt.is_none()
        {
            return;
        }

        self.context.log_record(AgentRecord::ConfigUpdate {
            time: None,
            update: changed.clone(),
        });
        self.context.push_config_updated_replay(&changed);

        if let Some(cwd) = changed.cwd.clone() {
            self.cwd = cwd;
            self.context.chdir(&self.cwd);
        }
        if let Some(alias) = changed.model_alias.clone() {
            self.model_alias = Some(alias);
        }
        if let Some(profile) = changed.profile_name.clone() {
            self.profile_name = Some(profile);
        }
        if let Some(level) = changed.thinking_level.as_deref() {
            self.thinking_level =
                resolve_thinking_effort(Some(level), self.context.thinking_config().as_ref());
        }
        if let Some(prompt) = changed.system_prompt.clone() {
            self.system_prompt = prompt;
        }

        if self.has_provider() && (changed.cwd.is_some() || changed.model_alias.is_some()) {
            let model_capabilities = self.data().model_capabilities;
            let ctx = self
                .context
                .builtin_tool_provision_context(model_capabilities);
            self.context.initialize_builtin_tools(ctx);
        }

        self.context.emit_status_updated();
    }

    pub fn data(&self) -> AgentConfigData {
        let resolved = self.try_resolved_provider_config();
        AgentConfigData {
            cwd: self.cwd.clone(),
            provider: resolved.as_ref().map(|r| r.provider.clone()),
            model_alias: self.model_alias.clone(),
            model_capabilities: resolved
                .as_ref()
                .map(|r| r.model_capabilities.clone())
                .unwrap_or_else(ModelCapability::unknown),
            profile_name: self.profile_name.clone(),
            thinking_level: format!("{:?}", self.thinking_level).to_lowercase(),
            system_prompt: self.system_prompt.clone(),
        }
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn has_model(&self) -> bool {
        self.model_alias.is_some()
    }

    pub fn has_provider(&self) -> bool {
        self.try_resolved_provider_config().is_some()
    }

    pub fn provider_config(&self) -> ProviderConfig {
        self.resolved_provider_config().provider
    }

    pub fn provider(&self) -> Box<dyn ChatProvider> {
        let resolved = self.resolved_provider_config();
        let provider_config = &resolved.provider;
        kosong_rs::create_chat_provider(kosong_rs::ProviderFactoryConfig {
            provider_id: resolved.provider_name,
            model: self.model(),
            api_key: provider_config.api_key.clone().filter(|k| !k.is_empty()),
            base_url: provider_config.base_url.clone(),
            default_headers: provider_config.default_headers.clone(),
        })
        .expect("provider resolution already succeeded")
    }

    pub fn model(&self) -> String {
        self.model_alias.clone().expect("model not set")
    }

    pub fn model_alias(&self) -> Option<&str> {
        self.model_alias.as_deref()
    }

    pub fn thinking_level(&self) -> ThinkingEffort {
        self.thinking_level
    }

    pub fn profile_name(&self) -> Option<&str> {
        self.profile_name.as_deref()
    }

    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    pub fn model_capabilities(&self) -> ModelCapability {
        self.try_resolved_provider_config()
            .map(|r| r.model_capabilities)
            .unwrap_or_else(ModelCapability::unknown)
    }

    pub fn context(&self) -> &C {
        &self.context
    }

    pub fn into_inner(self) -> C {
        self.context
    }

    fn resolved_provider_config(&self) -> ResolvedRuntimeProvider {
        self.try_resolved_provider_config()
            .expect("provider not configured")
    }

    fn try_resolved_provider_config(&self) -> Option<ResolvedRuntimeProvider> {
        let alias = self.model_alias.as_deref()?;
        self.context.resolve_provider_config(alias)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::thinking::ThinkingConfig;
    use crate::config::types::ProviderConfig;
    use crate::records::nested::AgentConfigUpdateData;

    struct Ctx(AgentConfigUpdateData);
    impl AgentConfigContext for Ctx {
        fn log_record(&mut self, _r: AgentRecord) {}
        fn emit_status_updated(&self) {}
        fn initialize_builtin_tools(&self, _ctx: BuiltinToolProvisionContext) {}
        fn builtin_tool_provision_context(
            &self,
            _model_capabilities: kosong_rs::provider::ModelCapability,
        ) -> BuiltinToolProvisionContext {
            BuiltinToolProvisionContext::default()
        }
        fn get_cwd(&self) -> String {
            "/".into()
        }
        fn chdir(&self, _cwd: &str) {}
        fn default_model(&self) -> Option<String> {
            None
        }
        fn resolve_provider_config(&self, alias: &str) -> Option<ResolvedRuntimeProvider> {
            Some(ResolvedRuntimeProvider {
                provider_name: "openai".into(),
                provider: ProviderConfig {
                    r#type: kosong_rs::provider::ProviderType::OpenAi,
                    model: alias.into(),
                    api_key: Some("sk-test".into()),
                    base_url: Some("https://example.com/v1".into()),
                    default_headers: None,
                },
                model_capabilities: kosong_rs::provider::ModelCapability::unknown(),
            })
        }
        fn thinking_config(&self) -> Option<ThinkingConfig> {
            None
        }
        fn push_config_updated_replay(&self, _c: &AgentConfigUpdateData) {}
    }

    #[test]
    fn provider_uses_resolved_credentials() {
        let mut state = ConfigState::new(Ctx(AgentConfigUpdateData::default()));
        state.update(AgentConfigUpdateData {
            model_alias: Some("gpt-4o-mini".into()),
            ..Default::default()
        });
        let provider = state.provider();
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model_name(), "gpt-4o-mini");
    }
}
