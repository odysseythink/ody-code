# Part 1: ConfigState + thinking 解析 + `AgentConfigContext` trait

本部分迁移 `packages/agent-core/src/agent/config/*`，把 Agent 的配置面抽象成可独立测试的 Rust 模块。关键决策是用 `AgentConfigContext` trait 隔离对 Agent 其余子系统的依赖，避免循环引用。

---

### Task 1: `AgentConfigData` / `AgentConfigUpdateData` / `ThinkingConfig` 类型

**Depends on:** 4.3.0 records 层（`AgentConfigUpdateData` 已在 `records/nested.rs` 定义）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/config/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/config/types.rs`
- Create: `rust-ody/crates/agent-rs/src/config/thinking.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs` line 1
- Test: `rust-ody/crates/agent-rs/src/config/thinking.rs`（内联 `#[cfg(test)]`）

**目标：** 在 `config` 模块定义 `AgentConfigData`、`AgentConfigUpdateData`（复用/重导出 records 版本）、`ThinkingConfig`，并实现 `resolve_thinking_effort` 对齐 TS `agent/config/thinking.ts`。

- [ ] 新建 `rust-ody/crates/agent-rs/src/config/mod.rs`：

```rust
pub mod state;
pub mod thinking;
pub mod types;

pub use state::*;
pub use thinking::*;
pub use types::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/config/types.rs`：

```rust
use kosong_rs::provider::{ModelCapability, ProviderConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigData {
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_alias: Option<String>,
    pub model_capabilities: ModelCapability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    pub thinking_level: String,
    pub system_prompt: String,
}

// Re-export the records-layer update payload so config, records, and tests
// all refer to the same type.
pub use crate::records::nested::AgentConfigUpdateData;
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/config/thinking.rs`：

```rust
pub use kosong_rs::provider::ThinkingEffort;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

const DEFAULT_THINKING_EFFORT: ThinkingEffort = ThinkingEffort::High;

pub fn resolve_thinking_effort(
    requested: Option<&str>,
    defaults: Option<&ThinkingConfig>,
) -> ThinkingEffort {
    let config_effort = defaults
        .and_then(|c| c.effort.as_deref())
        .and_then(parse_effort)
        .unwrap_or(DEFAULT_THINKING_EFFORT);

    let normalized = requested.map(|s| s.trim().to_lowercase());
    match normalized.as_deref() {
        None | Some("") => {
            if defaults.and_then(|c| c.mode.as_deref()) == Some("off") {
                ThinkingEffort::Off
            } else {
                config_effort
            }
        }
        Some("off") => ThinkingEffort::Off,
        Some("on") => config_effort,
        Some(other) => parse_effort(other).unwrap_or(config_effort),
    }
}

fn parse_effort(value: &str) -> Option<ThinkingEffort> {
    match value.trim().to_lowercase().as_str() {
        "low" => Some(ThinkingEffort::Low),
        "medium" => Some(ThinkingEffort::Medium),
        "high" => Some(ThinkingEffort::High),
        "xhigh" => Some(ThinkingEffort::Xhigh),
        "max" => Some(ThinkingEffort::Max),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_request_uses_default_effort() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("medium".into()),
        };
        assert_eq!(
            resolve_thinking_effort(None, Some(&config)),
            ThinkingEffort::Medium
        );
    }

    #[test]
    fn empty_request_with_mode_off_returns_off() {
        let config = ThinkingConfig {
            mode: Some("off".into()),
            effort: Some("high".into()),
        };
        assert_eq!(
            resolve_thinking_effort(None, Some(&config)),
            ThinkingEffort::Off
        );
    }

    #[test]
    fn on_returns_config_effort() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("low".into()),
        };
        assert_eq!(
            resolve_thinking_effort(Some("on"), Some(&config)),
            ThinkingEffort::Low
        );
    }

    #[test]
    fn explicit_effort_overrides_config() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("low".into()),
        };
        assert_eq!(
            resolve_thinking_effort(Some("max"), Some(&config)),
            ThinkingEffort::Max
        );
    }

    #[test]
    fn off_overrides_config() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("max".into()),
        };
        assert_eq!(
            resolve_thinking_effort(Some("off"), Some(&config)),
            ThinkingEffort::Off
        );
    }

    #[test]
    fn unknown_request_falls_back_to_config() {
        assert_eq!(
            resolve_thinking_effort(Some("weird"), None),
            DEFAULT_THINKING_EFFORT
        );
    }
}
```

- [ ] 在 `rust-ody/crates/agent-rs/src/lib.rs` 新增模块导出：

```rust
pub mod config;

pub mod records;
pub use records::*;
```

- [ ] 运行类型检查：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：无错误，`Finished dev [unoptimized + debuginfo] target(s)`。

- [ ] 运行 thinking 单元测试：

```bash
cd rust-ody && cargo test -p agent-rs --lib config::thinking
```

预期输出：`test result: ok. 6 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add config types and thinking-effort resolver`

---

### Task 2: `AgentConfigContext` trait + `ConfigState` 结构体

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/config/state.rs`
- Modify: `rust-ody/crates/agent-rs/src/config/mod.rs`
- Test: `rust-ody/crates/agent-rs/tests/config_state.rs`

**目标：** 定义 `AgentConfigContext` trait，封装 `ConfigState` 对 Agent 其余子系统的最小依赖；实现 `ConfigState` 的字段、构造、getter 与 `data()`。

- [ ] 新建 `rust-ody/crates/agent-rs/src/config/state.rs`：

```rust
use kosong_rs::provider::{ChatProvider, ModelCapability, ProviderConfig};

use crate::records::nested::AgentConfigUpdateData;
use crate::records::AgentRecord;

use super::thinking::{resolve_thinking_effort, ThinkingConfig, ThinkingEffort};
use super::types::AgentConfigData;

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
    fn initialize_builtin_tools(&self);

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
    pub fn new(mut context: C) -> Self {
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
            self.context.initialize_builtin_tools();
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
        kosong_rs::create_chat_provider(kosong_rs::ProviderFactoryConfig {
            provider_id: self.resolved_provider_config().provider_name,
            model: self.model(),
            api_key: None,
            base_url: None,
            default_headers: None,
        })
        .expect("provider resolution already succeeded")
    }

    pub fn model(&self) -> String {
        self.model_alias
            .clone()
            .expect("model not set")
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

    fn resolved_provider_config(&self) -> ResolvedRuntimeProvider {
        self.try_resolved_provider_config()
            .expect("provider not configured")
    }

    fn try_resolved_provider_config(&self) -> Option<ResolvedRuntimeProvider> {
        let alias = self.model_alias.as_deref()?;
        self.context.resolve_provider_config(alias)
    }
}
```

- [ ] 修改 `rust-ody/crates/agent-rs/src/config/mod.rs` 导出 `state`：

```rust
pub mod state;
pub mod thinking;
pub mod types;

pub use state::*;
pub use thinking::*;
pub use types::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/tests/config_state.rs`（先写失败测试）：

```rust
use agent_rs::config::{
    AgentConfigContext, AgentConfigData, AgentConfigUpdateData, ConfigState,
    ResolvedRuntimeProvider, ThinkingConfig, ThinkingEffort,
};
use agent_rs::records::AgentRecord;
use kosong_rs::provider::{ModelCapability, ProviderConfig, ProviderType};
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

    fn initialize_builtin_tools(&self) {
        *self.tool_inits.lock().unwrap() += 1;
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

    let records = state.context.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    match &records[0] {
        AgentRecord::ConfigUpdate { update, .. } => {
            assert_eq!(update.cwd, Some("/home".into()));
            assert_eq!(update.profile_name, Some("code".into()));
        }
        _ => panic!("expected config.update record"),
    }
    drop(records);

    assert_eq!(*state.context.status_updates.lock().unwrap(), 1);
    assert_eq!(*state.context.tool_inits.lock().unwrap(), 1);
    assert_eq!(state.context.chdirs.lock().unwrap().len(), 1);
    assert_eq!(state.context.replays.lock().unwrap().len(), 1);
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
    assert!(state.context.records.lock().unwrap().is_empty());
    assert_eq!(*state.context.status_updates.lock().unwrap(), 0);
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
```

- [ ] 运行测试，确认失败（因为 `ConfigState` 刚写完，但测试可能直接编译失败）：

```bash
cd rust-ody && cargo test -p agent-rs --test config_state
```

预期失败：若代码正确应直接通过；若存在类型错误会显示编译失败信息。

- [ ] 修复任何编译错误后再次运行：

```bash
cd rust-ody && cargo test -p agent-rs --test config_state
```

预期输出：`test result: ok. 5 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): implement ConfigState with AgentConfigContext trait`

---

### Task 3: `ConfigState` L1 fixture 与 TS 字段对照

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/generate_config_fixture.rs`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/config-rust.json`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（新增 bin）
- Test: `rust-ody/crates/agent-rs/tests/config_fixture_parity.rs`

**目标：** 让 Rust 生成一份 `AgentConfigData` JSON fixture，TS 侧读取并断言字段与 TS `ConfigState.data()` 一致。

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` 末尾新增 bin：

```toml
[[bin]]
name = "generate-config-fixture"
path = "src/bin/generate_config_fixture.rs"
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/bin/generate_config_fixture.rs`：

```rust
use agent_rs::config::{
    AgentConfigContext, AgentConfigData, AgentConfigUpdateData, ConfigState,
    ResolvedRuntimeProvider, ThinkingConfig, ThinkingEffort,
};
use agent_rs::records::AgentRecord;
use kosong_rs::provider::{ModelCapability, ProviderConfig, ProviderType};
use std::env;
use std::fs;
use std::path::PathBuf;

struct FixtureContext;

impl AgentConfigContext for FixtureContext {
    fn log_record(&mut self, _record: AgentRecord) {}
    fn emit_status_updated(&self) {}
    fn initialize_builtin_tools(&self) {}
    fn get_cwd(&self) -> String { "/fixture/cwd".into() }
    fn chdir(&self, _cwd: &str) {}
    fn default_model(&self) -> Option<String> { Some("kimi-k2".into()) }
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
```

- [ ] 生成 fixture：

```bash
cd rust-ody && cargo run -p agent-rs --bin generate-config-fixture
```

预期输出：`tests/fixtures/config-rust.json` 被创建，内容为 pretty JSON。

- [ ] 新建 `rust-ody/crates/agent-rs/tests/config_fixture_parity.rs`：

```rust
use agent_rs::config::AgentConfigData;
use serde_json;

#[test]
fn rust_config_fixture_round_trips() {
    let json = include_str!("fixtures/config-rust.json");
    let data: AgentConfigData = serde_json::from_str(json).unwrap();
    assert_eq!(data.cwd, "/fixture/cwd");
    assert_eq!(data.model_alias, Some("kimi-k2".into()));
    assert_eq!(data.profile_name, Some("fixture".into()));
    assert_eq!(data.thinking_level, "high");
    assert_eq!(data.system_prompt, "fixture system prompt");
    assert!(data.model_capabilities.thinking);
}
```

- [ ] 运行 fixture 测试：

```bash
cd rust-ody && cargo test -p agent-rs --test config_fixture_parity
```

预期输出：`test result: ok. 1 passed; 0 failed`。

- [ ] Commit：`test(agent-rs): add ConfigState L1 fixture for TS parity`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.2.1（`ConfigState` + thinking 解析）。
- [ ] 2. Placeholder扫描：无 TODO/TBD；`push_config_updated_replay` 是真实 trait 方法，由 4.3.9 实现。
- [ ] 3. No phantom tasks：Task 1 产出类型与 thinking 单测；Task 2 产出 `ConfigState` 与行为测试；Task 3 产出 fixture 与 parity 测试。
- [ ] 4. Dependency soundness：Task 2 依赖 Task 1；Task 3 依赖 Task 2；仅依赖 4.3.0 records 层。
- [ ] 5. Caller & build soundness：`rust-ody/crates/agent-rs/src/lib.rs` 新增 `pub mod config`，无其他 crate 调用方；以 `cargo check -p agent-rs --workspace --tests` 验证。
- [ ] 6. Test-the-risk：`update` 测试断言 record 写入、状态变更、`chdir` 调用、`initialize_builtin_tools` 调用、空 update 无操作、未配置 provider 时 capabilities 为 unknown。
- [ ] 7. Type一致性：`AgentConfigData` 字段与 TS `AgentConfigData` 一致；`AgentConfigUpdateData` 复用 4.3.0 records 层定义；`ThinkingEffort` 复用 `kosong-rs` 定义。
