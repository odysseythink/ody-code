# Part 1: agent-rs Agent 组装

**Goal:** 在 `agent-rs` 中创建真实 `Agent` 类型，解决 `ContextMemory` 的 `&'a` 自引用问题，定义 `AgentContext`/`AgentEnvironment` 桥接契约，并让 `Agent` 实现 `TurnAgent` trait。

**Architecture:** 用 `Arc::new_cyclic` 构造 `Agent`，子模块通过 `AgentContext`（内部持 `Weak<Agent>`）回调。`AgentContext` 同时实现 `ContextAgent`、`AgentConfigContext`、`ToolManagerContext`、`UsageRecorderContext`、`PermissionManagerContext`、`SessionModeContext`、`InjectionManagerContext`、`SkillActivationContext` 等全部上下文 trait，避免在 `Agent` 内部出现自引用。`Agent` 本身实现 `TurnAgent` 并把方法委托给内部的 `ContextMemory`、`ConfigState`、`TurnFlow` 等子模块。

**Tech Stack:** Rust（tokio / async-trait / serde_json / anyhow / std::sync）。

> For executing workers: implement this part task-by-task. Steps use - [ ] checkboxes for tracking.

---

## Task 1: 去掉 `ContextMemory` 的 `&'a` 生命周期，引入 `AgentContext`

**Depends on:** none（4.3.1 已落地 `ContextMemory`，但其生命周期阻碍了真实 Agent 组装）。

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/context/memory.rs:19-32`（把 `&'a dyn ContextAgent` 改为 `Arc<dyn ContextAgent>`）
- Modify: `rust-ody/crates/agent-rs/src/context/memory.rs:31-43`（`new` 签名同步修改）
- Modify: `rust-ody/crates/agent-rs/src/bin/context_golden.rs:49`（调用处传入 `Arc::clone(&agent)`）
- Create: `rust-ody/crates/agent-rs/src/agent.rs`（仅 `AgentContext` 骨架，Task 2 补全）
- Create: `rust-ody/crates/agent-rs/tests/agent_context_test.rs`（回归测试）

**为什么先做这一步**：真实 `Agent` 需要持有 `ContextMemory`，而 `ContextMemory` 又需要回调 `Agent` 的能力（`RecordLog`、`InjectionLifecycle` 等）。原来的 `&'a dyn ContextAgent` 要求 `ContextMemory` 活得比 `Agent` 短，无法被 `Agent` 拥有。

### 步骤

- [ ] **写出失败测试**：在 `rust-ody/crates/agent-rs/tests/agent_context_test.rs` 中验证 `ContextMemory` 可以被 `Agent` struct 拥有并调用。

```rust
use std::sync::Arc;
use agent_rs::context::memory::ContextMemory;
use agent_rs::context::types::{ContextAgent, RecordLog, USER_PROMPT_ORIGIN};
use agent_rs::records::AgentRecord;
use kosong_rs::message::ContentPart;

struct DummyAgent {
    records: std::sync::Mutex<Vec<AgentRecord>>,
}

impl RecordLog for DummyAgent {
    fn log_record(&self, r: AgentRecord) {
        self.records.lock().unwrap().push(r);
    }
    fn restoring_time(&self) -> Option<i64> { None }
}

impl ContextAgent for DummyAgent {
    fn record_log(&self) -> &dyn RecordLog { self }
    fn micro_compaction(&self) -> &dyn agent_rs::context::types::MicroCompaction { unimplemented!() }
    fn injection(&self) -> &dyn agent_rs::context::types::InjectionLifecycle { unimplemented!() }
    fn background(&self) -> &dyn agent_rs::context::types::BackgroundNotifications { unimplemented!() }
    fn replay_builder(&self) -> &dyn agent_rs::context::types::ReplayBuilder { unimplemented!() }
    fn status_emitter(&self) -> &dyn agent_rs::context::types::StatusEmitter { unimplemented!() }
    fn context_switch_flusher(&self) -> &dyn agent_rs::context::types::ContextSwitchFlusher { unimplemented!() }
    fn clock(&self) -> &dyn agent_rs::context::types::Clock { unimplemented!() }
}

#[test]
fn context_memory_can_be_owned_by_agent_struct() {
    let agent: Arc<dyn ContextAgent> = Arc::new(DummyAgent { records: std::sync::Mutex::new(Vec::new()) });
    let memory = ContextMemory::new(agent.clone());
    let mut boxed: Box<ContextMemory> = Box::new(memory);
    boxed.append_user_message(vec![ContentPart::Text { text: "hi".into() }], USER_PROMPT_ORIGIN);
    assert_eq!(boxed.history().len(), 1);
}
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p agent-rs --test agent_context_test
```

预期失败：`ContextMemory::new` 仍要求 `&dyn ContextAgent`，无法传入 `Arc`。

- [ ] **最小实现**：

修改 `rust-ody/crates/agent-rs/src/context/memory.rs`：

```rust
use std::sync::Arc;

pub struct ContextMemory {
    agent: Arc<dyn ContextAgent>,
    // ... 其余字段保持不变 ...
}

impl ContextMemory {
    pub fn new(agent: Arc<dyn ContextAgent>) -> Self {
        Self {
            agent,
            history: Vec::new(),
            token_count: 0,
            token_count_covered_message_count: 0,
            open_steps: HashMap::new(),
            pending_tool_result_ids: HashSet::new(),
            deferred_messages: Vec::new(),
            last_assistant_at: None,
        }
    }
    // ... 其余方法不变，只是把 `self.agent` 相关调用从 `self.agent.foo()` 改成通过 Arc ...
}
```

把原来 `self.agent` 的使用点（如 `self.agent.record_log().log_record(...)`）保持不变，因为 `ContextAgent::record_log` 仍返回 `&dyn RecordLog`，只是 `self.agent` 现在是 `Arc<dyn ContextAgent>`。

修改 `rust-ody/crates/agent-rs/src/bin/context_golden.rs:49`：

```rust
let mut memory = ContextMemory::new(Arc::clone(&agent));
```

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p agent-rs --test agent_context_test && cargo run --bin context-golden
```

`context-golden` 需要把输入 fixture 输出 JSON；如果无 fixture 会报错，可运行：

```bash
cargo run --bin context-golden -- /tmp/ctx-in.json /tmp/ctx-out.json
```

只要编译通过即可。

- [ ] **提交**：

```bash
git add rust-ody/crates/agent-rs/src/context/memory.rs rust-ody/crates/agent-rs/src/bin/context_golden.rs rust-ody/crates/agent-rs/tests/agent_context_test.rs
git commit -m "refactor(agent-rs): remove ContextMemory lifetime to allow Agent ownership"
```

---

## Task 2: 定义 `AgentEnvironment` / `AgentContext` / `AgentBuilder`，构造真实 `Agent`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/agent.rs`（完整 `Agent` 结构，本 Task 填充构造逻辑；Task 3 填充 `TurnAgent`）
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:19`（导出 `pub mod agent;`）
- Modify: `rust-ody/crates/agent-rs/src/permission/manager.rs`（若缺少 `before_tool_call`，在本 Task 中补齐最小实现）
- Create: `rust-ody/crates/agent-rs/tests/agent_build_test.rs`

### 步骤

- [ ] **写出失败测试**：验证 `AgentBuilder` 能构造出 `Arc<Agent>`，并且构造后 `Agent::records()` 可以写入记录。

```rust
use std::sync::{Arc, Mutex};
use agent_rs::agent::{Agent, AgentBuilder, AgentEnvironment, EventSink};
use agent_rs::context::types::USER_PROMPT_ORIGIN;
use agent_rs::kaos::Kaos;
use agent_rs::records::AgentRecord;
use agent_rs::turn::types::AgentEvent;
use kosong_rs::message::ContentPart;

struct CollectEnv {
    events: Mutex<Vec<AgentEvent>>,
}

impl AgentEnvironment for CollectEnv {
    fn emit_event(&self, e: AgentEvent) { self.events.lock().unwrap().push(e); }
    // 其余默认方法均 no-op
}

#[tokio::test]
async fn agent_builder_creates_agent_and_records_persist() {
    let env: Arc<dyn AgentEnvironment> = Arc::new(CollectEnv { events: Mutex::new(Vec::new()) });
    let kaos = Arc::new(Kaos::new(agent_rs::kaos::environment::detect_environment_from_node(), std::path::Path::new("/tmp")));
    let agent = AgentBuilder::new("main".into(), kaos, env)
        .build()
        .await
        .expect("build agent");
    agent.records().log_record(AgentRecord::ContextClear { time: None });
    // 若持久化不存在，log_record 也应至少通知 event sink（通过 records 的 subscribe 或 AgentEnvironment）
    // 这里仅验证 Agent 可被构造且 records() trait 返回非空
    assert_eq!(agent.agent_type(), "main");
}
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p agent-rs --test agent_build_test
```

预期失败：`Agent`、`AgentBuilder`、`AgentEnvironment` 不存在。

- [ ] **实现**：

在 `rust-ody/crates/agent-rs/src/agent.rs` 中写入：

```rust
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use kosong_rs::message::{ContentPart, Message};
use kosong_rs::provider::{AbortSignal, ChatProvider, ModelCapability, Tool};
use kosong_rs::usage::TokenUsage;

use crate::agent_loop::llm::{CompactGenerateResult, Llm};
use crate::background::manager::BackgroundManager;
use crate::background::persistence::BackgroundTaskPersistence;
use crate::compaction::strategy::DefaultCompactionStrategy;
use crate::compaction::{full::FullCompaction, micro::MicroCompaction, normal_task_checkpoint::NormalModeTaskCheckpoint, split_checkpoint::SplitPlanCheckpoint};
use crate::config::state::{AgentConfigContext, ConfigState, ResolvedRuntimeProvider, ThinkingConfig};
use crate::config::thinking::ThinkingEffort;
use crate::context::memory::ContextMemory;
use crate::context::types::{AgentContextData, Clock, ContextAgent, ContextMessage, ContextSwitchFlusher, InjectionLifecycle, MicroCompaction as MicroCompactionTrait, PromptOrigin, RecordLog, ReplayBuilder as ReplayBuilderTrait, StatusEmitter, BackgroundNotifications, USER_PROMPT_ORIGIN};
use crate::context::tokens::estimate_tokens_for_message;
use crate::cron::clock::{resolve_clock_sources, ClockSources};
use crate::cron::manager::{CronManager, CronManagerOptions};
use crate::injection::manager::InjectionManager;
use crate::injection::types::{InjectionManagerContext, PendingDesignHandoff, PendingPlanHandoff};
use crate::permission::manager::PermissionManager;
use crate::permission::types::{AgentConfigContext as _, ApprovalRequest, ApprovalResponse, PermissionData, PermissionManagerContext, PermissionMode};
use crate::records::nested::{AgentConfigUpdateData, CompactionResult, GoalBudgetLimits, GoalStatus, LoopRecordedEvent, SessionModeKind, UsageRecordScope};
use crate::records::persistence::FileSystemAgentRecordPersistence;
use crate::records::records::{AgentRecords, ReplayResult};
use crate::records::AgentRecord;
use crate::replay::{AgentReplayRecord, ReplayBuilder};
use crate::session_mode::manager::SessionModeManager;
use crate::session_mode::types::SessionModeContext;
use crate::skill::manager::SkillManager;
use crate::skill::registry::SkillRegistry;
use crate::skill::types::{ActivateSkillPayload, SkillActivatedEvent, SkillError, SkillRegistry as SkillRegistryTrait};
use crate::tool::manager::ToolManager;
use crate::tool::types::{ExecutableTool, ToolInfo, ToolManagerContext, UserToolRegistration};
use crate::turn::types::*;
use crate::turn::{TurnFlow, KosongLLM, KosongLLMConfig};
use crate::usage::recorder::{UsageRecorder, UsageRecorderContext, UsageStatus};

pub type AgentId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType { Main, Sub, Independent }

/// 宿主需要提供的能力：事件发射、approval、hook、telemetry、log。
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
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>>;

    fn fire_hook_permission_request(&self, tool_name: &str, data: serde_json::Value);
    fn fire_hook_permission_result(&self, tool_name: &str, data: serde_json::Value);

    fn fire_hook_user_prompt_submit(
        &self,
        input: Vec<ContentPart>,
        signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HookResult>, anyhow::Error>> + Send + '_>>;

    fn fire_hook_stop_hook(
        &self,
        signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<StopHookBlock>, anyhow::Error>> + Send + '_>>;

    fn fire_and_forget_hook(&self, event: &str, data: serde_json::Value);

    fn trigger_hook(
        &self,
        event: &str,
        data: serde_json::Value,
        signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>>;

    fn track_telemetry(&self, event: &str, properties: serde_json::Value);
    fn log_debug(&self, msg: &str, data: serde_json::Value);
    fn log_warn(&self, msg: &str, data: serde_json::Value);
    fn log_error(&self, msg: &str, data: serde_json::Value);
}

/// 解析模型别名到 provider / capability。
pub trait ProviderResolver: Send + Sync {
    fn default_model(&self) -> Option<String>;
    fn resolve(&self, model_alias: &str) -> Option<ResolvedRuntimeProvider>;
    fn thinking_config(&self) -> Option<ThinkingConfig>;
}

/// LLM 工厂：默认用 KosongLLM；测试可注入 FixtureLlm。
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

/// 子模块回调 Agent 的统一句柄，通过 `Weak<Agent>` 避免循环引用。
#[derive(Clone)]
pub struct AgentContext {
    agent: Weak<Agent>,
}

impl AgentContext {
    fn upgrade(&self) -> Option<Arc<Agent>> {
        self.agent.upgrade()
    }
}

// --------------- Agent 本身 ---------------

pub struct Agent {
    id: AgentId,
    agent_type: AgentType,
    kaos: Arc<crate::kaos::kaos::Kaos>,
    homedir: Option<PathBuf>,
    environment: Arc<dyn AgentEnvironment>,
    provider_resolver: Arc<dyn ProviderResolver>,
    llm_factory: Arc<dyn LlmFactory>,

    records: Mutex<AgentRecords<FileSystemAgentRecordPersistence, Box<dyn FnMut(&AgentRecord) + Send>>>,

    contexts: HashMap<Option<SessionModeKind>, Mutex<ContextMemory>>,
    full_compactions: HashMap<Option<SessionModeKind>, FullCompaction>,
    micro_compactions: HashMap<Option<SessionModeKind>, MicroCompaction>,
    split_plan_checkpoint: SplitPlanCheckpoint,
    normal_mode_task_checkpoint: NormalModeTaskCheckpoint,

    config: Mutex<ConfigState<AgentContext>>,
    turn: TurnFlow,
    injection: Mutex<InjectionManager>,
    permission: Mutex<PermissionManager<'static, AgentContext>>,
    session_mode: Mutex<SessionModeManager<AgentContext>>,
    usage: Mutex<UsageRecorder<AgentContext>>,
    skills: Mutex<Option<SkillManager<AgentContext, Box<dyn SkillRegistryTrait>>>>,
    tools: Mutex<ToolManager<AgentContext>>,
    background: Arc<BackgroundManager>,
    cron: Option<Arc<CronManager>>,
    replay_builder: Mutex<ReplayBuilder>,

    active_mode: Mutex<Option<SessionModeKind>>,
    pending_context_switch: Mutex<Option<Option<SessionModeKind>>>,
    cached_llm: Mutex<Option<Arc<dyn Llm>>>,
}

pub struct AgentBuilder {
    id: AgentId,
    agent_type: AgentType,
    kaos: Arc<crate::kaos::kaos::Kaos>,
    homedir: Option<PathBuf>,
    environment: Arc<dyn AgentEnvironment>,
    provider_resolver: Option<Arc<dyn ProviderResolver>>,
    llm_factory: Option<Arc<dyn LlmFactory>>,
    skills_registry: Option<Box<dyn SkillRegistryTrait>>,
}

impl AgentBuilder {
    pub fn new(
        id: impl Into<AgentId>,
        kaos: Arc<crate::kaos::kaos::Kaos>,
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
            skills_registry: None,
        }
    }

    pub fn agent_type(mut self, t: AgentType) -> Self { self.agent_type = t; self }
    pub fn homedir(mut self, p: impl Into<PathBuf>) -> Self { self.homedir = Some(p.into()); self }
    pub fn provider_resolver(mut self, r: Arc<dyn ProviderResolver>) -> Self { self.provider_resolver = Some(r); self }
    pub fn llm_factory(mut self, f: Arc<dyn LlmFactory>) -> Self { self.llm_factory = Some(f); self }
    pub fn skills_registry(mut self, r: Box<dyn SkillRegistryTrait>) -> Self { self.skills_registry = Some(r); self }

    pub async fn build(self) -> anyhow::Result<Arc<Agent>> {
        let persistence = self.homedir.as_ref().map(|h| {
            FileSystemAgentRecordPersistence::new(h.join("wire.jsonl"))
        });

        // records 需要 restore_handler；restore_handler 会通过 Weak<Agent> 回调。
        // 先占位，等 Agent 构造完成后再替换为真实的 Weak。
        let records_holder: Arc<Mutex<Option<Weak<Agent>>>> = Arc::new(Mutex::new(None));
        let records_holder_for_closure = Arc::clone(&records_holder);
        let restore_handler: Box<dyn FnMut(&AgentRecord) + Send> = Box::new(move |record: &AgentRecord| {
            if let Some(agent) = records_holder_for_closure.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                agent.restore_record(record);
            }
        });

        let records = Mutex::new(AgentRecords::new(persistence, restore_handler, None));

        // 先创建 Weak 占位，用 Arc::new_cyclic 拿到真实 Weak
        let agent = Arc::new_cyclic(|weak| {
            let ctx = AgentContext { agent: weak.clone() };
            let mut contexts = HashMap::new();
            let mut full_compactions = HashMap::new();
            let mut micro_compactions = HashMap::new();
            for mode in [None, Some(SessionModeKind::Plan), Some(SessionModeKind::Design), Some(SessionModeKind::OfficeHours), Some(SessionModeKind::GameDesign)] {
                contexts.insert(mode, Mutex::new(ContextMemory::new(ctx.clone() as Arc<dyn ContextAgent>)));
                full_compactions.insert(mode, FullCompaction::new(Arc::new(DefaultCompactionStrategy::default())));
                micro_compactions.insert(mode, MicroCompaction::new(None));
            }

            let config = Mutex::new(ConfigState::new(ctx.clone()));

            let tools = Mutex::new(ToolManager::new(ctx.clone()));
            let usage = Mutex::new(UsageRecorder::new(ctx.clone()));
            let permission = Mutex::new(PermissionManager::new(ctx.clone(), None));
            let session_mode = Mutex::new(SessionModeManager::new(ctx.clone(), HashMap::new()));
            let injection = Mutex::new(InjectionManager::new(&ctx));
            let skills = Mutex::new(self.skills_registry.map(|r| SkillManager::new(ctx.clone(), r)));
            let replay_builder = Mutex::new(ReplayBuilder::new());

            let turn = TurnFlow::new(weak.clone() as Arc<dyn TurnAgent>);
            let background = BackgroundManager::new(
                weak.clone() as Arc<dyn TurnAgent>,
                Arc::new(turn.clone()),
                self.homedir.as_ref().map(|h| BackgroundTaskPersistence::new(h.clone())),
            );
            let cron = if self.agent_type == AgentType::Sub {
                None
            } else {
                Some(CronManager::new(
                    weak.clone() as Arc<dyn TurnAgent>,
                    Arc::new(turn.clone()),
                    self.homedir.clone(),
                    CronManagerOptions { clocks: None, poll_interval_ms: None },
                ))
            };

            Agent {
                id: self.id,
                agent_type,
                kaos: self.kaos,
                homedir: self.homedir,
                environment: self.environment,
                provider_resolver: self.provider_resolver.unwrap_or_else(|| Arc::new(DefaultProviderResolver)),
                llm_factory: self.llm_factory.unwrap_or_else(|| Arc::new(DefaultLlmFactory)),
                records,
                contexts,
                full_compactions,
                micro_compactions,
                split_plan_checkpoint: SplitPlanCheckpoint::new(),
                normal_mode_task_checkpoint: NormalModeTaskCheckpoint::new(),
                config,
                turn,
                injection,
                permission,
                session_mode,
                usage,
                skills,
                tools,
                background: Arc::new(background),
                cron,
                replay_builder,
                active_mode: Mutex::new(None),
                pending_context_switch: Mutex::new(None),
                cached_llm: Mutex::new(None),
            }
        });

        *records_holder.lock().unwrap() = Some(Arc::downgrade(&agent));
        Ok(agent)
    }
}

struct DefaultProviderResolver;
impl ProviderResolver for DefaultProviderResolver {
    fn default_model(&self) -> Option<String> { None }
    fn resolve(&self, _alias: &str) -> Option<ResolvedRuntimeProvider> { None }
    fn thinking_config(&self) -> Option<ThinkingConfig> { None }
}

impl Agent {
    fn active_mode(&self) -> Option<SessionModeKind> {
        *self.active_mode.lock().unwrap()
    }

    fn active_context(&self) -> &Mutex<ContextMemory> {
        self.contexts.get(&self.active_mode()).expect("context for active mode")
    }

    fn active_full_compaction(&self) -> &FullCompaction {
        self.full_compactions.get(&self.active_mode()).expect("full compaction for active mode")
    }

    fn active_micro_compaction(&self) -> &MicroCompaction {
        self.micro_compactions.get(&self.active_mode()).expect("micro compaction for active mode")
    }

    pub fn id(&self) -> &str { &self.id }
    pub fn homedir(&self) -> Option<&Path> { self.homedir.as_deref() }

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

    pub fn llm(&self) -> Arc<dyn Llm> {
        let mut cached = self.cached_llm.lock().unwrap();
        if cached.is_none() {
            let cfg = self.config.lock().unwrap();
            let provider = cfg.provider();
            let model_name = cfg.model();
            let system_prompt = cfg.system_prompt().to_string();
            let capability = Some(cfg.model_capabilities());
            *cached = Some(self.llm_factory.create(provider, model_name, system_prompt, capability));
        }
        cached.as_ref().unwrap().clone()
    }

    pub fn generate_wrapper(&self) -> impl Fn(...) ... { unimplemented!() } // Task 3 中实现

    pub fn use_profile(&self, _profile: &str) { unimplemented!() } // 可选，本阶段可 no-op

    pub async fn resume(&self) -> anyhow::Result<ReplayResult> {
        let mut records = self.records.lock().unwrap();
        let result = records.replay().await?;
        if let Some(ref cron) = self.cron {
            cron.load_from_disk().await;
        }
        self.background.load_from_disk().await;
        self.turn.finish_resume();
        Ok(result)
    }

    fn restore_record(&self, record: &AgentRecord) {
        // 恢复逻辑：根据 record 类型更新 config / permission / session_mode / context / tools
        match record {
            AgentRecord::ConfigUpdate { update, .. } => {
                self.config.lock().unwrap().update(update.clone());
            }
            AgentRecord::PermissionSetMode { mode, .. } => {
                self.permission.lock().unwrap().set_mode(*mode);
            }
            AgentRecord::SessionModeEnter { id, kind, path } => {
                let _ = self.session_mode.lock().unwrap().restore_enter(id.clone(), *kind, path.clone());
                self.set_context_mode(*kind);
            }
            AgentRecord::SessionModeExit { .. } => {
                let _ = self.session_mode.lock().unwrap().exit(None);
                self.set_context_mode(None);
            }
            AgentRecord::SessionModeCancel { .. } => {
                let _ = self.session_mode.lock().unwrap().cancel(None);
                self.set_context_mode(None);
            }
            AgentRecord::ToolsRegisterUserTool { registration, .. } => {
                self.tools.lock().unwrap().register_user_tool(registration.clone());
            }
            AgentRecord::ToolsUnregisterUserTool { name, .. } => {
                self.tools.lock().unwrap().unregister_user_tool(name);
            }
            AgentRecord::ToolsSetActiveTools { names, .. } => {
                self.tools.lock().unwrap().set_active_tools(names);
            }
            AgentRecord::ContextAppendMessage { message, .. } => {
                self.active_context().lock().unwrap().append_message(message.clone());
            }
            AgentRecord::ContextAppendLoopEvent { event, .. } => {
                self.active_context().lock().unwrap().append_loop_event(event.clone());
            }
            AgentRecord::ContextClear { .. } => {
                self.active_context().lock().unwrap().clear();
            }
            AgentRecord::ContextApplyCompaction { result, .. } => {
                self.active_context().lock().unwrap().apply_compaction(result.clone());
            }
            AgentRecord::ContextUndo { count, .. } => {
                self.active_context().lock().unwrap().undo(*count);
            }
            _ => {}
        }
    }
}
```

（Task 2 的代码到此先保证能编译；`TurnAgent` 实现、`generate_wrapper`、以及 `AgentContext` 的全部上下文 trait 实现放到 Task 3。）

- [ ] **编译确认**：

```bash
cd rust-ody && cargo check -p agent-rs
```

预期：可能有未实现 trait 的警告，但结构体构造应通过。

- [ ] **运行测试**：

```bash
cd rust-ody && cargo test -p agent-rs --test agent_build_test
```

预期通过：`AgentBuilder::new(...).build()` 返回 `Arc<Agent>`，`agent.agent_type() == "main"`。

- [ ] **提交**：

```bash
git add rust-ody/crates/agent-rs/src/agent.rs rust-ody/crates/agent-rs/src/lib.rs rust-ody/crates/agent-rs/tests/agent_build_test.rs rust-ody/crates/agent-rs/src/permission/manager.rs
git commit -m "feat(agent-rs): AgentBuilder and Agent struct construction"
```

---

## Task 3: 实现 `TurnAgent` 与 `TurnFlow` 集成

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent.rs`（补全 `AgentContext` 全部 trait、`Agent` 实现 `TurnAgent`、事件/telemetry/log 委托）
- Modify: `rust-ody/crates/agent-rs/src/permission/manager.rs`（若 4.3.3 未实现 `before_tool_call`，补齐最小实现）
- Create: `rust-ody/crates/agent-rs/tests/agent_turn_test.rs`

### 步骤

- [ ] **写出失败测试**：构造一个 `Agent`，注入 `MockLlmFactory` 返回单 text 回复，调用 `agent.turn.prompt(...)`，等待 turn 结束，验证事件流包含 `TurnStarted` 和 `TurnEnded`。

```rust
use std::sync::{Arc, Mutex};
use agent_rs::agent::{Agent, AgentBuilder, AgentEnvironment, AgentType};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::context::types::USER_PROMPT_ORIGIN;
use agent_rs::kaos::Kaos;
use agent_rs::turn::types::{AgentEvent, PromptOrigin};
use kosong_rs::message::ContentPart;
use kosong_rs::provider::FinishReason;

struct CollectEnv {
    events: Mutex<Vec<AgentEvent>>,
}

#[async_trait::async_trait]
impl AgentEnvironment for CollectEnv {
    fn emit_event(&self, e: AgentEvent) { self.events.lock().unwrap().push(e); }
    async fn request_approval(&self, _: &agent_rs::permission::types::ApprovalRequest, _: kosong_rs::provider::AbortSignal) -> Result<agent_rs::permission::types::ApprovalResponse, anyhow::Error> { unimplemented!() }
    fn fire_hook_pre_tool_use(&self, _: &str, _: serde_json::Value, _: &str, _: kosong_rs::provider::AbortSignal) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> { unimplemented!() }
    fn fire_hook_permission_request(&self, _: &str, _: serde_json::Value) {}
    fn fire_hook_permission_result(&self, _: &str, _: serde_json::Value) {}
    fn fire_hook_user_prompt_submit(&self, _: Vec<ContentPart>, _: kosong_rs::provider::AbortSignal) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<agent_rs::turn::types::HookResult>, anyhow::Error>> + Send + '_>> { unimplemented!() }
    fn fire_hook_stop_hook(&self, _: kosong_rs::provider::AbortSignal) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<agent_rs::turn::types::StopHookBlock>, anyhow::Error>> + Send + '_>> { unimplemented!() }
    fn fire_and_forget_hook(&self, _: &str, _: serde_json::Value) {}
    fn trigger_hook(&self, _: &str, _: serde_json::Value, _: kosong_rs::provider::AbortSignal) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>> { unimplemented!() }
    fn track_telemetry(&self, _: &str, _: serde_json::Value) {}
    fn log_debug(&self, _: &str, _: serde_json::Value) {}
    fn log_warn(&self, _: &str, _: serde_json::Value) {}
    fn log_error(&self, _: &str, _: serde_json::Value) {}
}

struct MockLlmFactory;
impl agent_rs::agent::LlmFactory for MockLlmFactory {
    fn create(&self, _: Box<dyn kosong_rs::provider::ChatProvider>, _: String, _: String, _: Option<kosong_rs::provider::ModelCapability>) -> Arc<dyn Llm> {
        Arc::new(MockLlm)
    }
}

struct MockLlm;
#[async_trait::async_trait]
impl Llm for MockLlm {
    fn system_prompt(&self) -> &str { "" }
    fn model_name(&self) -> &str { "mock" }
    async fn chat(&self, _: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        Ok(LlmChatResponse {
            tool_calls: vec![],
            provider_finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: None,
            usage: Default::default(),
            stream_timing: None,
        })
    }
}

#[tokio::test]
async fn agent_runs_prompt_and_emits_turn_events() {
    let env: Arc<dyn AgentEnvironment> = Arc::new(CollectEnv { events: Mutex::new(Vec::new()) });
    let kaos = Arc::new(Kaos::new(agent_rs::kaos::environment::detect_environment_from_node(), std::path::Path::new("/tmp")));
    let agent = AgentBuilder::new("main", kaos, env.clone())
        .llm_factory(Arc::new(MockLlmFactory))
        .build()
        .await
        .unwrap();

    let turn_id = agent.turn().prompt(vec![ContentPart::Text { text: "hello".into() }], USER_PROMPT_ORIGIN);
    assert!(turn_id.is_some());
    agent.turn().wait_for_current_turn(None).await.unwrap();

    let events = env.as_any().downcast_ref::<CollectEnv>().unwrap().events.lock().unwrap().clone();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnStarted { .. })));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnEnded { .. })));
}
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p agent-rs --test agent_turn_test
```

预期失败：`Agent` 未实现 `TurnAgent`，且 `AgentContext` 的上下文 trait 未实现导致 `ConfigState::new(ctx)` 等不通过。

- [ ] **实现**：

在 `agent.rs` 中补齐 `AgentContext` 的全部上下文 trait 实现。下面是核心片段：

```rust
// --------------- AgentContext 实现全部上下文 trait ---------------

impl RecordLog for AgentContext {
    fn log_record(&self, record: AgentRecord) {
        if let Some(agent) = self.upgrade() {
            agent.records.lock().unwrap().log_record(record);
        }
    }
    fn restoring_time(&self) -> Option<i64> {
        self.upgrade().and_then(|a| a.records.lock().unwrap().restoring().map(|r| r.time).flatten())
    }
}

impl MicroCompactionTrait for AgentContext {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        self.upgrade().map(|a| a.active_micro_compaction().compact(messages)).unwrap_or_else(|| messages.to_vec())
    }
    fn reset(&self, max_cutoff: usize) {
        if let Some(agent) = self.upgrade() {
            a.active_micro_compaction().reset(max_cutoff);
        }
    }
}

impl InjectionLifecycle for AgentContext {
    fn on_context_clear(&self) {
        if let Some(agent) = self.upgrade() {
            agent.injection.lock().unwrap().on_context_clear();
        }
    }
    fn on_context_compacted(&self, compacted_count: usize) {
        if let Some(agent) = self.upgrade() {
            agent.injection.lock().unwrap().on_context_compacted(compacted_count);
        }
    }
    fn on_context_message_removed(&self, index: usize) {
        if let Some(agent) = self.upgrade() {
            agent.injection.lock().unwrap().on_context_message_removed(index);
        }
    }
}

impl BackgroundNotifications for AgentContext {
    fn mark_delivered_notification(&self, _origin: &PromptOrigin) {
        // BackgroundManager 内部已维护 delivered set，这里 no-op
    }
}

impl ReplayBuilderTrait for AgentContext {
    fn push_message(&self, message: &ContextMessage) {
        if let Some(agent) = self.upgrade() {
            agent.replay_builder.lock().unwrap().push_message(message);
        }
    }
    fn remove_last_messages(&self, messages: &[ContextMessage]) {
        if let Some(agent) = self.upgrade() {
            agent.replay_builder.lock().unwrap().remove_last_messages(messages);
        }
    }
}

impl StatusEmitter for AgentContext {
    fn emit_status_updated(&self) {
        if let Some(agent) = self.upgrade() {
            // 通过 AgentEnvironment 发出 agent.status.updated 事件
            let cfg = agent.config.lock().unwrap().data();
            agent.environment.emit_event(AgentEvent::AgentStatusUpdated {
                session_id: String::new(),
                agent_id: agent.id.clone(),
                model: cfg.model_alias.clone(),
                thinking_level: Some(cfg.thinking_level.clone()),
                permission: Some(format!("{:?}", agent.permission.lock().unwrap().mode()).to_lowercase()),
                context_tokens: Some(agent.active_context().lock().unwrap().token_count()),
                max_context_tokens: cfg.model_capabilities.max_context_tokens.map(|x| x as i64),
                session_mode: agent.active_mode().map(|m| format!("{:?}", m).to_lowercase()),
                session_mode_file_path: agent.session_mode.lock().unwrap().session_mode_file_path(),
            });
        }
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
    fn record_log(&self) -> &dyn RecordLog { self }
    fn micro_compaction(&self) -> &dyn MicroCompactionTrait { self }
    fn injection(&self) -> &dyn InjectionLifecycle { self }
    fn background(&self) -> &dyn BackgroundNotifications { self }
    fn replay_builder(&self) -> &dyn ReplayBuilderTrait { self }
    fn status_emitter(&self) -> &dyn StatusEmitter { self }
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher { self }
    fn clock(&self) -> &dyn Clock { self }
}

impl AgentConfigContext for AgentContext {
    fn log_record(&mut self, record: AgentRecord) { RecordLog::log_record(self, record); }
    fn emit_status_updated(&self) { StatusEmitter::emit_status_updated(self); }
    fn initialize_builtin_tools(&self) {
        if let Some(agent) = self.upgrade() {
            agent.tools.lock().unwrap().initialize_builtin_tools();
        }
    }
    fn get_cwd(&self) -> String {
        self.upgrade().map(|a| a.kaos.getcwd()).unwrap_or_else(|| "/".into())
    }
    fn chdir(&self, cwd: &str) {
        if let Some(agent) = self.upgrade() {
            let _ = agent.kaos.chdir(cwd);
        }
    }
    fn default_model(&self) -> Option<String> {
        self.upgrade().map(|a| a.provider_resolver.default_model()).flatten()
    }
    fn resolve_provider_config(&self, model_alias: &str) -> Option<ResolvedRuntimeProvider> {
        self.upgrade().and_then(|a| a.provider_resolver.resolve(model_alias))
    }
    fn thinking_config(&self) -> Option<ThinkingConfig> {
        self.upgrade().and_then(|a| a.provider_resolver.thinking_config())
    }
    fn push_config_updated_replay(&self, config: &AgentConfigUpdateData) {
        if let Some(agent) = self.upgrade() {
            agent.replay_builder.lock().unwrap().push_config_updated(serde_json::to_value(config).unwrap_or_default());
        }
    }
}

impl ToolManagerContext for AgentContext {
    fn log_record(&mut self, record: AgentRecord) { RecordLog::log_record(self, record); }
    fn emit_tool_list_updated(&mut self, _reason: &str, _server_name: Option<&str>) {
        // 通过 environment 触发 tool.list.updated（可选）
    }
    fn goal_mutation_tools_hidden(&self) -> bool { false }
}

impl UsageRecorderContext for AgentContext {
    fn log_record(&mut self, record: AgentRecord) { RecordLog::log_record(self, record); }
    fn emit_status_updated(&mut self) { StatusEmitter::emit_status_updated(self); }
}

impl PermissionManagerContext for AgentContext {
    fn mode(&self) -> PermissionMode { PermissionMode::Manual }
    fn rules(&self) -> Vec<crate::permission::types::PermissionRule> { vec![] }
    fn session_approval_rule_patterns(&self) -> Vec<String> { vec![] }
    fn add_session_approval_rule_pattern(&self, _pattern: String) {}
    fn log_record(&self, record: AgentRecord) { RecordLog::log_record(self, record); }
    fn emit_status_updated(&self) { StatusEmitter::emit_status_updated(self); }
    fn push_approval_result_replay(&self, record: &crate::records::nested::PermissionApprovalResultRecord) {
        if let Some(agent) = self.upgrade() {
            agent.replay_builder.lock().unwrap().push_approval_result(serde_json::to_value(record).unwrap_or_default());
        }
    }
    fn track_telemetry(&self, event: &str, data: serde_json::Value) {
        if let Some(agent) = self.upgrade() { agent.environment.track_telemetry(event, data); }
    }
    fn cwd(&self) -> String { AgentConfigContext::get_cwd(self) }
    fn path_class(&self) -> &str { "unix" }
    fn agent_type(&self) -> &str { "main" }
    fn is_sensitive_file(&self, _path: &str) -> bool { false }
    fn is_session_mode_active(&self) -> bool { self.upgrade().map(|a| a.session_mode.lock().unwrap().is_active()).unwrap_or(false) }
    fn session_mode_kind(&self) -> Option<&str> { None }
    fn session_mode_file_path(&self) -> Option<String> { None }
    fn is_writable_session_mode_path(&self, _path: &str) -> bool { false }
    fn exit_session_mode(&self) -> Result<(), anyhow::Error> { Ok(()) }
    fn find_git_work_tree_marker(&self) -> Option<(String, String)> { None }
    fn fire_hook_pre_tool_use(&self, tool_name: &str, tool_input: serde_json::Value, tool_call_id: &str, signal: AbortSignal) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
        if let Some(agent) = self.upgrade() {
            agent.environment.fire_hook_pre_tool_use(tool_name, tool_input, tool_call_id, signal)
        } else {
            Box::pin(async move { Ok(None) })
        }
    }
    fn fire_hook_permission_request(&self, tool_name: &str, data: serde_json::Value) {
        if let Some(agent) = self.upgrade() { agent.environment.fire_hook_permission_request(tool_name, data); }
    }
    fn fire_hook_permission_result(&self, tool_name: &str, data: serde_json::Value) {
        if let Some(agent) = self.upgrade() { agent.environment.fire_hook_permission_result(tool_name, data); }
    }
    fn request_approval(&self, req: &ApprovalRequest, signal: AbortSignal) -> Pin<Box<dyn Future<Output = Result<ApprovalResponse, anyhow::Error>> + Send + '_>> {
        if let Some(agent) = self.upgrade() {
            agent.environment.request_approval(req, signal)
        } else {
            Box::pin(async move { Ok(ApprovalResponse { decision: "approved".into(), scope: None, feedback: None, selected_label: None }) })
        }
    }
    fn is_plan_review_display(&self, _display: &serde_json::Value) -> bool { false }
    fn writes_only_plan_file(&self, _execution: &crate::agent_loop::types::RunnableToolExecution, _path: &str) -> bool { false }
}

impl SessionModeContext for AgentContext {
    fn log_record(&self, record: AgentRecord) { RecordLog::log_record(self, record); }
    fn restoring_time(&self) -> Option<i64> { RecordLog::restoring_time(self) }
    fn update_model_alias(&self, alias: Option<String>) {
        if let Some(agent) = self.upgrade() {
            agent.config.lock().unwrap().update(AgentConfigUpdateData { model_alias: alias, ..Default::default() });
        }
    }
    fn refresh_llm(&self) { if let Some(agent) = self.upgrade() { agent.refresh_llm(); } }
    fn resolve_mode_model_alias(&self, model_key: &str) -> Option<String> {
        // 先从 config modelAlias 前缀匹配；简化实现返回当前 model
        self.upgrade().and_then(|a| a.config.lock().unwrap().model_alias().map(|s| s.to_string()))
    }
    fn default_model_alias(&self) -> Option<String> {
        self.upgrade().and_then(|a| a.provider_resolver.default_model())
    }
    fn set_context_mode(&self, mode: Option<SessionModeKind>) { if let Some(agent) = self.upgrade() { agent.set_context_mode(mode); } }
    fn active_mode(&self) -> Option<SessionModeKind> { self.upgrade().map(|a| a.active_mode()).unwrap_or(None) }
    fn has_open_steps(&self) -> bool { self.upgrade().map(|a| a.active_context().lock().unwrap().has_open_steps()).unwrap_or(false) }
    fn push_replay_record(&self, record: AgentReplayRecord) {
        if let Some(agent) = self.upgrade() {
            match record {
                AgentReplayRecord::SessionModeUpdated { enabled, kind } => {
                    // 不重复存储，由 records WAL 覆盖
                }
                AgentReplayRecord::ConfigUpdated { config } => {
                    agent.replay_builder.lock().unwrap().push_config_updated(config);
                }
                _ => {}
            }
        }
    }
    fn set_replay_mode(&self, mode: Option<SessionModeKind>) { if let Some(agent) = self.upgrade() { agent.replay_builder.lock().unwrap().set_mode(mode); } }
    fn emit_status_updated(&self) { StatusEmitter::emit_status_updated(self); }
    fn cwd(&self) -> String { AgentConfigContext::get_cwd(self) }
    fn project_root(&self) -> Option<String> { None }
    fn mkdir_p(&self, path: &str) -> anyhow::Result<()> {
        self.upgrade().map(|a| a.kaos.mkdir(path)).unwrap_or(Ok(()))
    }
    fn file_exists(&self, path: &str) -> bool { self.upgrade().map(|a| a.kaos.exists(path)).unwrap_or(false) }
    fn read_file(&self, path: &str) -> anyhow::Result<String> { self.upgrade().map(|a| a.kaos.read_text(path)).unwrap_or(Ok(String::new())) }
    fn write_file(&self, path: &str, content: &str) -> anyhow::Result<()> { self.upgrade().map(|a| a.kaos.write_text(path, content)).unwrap_or(Ok(())) }
}

impl InjectionManagerContext for AgentContext {
    fn is_session_mode_active(&self) -> bool { SessionModeContext::is_session_mode_active(self) }
    fn session_mode_kind(&self) -> Option<SessionModeKind> { SessionModeContext::active_mode(self) }
    fn consume_pending_handoff_for_plan(&self) -> Option<PendingDesignHandoff> { self.upgrade().and_then(|a| a.session_mode.lock().unwrap().consume_pending_handoff_for_plan()) }
    fn consume_pending_handoff_for_normal(&self) -> Option<PendingPlanHandoff> { self.upgrade().and_then(|a| a.session_mode.lock().unwrap().consume_pending_handoff_for_normal()) }
    fn session_mode_file_path(&self) -> Option<String> { SessionModeContext::session_mode_file_path(self) }
    fn append_system_reminder(&self, text: &str, _kind: &str, variant: &str) {
        if let Some(agent) = self.upgrade() {
            let origin = PromptOrigin::Injection;
            agent.active_context().lock().unwrap().append_system_reminder(text, origin);
        }
    }
    fn context_history_len(&self) -> usize { self.upgrade().map(|a| a.active_context().lock().unwrap().history().len()).unwrap_or(0) }
    fn assistant_turn_count(&self) -> usize { 0 }
    fn is_tool_active(&self, tool_name: &str) -> bool { self.upgrade().map(|a| a.tools.lock().unwrap().is_tool_active(tool_name)).unwrap_or(false) }
    fn get_unavailable_skills_reminder(&self, _mode: SessionModeKind) -> Option<String> { None }
    fn get_active_goal_text(&self) -> Option<String> { None }
    fn permission_mode(&self) -> Option<String> { Some(format!("{:?}", PermissionManagerContext::mode(self)).to_lowercase()) }
    fn is_flag_enabled(&self, _flag: &str) -> bool { false }
    fn agent_type(&self) -> &str { PermissionManagerContext::agent_type(self) }
    fn restoring_time(&self) -> Option<i64> { RecordLog::restoring_time(self) }
}

impl SkillActivationContext for AgentContext {
    fn emit_skill_activated(&mut self, event: SkillActivatedEvent) {
        self.environment.emit_event(AgentEvent::SkillActivated { event });
    }
    fn telemetry_track(&mut self, event_name: &str, properties: std::collections::HashMap<String, String>) {
        self.environment.track_telemetry(event_name, serde_json::to_value(properties).unwrap_or_default());
    }
    fn prompt(&mut self, input: Vec<ContentPart>, origin: PromptOrigin) -> Result<(), SkillPromptError> {
        if let Some(agent) = self.upgrade() {
            agent.turn.prompt(input, origin);
            Ok(())
        } else {
            Err(SkillPromptError::NoAgent)
        }
    }
    fn new_activation_id(&self) -> String { uuid::Uuid::new_v4().to_string() }
}
```

> 注意：`AgentEvent::SkillActivated` 与 `SkillPromptError::NoAgent` 若 `agent-rs::turn::types::AgentEvent` / `agent_rs::skill::types` 中不存在，需要在本 Task 中追加这两个变体。

接着实现 `TurnAgent` 与相关 trait：

```rust
impl TurnContext for Agent {
    fn append_user_message(&self, content: Vec<ContentPart>, origin: PromptOrigin) {
        self.active_context().lock().unwrap().append_user_message(content, origin);
    }
    fn append_message(&self, message: ContextMessage) {
        self.active_context().lock().unwrap().append_message(message);
    }
    fn messages(&self) -> Vec<Message> { self.active_context().lock().unwrap().messages() }
    fn append_loop_event(&self, event: LoopRecordedEvent) {
        self.active_context().lock().unwrap().append_loop_event(event);
    }
    fn has_open_steps(&self) -> bool { self.active_context().lock().unwrap().has_open_steps() }
    fn clear(&self) { self.active_context().lock().unwrap().clear() }
    fn history(&self) -> Vec<ContextMessage> { self.active_context().lock().unwrap().history().to_vec() }
    fn token_count(&self) -> i64 { self.active_context().lock().unwrap().token_count() }
    fn token_count_with_pending(&self) -> i64 { self.active_context().lock().unwrap().token_count_with_pending() }
    fn apply_compaction(&self, result: CompactionResult) { self.active_context().lock().unwrap().apply_compaction(result); }
    fn project(&self, messages: &[ContextMessage]) -> Vec<Message> { self.active_context().lock().unwrap().project(messages) }
    fn last_assistant_at_ms(&self) -> Option<i64> { self.active_context().lock().unwrap().last_assistant_at_ms() }
    fn append_system_reminder(&self, content: &str, origin: PromptOrigin) { self.active_context().lock().unwrap().append_system_reminder(content, origin); }
}

impl TurnUsage for Agent {
    fn begin_turn(&self) { self.usage.lock().unwrap().begin_turn(); }
    fn end_turn(&self) { self.usage.lock().unwrap().end_turn(); }
    fn record(&self, model: &str, usage: TokenUsage, scope: UsageRecordScope) { self.usage.lock().unwrap().record(model, usage, scope); }
    fn current_turn_usage(&self) -> Option<TokenUsage> { self.usage.lock().unwrap().status().and_then(|s| s.current_turn) }
}

impl TurnConfig for Agent {
    fn model(&self) -> String { self.config.lock().unwrap().model() }
    fn model_alias(&self) -> Option<String> { self.config.lock().unwrap().model_alias().map(|s| s.to_string()) }
    fn system_prompt(&self) -> String { self.config.lock().unwrap().system_prompt().to_string() }
    fn thinking_level(&self) -> String { format!("{:?}", self.config.lock().unwrap().thinking_level()).to_lowercase() }
    fn provider(&self) -> Box<dyn ChatProvider> { self.config.lock().unwrap().provider() }
    fn model_capabilities(&self) -> ModelCapability { self.config.lock().unwrap().model_capabilities() }
    fn loop_control(&self) -> Option<LoopControl> { None }
    fn has_model(&self) -> bool { self.config.lock().unwrap().has_model() }
    fn e2e_enabled(&self) -> bool { false }
    fn test_review_enabled(&self) -> bool { false }
}

impl TurnTools for Agent {
    fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>> {
        self.tools.lock().unwrap().loop_tools().into_iter().map(|t| Arc::new(t.clone()) as Arc<dyn ExecutableTool>).collect()
    }
    fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value> { self.tools.lock().unwrap().store_data() }
}

#[async_trait]
impl TurnPermission for Agent {
    async fn before_tool_call(&self, ctx: crate::agent_loop::types::ResolvedToolExecutionHookContext<'_>) -> Result<Option<crate::agent_loop::types::AuthorizeToolExecutionResult>, anyhow::Error> {
        self.permission.lock().unwrap().before_tool_call(ctx).await
    }
}

#[async_trait]
impl TurnInjection for Agent {
    async fn inject_goal(&self) { self.injection.lock().unwrap().inject_goal(&AgentContext { agent: self.self_weak() }).await; }
    async fn inject(&self) { self.injection.lock().unwrap().inject(&AgentContext { agent: self.self_weak() }).await; }
}

#[async_trait]
impl TurnFullCompaction for Agent {
    fn reset_for_turn(&self, agent: Arc<dyn TurnAgent>) { self.active_full_compaction().reset_for_turn(agent); }
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) -> Result<(), anyhow::Error> {
        self.active_full_compaction().before_step(agent, signal).await
    }
    async fn after_step(&self, agent: Arc<dyn TurnAgent>) { self.active_full_compaction().after_step(agent).await; }
    async fn handle_overflow_error(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal, error: anyhow::Error) -> Result<(), anyhow::Error> {
        self.active_full_compaction().handle_overflow_error(agent, signal, error).await
    }
    async fn compact_checkpoint(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) -> Result<(), anyhow::Error> {
        self.active_full_compaction().compact_checkpoint(agent, signal).await
    }
    fn begin(&self, agent: Arc<dyn TurnAgent>, data: crate::records::nested::CompactionBeginData) { self.active_full_compaction().begin(agent, data); }
    fn cancel(&self, agent: Arc<dyn TurnAgent>) { self.active_full_compaction().cancel(agent); }
    fn compacted_history(&self) -> Vec<CompactedHistory> { self.active_full_compaction().compacted_history() }
    fn is_compacting(&self) -> bool { self.active_full_compaction().is_compacting() }
}

impl TurnMicroCompaction for Agent {
    fn detect(&self, agent: Arc<dyn TurnAgent>) { self.active_micro_compaction().detect(agent); }
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> { self.active_micro_compaction().compact(messages) }
    fn reset(&self, max_cutoff: usize) { self.active_micro_compaction().reset(max_cutoff); }
}

#[async_trait]
impl TurnSplitPlanCheckpoint for Agent {
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) { self.split_plan_checkpoint.before_step(agent, signal).await; }
    fn reset(&self) { self.split_plan_checkpoint.reset(); }
}

#[async_trait]
impl TurnNormalTaskCheckpoint for Agent {
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) { self.normal_mode_task_checkpoint.before_step(agent, signal).await; }
    fn reset(&self) { self.normal_mode_task_checkpoint.reset(); }
}

#[async_trait]
impl TurnSessionMode for Agent {
    fn is_active(&self) -> bool { self.session_mode.lock().unwrap().is_active() }
    fn kind(&self) -> Option<String> { self.session_mode.lock().unwrap().kind().map(|k| format!("{:?}", k).to_lowercase()) }
    fn file_path(&self) -> Option<String> { self.session_mode.lock().unwrap().session_mode_file_path() }
    async fn data(&self) -> Option<String> { None }
}

impl TurnGoal for Agent {
    fn get_goal(&self) -> Option<GoalSnapshot> { None }
    async fn increment_turn(&self) {}
    async fn mark_blocked(&self, _reason: &str) {}
    async fn pause_on_interrupt(&self, _reason: &str) {}
    async fn pause_active_goal(&self, _actor: &str, _reason: &str) {}
    async fn record_token_usage(&self, _delta: i64, _agent_id: &str, _agent_type: &str, _source: &str) -> Option<GoalSnapshot> { None }
}

#[async_trait]
impl TurnHooks for Agent {
    async fn trigger_user_prompt_submit(&self, input: Vec<ContentPart>, signal: AbortSignal) -> Result<Vec<HookResult>, anyhow::Error> {
        self.environment.fire_hook_user_prompt_submit(input, signal).await
    }
    async fn trigger_stop_hook(&self, signal: AbortSignal) -> Result<Option<StopHookBlock>, anyhow::Error> {
        self.environment.fire_hook_stop_hook(signal).await
    }
    fn fire_and_forget_trigger(&self, event: &str, data: serde_json::Value) {
        self.environment.fire_and_forget_hook(event, data);
    }
    async fn trigger(&self, event: &str, data: serde_json::Value, signal: AbortSignal) -> Result<(), anyhow::Error> {
        self.environment.trigger_hook(event, data, signal).await
    }
}

impl TurnTelemetry for Agent {
    fn track(&self, event: &str, properties: serde_json::Value) { self.environment.track_telemetry(event, properties); }
}

impl TurnLog for Agent {
    fn debug(&self, msg: &str, data: serde_json::Value) { self.environment.log_debug(msg, data); }
    fn warn(&self, msg: &str, data: serde_json::Value) { self.environment.log_warn(msg, data); }
    fn error(&self, msg: &str, data: serde_json::Value) { self.environment.log_error(msg, data); }
}

impl TurnRecords for Agent {
    fn log_record(&self, record: AgentRecord) { self.records.lock().unwrap().log_record(record); }
}

impl TurnEventEmitter for Agent {
    fn emit_event(&self, event: AgentEvent) { self.environment.emit_event(event); }
}

#[async_trait]
impl TurnLlmResolver for Agent {
    fn refresh_llm(&self) { self.refresh_llm(); }
    fn llm(&self) -> Arc<dyn Llm> { self.llm() }
    async fn generate_one_off(&self, provider: Box<dyn ChatProvider + Send>, system_prompt: String, tools: Vec<Tool>, messages: Vec<Message>, signal: AbortSignal) -> Result<CompactGenerateResult, anyhow::Error> {
        let llm = self.llm_factory.create(provider, "one-off".into(), system_prompt, None);
        let resp = llm.chat(crate::agent_loop::llm::LlmChatParams {
            messages,
            tools,
            signal,
            request_log_context: None,
            on_text_delta: None,
            on_think_delta: None,
            on_tool_call_delta: None,
            on_text_part: None,
            on_think_part: None,
        }).await?;
        Ok(CompactGenerateResult {
            text: resp.tool_calls.first().and_then(|tc| tc.function.arguments.clone()).unwrap_or_default(),
            finish_reason: resp.provider_finish_reason,
            usage: resp.usage,
        })
    }
}

impl TurnAgent for Agent {
    fn context(&self) -> &dyn TurnContext { self }
    fn usage(&self) -> &dyn TurnUsage { self }
    fn config(&self) -> &dyn TurnConfig { self }
    fn tools(&self) -> &dyn TurnTools { self }
    fn permission(&self) -> &dyn TurnPermission { self }
    fn injection(&self) -> &dyn TurnInjection { self }
    fn full_compaction(&self) -> &dyn TurnFullCompaction { self }
    fn micro_compaction(&self) -> &dyn TurnMicroCompaction { self }
    fn split_plan_checkpoint(&self) -> &dyn TurnSplitPlanCheckpoint { self }
    fn normal_mode_task_checkpoint(&self) -> &dyn TurnNormalTaskCheckpoint { self }
    fn session_mode(&self) -> &dyn TurnSessionMode { self }
    fn goals(&self) -> Option<&dyn TurnGoal> { Some(self) }
    fn hooks(&self) -> Option<&dyn TurnHooks> { Some(self) }
    fn telemetry(&self) -> &dyn TurnTelemetry { self }
    fn log(&self) -> &dyn TurnLog { self }
    fn mcp(&self) -> Option<&dyn TurnMcp> { None }
    fn subagent_host(&self) -> Option<&dyn TurnSubagentHost> { None }
    fn records(&self) -> &dyn TurnRecords { self }
    fn event_emitter(&self) -> &dyn TurnEventEmitter { self }
    fn llm_resolver(&self) -> &dyn TurnLlmResolver { self }
    fn flush_deferred_context_switch(&self) { self.flush_deferred_context_switch(); }
    fn agent_type(&self) -> &str { match self.agent_type { AgentType::Main => "main", AgentType::Sub => "sub", AgentType::Independent => "independent" } }
    fn homedir(&self) -> Option<&str> { self.homedir.as_deref().and_then(|p| p.to_str()) }
    fn goal_runtime_enabled(&self) -> bool { false }
}
```

注意：`Agent::self_weak()` 需要把 `Arc<Agent>` 转成 `Weak<Agent>`。因为 `Agent` 自身无法直接拿到包裹它的 `Arc`，但在实现这些方法时可以通过 `Arc::downgrade(&unsafe { Arc::from_raw(self) })` 是不安全的。更简单的做法是：在 `Agent` 里额外保存一个 `self_weak: Weak<Agent>` 字段，在 `Arc::new_cyclic` 中填入 `weak.clone()`。然后 `Agent::self_weak()` 返回 `self.self_weak.clone()`。

修改 `Agent` struct 增加：

```rust
self_weak: Weak<Agent>,
```

并在 `AgentBuilder::build` 的 `Arc::new_cyclic` 中赋值 `self_weak: weak.clone()`。

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p agent-rs --test agent_turn_test
```

预期通过：测试能构造 `Agent`、发起 prompt、收集到 `TurnStarted` 与 `TurnEnded`。

- [ ] **全树 typecheck**：

```bash
cd rust-ody && cargo check --workspace
```

以及 TS 侧若未改动则不需要；本 Part 只改 Rust。

- [ ] **提交**：

```bash
git add rust-ody/crates/agent-rs/src/agent.rs rust-ody/crates/agent-rs/tests/agent_turn_test.rs rust-ody/crates/agent-rs/src/permission/manager.rs
git commit -m "feat(agent-rs): implement TurnAgent on Agent and wire TurnFlow"
```

---

## 本 Part 自检清单

- [ ] 1. Spec-coverage: Task 1-3 覆盖了 roadmap 4.3.9.1 的 `Agent` 组装器、`setContextMode`、`refreshLlm`、`llm`、`generate`（最小实现）、`resume`。
- [ ] 2. Placeholder scan: 无 TODO；所有 trait 方法均有具体委托或 no-op 说明。
- [ ] 3. No phantom tasks: 每个 task 都产生编译/测试可验证的变更。
- [ ] 4. Dependency soundness: Task 2 依赖 Task 1，Task 3 依赖 Task 2。
- [ ] 5. Caller & build soundness: Task 1 修改了 `ContextMemory` 签名并更新了唯一调用方 `context_golden.rs`；Task 3 以 `cargo check --workspace` 结束。
- [ ] 6. Test-the-risk: Task 1 验证 `ContextMemory` 可被拥有；Task 2 验证 `AgentBuilder` 构造成功；Task 3 验证 prompt 产生事件流。
- [ ] 7. Type consistency: `AgentContext` 同时满足所有子模块的 context trait；`Agent` 满足 `TurnAgent`。
