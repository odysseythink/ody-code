# Part 4: `SkillRegistry` trait + `SkillManager` + `SkillActivationContext`

本部分迁移 `packages/agent-core/src/agent/skill/index.ts` 与 `packages/agent-core/src/skill/*`，把 Agent 的 skill 面抽象成可独立测试的 Rust 模块。`SkillActivationContext` trait 隔离对 `TurnFlow.prompt()` 的依赖；`SkillRegistry` trait 提供一个内存实现用于测试，真实文件扫描在 4.3.9 接入。

---

### Task 1: Skill 类型与 `SkillRegistry` trait

**Depends on:** 4.3.0 records 层（`PromptOrigin::SkillActivation` 已定义）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/skill/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/skill/types.rs`
- Create: `rust-ody/crates/agent-rs/src/skill/registry.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs`
- Test: `rust-ody/crates/agent-rs/tests/skill_registry.rs`

**目标：** 定义与 TS 字段名一致的 skill 类型，以及 `SkillRegistry` trait 与其内存实现 `InMemorySkillRegistry`。

- [ ] 新建 `rust-ody/crates/agent-rs/src/skill/mod.rs`：

```rust
pub mod manager;
pub mod registry;
pub mod types;

pub use manager::{SkillActivationContext, SkillManager, SkillPromptError};
pub use registry::{InMemorySkillRegistry, SkillRegistry};
pub use types::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/skill/types.rs`：

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Project,
    User,
    Extra,
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub skill_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_model_invocation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_in_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginContext {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub path: String,
    pub dir: String,
    pub content: String,
    pub metadata: SkillMetadata,
    pub source: SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<SkillPluginContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mermaid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d2: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub path: String,
    pub source: SkillSource,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub skill_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_model_invocation: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillRoot {
    pub path: String,
    pub source: SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<SkillPluginContext>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedSkill {
    pub path: String,
    #[serde(rename = "type")]
    pub skipped_type: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivatedEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub activation_id: String,
    pub skill_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_args: Option<String>,
    pub trigger: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_source: Option<SkillSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivateSkillPayload {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Skill \"{0}\" was not found")]
    NotFound(String),
    #[error("Skill \"{0}\" cannot be activated by the user")]
    UnsupportedType(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SkillPromptError {
    #[error("skill prompt failed: {0}")]
    PromptFailed(String),
}

pub fn normalize_skill_name(name: &str) -> String {
    name.to_lowercase()
}

pub fn is_inline_skill_type(skill_type: Option<&str>) -> bool {
    matches!(skill_type, None | Some("prompt") | Some("inline"))
}

pub fn is_user_activatable_skill_type(skill_type: Option<&str>) -> bool {
    is_inline_skill_type(skill_type) || skill_type == Some("flow")
}

pub fn is_knowledge_skill_type(skill_type: Option<&str>) -> bool {
    skill_type == Some("knowledge")
}

pub fn is_supported_skill_type(skill_type: Option<&str>) -> bool {
    is_user_activatable_skill_type(skill_type) || is_knowledge_skill_type(skill_type)
}

pub fn summarize_skill(skill: &SkillDefinition) -> SkillSummary {
    SkillSummary {
        name: skill.name.clone(),
        description: skill.description.clone(),
        path: skill.path.clone(),
        source: skill.source,
        skill_type: skill.metadata.skill_type.clone(),
        disable_model_invocation: skill.metadata.disable_model_invocation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_source_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&SkillSource::Builtin).unwrap(),
            "\"builtin\""
        );
    }

    #[test]
    fn skill_metadata_type_field_round_trips() {
        let meta = SkillMetadata {
            name: None,
            description: None,
            skill_type: Some("flow".into()),
            when_to_use: None,
            disable_model_invocation: None,
            hidden_in_modes: None,
            safe: None,
            arguments: None,
            triggers: None,
            extra: HashMap::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"type\":\"flow\""));
        let parsed: SkillMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.skill_type, Some("flow".into()));
    }

    #[test]
    fn summarize_skill_preserves_source_and_type() {
        let skill = SkillDefinition {
            name: "simplicity-first".into(),
            description: "".into(),
            path: "/skills/simplicity-first.md".into(),
            dir: "/skills".into(),
            content: "".into(),
            metadata: SkillMetadata {
                skill_type: Some("inline".into()),
                ..Default::default()
            },
            source: SkillSource::Builtin,
            plugin: None,
            mermaid: None,
            d2: None,
        };
        let summary = summarize_skill(&skill);
        assert_eq!(summary.skill_type, Some("inline".into()));
        assert_eq!(summary.source, SkillSource::Builtin);
    }
}
```

注意：`SkillDefinition` 的 `metadata: SkillMetadata { ..Default::default() }` 要求 `SkillMetadata` 实现 `Default`。由于所有字段都是 `Option`，可在 `SkillMetadata` 上添加 `#[derive(Default)]`。请同步添加。

- [ ] 在 `SkillMetadata` 的 derive 列表中加入 `Default`：

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMetadata { ... }
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/skill/registry.rs`：

```rust
use std::collections::HashMap;

use super::types::{is_inline_skill_type, normalize_skill_name, SkillDefinition};

pub trait SkillRegistry: Send + Sync {
    fn get_skill(&self, name: &str) -> Option<&SkillDefinition>;
    fn list_skills(&self) -> Vec<&SkillDefinition>;
    fn list_invocable_skills(&self, session_mode: Option<&str>) -> Vec<&SkillDefinition>;
    fn render_skill_prompt(&self, skill: &SkillDefinition, raw_args: &str) -> String;
}

pub struct InMemorySkillRegistry {
    by_name: HashMap<String, SkillDefinition>,
}

impl InMemorySkillRegistry {
    pub fn new() -> Self {
        Self {
            by_name: HashMap::new(),
        }
    }

    pub fn register(&mut self, skill: SkillDefinition) {
        self.by_name
            .insert(normalize_skill_name(&skill.name), skill);
    }
}

impl Default for InMemorySkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry for InMemorySkillRegistry {
    fn get_skill(&self, name: &str) -> Option<&SkillDefinition> {
        self.by_name.get(&normalize_skill_name(name))
    }

    fn list_skills(&self) -> Vec<&SkillDefinition> {
        let mut skills: Vec<_> = self.by_name.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    fn list_invocable_skills(&self, session_mode: Option<&str>) -> Vec<&SkillDefinition> {
        self.list_skills()
            .into_iter()
            .filter(|skill| {
                if skill.metadata.disable_model_invocation == Some(true) {
                    return false;
                }
                if !is_inline_skill_type(skill.metadata.skill_type.as_deref()) {
                    return false;
                }
                if let Some(mode) = session_mode {
                    if let Some(hidden) = &skill.metadata.hidden_in_modes {
                        if hidden.iter().any(|m| m.eq_ignore_ascii_case(mode)) {
                            return false;
                        }
                    }
                }
                true
            })
            .collect()
    }

    fn render_skill_prompt(&self, skill: &SkillDefinition, _raw_args: &str) -> String {
        skill.content.clone()
    }
}
```

- [ ] 修改 `rust-ody/crates/agent-rs/src/lib.rs`，加入 `skill` 模块导出：

```rust
pub mod config;
pub mod records;
pub mod skill;
pub mod tool;
pub mod usage;

pub use records::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/tests/skill_registry.rs`（先写失败测试）：

```rust
use agent_rs::skill::{
    InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillRegistry, SkillSource,
};

fn sample_skill(name: &str, skill_type: Option<&str>, disabled: bool) -> SkillDefinition {
    SkillDefinition {
        name: name.into(),
        description: "".into(),
        path: format!("/skills/{}.md", name),
        dir: "/skills".into(),
        content: format!("content of {}", name),
        metadata: SkillMetadata {
            skill_type: skill_type.map(|s| s.into()),
            disable_model_invocation: if disabled { Some(true) } else { None },
            hidden_in_modes: None,
            ..SkillMetadata::default()
        },
        source: SkillSource::Project,
        plugin: None,
        mermaid: None,
        d2: None,
    }
}

#[test]
fn registry_get_is_case_insensitive() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("MySkill", Some("inline"), false));
    assert!(registry.get_skill("mYskill").is_some());
}

#[test]
fn registry_lists_sorted() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("beta", Some("inline"), false));
    registry.register(sample_skill("alpha", Some("inline"), false));
    let names: Vec<_> = registry
        .list_skills()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn registry_filters_invocable_skills() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("inline-skill", Some("inline"), false));
    registry.register(sample_skill("knowledge-skill", Some("knowledge"), false));
    registry.register(sample_skill("disabled-skill", Some("inline"), true));
    let invocable = registry.list_invocable_skills(None);
    assert_eq!(invocable.len(), 1);
    assert_eq!(invocable[0].name, "inline-skill");
}

#[test]
fn registry_hides_skills_in_session_mode() {
    let mut registry = InMemorySkillRegistry::new();
    let mut skill = sample_skill("hidden", Some("inline"), false);
    skill.metadata.hidden_in_modes = Some(vec!["design".into()]);
    registry.register(skill);
    assert_eq!(registry.list_invocable_skills(None).len(), 1);
    assert_eq!(registry.list_invocable_skills(Some("design")).len(), 0);
}

#[test]
fn render_skill_prompt_returns_content() {
    let registry = InMemorySkillRegistry::new();
    let skill = sample_skill("x", Some("inline"), false);
    assert_eq!(registry.render_skill_prompt(&skill, "args"), "content of x");
}
```

- [ ] 运行测试，确认失败：

```bash
cd rust-ody && cargo test -p agent-rs --test skill_registry
```

预期失败：`error[E0433]: failed to resolve: use of undeclared crate or module 'skill'`。

- [ ] 完成实现并再次运行：

```bash
cd rust-ody && cargo test -p agent-rs --test skill_registry
```

预期输出：`test result: ok. 5 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add skill types and in-memory SkillRegistry trait`

---

### Task 2: `SkillActivationContext` trait + `SkillManager`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/skill/manager.rs`
- Modify: `rust-ody/crates/agent-rs/src/skill/mod.rs`
- Test: `rust-ody/crates/agent-rs/tests/skill_manager.rs`

**目标：** 定义 `SkillActivationContext` trait，实现 `SkillManager::activate` 与 `record_activation`，行为与 TS `SkillManager` 对齐：校验 skill 存在与可激活类型、生成 `<kimi-skill-loaded>` system reminder、emit 事件、telemetry、调用 `context.prompt()`。

- [ ] 新建 `rust-ody/crates/agent-rs/src/skill/manager.rs`：

```rust
use std::collections::HashMap;

use kosong_rs::message::ContentPart;

use crate::records::nested::PromptOrigin;
use super::registry::SkillRegistry;
use super::types::{
    is_user_activatable_skill_type, ActivateSkillPayload, SkillActivatedEvent, SkillDefinition,
    SkillError, SkillPromptError, SkillSource,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SkillActivationOrigin {
    pub activation_id: String,
    pub skill_name: String,
    pub skill_args: Option<String>,
    pub trigger: String,
    pub skill_type: Option<String>,
    pub skill_path: Option<String>,
    pub skill_source: Option<SkillSource>,
}

/// Minimal Agent surface required by `SkillManager`.
pub trait SkillActivationContext: Send + Sync {
    fn emit_skill_activated(&mut self, event: SkillActivatedEvent);
    fn telemetry_track(&mut self, event_name: &str, properties: HashMap<String, String>);
    fn prompt(
        &mut self,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    ) -> Result<(), SkillPromptError>;
    fn new_activation_id(&self) -> String;
}

pub struct SkillManager<C: SkillActivationContext, R: SkillRegistry> {
    context: C,
    registry: R,
}

impl<C: SkillActivationContext, R: SkillRegistry> SkillManager<C, R> {
    pub fn new(context: C, registry: R) -> Self {
        Self { context, registry }
    }

    pub fn activate(&mut self, payload: ActivateSkillPayload) -> Result<(), SkillError> {
        let skill = self
            .registry
            .get_skill(&payload.name)
            .ok_or_else(|| SkillError::NotFound(payload.name.clone()))?;

        if !is_user_activatable_skill_type(skill.metadata.skill_type.as_deref()) {
            return Err(SkillError::UnsupportedType(skill.name.clone()));
        }

        let skill_content = self
            .registry
            .render_skill_prompt(skill, payload.args.as_deref().unwrap_or(""));
        let args_attr = payload
            .args
            .as_ref()
            .map(|a| format!(" args=\"{}\"", escape_xml(a)))
            .unwrap_or_default();
        let wrapped_text = format!(
            "<system-reminder>\n<kimi-skill-loaded name=\"{}\"{}>\n{}\n</kimi-skill-loaded>\n</system-reminder>",
            escape_xml(&skill.name),
            args_attr,
            skill_content
        );
        let wrapped = vec![ContentPart::Text { text: wrapped_text }];

        let origin = SkillActivationOrigin {
            activation_id: self.context.new_activation_id(),
            skill_name: skill.name.clone(),
            skill_args: payload.args,
            trigger: "user-slash".to_string(),
            skill_type: skill.metadata.skill_type.clone(),
            skill_path: Some(skill.path.clone()),
            skill_source: Some(skill.source),
        };
        self.record_activation(origin, Some(wrapped));
        Ok(())
    }

    pub fn record_activation(
        &mut self,
        origin: SkillActivationOrigin,
        input: Option<Vec<ContentPart>>,
    ) {
        self.context.emit_skill_activated(SkillActivatedEvent {
            event_type: "skill.activated".to_string(),
            activation_id: origin.activation_id.clone(),
            skill_name: origin.skill_name.clone(),
            skill_args: origin.skill_args.clone(),
            trigger: origin.trigger.clone(),
            skill_path: origin.skill_path.clone(),
            skill_source: origin.skill_source,
        });

        let mut props = HashMap::new();
        props.insert("skill_name".to_string(), origin.skill_name.clone());
        props.insert("trigger".to_string(), origin.trigger.clone());
        self.context.telemetry_track("skill_invoked", props);

        if origin.skill_type.as_deref() == Some("flow") {
            let mut flow_props = HashMap::new();
            flow_props.insert("flow_name".to_string(), origin.skill_name.clone());
            self.context.telemetry_track("flow_invoked", flow_props);
        }

        if let Some(input) = input {
            let prompt_origin = PromptOrigin::SkillActivation {
                activation_id: origin.activation_id,
                skill_name: origin.skill_name,
                skill_args: origin.skill_args,
                trigger: origin.trigger,
                skill_type: origin.skill_type,
                skill_path: origin.skill_path,
            };
            self.context
                .prompt(input, prompt_origin)
                .expect("prompt should succeed");
        }
    }

    pub fn into_inner(self) -> (C, R) {
        (self.context, self.registry)
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}
```

- [ ] 修改 `rust-ody/crates/agent-rs/src/skill/mod.rs` 导出 `manager`：

```rust
pub mod manager;
pub mod registry;
pub mod types;

pub use manager::{SkillActivationContext, SkillManager, SkillPromptError};
pub use registry::{InMemorySkillRegistry, SkillRegistry};
pub use types::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/tests/skill_manager.rs`（先写失败测试）：

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use agent_rs::records::nested::PromptOrigin;
use agent_rs::skill::{
    ActivateSkillPayload, InMemorySkillRegistry, SkillActivationContext, SkillActivatedEvent,
    SkillDefinition, SkillManager, SkillMetadata, SkillPromptError, SkillRegistry, SkillSource,
};
use kosong_rs::message::ContentPart;

#[derive(Debug, Default)]
struct MockCtx {
    events: Mutex<Vec<SkillActivatedEvent>>,
    tracks: Mutex<Vec<(String, HashMap<String, String>)>>,
    prompts: Mutex<Vec<(Vec<ContentPart>, PromptOrigin)>>,
}

impl SkillActivationContext for MockCtx {
    fn emit_skill_activated(&mut self, event: SkillActivatedEvent) {
        self.events.lock().unwrap().push(event);
    }

    fn telemetry_track(&mut self, event_name: &str, properties: HashMap<String, String>) {
        self.tracks
            .lock()
            .unwrap()
            .push((event_name.to_string(), properties));
    }

    fn prompt(
        &mut self,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    ) -> Result<(), SkillPromptError> {
        self.prompts.lock().unwrap().push((input, origin));
        Ok(())
    }

    fn new_activation_id(&self) -> String {
        "activation-1".to_string()
    }
}

fn sample_skill(name: &str, skill_type: Option<&str>) -> SkillDefinition {
    SkillDefinition {
        name: name.into(),
        description: "".into(),
        path: format!("/skills/{}.md", name),
        dir: "/skills".into(),
        content: format!("content of {}", name),
        metadata: SkillMetadata {
            skill_type: skill_type.map(|s| s.into()),
            ..SkillMetadata::default()
        },
        source: SkillSource::Builtin,
        plugin: None,
        mermaid: None,
        d2: None,
    }
}

#[test]
fn activate_inline_skill_emits_event_and_prompts() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("my-skill", Some("inline")));

    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    manager
        .activate(ActivateSkillPayload {
            name: "my-skill".into(),
            args: Some("foo=bar".into()),
        })
        .unwrap();

    let ctx = manager.into_inner().0;
    assert_eq!(ctx.events.lock().unwrap().len(), 1);
    assert_eq!(ctx.tracks.lock().unwrap().len(), 1);
    assert_eq!(ctx.prompts.lock().unwrap().len(), 1);

    let event = &ctx.events.lock().unwrap()[0];
    assert_eq!(event.event_type, "skill.activated");
    assert_eq!(event.skill_name, "my-skill");
    assert_eq!(event.skill_args, Some("foo=bar".into()));
    assert_eq!(event.trigger, "user-slash");

    let (parts, origin) = &ctx.prompts.lock().unwrap()[0];
    assert_eq!(parts.len(), 1);
    match origin {
        PromptOrigin::SkillActivation { skill_name, .. } => {
            assert_eq!(skill_name, "my-skill");
        }
        _ => panic!("expected skill_activation origin"),
    }
}

#[test]
fn activate_flow_skill_tracks_flow_invoked() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("my-flow", Some("flow")));

    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    manager
        .activate(ActivateSkillPayload {
            name: "my-flow".into(),
            args: None,
        })
        .unwrap();

    let ctx = manager.into_inner().0;
    let tracks = ctx.tracks.lock().unwrap();
    assert!(tracks.iter().any(|(name, _)| name == "skill_invoked"));
    assert!(tracks.iter().any(|(name, props)| {
        name == "flow_invoked" && props.get("flow_name") == Some(&"my-flow".to_string())
    }));
}

#[test]
fn activate_unknown_skill_returns_not_found() {
    let registry = InMemorySkillRegistry::new();
    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    let err = manager
        .activate(ActivateSkillPayload {
            name: "missing".into(),
            args: None,
        })
        .unwrap_err();
    assert!(err.to_string().contains("was not found"));
}

#[test]
fn activate_knowledge_skill_returns_unsupported() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("my-knowledge", Some("knowledge")));

    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    let err = manager
        .activate(ActivateSkillPayload {
            name: "my-knowledge".into(),
            args: None,
        })
        .unwrap_err();
    assert!(err.to_string().contains("cannot be activated by the user"));
}

#[test]
fn record_activation_without_input_does_not_prompt() {
    let registry = InMemorySkillRegistry::new();
    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    manager.record_activation(
        agent_rs::skill::manager::SkillActivationOrigin {
            activation_id: "a".into(),
            skill_name: "x".into(),
            skill_args: None,
            trigger: "model-tool".into(),
            skill_type: None,
            skill_path: None,
            skill_source: None,
        },
        None,
    );
    let ctx = manager.into_inner().0;
    assert_eq!(ctx.events.lock().unwrap().len(), 1);
    assert_eq!(ctx.prompts.lock().unwrap().len(), 0);
}
```

- [ ] 运行测试，确认失败：

```bash
cd rust-ody && cargo test -p agent-rs --test skill_manager
```

预期失败：`error[E0433]: failed to resolve: use of undeclared crate or module 'manager'`（因为 `manager.rs` 尚未创建）。

- [ ] 完成实现并再次运行：

```bash
cd rust-ody && cargo test -p agent-rs --test skill_manager
```

预期输出：`test result: ok. 5 passed; 0 failed`。

- [ ] 运行整 crate 类型检查：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：无错误，`Finished dev [unoptimized + debuginfo] target(s)`。

- [ ] Commit：`feat(agent-rs): implement SkillManager with SkillActivationContext trait`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.2.4（`SkillManager`）。
- [ ] 2. Placeholder扫描：无 TODO/TBD；`SkillRegistry` 的复杂扫描/渲染能力以 trait 方法保留，内存实现仅用于测试；真实文件扫描在 4.3.9 接入。
- [ ] 3. No phantom tasks：Task 1 产出 skill 类型与 `SkillRegistry` trait + 测试；Task 2 产出 `SkillManager` + `SkillActivationContext` + 测试。
- [ ] 4. Dependency soundness：Task 2 依赖 Task 1；仅依赖 4.3.0 records 层，无反向依赖。
- [ ] 5. Caller & build soundness：`lib.rs` 新增 `pub mod skill`，无其他 crate 调用方；以 `cargo check -p agent-rs --workspace --tests` 验证。
- [ ] 6. Test-the-risk：`activate` 测试断言 skill 不存在/不可激活返回错误、成功时 emit 事件、telemetry、调用 `prompt` trait；`record_activation` 测试断言无 input 时不调用 prompt。
- [ ] 7. Type一致性：`SkillSource`、`SkillMetadata`、`SkillDefinition`、`SkillActivatedEvent` 字段名/序列化与 TS 源一致；`PromptOrigin::SkillActivation` 复用 4.3.0 records 层定义；`ContentPart` 复用 `kosong-rs` 定义。
