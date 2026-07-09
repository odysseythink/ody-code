//! Agent orchestrator — assembles all agent-rs subsystems into a concrete Agent type.
//!
//! Uses `Arc::new_cyclic` so that child modules (ContextMemory, ConfigState, etc.)
//! can hold `Weak<Agent>` references and call back through `AgentContext`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use kosong_rs::message::ContentPart;
use kosong_rs::provider::{AbortSignal, ChatProvider, ModelCapability};

use crate::agent_loop::llm::Llm;
use crate::background::manager::BackgroundManager;
use crate::background::persistence::BackgroundTaskPersistence;
use crate::compaction::full::FullCompaction;
use crate::compaction::micro::{MicroCompaction, MicroCompactionConfig};
use crate::compaction::normal_task_checkpoint::NormalModeTaskCheckpoint;
use crate::compaction::split_checkpoint::SplitPlanCheckpoint;
use crate::compaction::strategy::{CompactionStrategy, DefaultCompactionStrategy};
use crate::config::state::{AgentConfigContext, ConfigState, ResolvedRuntimeProvider};
use crate::config::thinking::ThinkingConfig;
use crate::config::types::AgentConfigData;
use crate::context::memory::ContextMemory;
use crate::context::types::{
    BackgroundNotifications, Clock, ContextAgent, ContextMessage, ContextSwitchFlusher,
    InjectionLifecycle, MicroCompaction as MicroCompactionTrait, PromptOrigin, RecordLog,
    ReplayBuilder as ReplayBuilderTrait, StatusEmitter,
};
use crate::injection::manager::InjectionManager;
use crate::injection::types::InjectionManagerContext;
use crate::permission::manager::{PermissionManager, PermissionManagerContext};
use crate::permission::types::{ApprovalRequest, PermissionData, PermissionRule};
use crate::records::nested::{
    AgentConfigUpdateData, ApprovalResponse, PermissionMode, SessionModeKind,
};
use crate::records::persistence::FileSystemAgentRecordPersistence;
use crate::records::records::{AgentRecords, ReplayResult};
use crate::records::AgentRecord;
use crate::replay::{AgentReplayRecord, ReplayBuilder};
use crate::session_mode::behaviors::create_default_mode_behavior_registry;
use crate::session_mode::manager::SessionModeManager;
use crate::session_mode::types::SessionModeContext;
use crate::skill::manager::{SkillActivationContext, SkillManager};
use crate::skill::registry;
use crate::skill::types::{
    ActivateSkillPayload, SkillActivatedEvent, SkillError, SkillPromptError,
};
use crate::tool::manager::ToolManager;
use crate::tool::ToolManagerContext;
use crate::turn::kosong_llm::{KosongLLM, KosongLLMConfig};
use crate::turn::turn_flow::TurnFlow;
use crate::turn::types::{
    AgentEvent, HookResult, StopHookBlock, TurnAgent, TurnEndResult, TurnEventEmitter,
};
use crate::usage::recorder::{UsageRecorder, UsageRecorderContext};

pub type AgentId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    Main,
    Sub,
    Independent,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Main => write!(f, "main"),
            AgentType::Sub => write!(f, "sub"),
            AgentType::Independent => write!(f, "independent"),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentEnvironment — host-provided capabilities
// ---------------------------------------------------------------------------

#[async_trait]
pub trait AgentEnvironment: Send + Sync {
    fn emit_event(&self, event: AgentEvent);

    async fn request_approval(
        &self,
        req: &ApprovalRequest,
        signal: AbortSignal,
    ) -> Result<ApprovalResponse, anyhow::Error>;

    fn fire_hook_pre_tool_use(
        &self,
        tool_name: &str,
        tool_input: serde_json::Value,
        tool_call_id: &str,
        signal: AbortSignal,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>>;

    fn fire_hook_permission_request(&self, tool_name: &str, data: serde_json::Value);
    fn fire_hook_permission_result(&self, tool_name: &str, data: serde_json::Value);

    fn fire_hook_user_prompt_submit(
        &self,
        input: Vec<ContentPart>,
        signal: AbortSignal,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<Vec<HookResult>, anyhow::Error>> + Send + '_>,
    >;

    fn fire_hook_stop_hook(
        &self,
        signal: AbortSignal,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<StopHookBlock>, anyhow::Error>>
                + Send
                + '_,
        >,
    >;

    fn fire_and_forget_hook(&self, event: &str, data: serde_json::Value);

    fn trigger_hook(
        &self,
        event: &str,
        data: serde_json::Value,
        signal: AbortSignal,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>>;

    fn track_telemetry(&self, event: &str, properties: serde_json::Value);
    fn log_debug(&self, msg: &str, data: serde_json::Value);
    fn log_warn(&self, msg: &str, data: serde_json::Value);
    fn log_error(&self, msg: &str, data: serde_json::Value);
}

// ---------------------------------------------------------------------------
// ProviderResolver — resolves model aliases to provider config
// ---------------------------------------------------------------------------

pub trait ProviderResolver: Send + Sync {
    fn default_model(&self) -> Option<String>;
    fn resolve(&self, model_alias: &str) -> Option<ResolvedRuntimeProvider>;
    fn thinking_config(&self) -> Option<ThinkingConfig>;
}

// ---------------------------------------------------------------------------
// LlmFactory — creates Llm instances
// ---------------------------------------------------------------------------

pub trait LlmFactory: Send + Sync {
    fn create(
        &self,
        provider: Box<dyn ChatProvider>,
        model_name: String,
        system_prompt: String,
        capability: Option<ModelCapability>,
    ) -> Arc<dyn Llm>;
}

struct DefaultLlmFactory;
impl LlmFactory for DefaultLlmFactory {
    fn create(
        &self,
        provider: Box<dyn ChatProvider>,
        model_name: String,
        system_prompt: String,
        capability: Option<ModelCapability>,
    ) -> Arc<dyn Llm> {
        Arc::new(KosongLLM::new(KosongLLMConfig {
            provider,
            model_name,
            system_prompt,
            capability,
            completion_budget_config: None,
        }))
    }
}

struct DefaultProviderResolver;
impl ProviderResolver for DefaultProviderResolver {
    fn default_model(&self) -> Option<String> {
        None
    }
    fn resolve(&self, _alias: &str) -> Option<ResolvedRuntimeProvider> {
        None
    }
    fn thinking_config(&self) -> Option<ThinkingConfig> {
        None
    }
}

// ---------------------------------------------------------------------------
// AgentContext — Weak<Agent> handle, implements all context traits
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AgentContext {
    pub(crate) agent: Weak<Agent>,
}

impl AgentContext {
    fn upgrade(&self) -> Option<Arc<Agent>> {
        self.agent.upgrade()
    }

    pub(crate) fn weak(&self) -> std::sync::Weak<Agent> {
        self.agent.clone()
    }
}

// ---------------------------------------------------------------------------
// Agent — the concrete orchestrator
// ---------------------------------------------------------------------------

pub struct Agent {
    pub id: AgentId,
    pub agent_type: AgentType,
    pub kaos: Arc<kaos_rs::kaos::Kaos>,
    pub homedir: Option<PathBuf>,
    pub environment: Arc<dyn AgentEnvironment>,
    pub provider_resolver: Arc<dyn ProviderResolver>,
    pub llm_factory: Arc<dyn LlmFactory>,

    pub records:
        Mutex<AgentRecords<FileSystemAgentRecordPersistence, Box<dyn FnMut(&AgentRecord) + Send>>>,

    pub contexts: HashMap<Option<SessionModeKind>, Mutex<ContextMemory>>,
    pub full_compactions: HashMap<Option<SessionModeKind>, FullCompaction>,
    pub micro_compactions: HashMap<Option<SessionModeKind>, MicroCompaction>,
    pub split_plan_checkpoint: SplitPlanCheckpoint,
    pub normal_mode_task_checkpoint: NormalModeTaskCheckpoint,

    pub config: Mutex<ConfigState<AgentContext>>,
    pub injection: Mutex<InjectionManager>,
    pub permission: Mutex<PermissionManager<'static, AgentContext>>,
    pub session_mode: Mutex<SessionModeManager<AgentContext>>,
    pub usage: Mutex<UsageRecorder<AgentContext>>,
    pub tools: Mutex<ToolManager<AgentContext>>,
    pub replay_builder: Mutex<ReplayBuilder>,

    pub active_mode: Mutex<Option<SessionModeKind>>,
    pub pending_context_switch: Mutex<Option<Option<SessionModeKind>>>,
    pub cached_llm: Mutex<Option<Arc<dyn Llm>>>,

    // Deferred: filled in Task 3
    pub turn: Mutex<Option<Arc<TurnFlow>>>,
    pub background: Mutex<Option<Arc<BackgroundManager>>>,
    pub cron: Mutex<Option<Arc<crate::cron::manager::CronManager>>>,

    // Collaboration tool wiring (Phase 4.4.4 Part 5)
    pub skill_registry: Mutex<Option<Arc<dyn registry::SkillRegistry>>>,
    pub question_callback: Mutex<Option<crate::tool::collaboration::QuestionCallback>>,
    pub subagent_host: Mutex<Option<Arc<dyn tools_rs::builtin::collaboration::SubagentHost>>>,
    pub self_weak: std::sync::Weak<Agent>,

    // Session-mode provider (Phase 4.4.5 Part 2)
    pub session_mode_provider:
        Mutex<Option<Arc<dyn tools_rs::builtin::session_mode::SessionModeProvider>>>,

    pub builtin_tools_provider: Mutex<Option<Arc<dyn crate::tool::types::BuiltinToolsProvider>>>,
}

// ---------------------------------------------------------------------------
// DefaultBuiltinToolsProvider — fallback that builds collaboration/session-mode tools
// ---------------------------------------------------------------------------

struct DefaultBuiltinToolsProvider {
    agent: Weak<Agent>,
}

impl DefaultBuiltinToolsProvider {
    fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }
}

impl crate::tool::types::BuiltinToolsProvider for DefaultBuiltinToolsProvider {
    fn provide(
        &self,
        _ctx: crate::tool::types::BuiltinToolProvisionContext,
    ) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
        let Some(agent) = self.agent.upgrade() else {
            return Vec::new();
        };
        let context = agent.agent_context();
        let skill_registry = agent.skill_registry.lock().unwrap().clone();
        let question_callback = agent.question_callback.lock().unwrap().clone();
        let subagent_host = agent.subagent_host.lock().unwrap().clone();
        let background = agent.background.lock().unwrap().clone();
        let session_mode_provider = agent.session_mode_provider.lock().unwrap().clone();
        crate::tool::collaboration::CollaborationToolkit::build_tools(
            context,
            skill_registry,
            question_callback,
            subagent_host,
            background,
            session_mode_provider,
        )
    }
}

// ---------------------------------------------------------------------------
// AgentBuilder
// ---------------------------------------------------------------------------

pub struct AgentBuilder {
    id: AgentId,
    agent_type: AgentType,
    kaos: Arc<kaos_rs::kaos::Kaos>,
    homedir: Option<PathBuf>,
    environment: Arc<dyn AgentEnvironment>,
    provider_resolver: Option<Arc<dyn ProviderResolver>>,
    llm_factory: Option<Arc<dyn LlmFactory>>,
    skill_registry: Option<Arc<dyn registry::SkillRegistry>>,
    question_callback: Option<crate::tool::collaboration::QuestionCallback>,
    subagent_host: Option<Arc<dyn tools_rs::builtin::collaboration::SubagentHost>>,
    builtin_tools_provider: Option<Arc<dyn crate::tool::types::BuiltinToolsProvider>>,
}

impl AgentBuilder {
    pub fn new(
        id: impl Into<AgentId>,
        kaos: Arc<kaos_rs::kaos::Kaos>,
        environment: Arc<dyn AgentEnvironment>,
    ) -> Self {
        Self {
            id: id.into(),
            agent_type: AgentType::Main,
            kaos,
            homedir: None,
            environment,
            provider_resolver: None,
            llm_factory: None,
            skill_registry: None,
            question_callback: None,
            subagent_host: None,
            builtin_tools_provider: None,
        }
    }

    pub fn agent_type(mut self, t: AgentType) -> Self {
        self.agent_type = t;
        self
    }

    pub fn homedir(mut self, p: impl Into<PathBuf>) -> Self {
        self.homedir = Some(p.into());
        self
    }

    pub fn provider_resolver(mut self, r: Arc<dyn ProviderResolver>) -> Self {
        self.provider_resolver = Some(r);
        self
    }

    pub fn llm_factory(mut self, f: Arc<dyn LlmFactory>) -> Self {
        self.llm_factory = Some(f);
        self
    }

    pub fn skills_registry(mut self, r: Box<dyn registry::SkillRegistry>) -> Self {
        self.skill_registry = Some(Arc::from(r));
        self
    }

    pub fn question_callback(
        mut self,
        callback: crate::tool::collaboration::QuestionCallback,
    ) -> Self {
        self.question_callback = Some(callback);
        self
    }

    pub fn subagent_host(
        mut self,
        host: Arc<dyn tools_rs::builtin::collaboration::SubagentHost>,
    ) -> Self {
        self.subagent_host = Some(host);
        self
    }

    pub fn builtin_tools_provider(
        mut self,
        p: Arc<dyn crate::tool::types::BuiltinToolsProvider>,
    ) -> Self {
        self.builtin_tools_provider = Some(p);
        self
    }

    pub async fn build(self) -> anyhow::Result<Arc<Agent>> {
        let agent_type = self.agent_type;
        let inner_id = self.id;
        let env = self.environment;
        let provider_resolver = self
            .provider_resolver
            .unwrap_or_else(|| Arc::new(DefaultProviderResolver));
        let llm_factory = self
            .llm_factory
            .unwrap_or_else(|| Arc::new(DefaultLlmFactory));

        let initial_capability = provider_resolver
            .default_model()
            .as_ref()
            .and_then(|m| provider_resolver.resolve(m))
            .map(|r| r.model_capabilities)
            .unwrap_or_else(kosong_rs::provider::ModelCapability::unknown);

        let records_holder: Arc<Mutex<Option<Weak<Agent>>>> = Arc::new(Mutex::new(None));
        let records_holder_for_closure = Arc::clone(&records_holder);
        let restore_handler: Box<dyn FnMut(&AgentRecord) + Send> =
            Box::new(move |record: &AgentRecord| {
                if let Some(agent) = records_holder_for_closure
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|w| w.upgrade())
                {
                    agent.restore_record(record);
                }
            });

        let persistence = self
            .homedir
            .as_ref()
            .map(|h| FileSystemAgentRecordPersistence::new(h.join("wire.jsonl")));
        let records_val = Mutex::new(AgentRecords::new(persistence, restore_handler, None));

        let agent = Arc::new_cyclic(|weak| {
            let ctx = AgentContext {
                agent: weak.clone(),
            };

            let mut contexts: HashMap<Option<SessionModeKind>, Mutex<ContextMemory>> =
                HashMap::new();
            let mut full_compactions: HashMap<Option<SessionModeKind>, FullCompaction> =
                HashMap::new();
            let mut micro_compactions: HashMap<Option<SessionModeKind>, MicroCompaction> =
                HashMap::new();

            // DefaultCompactionStrategy takes Fn returning max_size; use 200_000 as default
            let strategy: Arc<dyn CompactionStrategy> =
                Arc::new(DefaultCompactionStrategy::new(|| 200_000, None));

            for mode in [
                None,
                Some(SessionModeKind::Plan),
                Some(SessionModeKind::Design),
                Some(SessionModeKind::OfficeHours),
                Some(SessionModeKind::GameDesign),
            ] {
                let agent_ctx: Arc<dyn ContextAgent> = Arc::new(ctx.clone());
                contexts.insert(mode, Mutex::new(ContextMemory::new(agent_ctx)));
                full_compactions.insert(mode, FullCompaction::new(Arc::clone(&strategy)));
                micro_compactions
                    .insert(mode, MicroCompaction::new(MicroCompactionConfig::default()));
            }

            let config = Mutex::new(ConfigState::new(ctx.clone()));
            let mut tools = ToolManager::new(ctx.clone());
            if let Some(provider) = &self.builtin_tools_provider {
                let provision_ctx = crate::tool::types::BuiltinToolProvisionContext {
                    agent_type,
                    model_capabilities: initial_capability.clone(),
                    homedir: self.homedir.clone(),
                    goal_command_enabled: false,
                    rpc_open_external: false,
                    rpc_request_question: false,
                    background_available: false,
                    cron_available: false,
                    has_invocable_skills: self.skill_registry.is_some(),
                    subagent_host_available: self.subagent_host.is_some(),
                    web_searcher_available: false,
                    url_fetcher_available: false,
                };
                tools.sync_builtins(provider.as_ref(), provision_ctx);
            }
            let tools = Mutex::new(tools);
            let usage = Mutex::new(UsageRecorder::new(ctx.clone()));
            let permission = Mutex::new(PermissionManager::new(ctx.clone(), None));
            let session_mode = Mutex::new(SessionModeManager::new(
                ctx.clone(),
                create_default_mode_behavior_registry(),
            ));
            let injection = Mutex::new(InjectionManager::new(&ctx));
            let replay_builder = Mutex::new(ReplayBuilder::new());

            Agent {
                id: inner_id,
                agent_type,
                kaos: self.kaos,
                homedir: self.homedir,
                environment: env,
                provider_resolver,
                llm_factory,
                records: records_val,
                contexts,
                full_compactions,
                micro_compactions,
                split_plan_checkpoint: SplitPlanCheckpoint::new(),
                normal_mode_task_checkpoint: NormalModeTaskCheckpoint::new(),
                config,
                injection,
                permission,
                session_mode,
                usage,
                tools,
                replay_builder,
                active_mode: Mutex::new(None),
                pending_context_switch: Mutex::new(None),
                cached_llm: Mutex::new(None),
                turn: Mutex::new(None),
                background: Mutex::new(None),
                cron: Mutex::new(None),
                skill_registry: Mutex::new(self.skill_registry),
                question_callback: Mutex::new(self.question_callback),
                subagent_host: Mutex::new(self.subagent_host),
                self_weak: weak.clone(),
                session_mode_provider: Mutex::new(None),
                builtin_tools_provider: Mutex::new(self.builtin_tools_provider),
            }
        });

        let session_mode_provider = Arc::new(
            crate::session_mode::provider::AgentSessionModeProvider::new(
                Arc::downgrade(&agent),
                Arc::new(
                    tools_rs::builtin::session_mode::stores::InMemoryOfficeHoursStateStore::new(),
                ),
                Arc::new(
                    tools_rs::builtin::session_mode::stores::InMemoryGameDesignStateStore::new(),
                ),
                Arc::new(crate::session_mode::provider::AgentTelemetryClient::new(
                    Arc::downgrade(&agent),
                )),
                Arc::new(crate::session_mode::provider::AgentMcpProvider::new(
                    Arc::downgrade(&agent),
                )),
            ),
        );
        *agent.session_mode_provider.lock().unwrap() = Some(session_mode_provider);

        *records_holder.lock().unwrap() = Some(Arc::downgrade(&agent));

        if agent.builtin_tools_provider.lock().unwrap().is_none() {
            *agent.builtin_tools_provider.lock().unwrap() = Some(Arc::new(
                DefaultBuiltinToolsProvider::new(agent.self_weak.clone()),
            ));
        }

        let final_ctx =
            agent.builtin_tool_provision_context(agent.config.lock().unwrap().model_capabilities());
        agent.initialize_builtin_tools(final_ctx);

        Ok(agent)
    }
}

// ---------------------------------------------------------------------------
// Agent — basic methods
// ---------------------------------------------------------------------------

impl Agent {
    pub fn active_mode(&self) -> Option<SessionModeKind> {
        *self.active_mode.lock().unwrap()
    }

    pub fn active_context(&self) -> &Mutex<ContextMemory> {
        self.contexts
            .get(&self.active_mode())
            .expect("context for active mode")
    }

    pub fn active_full_compaction(&self) -> &FullCompaction {
        self.full_compactions
            .get(&self.active_mode())
            .expect("full compaction for active mode")
    }

    pub fn active_micro_compaction(&self) -> &MicroCompaction {
        self.micro_compactions
            .get(&self.active_mode())
            .expect("micro compaction for active mode")
    }

    pub fn agent_type_name(&self) -> &str {
        match self.agent_type {
            AgentType::Main => "main",
            AgentType::Sub => "sub",
            AgentType::Independent => "independent",
        }
    }

    pub fn set_context_mode(&self, mode: Option<SessionModeKind>) {
        if self.active_context().lock().unwrap().has_open_steps() {
            *self.pending_context_switch.lock().unwrap() = Some(mode);
            return;
        }
        *self.active_mode.lock().unwrap() = mode;
        self.replay_builder.lock().unwrap().set_mode(mode);
        *self.pending_context_switch.lock().unwrap() = None;
    }

    pub fn flush_deferred_context_switch(&self) {
        if let Some(mode) = self.pending_context_switch.lock().unwrap().take() {
            *self.active_mode.lock().unwrap() = mode;
            self.replay_builder.lock().unwrap().set_mode(mode);
        }
    }

    pub fn refresh_llm(&self) {
        *self.cached_llm.lock().unwrap() = None;
    }

    fn restore_record(&self, record: &AgentRecord) {
        match record {
            AgentRecord::ConfigUpdate { update, .. } => {
                if let Ok(mut cfg) = self.config.lock() {
                    cfg.update(update.clone());
                }
            }
            AgentRecord::PermissionSetMode { mode, .. } => {
                if let Ok(mut perm) = self.permission.lock() {
                    perm.set_mode(*mode);
                }
            }
            AgentRecord::SessionModeEnter { id, kind, path, .. } => {
                if let Ok(mut sm) = self.session_mode.lock() {
                    let _ = sm.restore_enter(id.clone(), *kind, path.clone());
                }
                self.set_context_mode(*kind);
            }
            AgentRecord::SessionModeExit { .. } => {
                if let Ok(mut sm) = self.session_mode.lock() {
                    let _ = sm.exit(None);
                }
                self.set_context_mode(None);
            }
            AgentRecord::SessionModeCancel { .. } => {
                if let Ok(mut sm) = self.session_mode.lock() {
                    let _ = sm.cancel(None);
                }
                self.set_context_mode(None);
            }
            AgentRecord::ToolsRegisterUserTool { registration, .. } => {
                if let Ok(mut t) = self.tools.lock() {
                    t.register_user_tool_without_log(registration);
                }
            }
            AgentRecord::ToolsUnregisterUserTool { name, .. } => {
                if let Ok(mut t) = self.tools.lock() {
                    t.unregister_user_tool_without_log(name);
                }
            }
            AgentRecord::ToolsSetActiveTools { names, .. } => {
                if let Ok(mut t) = self.tools.lock() {
                    t.set_active_tools_without_log(names);
                }
            }
            AgentRecord::ToolsUpdateStore { update, .. } => {
                if let Ok(mut t) = self.tools.lock() {
                    t.update_store_without_log(&update.key, update.value.clone());
                }
            }
            AgentRecord::ContextAppendMessage { message, .. } => {
                self.active_context()
                    .lock()
                    .unwrap()
                    .append_message(message.clone());
            }
            AgentRecord::ContextAppendLoopEvent { event, .. } => {
                self.active_context()
                    .lock()
                    .unwrap()
                    .append_loop_event(event.clone());
            }
            AgentRecord::ContextClear { .. } => {
                self.active_context().lock().unwrap().clear();
            }
            AgentRecord::ContextApplyCompaction { result, .. } => {
                self.active_context()
                    .lock()
                    .unwrap()
                    .apply_compaction(result.clone());
            }
            AgentRecord::ContextUndo { count, .. } => {
                self.active_context().lock().unwrap().undo(*count);
            }
            _ => {}
        }
    }

    /// Getter for the TurnFlow instance (set after Agent is fully wired).
    pub fn turn(&self) -> Arc<TurnFlow> {
        self.turn
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .expect("TurnFlow not yet set")
    }

    /// Set the TurnFlow instance. Called by the host after Agent construction.
    pub fn set_turn_flow(&self, flow: Arc<TurnFlow>) {
        *self.turn.lock().unwrap() = Some(flow);
    }

    pub fn update_config(&self, update: AgentConfigUpdateData) {
        self.config.lock().unwrap().update(update);
        self.refresh_llm();
    }

    pub fn set_permission_mode(&self, mode: PermissionMode) {
        self.permission.lock().unwrap().set_mode(mode);
    }

    pub fn permission_data(&self) -> PermissionData {
        self.permission.lock().unwrap().data()
    }

    /// Returns an AgentContext that points to this Agent via Weak.
    pub fn agent_context(&self) -> AgentContext {
        AgentContext {
            agent: self.self_weak.clone(),
        }
    }

    pub async fn enter_session_mode(
        &self,
        kind: SessionModeKind,
        id: Option<String>,
    ) -> anyhow::Result<()> {
        let agent = self
            .self_weak
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("agent dropped"))?;
        tokio::task::spawn_blocking(move || {
            let mut sm = agent.session_mode.lock().unwrap();
            let rt = tokio::runtime::Handle::current();
            rt.block_on(sm.enter(kind, id, None))
        })
        .await?
    }

    pub async fn exit_session_mode(&self) -> anyhow::Result<()> {
        let agent = self
            .self_weak
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("agent dropped"))?;
        tokio::task::spawn_blocking(move || {
            let mut sm = agent.session_mode.lock().unwrap();
            let rt = tokio::runtime::Handle::current();
            rt.block_on(sm.exit(None))
        })
        .await?
    }

    pub fn config_data(&self) -> AgentConfigData {
        self.config.lock().unwrap().data()
    }

    pub fn initialize_builtin_tools(&self, ctx: crate::tool::types::BuiltinToolProvisionContext) {
        let provider = self.builtin_tools_provider.lock().unwrap().clone();
        if let Ok(mut tools) = self.tools.lock() {
            if let Some(provider) = provider {
                tools.sync_builtins(provider.as_ref(), ctx);
            } else {
                tools.initialize_builtin_tools();
            }
        }
    }

    pub fn builtin_tool_provision_context(
        &self,
        model_capabilities: kosong_rs::provider::ModelCapability,
    ) -> crate::tool::types::BuiltinToolProvisionContext {
        crate::tool::types::BuiltinToolProvisionContext {
            agent_type: self.agent_type,
            model_capabilities,
            homedir: self.homedir.clone(),
            goal_command_enabled: false,
            rpc_open_external: false,
            rpc_request_question: false,
            background_available: self.background.lock().unwrap().is_some(),
            cron_available: self.cron.lock().unwrap().is_some(),
            has_invocable_skills: self
                .skill_registry
                .lock()
                .unwrap()
                .as_ref()
                .map(|r| !r.list_invocable_skills(None).is_empty())
                .unwrap_or(false),
            subagent_host_available: self.subagent_host.lock().unwrap().is_some(),
            web_searcher_available: false,
            url_fetcher_available: false,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentContext — minimal ContextAgent impl (needed for ContextMemory)
// ---------------------------------------------------------------------------

impl RecordLog for AgentContext {
    fn log_record(&self, record: AgentRecord) {
        if let Some(agent) = self.upgrade() {
            if let Ok(mut records) = agent.records.lock() {
                records.log_record(record);
            }
        }
    }
    fn restoring_time(&self) -> Option<i64> {
        self.upgrade().and_then(|agent| {
            let records = agent.records.lock().ok()?;
            records.restoring().and_then(|rc| rc.time)
        })
    }
}

impl MicroCompactionTrait for AgentContext {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        self.upgrade()
            .map(|a| a.active_micro_compaction().compact(messages))
            .unwrap_or_else(|| messages.to_vec())
    }
    fn reset(&self, max_cutoff: usize) {
        if let Some(agent) = self.upgrade() {
            agent.active_micro_compaction().reset(max_cutoff);
        }
    }
}

impl InjectionLifecycle for AgentContext {
    fn on_context_clear(&self) {
        if let Some(agent) = self.upgrade() {
            if let Ok(mut inj) = agent.injection.lock() {
                inj.on_context_clear();
            }
        }
    }
    fn on_context_compacted(&self, compacted_count: usize) {
        if let Some(agent) = self.upgrade() {
            if let Ok(mut inj) = agent.injection.lock() {
                inj.on_context_compacted(compacted_count);
            }
        }
    }
    fn on_context_message_removed(&self, index: usize) {
        if let Some(agent) = self.upgrade() {
            if let Ok(mut inj) = agent.injection.lock() {
                inj.on_context_message_removed(index);
            }
        }
    }
}

impl BackgroundNotifications for AgentContext {
    fn mark_delivered_notification(&self, _origin: &PromptOrigin) {}
}

impl ReplayBuilderTrait for AgentContext {
    fn push_message(&self, message: &ContextMessage) {
        if let Some(agent) = self.upgrade() {
            agent.replay_builder.lock().unwrap().push_message(message);
        }
    }
    fn remove_last_messages(&self, messages: &[ContextMessage]) {
        if let Some(agent) = self.upgrade() {
            agent
                .replay_builder
                .lock()
                .unwrap()
                .remove_last_messages(messages);
        }
    }
}

impl StatusEmitter for AgentContext {
    fn emit_status_updated(&self) {
        // status events routed through environment when needed
    }
}

impl ContextSwitchFlusher for AgentContext {
    fn flush_deferred_context_switch(&self) {
        if let Some(agent) = self.upgrade() {
            agent.flush_deferred_context_switch();
        }
    }
}

impl Clock for AgentContext {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}

impl ContextAgent for AgentContext {
    fn record_log(&self) -> &dyn RecordLog {
        self
    }
    fn micro_compaction(&self) -> &dyn MicroCompactionTrait {
        self
    }
    fn injection(&self) -> &dyn InjectionLifecycle {
        self
    }
    fn background(&self) -> &dyn BackgroundNotifications {
        self
    }
    fn replay_builder(&self) -> &dyn ReplayBuilderTrait {
        self
    }
    fn status_emitter(&self) -> &dyn StatusEmitter {
        self
    }
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher {
        self
    }
    fn clock(&self) -> &dyn Clock {
        self
    }
}

// ---------------------------------------------------------------------------
// Stub trait impls for AgentContext — minimal for compilation (Task 2)
// These will be fully implemented in Task 3.
// ---------------------------------------------------------------------------

impl AgentConfigContext for AgentContext {
    fn log_record(&mut self, record: AgentRecord) {
        RecordLog::log_record(self, record);
    }
    fn emit_status_updated(&self) {}
    fn initialize_builtin_tools(&self, ctx: crate::tool::types::BuiltinToolProvisionContext) {
        if let Some(agent) = self.upgrade() {
            agent.initialize_builtin_tools(ctx);
        }
    }
    fn builtin_tool_provision_context(
        &self,
        model_capabilities: kosong_rs::provider::ModelCapability,
    ) -> crate::tool::types::BuiltinToolProvisionContext {
        self.upgrade()
            .map(|a| a.builtin_tool_provision_context(model_capabilities))
            .unwrap_or_default()
    }
    fn get_cwd(&self) -> String {
        self.upgrade()
            .map(|a| {
                a.homedir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("/"))
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_else(|| "/".into())
    }
    fn chdir(&self, _cwd: &str) {}
    fn default_model(&self) -> Option<String> {
        self.upgrade()
            .and_then(|a| a.provider_resolver.default_model())
    }
    fn resolve_provider_config(&self, alias: &str) -> Option<ResolvedRuntimeProvider> {
        self.upgrade()
            .and_then(|a| a.provider_resolver.resolve(alias))
    }
    fn thinking_config(&self) -> Option<ThinkingConfig> {
        self.upgrade()
            .and_then(|a| a.provider_resolver.thinking_config())
    }
    fn push_config_updated_replay(&self, _config: &AgentConfigUpdateData) {}
}

struct StubUserToolExecutor;

#[async_trait::async_trait]
impl crate::agent_loop::types::UserToolExecutor for StubUserToolExecutor {
    async fn execute_user_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
        _ctx: crate::agent_loop::types::ExecutableToolContext,
    ) -> Result<crate::agent_loop::types::ExecutableToolResult, anyhow::Error> {
        Ok(crate::agent_loop::types::ExecutableToolResult::Error(
            crate::records::nested::ExecutableToolErrorResult {
                output: crate::records::nested::ExecutableToolOutput::Text(
                    "User/MCP tool execution will be wired in host-wiring part".into(),
                ),
                is_error: true,
                stop_turn: None,
                message: Some("User/MCP tool execution will be wired in host-wiring part".into()),
            },
        ))
    }
}

impl ToolManagerContext for AgentContext {
    fn log_record(&mut self, record: AgentRecord) {
        RecordLog::log_record(self, record);
    }
    fn emit_tool_list_updated(&mut self, _reason: &str, _server_name: Option<&str>) {}
    fn goal_mutation_tools_hidden(&self) -> bool {
        true
    }
    fn user_tool_executor(&self) -> Arc<dyn crate::agent_loop::types::UserToolExecutor> {
        Arc::new(StubUserToolExecutor)
    }
    fn mcp_tool_executor(&self) -> Arc<dyn crate::agent_loop::types::UserToolExecutor> {
        Arc::new(StubUserToolExecutor)
    }
}

impl UsageRecorderContext for AgentContext {
    fn log_record(&mut self, record: AgentRecord) {
        RecordLog::log_record(self, record);
    }
    fn emit_status_updated(&mut self) {}
}

impl PermissionManagerContext for AgentContext {
    fn mode(&self) -> PermissionMode {
        PermissionMode::Manual
    }
    fn rules(&self) -> Vec<PermissionRule> {
        vec![]
    }
    fn session_approval_rule_patterns(&self) -> Vec<String> {
        vec![]
    }
    fn add_session_approval_rule_pattern(&self, _pattern: String) {}
    fn log_record(&self, record: AgentRecord) {
        RecordLog::log_record(self, record);
    }
    fn emit_status_updated(&self) {}
    fn push_approval_result_replay(
        &self,
        _record: &crate::records::nested::PermissionApprovalResultRecord,
    ) {
    }
    fn track_telemetry(&self, _event: &str, _data: serde_json::Value) {}
    fn cwd(&self) -> String {
        "/".into()
    }
    fn path_class(&self) -> &str {
        "unix"
    }
    fn agent_type(&self) -> &str {
        "main"
    }
    fn is_sensitive_file(&self, _path: &str) -> bool {
        false
    }
    fn is_session_mode_active(&self) -> bool {
        false
    }
    fn session_mode_kind(&self) -> Option<&str> {
        None
    }
    fn session_mode_file_path(&self) -> Option<String> {
        None
    }
    fn is_writable_session_mode_path(&self, _path: &str) -> bool {
        false
    }
    fn exit_session_mode(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }
    fn find_git_work_tree_marker(&self) -> Option<(String, String)> {
        None
    }
    fn fire_hook_pre_tool_use(
        &self,
        _tool_name: &str,
        _tool_input: serde_json::Value,
        _tool_call_id: &str,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>>
    {
        Box::pin(async move { Ok(None) })
    }
    fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn request_approval(
        &self,
        _req: &ApprovalRequest,
        _signal: AbortSignal,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<ApprovalResponse, anyhow::Error>> + Send + '_>,
    > {
        Box::pin(async move {
            Ok(ApprovalResponse {
                decision: "approved".into(),
                scope: None,
                feedback: None,
                selected_label: None,
            })
        })
    }
    fn is_plan_review_display(&self, _display: &serde_json::Value) -> bool {
        false
    }
    fn writes_only_plan_file(
        &self,
        _execution: &crate::agent_loop::types::RunnableToolExecution,
        _path: &str,
    ) -> bool {
        false
    }
}

impl SessionModeContext for AgentContext {
    fn log_record(&self, record: AgentRecord) {
        RecordLog::log_record(self, record);
    }
    fn restoring_time(&self) -> Option<i64> {
        RecordLog::restoring_time(self)
    }
    fn update_model_alias(&self, _alias: Option<String>) {}
    fn refresh_llm(&self) {}
    fn resolve_mode_model_alias(&self, _model_key: &str) -> Option<String> {
        None
    }
    fn default_model_alias(&self) -> Option<String> {
        None
    }
    fn set_context_mode(&self, mode: Option<SessionModeKind>) {
        if let Some(agent) = self.upgrade() {
            agent.set_context_mode(mode);
        }
    }
    fn active_mode(&self) -> Option<SessionModeKind> {
        self.upgrade().map(|a| a.active_mode()).unwrap_or(None)
    }
    fn has_open_steps(&self) -> bool {
        self.upgrade()
            .map(|a| a.active_context().lock().unwrap().has_open_steps())
            .unwrap_or(false)
    }
    fn push_replay_record(&self, _record: AgentReplayRecord) {}
    fn set_replay_mode(&self, _mode: Option<SessionModeKind>) {}
    fn emit_status_updated(&self) {}
    fn cwd(&self) -> String {
        "/".into()
    }
    fn project_root(&self) -> Option<String> {
        None
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

impl InjectionManagerContext for AgentContext {
    fn is_session_mode_active(&self) -> bool {
        false
    }
    fn session_mode_kind(&self) -> Option<SessionModeKind> {
        None
    }
    fn consume_pending_handoff_for_plan(
        &self,
    ) -> Option<crate::injection::types::PendingDesignHandoff> {
        None
    }
    fn consume_pending_handoff_for_normal(
        &self,
    ) -> Option<crate::injection::types::PendingPlanHandoff> {
        None
    }
    fn session_mode_file_path(&self) -> Option<String> {
        None
    }
    fn append_system_reminder(&self, _text: &str, _kind: &str, _variant: &str) {}
    fn context_history_len(&self) -> usize {
        0
    }
    fn assistant_turn_count(&self) -> usize {
        0
    }
    fn is_tool_active(&self, _tool_name: &str) -> bool {
        true
    }
    fn get_unavailable_skills_reminder(&self, _mode: SessionModeKind) -> Option<String> {
        None
    }
    fn get_active_goal_text(&self) -> Option<String> {
        None
    }
    fn permission_mode(&self) -> Option<String> {
        Some("manual".into())
    }
    fn is_flag_enabled(&self, _flag: &str) -> bool {
        false
    }
    fn agent_type(&self) -> &str {
        "main"
    }
    fn restoring_time(&self) -> Option<i64> {
        RecordLog::restoring_time(self)
    }
}

impl SkillActivationContext for AgentContext {
    fn emit_skill_activated(&mut self, _event: SkillActivatedEvent) {}
    fn telemetry_track(&mut self, _event_name: &str, _properties: HashMap<String, String>) {}
    fn prompt(
        &mut self,
        _input: Vec<ContentPart>,
        _origin: PromptOrigin,
    ) -> Result<(), SkillPromptError> {
        Ok(())
    }
    fn new_activation_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

// ---------------------------------------------------------------------------
// Agent — TurnAgent implementation (Task 3)
// ---------------------------------------------------------------------------

use crate::agent_loop::llm::LlmChatParams;
use crate::agent_loop::types::AuthorizeToolExecutionResult;
use crate::agent_loop::types::ResolvedToolExecutionHookContext;
use crate::turn::types::{
    CompactGenerateResult, CompactedHistory, GoalSnapshot, LoopControl, TurnConfig, TurnContext,
    TurnFullCompaction, TurnGoal, TurnHooks, TurnInjection, TurnLlmResolver, TurnLog, TurnMcp,
    TurnMicroCompaction, TurnNormalTaskCheckpoint, TurnPermission, TurnRecords, TurnSessionMode,
    TurnSplitPlanCheckpoint, TurnSubagentHost, TurnTelemetry, TurnTools, TurnUsage,
};

impl TurnAgent for Agent {
    fn context(&self) -> &dyn TurnContext {
        self
    }
    fn usage(&self) -> &dyn TurnUsage {
        self
    }
    fn config(&self) -> &dyn TurnConfig {
        self
    }
    fn tools(&self) -> &dyn TurnTools {
        self
    }
    fn permission(&self) -> &dyn TurnPermission {
        self
    }
    fn injection(&self) -> &dyn TurnInjection {
        self
    }
    fn full_compaction(&self) -> &dyn TurnFullCompaction {
        self
    }
    fn micro_compaction(&self) -> &dyn TurnMicroCompaction {
        self
    }
    fn split_plan_checkpoint(&self) -> &dyn TurnSplitPlanCheckpoint {
        self
    }
    fn normal_mode_task_checkpoint(&self) -> &dyn TurnNormalTaskCheckpoint {
        self
    }
    fn session_mode(&self) -> &dyn TurnSessionMode {
        self
    }
    fn goals(&self) -> Option<&dyn TurnGoal> {
        None
    }
    fn hooks(&self) -> Option<&dyn TurnHooks> {
        Some(self)
    }
    fn telemetry(&self) -> &dyn TurnTelemetry {
        self
    }
    fn log(&self) -> &dyn TurnLog {
        self
    }
    fn mcp(&self) -> Option<&dyn TurnMcp> {
        None
    }
    fn subagent_host(&self) -> Option<&dyn TurnSubagentHost> {
        None
    }
    fn records(&self) -> &dyn TurnRecords {
        self
    }
    fn event_emitter(&self) -> &dyn TurnEventEmitter {
        self
    }
    fn llm_resolver(&self) -> &dyn TurnLlmResolver {
        self
    }
    fn flush_deferred_context_switch(&self) {
        self.flush_deferred_context_switch();
    }
    fn agent_type(&self) -> &str {
        self.agent_type_name()
    }
    fn homedir(&self) -> Option<&str> {
        self.homedir.as_ref().and_then(|p| p.to_str())
    }
    fn goal_runtime_enabled(&self) -> bool {
        false
    }
}

// --------- TurnConfig ---------

impl TurnConfig for Agent {
    fn model(&self) -> String {
        self.config.lock().unwrap().model()
    }
    fn model_alias(&self) -> Option<String> {
        self.config.lock().unwrap().data().model_alias
    }
    fn system_prompt(&self) -> String {
        self.config.lock().unwrap().data().system_prompt
    }
    fn thinking_level(&self) -> String {
        self.config.lock().unwrap().data().thinking_level
    }
    fn provider(&self) -> Box<dyn ChatProvider> {
        self.config.lock().unwrap().provider()
    }
    fn model_capabilities(&self) -> ModelCapability {
        self.config.lock().unwrap().model_capabilities()
    }
    fn loop_control(&self) -> Option<LoopControl> {
        Some(LoopControl::default())
    }
    fn has_model(&self) -> bool {
        self.config.lock().unwrap().has_model()
    }
    fn e2e_enabled(&self) -> bool {
        false
    }
    fn test_review_enabled(&self) -> bool {
        false
    }
}

// --------- TurnContext ---------

impl TurnContext for Agent {
    fn append_user_message(&self, content: Vec<ContentPart>, origin: PromptOrigin) {
        self.active_context()
            .lock()
            .unwrap()
            .append_user_message(content, origin);
    }
    fn append_message(&self, message: ContextMessage) {
        self.active_context()
            .lock()
            .unwrap()
            .append_message(message);
    }
    fn messages(&self) -> Vec<kosong_rs::message::Message> {
        self.active_context().lock().unwrap().messages()
    }
    fn append_loop_event(&self, event: crate::records::nested::LoopRecordedEvent) {
        self.active_context()
            .lock()
            .unwrap()
            .append_loop_event(event);
    }
    fn has_open_steps(&self) -> bool {
        self.active_context().lock().unwrap().has_open_steps()
    }
    fn clear(&self) {
        self.active_context().lock().unwrap().clear();
    }
    fn history(&self) -> Vec<ContextMessage> {
        self.active_context().lock().unwrap().history().to_vec()
    }
    fn token_count(&self) -> i64 {
        self.active_context().lock().unwrap().token_count()
    }
    fn token_count_with_pending(&self) -> i64 {
        self.active_context()
            .lock()
            .unwrap()
            .token_count_with_pending()
    }
    fn apply_compaction(&self, result: crate::records::nested::CompactionResult) {
        self.active_context()
            .lock()
            .unwrap()
            .apply_compaction(result);
    }
    fn project(&self, _messages: &[ContextMessage]) -> Vec<kosong_rs::message::Message> {
        self.active_context().lock().unwrap().messages()
    }
    fn last_assistant_at_ms(&self) -> Option<i64> {
        self.active_context().lock().unwrap().last_assistant_at()
    }
    fn append_system_reminder(&self, content: &str, origin: PromptOrigin) {
        self.active_context()
            .lock()
            .unwrap()
            .append_system_reminder(content, origin);
    }
}

// --------- TurnUsage ---------

impl TurnUsage for Agent {
    fn begin_turn(&self) {
        self.usage.lock().unwrap().begin_turn();
    }
    fn end_turn(&self) {
        self.usage.lock().unwrap().end_turn();
    }
    fn record(
        &self,
        model: &str,
        usage: kosong_rs::usage::TokenUsage,
        scope: crate::records::nested::UsageRecordScope,
    ) {
        self.usage.lock().unwrap().record(model, usage, scope);
    }
    fn current_turn_usage(&self) -> Option<kosong_rs::usage::TokenUsage> {
        self.usage
            .lock()
            .unwrap()
            .status()
            .and_then(|s| s.current_turn)
    }
}

// --------- TurnTools ---------

impl TurnTools for Agent {
    fn loop_tools(&self) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
        self.tools.lock().unwrap().loop_tools()
    }
    fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value> {
        self.tools.lock().unwrap().store_data()
    }
}

// --------- TurnPermission ---------

#[async_trait]
impl TurnPermission for Agent {
    async fn before_tool_call(
        &self,
        _ctx: ResolvedToolExecutionHookContext<'_>,
    ) -> Result<Option<AuthorizeToolExecutionResult>, anyhow::Error> {
        Ok(None)
    }
}

// --------- TurnInjection ---------

#[async_trait]
impl TurnInjection for Agent {
    async fn inject_goal(&self) { /* stub */
    }
    async fn inject(&self) { /* stub */
    }
}

// --------- TurnFullCompaction ---------

#[async_trait]
impl TurnFullCompaction for Agent {
    fn reset_for_turn(&self, _agent: Arc<dyn TurnAgent>) {}
    async fn before_step(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }
    async fn after_step(&self, _agent: Arc<dyn TurnAgent>) {}
    async fn handle_overflow_error(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _signal: AbortSignal,
        _error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }
    async fn compact_checkpoint(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }
    fn begin(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _data: crate::records::nested::CompactionBeginData,
    ) {
    }
    fn cancel(&self, _agent: Arc<dyn TurnAgent>) {}
    fn compacted_history(&self) -> Vec<CompactedHistory> {
        vec![]
    }
    fn is_compacting(&self) -> bool {
        false
    }
}

// --------- TurnMicroCompaction ---------

impl TurnMicroCompaction for Agent {
    fn detect(&self, agent: Arc<dyn TurnAgent>) {
        self.active_micro_compaction().detect(agent);
    }
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        self.active_micro_compaction().compact(messages)
    }
    fn reset(&self, max_cutoff: usize) {
        self.active_micro_compaction().reset(max_cutoff);
    }
}

// --------- TurnSplitPlanCheckpoint ---------

#[async_trait]
impl TurnSplitPlanCheckpoint for Agent {
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) {
        let _ = self.split_plan_checkpoint.before_step(agent, signal).await;
    }
    fn reset(&self) {
        self.split_plan_checkpoint.reset();
    }
}

// --------- TurnNormalTaskCheckpoint ---------

#[async_trait]
impl TurnNormalTaskCheckpoint for Agent {
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) {
        let _ = self
            .normal_mode_task_checkpoint
            .before_step(agent, signal)
            .await;
    }
    fn reset(&self) {
        self.normal_mode_task_checkpoint.reset();
    }
}

// --------- TurnSessionMode ---------

#[async_trait]
impl TurnSessionMode for Agent {
    fn is_active(&self) -> bool {
        self.session_mode.lock().unwrap().is_active()
    }
    fn kind(&self) -> Option<String> {
        self.session_mode
            .lock()
            .unwrap()
            .kind()
            .map(|k| format!("{:?}", k).to_lowercase())
    }
    fn file_path(&self) -> Option<String> {
        self.session_mode.lock().unwrap().session_mode_file_path()
    }
    async fn data(&self) -> Option<String> {
        None
    }
}

// --------- TurnHooks ---------

#[async_trait]
impl TurnHooks for Agent {
    async fn trigger_user_prompt_submit(
        &self,
        input: Vec<ContentPart>,
        signal: AbortSignal,
    ) -> Result<Vec<HookResult>, anyhow::Error> {
        self.environment
            .fire_hook_user_prompt_submit(input, signal)
            .await
    }
    async fn trigger_stop_hook(
        &self,
        signal: AbortSignal,
    ) -> Result<Option<StopHookBlock>, anyhow::Error> {
        self.environment.fire_hook_stop_hook(signal).await
    }
    fn fire_and_forget_trigger(&self, event: &str, data: serde_json::Value) {
        self.environment.fire_and_forget_hook(event, data);
    }
    async fn trigger(
        &self,
        event: &str,
        data: serde_json::Value,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        self.environment.trigger_hook(event, data, signal).await
    }
}

// --------- TurnTelemetry ---------

impl TurnTelemetry for Agent {
    fn track(&self, event: &str, properties: serde_json::Value) {
        self.environment.track_telemetry(event, properties);
    }
}

// --------- TurnLog ---------

impl TurnLog for Agent {
    fn debug(&self, msg: &str, data: serde_json::Value) {
        self.environment.log_debug(msg, data);
    }
    fn warn(&self, msg: &str, data: serde_json::Value) {
        self.environment.log_warn(msg, data);
    }
    fn error(&self, msg: &str, data: serde_json::Value) {
        self.environment.log_error(msg, data);
    }
}

// --------- TurnRecords ---------

impl TurnRecords for Agent {
    fn log_record(&self, record: AgentRecord) {
        if let Ok(mut r) = self.records.lock() {
            r.log_record(record);
        }
    }
}

// --------- TurnEventEmitter ---------

impl TurnEventEmitter for Agent {
    fn emit_event(&self, event: AgentEvent) {
        self.environment.emit_event(event);
    }
}

// --------- TurnLlmResolver ---------

#[async_trait]
impl TurnLlmResolver for Agent {
    fn refresh_llm(&self) {
        self.refresh_llm();
    }
    fn llm(&self) -> Arc<dyn Llm> {
        let mut cached = self.cached_llm.lock().unwrap();
        if cached.is_none() {
            let cfg = self.config.lock().unwrap();
            let provider = cfg.provider();
            let model_name = cfg.model();
            let system_prompt = cfg.data().system_prompt;
            let capability = Some(cfg.model_capabilities());
            *cached =
                Some(
                    self.llm_factory
                        .create(provider, model_name, system_prompt, capability),
                );
        }
        cached.as_ref().unwrap().clone()
    }
    async fn generate_one_off(
        &self,
        _provider: Box<dyn ChatProvider + Send>,
        _system_prompt: String,
        _tools: Vec<kosong_rs::provider::Tool>,
        _messages: Vec<kosong_rs::message::Message>,
        _signal: AbortSignal,
    ) -> Result<CompactGenerateResult, anyhow::Error> {
        Ok(CompactGenerateResult::default())
    }
}

// Agent also implements InjectionManagerContext (delegates to sub-modules)
impl InjectionManagerContext for Agent {
    fn is_session_mode_active(&self) -> bool {
        self.session_mode.lock().unwrap().is_active()
    }
    fn session_mode_kind(&self) -> Option<SessionModeKind> {
        self.active_mode()
    }
    fn consume_pending_handoff_for_plan(
        &self,
    ) -> Option<crate::injection::types::PendingDesignHandoff> {
        None
    }
    fn consume_pending_handoff_for_normal(
        &self,
    ) -> Option<crate::injection::types::PendingPlanHandoff> {
        None
    }
    fn session_mode_file_path(&self) -> Option<String> {
        self.session_mode.lock().unwrap().session_mode_file_path()
    }
    fn append_system_reminder(&self, text: &str, _kind: &str, _variant: &str) {
        self.active_context()
            .lock()
            .unwrap()
            .append_system_reminder(
                text,
                PromptOrigin::Injection {
                    variant: "system".into(),
                },
            );
    }
    fn context_history_len(&self) -> usize {
        self.active_context().lock().unwrap().history().len()
    }
    fn assistant_turn_count(&self) -> usize {
        0
    }
    fn is_tool_active(&self, _tool_name: &str) -> bool {
        true
    }
    fn get_unavailable_skills_reminder(&self, _mode: SessionModeKind) -> Option<String> {
        None
    }
    fn get_active_goal_text(&self) -> Option<String> {
        None
    }
    fn permission_mode(&self) -> Option<String> {
        Some(format!("{:?}", self.permission.lock().unwrap().mode()).to_lowercase())
    }
    fn is_flag_enabled(&self, _flag: &str) -> bool {
        false
    }
    fn agent_type(&self) -> &str {
        self.agent_type_name()
    }
    fn restoring_time(&self) -> Option<i64> {
        self.records
            .lock()
            .ok()
            .and_then(|r| r.restoring().and_then(|rc| rc.time))
    }
}

#[cfg(test)]
mod restore_tests {
    use super::*;
    use std::collections::HashMap;

    struct NoopEnv;
    #[async_trait::async_trait]
    impl AgentEnvironment for NoopEnv {
        fn emit_event(&self, _event: AgentEvent) {}
        async fn request_approval(
            &self,
            _req: &ApprovalRequest,
            _signal: kosong_rs::provider::AbortSignal,
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
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_,
            >,
        > {
            Box::pin(async move { Ok(None) })
        }
        fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_user_prompt_submit(
            &self,
            _input: Vec<kosong_rs::message::ContentPart>,
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Vec<HookResult>, anyhow::Error>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async move { Ok(vec![]) })
        }
        fn fire_hook_stop_hook(
            &self,
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Option<StopHookBlock>, anyhow::Error>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async move { Ok(None) })
        }
        fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
        fn trigger_hook(
            &self,
            _event: &str,
            _data: serde_json::Value,
            _signal: kosong_rs::provider::AbortSignal,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>,
        > {
            Box::pin(async move { Ok(()) })
        }
        fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
        fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
    }

    #[tokio::test]
    async fn restore_preserves_active_tools_and_store() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            tmp.path(),
        ));

        let env = Arc::new(NoopEnv);
        let agent1 = AgentBuilder::new("main", Arc::clone(&kaos), env)
            .homedir(tmp.path())
            .build()
            .await
            .unwrap();

        {
            let mut t = agent1.tools.lock().unwrap();
            t.initialize_builtin_tools();
            t.register_user_tool(crate::tool::types::UserToolRegistration {
                name: "custom".into(),
                description: "custom tool".into(),
                parameters: serde_json::json!({"type": "object"}),
            });
            t.set_active_tools(&["Read".into(), "custom".into()]);
            t.update_store("todo", serde_json::json!(["a", "b"]));
        }

        // 强制把 records 刷盘
        agent1.records.lock().unwrap().flush().await.unwrap();

        let env2 = Arc::new(NoopEnv);
        let agent2 = AgentBuilder::new("main", Arc::clone(&kaos), env2)
            .homedir(tmp.path())
            .build()
            .await
            .unwrap();

        // 恢复前需要先初始化 builtin 工具表，否则 active set 找不到对应工具
        agent2.tools.lock().unwrap().initialize_builtin_tools();
        agent2.records.lock().unwrap().replay().await.unwrap();

        let active: HashMap<String, bool> = agent2
            .tools
            .lock()
            .unwrap()
            .data()
            .into_iter()
            .filter(|i| i.active)
            .map(|i| (i.name, true))
            .collect();
        assert!(active.contains_key("Read"));
        assert!(active.contains_key("custom"));
        assert!(!active.contains_key("Write"));

        let store = agent2.tools.lock().unwrap().store_data();
        assert_eq!(store.get("todo"), Some(&serde_json::json!(["a", "b"])));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_loop::types::ExecutableTool as LoopExecutableTool;
    use crate::tool::types::{BuiltinToolProvisionContext, BuiltinToolsProvider};
    use kosong_rs::message::ContentPart;
    use kosong_rs::provider::AbortSignal;
    use serde_json::json;
    use std::pin::Pin;

    struct NoopEnv;
    #[async_trait::async_trait]
    impl AgentEnvironment for NoopEnv {
        fn emit_event(&self, _event: AgentEvent) {}
        async fn request_approval(
            &self,
            _req: &crate::permission::types::ApprovalRequest,
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
        ) -> Pin<
            Box<
                dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_,
            >,
        > {
            Box::pin(async { Ok(None) })
        }
        fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
        fn fire_hook_user_prompt_submit(
            &self,
            _input: Vec<ContentPart>,
            _signal: AbortSignal,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Vec<crate::turn::types::HookResult>, anyhow::Error>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(vec![]) })
        }
        fn fire_hook_stop_hook(
            &self,
            _signal: AbortSignal,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Option<crate::turn::types::StopHookBlock>, anyhow::Error>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(None) })
        }
        fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
        fn trigger_hook(
            &self,
            _event: &str,
            _data: serde_json::Value,
            _signal: AbortSignal,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
        fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
        fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
        fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
    }

    struct SingleBuiltinProvider;
    impl BuiltinToolsProvider for SingleBuiltinProvider {
        fn provide(&self, _ctx: BuiltinToolProvisionContext) -> Vec<Arc<dyn LoopExecutableTool>> {
            vec![Arc::new(crate::tool::types::ExecutableTool {
                name: "HostBuiltin".into(),
                description: "from provider".into(),
                parameters: json!({"type":"object"}),
            })]
        }
    }

    #[tokio::test]
    async fn agent_uses_builtin_tools_provider() {
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let agent = AgentBuilder::new("test", kaos, Arc::new(NoopEnv))
            .builtin_tools_provider(Arc::new(SingleBuiltinProvider))
            .build()
            .await
            .unwrap();

        agent
            .tools
            .lock()
            .unwrap()
            .set_active_tools(&["HostBuiltin".into(), "Read".into()]);
        let names: Vec<String> = agent
            .tools
            .lock()
            .unwrap()
            .loop_tools()
            .into_iter()
            .map(|t| t.name().to_string())
            .collect();
        assert_eq!(names, vec!["HostBuiltin", "Read"]);
    }

    struct StoreProvider;
    impl BuiltinToolsProvider for StoreProvider {
        fn provide(&self, _ctx: BuiltinToolProvisionContext) -> Vec<Arc<dyn LoopExecutableTool>> {
            vec![Arc::new(crate::tool::types::ExecutableTool {
                name: "StoreBuiltin".into(),
                description: "store builtin".into(),
                parameters: json!({"type":"object"}),
            })]
        }
    }

    #[tokio::test]
    async fn turn_tools_uses_tool_manager_loop_tools_and_store() {
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::detect_environment_from_node(),
            std::env::current_dir().unwrap(),
        ));
        let agent = AgentBuilder::new("test", kaos, Arc::new(NoopEnv))
            .builtin_tools_provider(Arc::new(StoreProvider))
            .build()
            .await
            .unwrap();

        agent
            .tools
            .lock()
            .unwrap()
            .set_active_tools(&["StoreBuiltin".into()]);
        agent
            .tools
            .lock()
            .unwrap()
            .update_store("todos", json!("1 pending"));

        let loop_names: Vec<String> = agent
            .tools()
            .loop_tools()
            .into_iter()
            .map(|t| t.name().to_string())
            .collect();
        assert_eq!(loop_names, vec!["StoreBuiltin"]);
        assert_eq!(
            agent.tools().store_data().get("todos"),
            Some(&json!("1 pending"))
        );
    }
}
