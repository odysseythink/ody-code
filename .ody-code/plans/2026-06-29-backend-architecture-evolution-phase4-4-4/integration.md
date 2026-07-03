# Phase 4.4.4 Part 5 — agent-rs 接入、ToolManager 注册与 L3 行为一致性

**Goal:** 将 Rust 实现的 `SkillTool` / `AskUserQuestionTool` / `AgentTool` 接入 `agent-rs` 的 `AgentBuilder/Agent` 生命周期，使 `TurnFlow` 能把它们分发给 LLM，并用 L3 场景验证与 TypeScript 行为一致。

**Architecture:** 在 `agent-rs` 中实现 `SkillProvider`、`QuestionProvider`、`SubagentHost`、`BackgroundRegistrar` 的 agent 侧适配器；在 `Agent::loop_tools()` 中通过 `ToolBridge` 把 `tools-rs` 的协作工具桥接为 `agent_loop::ExecutableTool`；新增 `AgentBuilder` 配置入口；最后写一个 mock-LLM L3 测试，驱动一次包含三个协作工具调用的 turn。

**Tech Stack:** Rust 2021, `tools-rs`, `agent-rs`, `async-trait`, `tokio`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/agent-rs/
  Cargo.toml                                    # 确认/补充 tools-rs 依赖
  src/tool/mod.rs                               # 暴露 collaboration 子模块
  src/tool/bridge.rs                            # 复用 Part 1 的桥接器
  src/tool/collaboration/mod.rs                 # 子模块入口 + 共享类型
  src/tool/collaboration/skill_provider.rs      # AgentSkillProvider
  src/tool/collaboration/question_provider.rs   # AgentQuestionProvider + callback
  src/tool/collaboration/subagent_host.rs       # AgentSubagentHost
  src/tool/collaboration/background_registrar.rs# AgentBackgroundRegistrar
  src/agent.rs                                  # AgentBuilder 字段 + Agent::loop_tools
  tests/tool_collaboration_module.rs            # Task 1 编译测试
  tests/collaboration_l3.rs                     # Task 6 L3 行为一致性测试
```

## Dependency Overview

```
Part 1–4 已完成的 tools-rs 协作工具
  │
  ▼
Task 1: agent-rs 依赖与模块脚手架
  │
  ├──► Task 2: AgentSkillProvider
  │
  ├──► Task 3: AgentQuestionProvider
  │
  ├──► Task 4: AgentSubagentHost + AgentBackgroundRegistrar
  │
  ▼
Task 5: AgentBuilder/Agent 接入 + loop_tools 注册（共享签名变更）
  │
  ▼
Task 6: L3 行为一致性测试
```

- **Task 1** 是模块/依赖层准备，不改动共享签名。
- **Task 2–4** 并行实现三个适配器；它们之间无依赖。
- **Task 5** 是共享签名变更：修改 `AgentBuilder::skills_registry` 存储语义、新增 builder 字段、修改 `Agent` 字段与 `Agent::loop_tools()`；必须在同一任务中更新所有调用者并做全工作区类型检查。
- **Task 6** 依赖 Task 5 完成后的完整 wiring。

## Risks & Open Questions

- `Agent::loop_tools()` 当前返回空列表；接入后会与 `ToolManager` 并列返回协作工具。`ToolManager` 中的核心 builtin（Read/Write/…）仍未迁移，L3 场景若需要它们，请在测试中显式传入 mock 工具。
- `BackgroundManager` 在 `AgentBuilder::build()` 之后才注入，因此 `AgentBackgroundRegistrar` 使用 `Mutex<Option<Arc<BackgroundManager>>>` 并在运行时读取；Task 5 要保证 `Agent::loop_tools()` 每次调用都读取最新 background 引用。
- `AgentSubagentHost` 默认实现会真正创建子 agent 并跑 `run_turn`；若测试不想跑完整子 turn，可通过 `with_run_fn` 注入确定性 completion。

## Tasks

### Task 1: 添加 agent-rs → tools-rs 依赖并创建 collaboration 模块

**Depends on:** `2026-06-29-backend-architecture-evolution-phase4-4/infra.md`: Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/Cargo.toml:24-30`（添加 `tools-rs` 依赖）
- Modify: `rust-ody/crates/agent-rs/src/tool/mod.rs:1-5`（暴露子模块）
- Create: `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs`
- Test: `rust-ody/crates/agent-rs/tests/tool_collaboration_module.rs`

- [ ] Write the failing test.
  创建 `rust-ody/crates/agent-rs/tests/tool_collaboration_module.rs`：
  ```rust
  use std::sync::Arc;
  use agent_rs::tool::bridge::ToolBridge;

  #[test]
  fn collaboration_module_and_bridge_are_public() {
      let bridge_exists = std::any::TypeId::of::<ToolBridge>() == std::any::TypeId::of::<ToolBridge>();
      assert!(bridge_exists);
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p agent-rs collaboration_module_and_bridge_are_public
  ```
  Expected failure: `agent_rs::tool::bridge` 或 `collaboration` module 不存在；或 `Cargo.toml` 未链接 `tools-rs`。

- [ ] Write the minimal implementation.
  1. 确认 `rust-ody/crates/agent-rs/Cargo.toml` 已包含：
     ```toml
     tools-rs = { path = "../tools-rs" }
     ```
     若 Part 1 已添加则跳过；若缺失则添加在 `[dependencies]` 段任意位置。
  2. 创建 `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs`：
     ```rust
     pub mod background_registrar;
     pub mod question_provider;
     pub mod skill_provider;
     pub mod subagent_host;

     pub use background_registrar::AgentBackgroundRegistrar;
     pub use question_provider::{AgentQuestionProvider, QuestionCallback};
     pub use skill_provider::AgentSkillProvider;
     pub use subagent_host::{AgentSubagentHost, SubagentRunFn};
     ```
  3. 修改 `rust-ody/crates/agent-rs/src/tool/mod.rs`：
     ```rust
     pub mod bridge;
     pub mod collaboration;
     pub use bridge::ToolBridge;
     pub use manager::{ToolManager, ToolManagerContext};
     pub use types::*;
     ```

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p agent-rs collaboration_module_and_bridge_are_public
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/agent-rs/Cargo.toml \
         rust-ody/crates/agent-rs/src/tool/mod.rs \
         rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs \
         rust-ody/crates/agent-rs/tests/tool_collaboration_module.rs
  git commit -m "feat(agent-rs): scaffold tool/collaboration module and tools-rs dependency"
  ```

### Task 2: 实现 `AgentSkillProvider`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/collaboration/skill_provider.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs`（re-export）
- Test: `rust-ody/crates/agent-rs/src/tool/collaboration/skill_provider.rs`（模块级测试）

- [ ] Write the failing test.
  在 `skill_provider.rs` 中先写测试模块：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::sync::{Arc, Mutex};
      use agent_rs::agent::{AgentBuilder, AgentEnvironment, AgentType};
      use agent_rs::skill::{InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillSource};
      use kaos_rs::environment::detect_environment_from_node;
      use kaos_rs::kaos::Kaos;
      use kosong_rs::message::ContentPart;

      struct NoopEnv;
      #[async_trait::async_trait]
      impl AgentEnvironment for NoopEnv {
          fn emit_event(&self, _event: agent_rs::turn::types::AgentEvent) {}
          async fn request_approval(
              &self,
              _req: &agent_rs::permission::types::ApprovalRequest,
              _signal: kosong_rs::provider::AbortSignal,
          ) -> Result<agent_rs::records::nested::ApprovalResponse, anyhow::Error> {
              Ok(agent_rs::records::nested::ApprovalResponse {
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
          ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
              Box::pin(async { Ok(None) })
          }
          fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
          fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
          fn fire_hook_user_prompt_submit(
              &self,
              _input: Vec<ContentPart>,
              _signal: kosong_rs::provider::AbortSignal,
          ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<agent_rs::turn::types::HookResult>, anyhow::Error>> + Send + '_>> {
              Box::pin(async { Ok(vec![]) })
          }
          fn fire_hook_stop_hook(
              &self,
              _signal: kosong_rs::provider::AbortSignal,
          ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<agent_rs::turn::types::StopHookBlock>, anyhow::Error>> + Send + '_>> {
              Box::pin(async { Ok(None) })
          }
          fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
      }

      fn sample_skill() -> SkillDefinition {
          SkillDefinition {
              name: "refactor".into(),
              description: "".into(),
              path: "/skills/refactor.md".into(),
              dir: "/skills".into(),
              content: "Refactor this code.".into(),
              metadata: SkillMetadata {
                  skill_type: Some("prompt".into()),
                  ..SkillMetadata::default()
              },
              source: SkillSource::Project,
              plugin: None,
              mermaid: None,
              d2: None,
          }
      }

      fn build_agent() -> Arc<agent_rs::agent::Agent> {
          let kaos = Arc::new(Kaos::new(detect_environment_from_node()));
          let runtime = tokio::runtime::Runtime::new().unwrap();
          runtime.block_on(async {
              AgentBuilder::new("test", kaos, Arc::new(NoopEnv))
                  .build()
                  .await
                  .unwrap()
          })
      }

      #[test]
      fn provider_maps_registry_skill_to_tools_rs_info() {
          let agent = build_agent();
          let mut registry = InMemorySkillRegistry::new();
          registry.register(sample_skill());
.         let provider = AgentSkillProvider::new(Arc::downgrade(&agent), Box::new(registry));

          let info = provider.get_skill("refactor").expect("skill should exist");
          assert_eq!(info.name, "refactor");
          assert_eq!(info.skill_type.as_deref(), Some("prompt"));
          assert_eq!(info.content, "Refactor this code.");
          assert_eq!(info.source, "project");
      }

      #[test]
      fn record_activation_emits_event_and_tracks_telemetry() {
          let agent = build_agent();
          let mut registry = InMemorySkillRegistry::new();
          registry.register(sample_skill());
          let provider = AgentSkillProvider::new(Arc::downgrade(&agent), Box::new(registry));

          provider
              .record_activation(tools_rs::builtin::collaboration::SkillActivationOrigin {
                  activation_id: "a1".into(),
                  skill_name: "refactor".into(),
                  skill_args: Some("foo.rs".into()),
                  trigger: "model-tool".into(),
                  skill_type: Some("prompt".into()),
                  skill_path: Some("/skills/refactor.md".into()),
                  skill_source: Some("project".into()),
              })
              .unwrap();

          // AgentContext 当前为 stub 实现；这里至少验证调用不 panic。
          assert!(true);
      }
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p agent-rs skill_provider
  ```
  Expected failure: `AgentSkillProvider` 未定义。

- [ ] Write the minimal implementation.
  创建 `rust-ody/crates/agent-rs/src/tool/collaboration/skill_provider.rs`：
  ```rust
  use std::sync::{Arc, Mutex};

  use crate::agent::AgentContext;
  use crate::context::types::PromptOrigin;
  use crate::records::nested::SkillActivatedEvent;
  use crate::skill::{
      registry::SkillRegistry,
      types::{SkillDefinition, SkillSource},
      SkillActivationContext, SkillActivationOrigin as AgentSkillActivationOrigin,
  };
  use kosong_rs::message::ContentPart;
  use tools_rs::builtin::collaboration::{
      SkillActivationOrigin as ToolsSkillActivationOrigin, SkillError, SkillInfo, SkillProvider,
  };

  fn map_skill_source(s: &str) -> SkillSource {
      match s.to_ascii_lowercase().as_str() {
          "user" => SkillSource::User,
          "extra" => SkillSource::Extra,
          "builtin" => SkillSource::Builtin,
          _ => SkillSource::Project,
      }
  }

  fn map_tools_origin_to_agent_origin(
      origin: &ToolsSkillActivationOrigin,
  ) -> AgentSkillActivationOrigin {
      AgentSkillActivationOrigin {
          activation_id: origin.activation_id.clone(),
          skill_name: origin.skill_name.clone(),
          skill_args: origin.skill_args.clone(),
          trigger: origin.trigger.clone(),
          skill_type: origin.skill_type.clone(),
          skill_path: origin.skill_path.clone(),
          skill_source: origin.skill_source.as_deref().map(map_skill_source),
      }
  }

  pub struct AgentSkillProvider {
      context: Mutex<AgentContext>,
      registry: Mutex<Box<dyn SkillRegistry>>,
  }

  impl AgentSkillProvider {
      pub fn new(agent: std::sync::Weak<crate::agent::Agent>, registry: Box<dyn SkillRegistry>) -> Self {
          Self {
              context: Mutex::new(AgentContext { agent }),
              registry: Mutex::new(registry),
          }
      }
  }

  impl SkillProvider for AgentSkillProvider {
      fn get_skill(&self, name: &str) -> Option<SkillInfo> {
          let registry = self.registry.lock().unwrap();
          registry.get_skill(name).map(|s| SkillInfo {
              name: s.name.clone(),
              skill_type: s.metadata.skill_type.clone(),
              disable_model_invocation: s.metadata.disable_model_invocation,
              hidden_in_modes: s.metadata.hidden_in_modes.clone(),
              content: s.content.clone(),
              path: s.path.clone(),
              source: match s.source {
                  SkillSource::Project => "project".into(),
                  SkillSource::User => "user".into(),
                  SkillSource::Extra => "extra".into(),
                  SkillSource::Builtin => "builtin".into(),
              },
          })
      }

      fn record_activation(&self, origin: ToolsSkillActivationOrigin) -> Result<(), SkillError> {
          let mut ctx = self.context.lock().unwrap();
          let agent_origin = map_tools_origin_to_agent_origin(&origin);
          ctx.emit_skill_activated(SkillActivatedEvent {
              event_type: "skill.activated".into(),
              activation_id: agent_origin.activation_id,
              skill_name: agent_origin.skill_name,
              skill_args: agent_origin.skill_args,
              trigger: agent_origin.trigger,
              skill_path: agent_origin.skill_path,
              skill_source: agent_origin.skill_source,
          });
          let mut props = std::collections::HashMap::new();
          props.insert("skill_name".into(), origin.skill_name.clone());
          props.insert("trigger".into(), origin.trigger.clone());
          ctx.telemetry_track("skill_invoked", props);
          Ok(())
      }

      fn render_skill_prompt(&self, skill: &SkillInfo, args: &str) -> String {
          let registry = self.registry.lock().unwrap();
          let def = SkillDefinition {
              name: skill.name.clone(),
              description: String::new(),
              path: skill.path.clone(),
              dir: String::new(),
              content: skill.content.clone(),
              metadata: crate::skill::SkillMetadata {
                  skill_type: skill.skill_type.clone(),
                  disable_model_invocation: skill.disable_model_invocation,
                  hidden_in_modes: skill.hidden_in_modes.clone(),
                  ..Default::default()
              },
              source: skill.skill_source.as_deref().map(map_skill_source).unwrap_or(SkillSource::Project),
              plugin: None,
              mermaid: None,
              d2: None,
          };
          registry.render_skill_prompt(&def, args)
      }

      fn current_session_mode(&self) -> Option<String> {
          None
      }

      fn append_system_reminder(&self, content: String, origin: ToolsSkillActivationOrigin) -> Result<(), SkillError> {
          let mut ctx = self.context.lock().unwrap();
          let agent_origin = map_tools_origin_to_agent_origin(&origin);
          let _ = ctx.prompt(
              vec![ContentPart::Text { text: content }],
              PromptOrigin::SkillActivation {
                  activation_id: agent_origin.activation_id,
                  skill_name: agent_origin.skill_name,
                  skill_args: agent_origin.skill_args,
                  trigger: agent_origin.trigger,
                  skill_type: agent_origin.skill_type,
                  skill_path: agent_origin.skill_path,
              },
          );
          Ok(())
      }
  }
  ```
. 注意：
  - `AgentSkillProvider::new` 接收 `Weak<Agent>`，测试通过 `Arc::downgrade(&agent)` 传入。

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p agent-rs skill_provider
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/agent-rs/src/tool/collaboration/skill_provider.rs \
         rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs
  git commit -m "feat(agent-rs): implement AgentSkillProvider adapter"
  ```

### Task 3: 实现 `AgentQuestionProvider`（基于 callback 后端）

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/collaboration/question_provider.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs`（re-export）
- Test: `rust-ody/crates/agent-rs/src/tool/collaboration/question_provider.rs`（模块级测试）

- [ ] Write the failing test.
  在 `question_provider.rs` 中先写测试：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::collections::HashMap;
      use std::future::Future;
      use std::pin::Pin;
      use std::sync::{Arc, Mutex};
      use tools_rs::builtin::AbortSignal;
      use tools_rs::builtin::collaboration::{
          QuestionAnswers, QuestionItem, QuestionOption, QuestionRequest, QuestionResult,
      };

      #[tokio::test]
      async fn callback_receives_request_and_returns_answers() {
          let captured = Arc::new(Mutex::new(None::<QuestionRequest>));
          let captured_clone = Arc::clone(&captured);
          let callback: QuestionCallback = Arc::new(move |req, _signal| {
              *captured_clone.lock().unwrap() = Some(req);
              Box::pin(async move {
                  let mut answers = HashMap::new();
                  answers.insert("Pick a color?".into(), serde_json::json!("Red"));
                  Ok(QuestionResult::Answers(QuestionAnswers {
                      answers,
                      method: Some("enter".into()),
                  }))
              })
          });

          let provider = AgentQuestionProvider::new(callback);
          let result = provider
              .request_question(
                  QuestionRequest {
                      turn_id: Some(7),
                      tool_call_id: "call_q".into(),
                      questions: vec![QuestionItem {
                          question: "Pick a color?".into(),
                          header: "Style".into(),
                          options: vec![
                              QuestionOption { label: "Red".into(), description: "warm".into() },
                              QuestionOption { label: "Blue".into(), description: "cool".into() },
                          ],
                          multi_select: false,
                      }],
                  },
                  &AbortSignal::new(),
              )
              .await
              .unwrap();

          match result {
              QuestionResult::Answers(a) => {
                  assert_eq!(a.answers.get("Pick a color?"), Some(&serde_json::json!("Red")));
              }
              _ => panic!("expected answers"),
          }

          let req = captured.lock().unwrap().take().unwrap();
          assert_eq!(req.turn_id, Some(7));
          assert_eq!(req.tool_call_id, "call_q");
          assert_eq!(req.questions.len(), 1);
      }
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p agent-rs question_provider
  ```
  Expected failure: `AgentQuestionProvider`、`QuestionCallback` 未定义。

- [ ] Write the minimal implementation.
  创建 `rust-ody/crates/agent-rs/src/tool/collaboration/question_provider.rs`：
  ```rust
  use std::future::Future;
  use std::pin::Pin;
  use std::sync::Arc;

  use tools_rs::builtin::AbortSignal;
  use tools_rs::builtin::collaboration::{
      QuestionError, QuestionProvider, QuestionRequest, QuestionResult,
  };

  pub type QuestionCallback = Arc<
      dyn Fn(QuestionRequest, AbortSignal) -> Pin<Box<dyn Future<Output = Result<QuestionResult, QuestionError>> + Send>>
          + Send
          + Sync,
  >;

  pub struct AgentQuestionProvider {
      callback: QuestionCallback,
  }

  impl AgentQuestionProvider {
      pub fn new(callback: QuestionCallback) -> Self {
          Self { callback }
      }
  }

  #[async_trait::async_trait]
  impl QuestionProvider for AgentQuestionProvider {
      async fn request_question(
          &self,
          req: QuestionRequest,
          signal: &AbortSignal,
      ) -> Result<QuestionResult, QuestionError> {
          (self.callback)(req, signal.clone()).await
      }
  }
  ```

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p agent-rs question_provider
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/agent-rs/src/tool/collaboration/question_provider.rs \
         rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs
  git commit -m "feat(agent-rs): implement AgentQuestionProvider callback adapter"
  ```

### Task 4: 实现 `AgentSubagentHost` 与 `AgentBackgroundRegistrar`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/collaboration/subagent_host.rs`
- Create: `rust-ody/crates/agent-rs/src/tool/collaboration/background_registrar.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs`（re-export）
- Test: `rust-ody/crates/agent-rs/src/tool/collaboration/subagent_host.rs`（模块级测试）
- Test: `rust-ody/crates/agent-rs/src/tool/collaboration/background_registrar.rs`（模块级测试）

- [ ] Write the failing tests.
  在 `subagent_host.rs` 中先写测试：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::future::Future;
      use std::pin::Pin;
      use std::sync::{Arc, Mutex};
      use tools_rs::builtin::AbortSignal;
      use tools_rs::builtin::collaboration::{
          SubagentCompletion, SubagentOptions, SubagentResult, SubagentUsage,
      };

      #[tokio::test]
      async fn injected_run_fn_determines_completion() {
          let calls = Arc::new(Mutex::new(Vec::new()));
          let calls_clone = Arc::clone(&calls);
          let run_fn: SubagentRunFn = Arc::new(move |_parent, _prompt, _signal| {
              calls_clone.lock().unwrap().push(());
              Box::pin(async move {
                  Ok(SubagentResult {
                      result: "done".into(),
                      usage: Some(SubagentUsage {
                          input: 1,
                          output: 2,
                          cache_read: None,
                          cache_write: None,
                      }),
                  })
              })
          });

          let host = AgentSubagentHost::with_run_fn(run_fn);
          let handle = host
              .spawn(
                  "coder",
                  SubagentOptions {
                      parent_tool_call_id: "call_a".into(),
                      prompt: "do it".into(),
                      description: "test".into(),
                      run_in_background: false,
                      signal: AbortSignal::new(),
                  },
              )
              .await
              .unwrap();

          assert_eq!(handle.agent_id, "mock-agent");
          assert_eq!(handle.profile_name, "coder");
          let result = handle.completion.await.unwrap();
          assert_eq!(result.result, "done");
          assert_eq!(calls.lock().unwrap().len(), 1);
      }
  }
  ```
  在 `background_registrar.rs` 中先写测试：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::future::Future;
      use std::pin::Pin;
      use tools_rs::builtin::collaboration::{
          AgentCompletion, AgentTaskOptions, BackgroundError,
      };

      #[tokio::test]
      async fn registrar_without_manager_returns_unavailable() {
          let registrar = AgentBackgroundRegistrar::new(None);
          let completion: AgentCompletion = Box::pin(async move {
              Ok(tools_rs::builtin::collaboration::SubagentResult {
                  result: "x".into(),
                  usage: None,
              })
          });
          let result = registrar
              .register_agent_task(completion, "desc".into(), AgentTaskOptions {
                  timeout_ms: None,
                  agent_id: "a".into(),
                  subagent_type: "coder".into(),
                  abort: Arc::new(|| {}),
              })
              .await;
          assert!(matches!(result, Err(BackgroundError::Unavailable)));
      }
  }
  ```

- [ ] Run them and verify they FAIL.
  ```bash
  cd rust-ody && cargo test -p agent-rs agent_subagent_host
  cd rust-ody && cargo test -p agent-rs agent_background_registrar
  ```
  Expected failure: `AgentSubagentHost`、`AgentBackgroundRegistrar` 未定义。

- [ ] Write the minimal implementation.
  创建 `rust-ody/crates/agent-rs/src/tool/collaboration/subagent_host.rs`：
  ```rust
  use std::future::Future;
  use std::pin::Pin;
  use std::sync::{Arc, Weak};

  use crate::agent::{Agent, AgentContext, AgentType};
  use crate::agent_loop::events::DefaultLoopEventDispatcher;
  use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
  use crate::agent_loop::run_turn::run_turn;
  use crate::agent_loop::types::{LoopMessageBuilder, RunTurnInput};
  use crate::context::types::PromptOrigin;
  use kosong_rs::message::{ContentPart, Message};
  use kosong_rs::provider::AbortSignal as KosongAbortSignal;
  use tools_rs::builtin::AbortSignal as ToolsAbortSignal;
  use tools_rs::builtin::collaboration::{
      AgentCompletion, SubagentError, SubagentHandle, SubagentHost, SubagentOptions, SubagentResult,
      SubagentUsage,
  };

  pub type SubagentRunFn = Arc<
      dyn Fn(
              Weak<Agent>,
              String,
              ToolsAbortSignal,
          ) -> Pin<Box<dyn Future<Output = Result<SubagentResult, SubagentError>> + Send>>
          + Send
          + Sync,
  >;

  struct SharedLlm(Arc<dyn Llm>);

  #[async_trait::async_trait]
  impl Llm for SharedLlm {
      fn system_prompt(&self) -> &str { self.0.system_prompt() }
      fn model_name(&self) -> &str { self.0.model_name() }
      fn capability(&self) -> Option<&kosong_rs::provider::ModelCapability> { self.0.capability() }
      fn is_retryable_error(&self, error: &anyhow::Error) -> bool { self.0.is_retryable_error(error) }
      async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
          self.0.chat(params).await
      }
  }

  pub struct AgentSubagentHost {
      parent: Weak<Agent>,
      run_fn: SubagentRunFn,
  }

  impl AgentSubagentHost {
      pub fn new(parent: Weak<Agent>) -> Self {
          Self {
              parent,
              run_fn: Arc::new(default_run_child_turn),
          }
      }

      pub fn with_run_fn(run_fn: SubagentRunFn) -> Self {
          Self {
              parent: Weak::new(),
              run_fn,
          }
      }
  }

  #[async_trait::async_trait]
  impl SubagentHost for AgentSubagentHost {
      async fn spawn(&self, profile: &str, options: SubagentOptions) -> Result<SubagentHandle, SubagentError> {
          let agent_id = format!("subagent-{}", uuid::Uuid::new_v4());
          let completion = (self.run_fn)(self.parent.clone(), options.prompt.clone(), options.signal.clone());
          Ok(SubagentHandle {
              agent_id,
              profile_name: profile.into(),
              completion,
          })
      }

      async fn resume(&self, agent_id: &str, options: SubagentOptions) -> Result<SubagentHandle, SubagentError> {
          let completion = (self.run_fn)(self.parent.clone(), options.prompt.clone(), options.signal.clone());
          Ok(SubagentHandle {
              agent_id: agent_id.into(),
              profile_name: "coder".into(),
              completion,
          })
      }

      fn get_profile_name(&self, _agent_id: &str) -> Option<String> { None }
      fn background_task_timeout_ms(&self) -> u64 { 600_000 }
      fn cancel_all(&self, _reason: &str) {}
  }

  async fn default_run_child_turn(
      parent: Weak<Agent>,
      prompt: String,
      signal: ToolsAbortSignal,
  ) -> Result<SubagentResult, SubagentError> {
      let parent = parent.upgrade().ok_or_else(|| SubagentError::Unavailable)?;
      let child = crate::agent::AgentBuilder::new(
          format!("subagent-{}", uuid::Uuid::new_v4()),
          parent.kaos.clone(),
          parent.environment.clone(),
      )
      .agent_type(AgentType::Sub)
      .provider_resolver(parent.provider_resolver.clone())
      .llm_factory(parent.llm_factory.clone())
      .build()
      .await
      .map_err(|e| SubagentError::Message(format!("failed to build subagent: {}", e)))?;

      {
          let mode = child.active_mode();
          let ctx = child.contexts.get(&mode).expect("active context");
          let mut mem = ctx.lock().unwrap();
          mem.append_user_message(
              vec![ContentPart::Text { text: prompt }],
              PromptOrigin::UserInput,
          );
      }

      let kosong_signal = KosongAbortSignal::new();
      let abort_forwarder = {
          let mut watch = signal.clone();
          tokio::spawn(async move {
              while !watch.aborted() {
                  tokio::task::yield_now().await;
              }
              kosong_signal.abort();
          })
      };

      let dispatcher: std::sync::Arc<dyn crate::agent_loop::events::LoopEventDispatcher> =
          std::sync::Arc::new(DefaultLoopEventDispatcher::new(|_| async { Ok(()) }, None));
      let child_for_messages = child.clone();
      let build_messages: LoopMessageBuilder = std::sync::Arc::new(move || {
          let child = child_for_messages.clone();
          Box::pin(async move { Ok(child.context().messages()) })
      });

      let result = run_turn(RunTurnInput {
          turn_id: uuid::Uuid::new_v4().to_string(),
          signal: kosong_signal.clone(),
          llm: Box::new(SharedLlm(child.llm_resolver().llm())),
          build_messages,
          dispatch_event: dispatcher,
          tools: Some(child.tools().loop_tools()),
          hooks: None,
          max_steps: Some(10),
          max_retry_attempts: Some(3),
          record_step_usage: None,
      })
      .await;

      abort_forwarder.abort();

      match result {
          Ok(_turn) => Ok(SubagentResult {
              result: "Subagent turn completed.".into(),
              usage: None,
          }),
          Err(e) => Err(SubagentError::Message(format!("subagent turn failed: {}", e))),
      }
  }
  ```
  创建 `rust-ody/crates/agent-rs/src/tool/collaboration/background_registrar.rs`：
  ```rust
  use std::future::Future;
  use std::pin::Pin;
  use std::sync::{Arc, Mutex};

  use crate::background::manager::BackgroundManager;
  use crate::background::types::{
      BackgroundTaskBase, BackgroundTaskKind, BackgroundTaskSettlement, BackgroundTaskStatus,
  };
  use crate::background::BackgroundTask;
  use tools_rs::builtin::collaboration::{
      AgentCompletion, AgentTaskOptions, BackgroundError, BackgroundRegistrar,
      QuestionRunFn, QuestionTaskOptions,
  };

  pub struct AgentBackgroundRegistrar {
      manager: Mutex<Option<Arc<BackgroundManager>>>,
  }

  impl AgentBackgroundRegistrar {
      pub fn new(manager: Option<Arc<BackgroundManager>>) -> Self {
          Self {
              manager: Mutex::new(manager),
          }
      }
  }

  #[async_trait::async_trait]
  impl BackgroundRegistrar for AgentBackgroundRegistrar {
      async fn register_question_task(
          &self,
          description: String,
          run: QuestionRunFn,
          options: QuestionTaskOptions,
      ) -> Result<String, BackgroundError> {
          let manager = self.manager.lock().unwrap().clone();
          let Some(manager) = manager else {
              return Err(BackgroundError::Unavailable);
          };

          let task: Box<dyn BackgroundTask> = Box::new(QuestionBackgroundTask {
              base: BackgroundTaskBase {
                  id: crate::background::types::BackgroundTaskId::new(""),
                  kind: BackgroundTaskKind::Question,
                  description,
                  timeout_ms: None,
              },
              run,
              options,
          });
          Ok(manager.register_task(task))
      }

      async fn register_agent_task(
          &self,
          completion: AgentCompletion,
          description: String,
          options: AgentTaskOptions,
      ) -> Result<String, BackgroundError> {
          let manager = self.manager.lock().unwrap().clone();
          let Some(manager) = manager else {
              return Err(BackgroundError::Unavailable);
          };

          let abort = options.abort.clone();
          let task: Box<dyn BackgroundTask> = Box::new(AgentBackgroundTask {
              base: BackgroundTaskBase {
                  id: crate::background::types::BackgroundTaskId::new(""),
                  kind: BackgroundTaskKind::Agent,
                  description,
                  timeout_ms: options.timeout_ms,
              },
              completion,
              abort,
          });
          Ok(manager.register_task(task))
      }
  }

  struct QuestionBackgroundTask {
      base: BackgroundTaskBase,
      run: QuestionRunFn,
      options: tools_rs::builtin::collaboration::QuestionTaskOptions,
  }

  impl BackgroundTask for QuestionBackgroundTask {
      fn base(&self) -> &BackgroundTaskBase { &self.base }
      fn set_id(&mut self, id: crate::background::types::BackgroundTaskId) { self.base.id = id; }

      async fn run(
          &self,
          sink: Arc<dyn crate::background::types::BackgroundTaskSink>,
          mut stop: tokio::sync::watch::Receiver<bool>,
      ) -> BackgroundTaskSettlement {
          let signal = tools_rs::builtin::AbortSignal::new();
          let run = self.run.clone();
          let result = tokio::select! {
              biased;
              _ = stop.changed() => {
                  signal.abort();
                  Ok(tools_rs::builtin::ExecutableToolResult::error_text(
                      "Cancelled".into(),
                      "Cancelled".into(),
                  ))
              }
              r = run(signal) => r,
          };
          let output = match result {
              Ok(res) => res.to_text(),
              Err(e) => format!("error: {}", e),
          };
          sink.append_output(&output);
          BackgroundTaskSettlement {
              status: BackgroundTaskStatus::Completed,
              stop_reason: None,
          }
      }
  }

  struct AgentBackgroundTask {
      base: BackgroundTaskBase,
      completion: AgentCompletion,
      abort: Arc<dyn Fn() + Send + Sync>,
  }

  impl BackgroundTask for AgentBackgroundTask {
      fn base(&self) -> &BackgroundTaskBase { &self.base }
      fn set_id(&mut self, id: crate::background::types::BackgroundTaskId) { self.base.id = id; }

      async fn run(
          &self,
          sink: Arc<dyn crate::background::types::BackgroundTaskSink>,
          mut stop: tokio::sync::watch::Receiver<bool>,
      ) -> BackgroundTaskSettlement {
          let completion = self.completion.clone();
          let abort = self.abort.clone();
          let result = tokio::select! {
              biased;
              _ = stop.changed() => {
                  abort();
                  Ok(tools_rs::builtin::collaboration::SubagentResult {
                      result: "Cancelled".into(),
                      usage: None,
                  })
              }
              r = completion => r,
          };
          match result {
              Ok(res) => {
                  sink.append_output(&res.result);
                  BackgroundTaskSettlement {
                      status: BackgroundTaskStatus::Completed,
                      stop_reason: None,
                  }
              }
              Err(e) => BackgroundTaskSettlement {
                  status: BackgroundTaskStatus::Failed,
                  stop_reason: Some(format!("{:?}", e)),
              },
          }
      }
  }
  ```
  注意：
  - `AgentSubagentHost::with_run_fn` 主要用于测试注入。
  - `default_run_child_turn` 使用 `child.context().messages()` 作为 `build_messages` 的返回值；`ContextMemory::messages()` 在 `ContextAgent` 的默认实现下可正常工作。
  - `BackgroundManager::register_task` 会内部生成 task_id 并启动任务。

- [ ] Run them and verify they PASS.
  ```bash
  cd rust-ody && cargo test -p agent-rs agent_subagent_host
  cd rust-ody && cargo test -p agent-rs agent_background_registrar
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/agent-rs/src/tool/collaboration/subagent_host.rs \
         rust-ody/crates/agent-rs/src/tool/collaboration/background_registrar.rs \
         rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs
  git commit -m "feat(agent-rs): implement AgentSubagentHost and AgentBackgroundRegistrar"
  ```

### Task 5: 接入 `AgentBuilder/Agent` 并暴露 `loop_tools`

**Depends on:** Task 2, Task 3, Task 4

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent.rs:216-415`（`Agent` 字段、`AgentBuilder` 字段、`build()`、`loop_tools()`）
- Modify: `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs`（暴露 `CollaborationToolkit` 构造辅助）
- Test: `rust-ody/crates/agent-rs/tests/agent_collaboration_loop_tools.rs`

- [ ] Write the failing test.
  创建 `rust-ody/crates/agent-rs/tests/agent_collaboration_loop_tools.rs`：
  ```rust
  use std::collections::HashMap;
  use std::future::Future;
  use std::pin::Pin;
  use std::sync::{Arc, Mutex};

  use agent_rs::agent::{AgentBuilder, AgentEnvironment, AgentType};
  use agent_rs::skill::{InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillSource};
  use agent_rs::tool::collaboration::QuestionCallback;
  use kaos_rs::environment::detect_environment_from_node;
  use kaos_rs::kaos::Kaos;
  use kosong_rs::message::ContentPart;
  use kosong_rs::provider::AbortSignal;

  struct NoopEnv;
  #[async_trait::async_trait]
  impl AgentEnvironment for NoopEnv {
      fn emit_event(&self, _event: agent_rs::turn::types::AgentEvent) {}
      async fn request_approval(
          &self,
          _req: &agent_rs::permission::types::ApprovalRequest,
          _signal: AbortSignal,
      ) -> Result<agent_rs::records::nested::ApprovalResponse, anyhow::Error> {
          Ok(agent_rs::records::nested::ApprovalResponse {
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
      ) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
          Box::pin(async { Ok(None) })
      }
      fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
      fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
      fn fire_hook_user_prompt_submit(
          &self,
          _input: Vec<ContentPart>,
          _signal: AbortSignal,
      ) -> Pin<Box<dyn Future<Output = Result<Vec<agent_rs::turn::types::HookResult>, anyhow::Error>> + Send + '_>> {
          Box::pin(async { Ok(vec![]) })
      }
      fn fire_hook_stop_hook(
          &self,
          _signal: AbortSignal,
      ) -> Pin<Box<dyn Future<Output = Result<Option<agent_rs::turn::types::StopHookBlock>, anyhow::Error>> + Send + '_>> {
          Box::pin(async { Ok(None) })
      }
      fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
  }

  fn sample_skill() -> SkillDefinition {
      SkillDefinition {
          name: "refactor".into(),
          description: "".into(),
          path: "/skills/refactor.md".into(),
          dir: "/skills".into(),
          content: "Refactor this code.".into(),
          metadata: SkillMetadata {
              skill_type: Some("prompt".into()),
              ..SkillMetadata::default()
          },
          source: SkillSource::Project,
          plugin: None,
          mermaid: None,
          d2: None,
      }
  }

  #[tokio::test]
  async fn loop_tools_includes_collaboration_tools_when_configured() {
      let mut registry = InMemorySkillRegistry::new();
      registry.register(sample_skill());

      let callback: QuestionCallback = Arc::new(|_req, _signal| {
          Box::pin(async move {
              Ok(tools_rs::builtin::collaboration::QuestionResult::Dismissed)
          })
      });

      let kaos = Arc::new(Kaos::new(detect_environment_from_node()));
      let agent = AgentBuilder::new("test", kaos, Arc::new(NoopEnv))
          .skills_registry(Box::new(registry))
          .question_callback(callback)
          .build()
          .await
          .unwrap();

      let tools = agent.tools().loop_tools();
      let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
      assert!(names.contains(&"Skill"), "got {:?}", names);
      assert!(names.contains(&"AskUserQuestion"), "got {:?}", names);
      // AgentTool 需要 subagent_host；未配置时不应出现
      assert!(!names.contains(&"Agent"));
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p agent-rs loop_tools_includes_collaboration_tools
  ```
  Expected failure: `AgentBuilder::skills_registry` 不保存 registry；`question_callback`、`Agent::loop_tools()` 未返回协作工具。

- [ ] Write the minimal implementation.
  1. 在 `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs` 中增加 toolkit 构造辅助：
     ```rust
     use std::sync::{Arc, Weak};
     use crate::agent::{Agent, AgentContext};
     use crate::background::manager::BackgroundManager;
     use crate::tool::bridge::ToolBridge;
     use tools_rs::builtin::collaboration::{
         AskUserQuestionOptions, AskUserQuestionTool, SkillTool, SkillToolOptions,
     };
     use tools_rs::builtin::AgentTool;

     pub struct CollaborationToolkit;

     impl CollaborationToolkit {
         pub fn build_tools(
             context: AgentContext,
             skill_registry: Option<Box<dyn crate::skill::registry::SkillRegistry>>,
             question_callback: Option<QuestionCallback>,
             subagent_host: Option<Arc<dyn tools_rs::builtin::collaboration::SubagentHost>>,
             background_manager: Option<Arc<BackgroundManager>>,
         ) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
             let mut tools: Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> = Vec::new();

             if let Some(registry) = skill_registry {
                 let provider = Arc::new(AgentSkillProvider::new(context.weak(), registry));
                 tools.push(Arc::new(ToolBridge::new(Arc::new(SkillTool::new(
                     provider,
                     SkillToolOptions::default(),
                 )))) as Arc<dyn crate::agent_loop::types::ExecutableTool>);
             }

             if let Some(callback) = question_callback {
                 let provider = Arc::new(AgentQuestionProvider::new(callback));
                 let registrar = Arc::new(AgentBackgroundRegistrar::new(background_manager.clone()));
                 tools.push(Arc::new(ToolBridge::new(Arc::new(AskUserQuestionTool::new(
                     provider,
                     registrar,
                     AskUserQuestionOptions::default(),
                 )))) as Arc<dyn crate::agent_loop::types::ExecutableTool>);
             }

             if let Some(host) = subagent_host {
                 let registrar = Arc::new(AgentBackgroundRegistrar::new(background_manager));
                 tools.push(Arc::new(ToolBridge::new(Arc::new(AgentTool::new(
                     host,
                     Some(registrar),
                     tools_rs::builtin::AgentToolOptions::default(),
                 )))) as Arc<dyn crate::agent_loop::types::ExecutableTool>);
             }

             tools
         }
     }
     ```
  2. 修改 `rust-ody/crates/agent-rs/src/agent.rs`：
     - 在 `Agent` struct 中新增字段（约第 245 行附近）：
       ```rust
       pub skill_registry: Mutex<Option<Arc<Box<dyn crate::skill::registry::SkillRegistry>>>>,
       pub question_callback: Mutex<Option<crate::tool::collaboration::QuestionCallback>>,
       pub subagent_host: Mutex<Option<Arc<dyn tools_rs::builtin::collaboration::SubagentHost>>>,
       pub self_weak: std::sync::Weak<Agent>,
       ```
     - 在 `AgentBuilder` struct 中新增字段（约第 257 行）：
       ```rust
       skill_registry: Option<Arc<Box<dyn crate::skill::registry::SkillRegistry>>>,
       question_callback: Option<crate::tool::collaboration::QuestionCallback>,
       subagent_host: Option<Arc<dyn tools_rs::builtin::collaboration::SubagentHost>>,
       ```
     - 在 `AgentBuilder::new` 中初始化这三个字段为 `None`。
     - 修改 `AgentBuilder::skills_registry`（约第 304 行）：
       ```rust
       pub fn skills_registry(mut self, r: Box<dyn crate::skill::registry::SkillRegistry>) -> Self {
           self.skill_registry = Some(Arc::new(r));
           self
       }
       ```
     - 新增 setter：
       ```rust
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
       ```
     - 在 `AgentBuilder::build()` 的 `Arc::new_cyclic(|weak| { ... })` 闭包内：
       - 把 `ctx = AgentContext { agent: weak.clone() }` 保留。
       - 在构造 `Agent { ... }` 时加入：
         ```rust
         skill_registry: Mutex::new(self.skill_registry),
         question_callback: Mutex::new(self.question_callback),
         subagent_host: Mutex::new(self.subagent_host),
         self_weak: weak.clone(),
         ```
     - 在 `AgentContext` 的 `impl AgentContext` 块中新增（让 collaboration 模块能拿到 weak 指针）：
       ```rust
       pub(crate) fn weak(&self) -> std::sync::Weak<Agent> {
           self.agent.clone()
       }
       ```
     - 在 `Agent` 的 `impl Agent` 块中新增：
       ```rust
       pub fn agent_context(&self) -> AgentContext {
           AgentContext { agent: self.self_weak.clone() }
       }
       ```
     - 修改 `Agent::loop_tools()`（约第 953 行）为：
       ```rust
       fn loop_tools(&self) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
           let context = self.agent_context();
           let background = self.background.lock().unwrap().clone();
           crate::tool::collaboration::CollaborationToolkit::build_tools(
               context,
               self.skill_registry.lock().unwrap().clone(),
               self.question_callback.lock().unwrap().clone(),
               self.subagent_host.lock().unwrap().clone(),
               background,
           )
       }
       ```
       注意：`skill_registry` 字段类型应为 `Mutex<Option<Arc<Box<dyn crate::skill::registry::SkillRegistry>>>>` 或 `Mutex<Option<Box<dyn crate::skill::registry::SkillRegistry>>>` 只要能 `clone()`。由于 `Box<dyn SkillRegistry>` 未实现 `Clone`，推荐包成 `Arc<Box<dyn SkillRegistry>>`：
       - `AgentBuilder` 字段：`Option<Arc<Box<dyn crate::skill::registry::SkillRegistry>>>`
       - `Agent` 字段：`Mutex<Option<Arc<Box<dyn crate::skill::registry::SkillRegistry>>>>`
       - `build()` 中：`skill_registry: Mutex::new(self.skill_registry),`

- [ ] Find and update every caller.
  - 搜索 `AgentBuilder::skills_registry(`：
    ```bash
    grep -rn "skills_registry(" rust-ody/crates/agent-rs/src rust-ody/crates/agent-rs/tests
    ```
    当前只有 `agent.rs:304` 的定义本身，无需更新外部调用者。
  - 搜索 `Agent::loop_tools` 和 `TurnTools::loop_tools` 的实现：
    ```bash
    grep -rn "fn loop_tools" rust-ody/crates/agent-rs/src rust-ody/crates/agent-rs/tests
    ```
    需要保证 `fixture_agent.rs` 等 mock 实现签名不变；本任务只修改 `Agent` 的实现。

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p agent-rs loop_tools_includes_collaboration_tools
  ```

- [ ] Whole-tree typecheck（共享签名变更）。
  ```bash
  cd rust-ody && cargo test
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/agent-rs/src/agent.rs \
         rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs \
         rust-ody/crates/agent-rs/tests/agent_collaboration_loop_tools.rs
  git commit -m "feat(agent-rs): wire collaboration tools into Agent::loop_tools"
  ```

### Task 6: L3 行为一致性测试

**Depends on:** Task 5

**Files:**
- Create: `rust-ody/crates/agent-rs/tests/collaboration_l3.rs`

- [ ] Write the failing test.
  创建 `rust-ody/crates/agent-rs/tests/collaboration_l3.rs`：
  ```rust
  use std::collections::HashMap;
  use std::future::Future;
  use std::pin::Pin;
  use std::sync::{Arc, Mutex};

  use agent_rs::agent::{AgentBuilder, AgentEnvironment};
  use agent_rs::agent_loop::events::DefaultLoopEventDispatcher;
  use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
  use agent_rs::agent_loop::run_turn::run_turn;
  use agent_rs::agent_loop::types::{LoopTurnStopReason, RunTurnInput};
  use agent_rs::skill::{
      registry::SkillRegistry, InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillSource,
  };
  use agent_rs::tool::collaboration::{
      AgentSubagentHost, QuestionCallback, SubagentRunFn,
  };
  use kaos_rs::environment::detect_environment_from_node;
  use kaos_rs::kaos::Kaos;
  use kosong_rs::message::{ContentPart, ToolCall};
  use kosong_rs::provider::{AbortSignal, FinishReason};
  use kosong_rs::usage::TokenUsage;

  struct NoopEnv;
  #[async_trait::async_trait]
  impl AgentEnvironment for NoopEnv {
      fn emit_event(&self, _event: agent_rs::turn::types::AgentEvent) {}
      async fn request_approval(
          &self,
          _req: &agent_rs::permission::types::ApprovalRequest,
          _signal: AbortSignal,
      ) -> Result<agent_rs::records::nested::ApprovalResponse, anyhow::Error> {
          Ok(agent_rs::records::nested::ApprovalResponse {
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
      ) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
          Box::pin(async { Ok(None) })
      }
      fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
      fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
      fn fire_hook_user_prompt_submit(
          &self,
          _input: Vec<ContentPart>,
          _signal: AbortSignal,
      ) -> Pin<Box<dyn Future<Output = Result<Vec<agent_rs::turn::types::HookResult>, anyhow::Error>> + Send + '_>> {
          Box::pin(async { Ok(vec![]) })
      }
      fn fire_hook_stop_hook(
          &self,
          _signal: AbortSignal,
      ) -> Pin<Box<dyn Future<Output = Result<Option<agent_rs::turn::types::StopHookBlock>, anyhow::Error>> + Send + '_>> {
          Box::pin(async { Ok(None) })
      }
      fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
  }

  struct CountingRegistry {
      skills: HashMap<String, SkillDefinition>,
      calls: Mutex<Vec<String>>,
  }

  impl CountingRegistry {
      fn new(skills: Vec<SkillDefinition>) -> Self {
          let map = skills.into_iter().map(|s| (s.name.clone(), s)).collect();
          Self { skills: map, calls: Mutex::new(Vec::new()) }
      }
  }

  impl Clone for CountingRegistry {
      fn clone(&self) -> Self {
          Self {
              skills: self.skills.clone(),
              calls: Mutex::new(self.calls.lock().unwrap().clone()),
          }
      }
  }

  impl SkillRegistry for CountingRegistry {
      fn get_skill(&self, name: &str) -> Option<&SkillDefinition> {
          self.calls.lock().unwrap().push(name.to_string());
          self.skills.get(name)
      }
      fn list_skills(&self) -> Vec<&SkillDefinition> {
          let mut v: Vec<_> = self.skills.values().collect();
          v.sort_by(|a, b| a.name.cmp(&b.name));
          v
      }
      fn list_invocable_skills(&self, session_mode: Option<&str>) -> Vec<&SkillDefinition> {
          self.list_skills()
              .into_iter()
              .filter(|skill| {
                  if skill.metadata.disable_model_invocation == Some(true) {
                      return false;
                  }
                  let inline = matches!(
                      skill.metadata.skill_type.as_deref(),
                      None | Some("prompt") | Some("inline")
                  );
                  if !inline {
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

  struct SequencedLlm {
      step: Mutex<usize>,
  }

  #[async_trait::async_trait]
  impl Llm for SequencedLlm {
      fn system_prompt(&self) -> &str { "" }
      fn model_name(&self) -> &str { "sequenced" }
      async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
          let step = { let mut s = self.step.lock().unwrap(); *s += 1; *s };
          let tool_call = |name: &str, args: serde_json::Value| ToolCall {
              call_type: "tool_call".into(),
              id: format!("call_{}", name.to_lowercase()),
              name: name.into(),
              arguments: Some(serde_json::to_string(&args).unwrap()),
              extras: None,
              stream_index: None,
          };
          let (tool_calls, finish) = match step {
              1 => (vec![tool_call("Skill", serde_json::json!({"skill":"refactor"}))], FinishReason::ToolUse),
              2 => (vec![tool_call("AskUserQuestion", serde_json::json!({"questions":[{"question":"Pick a color?","header":"Style","options":[{"label":"Red","description":"warm"},{"label":"Blue","description":"cool"}],"multi_select":false}]}))], FinishReason::ToolUse),
              3 => (vec![tool_call("Agent", serde_json::json!({"prompt":"do it","description":"sub"}))], FinishReason::ToolUse),
              _ => (vec![], FinishReason::Completed),
          };
          Ok(LlmChatResponse {
              tool_calls,
              provider_finish_reason: Some(finish),
              raw_finish_reason: Some(format!("{:?}", finish)),
              usage: TokenUsage::default(),
              stream_timing: None,
          })
      }
  }

  fn sample_skill() -> SkillDefinition {
      SkillDefinition {
          name: "refactor".into(),
          description: "".into(),
          path: "/skills/refactor.md".into(),
          dir: "/skills".into(),
          content: "Refactor this code.".into(),
          metadata: SkillMetadata {
              skill_type: Some("prompt".into()),
              ..SkillMetadata::default()
          },
          source: SkillSource::Project,
          plugin: None,
          mermaid: None,
          d2: None,
      }
  }

  #[tokio::test]
  async fn collaboration_tools_execute_in_turn() {
      let counting_registry = Arc::new(CountingRegistry::new(vec![sample_skill()]));

      let question_calls = Arc::new(Mutex::new(Vec::new()));
      let question_calls_clone = Arc::clone(&question_calls);
      let callback: QuestionCallback = Arc::new(move |req, _signal| {
          question_calls_clone.lock().unwrap().push(req.tool_call_id.clone());
          let mut answers = HashMap::new();
          answers.insert("Pick a color?".into(), serde_json::json!("Red"));
          Box::pin(async move {
              Ok(tools_rs::builtin::collaboration::QuestionResult::Answers(
                  tools_rs::builtin::collaboration::QuestionAnswers {
                      answers,
                      method: Some("enter".into()),
                  },
              ))
          })
      });

      let spawn_calls = Arc::new(Mutex::new(Vec::new()));
      let spawn_calls_clone = Arc::clone(&spawn_calls);
      let run_fn: SubagentRunFn = Arc::new(move |_parent, prompt, _signal| {
          spawn_calls_clone.lock().unwrap().push(prompt.clone());
          Box::pin(async move {
              Ok(tools_rs::builtin::collaboration::SubagentResult {
                  result: "subagent done".into(),
                  usage: None,
              })
          })
      });
      let host = AgentSubagentHost::with_run_fn(run_fn);

      let kaos = Arc::new(Kaos::new(detect_environment_from_node()));
      let agent = AgentBuilder::new("l3-test", kaos, Arc::new(NoopEnv))
          .skills_registry(Box::new(counting_registry.clone()))
          .question_callback(callback)
          .subagent_host(Arc::new(host))
          .build()
          .await
          .unwrap();

      {
          let mode = agent.active_mode();
          let ctx = agent.contexts.get(&mode).expect("context");
          let mut mem = ctx.lock().unwrap();
          mem.append_user_message(
              vec![ContentPart::Text { text: "use all collaboration tools".into() }],
              agent_rs::context::types::PromptOrigin::UserInput,
          );
      }

      let dispatcher = DefaultLoopEventDispatcher::new(|_| async { Ok(()) }, None);
      let result = run_turn(RunTurnInput {
          turn_id: "turn-1".into(),
          signal: AbortSignal::new(),
          llm: Box::new(SequencedLlm { step: Mutex::new(0) }),
          build_messages: {
              let agent = agent.clone();
              Arc::new(move || {
                  let agent = agent.clone();
                  Box::pin(async move { Ok(agent.context().messages()) })
              })
          },
          dispatch_event: Arc::new(dispatcher),
          tools: Some(agent.tools().loop_tools()),
          hooks: None,
          max_steps: Some(10),
          max_retry_attempts: Some(3),
          record_step_usage: None,
      })
      .await
      .unwrap();

      assert_eq!(result.stop_reason, LoopTurnStopReason::EndTurn);
      assert!(counting_registry.calls.lock().unwrap().iter().any(|n| n == "refactor"));
      assert!(!question_calls.lock().unwrap().is_empty());
      assert!(spawn_calls.lock().unwrap().iter().any(|p| p == "do it"));
  }
  ```
. 注意：
  - `CountingRegistry` 已内置 `Clone`，因此可以同时保留一份用于断言、传一份给 `AgentBuilder`。

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p agent-rs collaboration_tools_execute_in_turn
  ```
  Expected failure: 测试文件引用尚未完全导出的类型，或 `AgentBuilder` 方法缺失。

- [ ] Write the minimal implementation.
  无需新增实现代码；本测试验证 Task 5 的 wiring。若编译失败仅因类型/方法未导出，在 Task 5 中补充对应 `pub use` 或 `pub fn` 即可。

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p agent-rs collaboration_tools_execute_in_turn
  ```

- [ ] Whole-tree typecheck。
  ```bash
  cd rust-ody && cargo test
  ```

- [ ] Commit。
  ```bash
  git add rust-ody/crates/agent-rs/tests/collaboration_l3.rs
  git commit -m "test(agent-rs): add L3 collaboration tool parity scenario"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage table: Task 1 覆盖依赖/模块脚手架；Task 2 覆盖 `SkillProvider` agent 适配；Task 3 覆盖 `QuestionProvider` 回调适配；Task 4 覆盖 `SubagentHost` + `BackgroundRegistrar` 适配；Task 5 覆盖 `AgentBuilder/Agent` 接入与 `loop_tools` 注册；Task 6 覆盖 L3 行为一致性。无 GAP。
- [ ] 2. Placeholder scan: 无 TODO/TBD；所有代码块均为可编译的具体实现；所有依赖均指向前序 Part 1–4 已定义的 trait/类型。
- [ ] 3. No phantom tasks: 每个 task 都创建/修改文件并伴随测试与 commit；无 `--allow-empty` 或 "already done"。
- [ ] 4. Dependency soundness: Task 1（基础设施）→ Task 2/3/4（独立并行）→ Task 5（接入）→ Task 6（L3）。无后向依赖。
- [ ] 5. Caller & build soundness: Task 5 修改 `AgentBuilder::skills_registry` 语义并新增 builder 字段；已搜索调用者（仅定义本身），并在任务内完成 `cargo test` 全工作区类型检查。`Agent::loop_tools` 仅修改 `Agent` 实现，不影响 mock 的签名。
- [ ] 6. Test-the-risk: Task 2 测试 skill 查询映射和 activation 事件；Task 3 测试 question callback 收到请求；Task 4 测试 subagent completion 与 background registrar 的 unavailable 路径；Task 5 测试 `loop_tools` 按配置返回工具；Task 6 用 mock LLM 验证三个协作工具在一次 turn 中都被实际调用。
- [ ] 7. Type consistency: `AgentSkillProvider::new`、`AgentSubagentHost`、`AgentBackgroundRegistrar`、回调类型、以及 `AgentBuilder` 字段类型与 Part 1–4 定义的 trait 签名一致；`QuestionRunFn` 使用 Part 3 修正后的 `AbortSignal` 签名。
