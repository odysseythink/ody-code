# Phase 4.4.4 Part 1 — Shared Context + Trait Boundaries + Tool Bridge

**Goal:** Extend the tool execution context with turn/tool-call identity, define the minimal agent-capability traits that collaboration tools consume, and bridge `tools-rs::BuiltinTool` into `agent-rs::agent_loop::ExecutableTool`.

**Architecture:** Add `turn_id`/`tool_call_id` to `ExecutableToolContext` in both crates so collaboration tools can report telemetry and background task lineage. Introduce a small `tools-rs::builtin::collaboration` module with trait objects for `SkillProvider`, `QuestionProvider`, `SubagentHost`, and `BackgroundRegistrar`. In `agent-rs`, implement a `ToolBridge` adapter that wraps a `tools-rs::BuiltinTool` and satisfies the `agent_loop::ExecutableTool` trait so existing `ToolManager` metadata and loop execution can coexist.

**Tech Stack:** Rust 2021, `async-trait`, `serde_json`, existing `tools-rs`/`agent-rs` crates.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/tools-rs/
  Cargo.toml                                  # add async-trait dependency
  src/builtin/mod.rs                          # extend ExecutableToolContext + re-export collaboration
  src/builtin/collaboration/mod.rs            # trait boundaries
rust-ody/crates/agent-rs/
  Cargo.toml                                  # add tools-rs dependency
  src/agent_loop/types.rs                     # extend ExecutableToolContext
  src/tool/bridge.rs                          # NEW: BuiltinTool → ExecutableTool bridge
  src/tool/mod.rs                             # expose bridge
  src/bin/loop_l3.rs                          # update direct ExecutableToolContext construction
  src/turn/fixture_agent.rs                   # update direct ExecutableToolContext construction
```

## Dependency Overview

```
Task 1: Extend ExecutableToolContext (shared signature)
  │
  ▼
Task 2: Add tools-rs → agent_loop bridge
  │
  ▼
Task 3: Define collaboration trait boundaries
```

- **Task 1** is a shared-signature change: it touches both `tools-rs` and `agent-rs` and must update every caller, ending with a whole-workspace typecheck.
- **Task 2** depends on Task 1 because the bridge constructs `ExecutableToolContext`.
- **Task 3** is independent of Task 2 (pure trait definitions) but is grouped here because both are foundation for Parts 2–4.

## Tasks

### Task 1: Extend `ExecutableToolContext` with `turn_id` and `tool_call_id`

**Depends on:** none

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs:33-36` (struct fields)
- Modify: `rust-ody/crates/agent-rs/src/agent_loop/types.rs:19-25` (struct fields)
- Modify: `rust-ody/crates/agent-rs/src/agent_loop/tool_call.rs:663-674` (construction site)
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs` (any direct construction)
- Modify: `rust-ody/crates/agent-rs/src/bin/loop_l3.rs` (any direct construction)
- Test: `rust-ody/crates/tools-rs/src/builtin/mod.rs:119-151` (existing tests, add new)
- Test: `rust-ody/crates/agent-rs/src/agent_loop/tool_call.rs` (existing tests)

- [ ] Write the failing test.
  Add a unit test in `tools-rs/src/builtin/mod.rs` that constructs `ExecutableToolContext` with explicit `turn_id` and `tool_call_id` and asserts round-trip:
  ```rust
  #[test]
  fn context_carries_turn_and_tool_call_id() {
      let ctx = ExecutableToolContext {
          turn_id: "42".into(),
          tool_call_id: "call_abc".into(),
          signal: AbortSignal::new(),
      };
      assert_eq!(ctx.turn_id, "42");
      assert_eq!(ctx.tool_call_id, "call_abc");
      assert!(!ctx.signal.aborted());
  }
  ```
  Add a matching test in `agent-rs/src/agent_loop/types.rs` (inside the existing `tests` module):
  ```rust
  #[test]
  fn executable_tool_context_carries_ids() {
      let ctx = ExecutableToolContext {
          turn_id: "7".into(),
          tool_call_id: "call_xyz".into(),
          metadata: None,
          signal: kosong_rs::provider::AbortSignal::new(),
          on_update: None,
      };
      assert_eq!(ctx.turn_id, "7");
      assert_eq!(ctx.tool_call_id, "call_xyz");
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p tools-rs context_carries_turn_and_tool_call_id
  cd rust-ody && cargo test -p agent-rs executable_tool_context_carries_ids
  ```
  Expected failure: field `turn_id`/`tool_call_id` does not exist on `ExecutableToolContext`.

- [ ] Write the minimal implementation.
  In `tools-rs/src/builtin/mod.rs`:
  ```rust
  #[derive(Debug, Clone)]
  pub struct ExecutableToolContext {
      pub turn_id: String,
      pub tool_call_id: String,
      pub signal: AbortSignal,
  }
  ```
  In `agent-rs/src/agent_loop/types.rs`:
  ```rust
  pub struct ExecutableToolContext {
      pub turn_id: String,
      pub tool_call_id: String,
      pub metadata: Option<JsonValue>,
      pub signal: AbortSignal,
      pub on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
  }
  ```
  Find and update every caller. Use `grep -rn "ExecutableToolContext {" rust-ody/crates/agent-rs/src` and `grep -rn "ExecutableToolContext {" rust-ody/crates/tools-rs/src`. Known sites:
  - `agent-rs/src/agent_loop/tool_call.rs:663` constructs the context for `run_runnable_tool_call`; pass `step.turn_id.clone()` and `tool_call_id.clone()`.
  - `tools-rs/src/builtin/read.rs:487` (and other tools' tests) constructs `ExecutableToolContext { signal: ... }`; add `turn_id: "".into(), tool_call_id: "".into()`.
  - `agent-rs/src/turn/fixture_agent.rs:773` and `agent-rs/src/bin/loop_l3.rs:143` implement `ExecutableTool` and construct context inside `execute`; add `"".into()` defaults.

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p tools-rs context_carries_turn_and_tool_call_id
  cd rust-ody && cargo test -p agent-rs executable_tool_context_carries_ids
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/tools-rs/src/builtin/mod.rs \
         rust-ody/crates/agent-rs/src/agent_loop/types.rs \
         rust-ody/crates/agent-rs/src/agent_loop/tool_call.rs \
         rust-ody/crates/agent-rs/src/turn/fixture_agent.rs \
         rust-ody/crates/agent-rs/src/bin/loop_l3.rs
  git commit -m "feat(agent-rs,tools-rs): add turn_id and tool_call_id to ExecutableToolContext"
  ```

### Task 2: Add `tools-rs` → `agent_loop::ExecutableTool` bridge

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/Cargo.toml:24-30` (add dependency)
- Create: `rust-ody/crates/agent-rs/src/tool/bridge.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/mod.rs:1-5` (expose bridge)
- Modify: `rust-ody/crates/agent-rs/src/tool/manager.rs:1-50` (use bridge if needed)
- Test: `rust-ody/crates/agent-rs/src/tool/bridge.rs` (module-level tests)

- [ ] Write the failing test.
  In `agent-rs/src/tool/bridge.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use serde_json::json;
      use tools_rs::builtin::{AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolExecution};
      use tools_rs::tool_accesses::ToolAccesses;

      struct EchoTool;
      impl BuiltinTool for EchoTool {
          fn name(&self) -> &str { "Echo" }
          fn description(&self) -> &str { "echo" }
          fn parameters(&self) -> serde_json::Value { json!({"type":"object"}) }
          fn resolve_execution(&self, args: serde_json::Value) -> Result<ToolExecution, tools_rs::builtin::ToolError> {
              Ok(ToolExecution {
                  accesses: ToolAccesses::none(),
                  description: "echo".into(),
                  approval_rule: "Echo".into(),
                  execute: Box::new(move |ctx| {
                      let msg = format!("turn={} call={} arg={}", ctx.turn_id, ctx.tool_call_id, args["msg"].as_str().unwrap_or(""));
                      Box::pin(async move { Ok(ExecutableToolResult::ok_text(msg)) })
                  }),
              })
          }
      }

      #[tokio::test]
      async fn bridge_forwards_turn_and_call_id() {
          let bridge = ToolBridge::new(Arc::new(EchoTool));
          let exec = bridge.resolve_execution(json!({"msg":"hi"})).await.unwrap();
          let ctx = ExecutableToolContext {
              turn_id: "3".into(),
              tool_call_id: "call_1".into(),
              signal: kosong_rs::provider::AbortSignal::new(),
          };
          let result = (exec.execute)(ctx).await.unwrap();
          let text = match result {
              ExecutableToolResult::Success(s) => s.output.to_text(),
              ExecutableToolResult::Error(e) => panic!("unexpected error: {:?}", e),
          };
          assert_eq!(text, "turn=3 call=call_1 arg=hi");
      }
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p agent-rs bridge_forwards_turn_and_call_id
  ```
  Expected failure: `ToolBridge` does not exist; `tools-rs` crate not linked.

- [ ] Write the minimal implementation.
  Add to `agent-rs/Cargo.toml`:
  ```toml
  tools-rs = { path = "../tools-rs" }
  ```
  Create `agent-rs/src/tool/bridge.rs`:
  ```rust
  use std::future::Future;
  use std::pin::Pin;
  use std::sync::Arc;

  use crate::agent_loop::types::{ExecutableTool, ExecutableToolContext as LoopContext, ToolExecution as LoopToolExecution};
  use tools_rs::builtin::{BuiltinTool, ExecutableToolContext as ToolsContext, ExecutableToolResult as ToolsResult};

  pub struct ToolBridge {
      inner: Arc<dyn BuiltinTool>,
  }

  impl ToolBridge {
      pub fn new(inner: Arc<dyn BuiltinTool>) -> Self {
          Self { inner }
      }
  }

  #[async_trait::async_trait]
  impl ExecutableTool for ToolBridge {
      fn name(&self) -> &str { self.inner.name() }
      fn description(&self) -> &str { self.inner.description() }
      fn parameters(&self) -> serde_json::Value { self.inner.parameters() }
      async fn resolve_execution(&self, input: serde_json::Value) -> Result<LoopToolExecution, anyhow::Error> {
          let tools_exec = self.inner.resolve_execution(input)?;
          let execute = tools_exec.execute;
          Ok(LoopToolExecution {
              is_error: None,
              accesses: Some(tools_exec.accesses),
              display: None,
              description: Some(tools_exec.description),
              stop_batch_after_this: None,
              approval_rule: tools_exec.approval_rule,
              matches_rule: None,
              execute: Box::new(move |loop_ctx: LoopContext| {
                  let tools_ctx = ToolsContext {
                      turn_id: loop_ctx.turn_id,
                      tool_call_id: loop_ctx.tool_call_id,
                      signal: tools_rs::builtin::AbortSignal {
                          flag: loop_ctx.signal.inner().clone(),
                      },
                  };
                  let fut = execute(tools_ctx);
                  Box::pin(async move {
                      let result: ToolsResult = fut.await;
                      Ok(result.into())
                  }) as Pin<Box<dyn Future<Output = Result<crate::records::nested::ExecutableToolResult, anyhow::Error>> + Send>>
              }),
          })
      }
  }
  ```
  Notes:
  - The `AbortSignal` types differ between crates. The simplest mapping is to add a constructor in `tools-rs` that takes the `Arc<AtomicBool>` from `kosong_rs::provider::AbortSignal` (which exposes an `inner()` or similar accessor), or to define a shared abort flag type. If `kosong_rs::provider::AbortSignal` does not expose its inner flag, change Task 1 to also expose `tools-rs::builtin::AbortSignal::from_flag(Arc<AtomicBool>)`.
  - `ExecutableToolResult` from `tools-rs` must convert to `crate::records::nested::ExecutableToolResult`. Verify the shapes match; if not, implement an explicit `From` mapping in the bridge module.
  Expose in `agent-rs/src/tool/mod.rs`:
  ```rust
  pub mod bridge;
  pub use bridge::ToolBridge;
  ```

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p agent-rs bridge_forwards_turn_and_call_id
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/agent-rs/Cargo.toml \
         rust-ody/crates/agent-rs/src/tool/bridge.rs \
         rust-ody/crates/agent-rs/src/tool/mod.rs
  git commit -m "feat(agent-rs): bridge tools-rs BuiltinTool into agent_loop ExecutableTool"
  ```

### Task 3: Define collaboration trait boundaries in `tools-rs`

**Depends on:** Task 1 (context identity is part of the trait method signatures)

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs:107-118` (add `pub mod collaboration;`)
- Modify: `rust-ody/crates/tools-rs/src/lib.rs:1-16` (expose collaboration if desired)
- Test: `rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs` (module-level tests)

- [ ] Write the failing test.
  In the new module, add compile-time + behavioral tests:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::sync::Arc;

      #[test]
      fn skill_provider_trait_is_object_safe() {
          let _: Arc<dyn SkillProvider> = Arc::new(MockSkillProvider);
      }

      #[test]
      fn question_provider_trait_is_object_safe() {
          let _: Arc<dyn QuestionProvider> = Arc::new(MockQuestionProvider);
      }

      #[test]
      fn subagent_host_trait_is_object_safe() {
          let _: Arc<dyn SubagentHost> = Arc::new(MockSubagentHost);
      }

      #[test]
      fn background_registrar_trait_is_object_safe() {
          let _: Arc<dyn BackgroundRegistrar> = Arc::new(MockBackgroundRegistrar);
      }

      struct MockSkillProvider;
      impl SkillProvider for MockSkillProvider {
          fn get_skill(&self, _name: &str) -> Option<SkillInfo> { None }
          fn record_activation(&self, _origin: SkillActivationOrigin) -> Result<(), SkillError> { Ok(()) }
          fn render_skill_prompt(&self, _skill: &SkillInfo, _args: &str) -> String { String::new() }
          fn current_session_mode(&self) -> Option<String> { None }
      }

      struct MockQuestionProvider;
      #[async_trait::async_trait]
      impl QuestionProvider for MockQuestionProvider {
          async fn request_question(&self, _req: QuestionRequest, _signal: &AbortSignal) -> Result<QuestionResult, QuestionError> {
              Ok(QuestionResult::Dismissed)
          }
      }

      struct MockSubagentHost;
      #[async_trait::async_trait]
      impl SubagentHost for MockSubagentHost {
          async fn spawn(&self, _profile: &str, _options: SubagentOptions) -> Result<SubagentHandle, SubagentError> {
              Err(SubagentError::Unavailable)
          }
          async fn resume(&self, _agent_id: &str, _options: SubagentOptions) -> Result<SubagentHandle, SubagentError> {
              Err(SubagentError::Unavailable)
          }
          fn get_profile_name(&self, _agent_id: &str) -> Option<String> { None }
          fn background_task_timeout_ms(&self) -> u64 { 600_000 }
          fn cancel_all(&self, _reason: &str) {}
      }

      struct MockBackgroundRegistrar;
      #[async_trait::async_trait]
      impl BackgroundRegistrar for MockBackgroundRegistrar {
          async fn register_question_task(&self, _description: String, _run: QuestionRunFn, _options: QuestionTaskOptions) -> Result<String, BackgroundError> {
              Ok("question-12345678".into())
          }
          async fn register_agent_task(&self, _completion: AgentCompletion, _description: String, _options: AgentTaskOptions) -> Result<String, BackgroundError> {
              Ok("agent-12345678".into())
          }
      }
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo test -p tools-rs collaboration_trait_object_safe
  ```
  Expected failure: module/trait/type does not exist.

- [ ] Write the minimal implementation.
  Create `tools-rs/src/builtin/collaboration/mod.rs`:
  ```rust
  use std::future::Future;
  use std::pin::Pin;
  use std::sync::Arc;

  use crate::builtin::AbortSignal;
  use crate::builtin::ExecutableToolResult;

  // ---------------------------------------------------------------------------
  // Skill provider
  // ---------------------------------------------------------------------------

  #[derive(Debug, Clone, PartialEq)]
  pub struct SkillInfo {
      pub name: String,
      pub skill_type: Option<String>,
      pub disable_model_invocation: Option<bool>,
      pub hidden_in_modes: Option<Vec<String>>,
      pub content: String,
      pub path: String,
      pub source: String,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct SkillActivationOrigin {
      pub activation_id: String,
      pub skill_name: String,
      pub skill_args: Option<String>,
      pub trigger: String,
      pub skill_type: Option<String>,
      pub skill_path: Option<String>,
      pub skill_source: Option<String>,
  }

  #[derive(Debug, thiserror::Error, PartialEq)]
  pub enum SkillError {
      #[error("skill not found")]
      NotFound,
      #[error("model invocation disabled")]
      ModelInvocationDisabled,
      #[error("not an inline skill")]
      NotInline,
      #[error("skill hidden in current mode")]
      HiddenInMode,
  }

  pub trait SkillProvider: Send + Sync {
      fn get_skill(&self, name: &str) -> Option<SkillInfo>;
      fn record_activation(&self, origin: SkillActivationOrigin) -> Result<(), SkillError>;
      fn render_skill_prompt(&self, skill: &SkillInfo, args: &str) -> String;
      fn current_session_mode(&self) -> Option<String>;
  }

  // ---------------------------------------------------------------------------
  // Question provider
  // ---------------------------------------------------------------------------

  #[derive(Debug, Clone, PartialEq)]
  pub struct QuestionOption {
      pub label: String,
      pub description: String,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct QuestionItem {
      pub question: String,
      pub header: String,
      pub options: Vec<QuestionOption>,
      pub multi_select: bool,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct QuestionRequest {
      pub turn_id: Option<i64>,
      pub tool_call_id: String,
      pub questions: Vec<QuestionItem>,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct QuestionAnswers {
      pub answers: std::collections::HashMap<String, serde_json::Value>,
      pub method: Option<String>,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum QuestionResult {
      Dismissed,
      Answers(QuestionAnswers),
  }

  #[derive(Debug, thiserror::Error, PartialEq)]
  pub enum QuestionError {
      #[error("question RPC not implemented")]
      NotImplemented,
      #[error("question aborted")]
      Aborted,
      #[error(transparent)]
      Other(#[from] anyhow::Error),
  }

  #[async_trait::async_trait]
  pub trait QuestionProvider: Send + Sync {
      async fn request_question(
          &self,
          req: QuestionRequest,
          signal: &AbortSignal,
      ) -> Result<QuestionResult, QuestionError>;
  }

  // ---------------------------------------------------------------------------
  // Subagent host
  // ---------------------------------------------------------------------------

  #[derive(Debug, Clone, PartialEq)]
  pub struct SubagentOptions {
      pub parent_tool_call_id: String,
      pub prompt: String,
      pub description: String,
      pub run_in_background: bool,
      pub signal: AbortSignal,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct SubagentHandle {
      pub agent_id: String,
      pub profile_name: String,
      pub completion: SubagentCompletion,
  }

  pub type SubagentCompletion = Pin<Box<dyn Future<Output = Result<SubagentResult, SubagentError>> + Send>>;

  #[derive(Debug, Clone, PartialEq)]
  pub struct SubagentResult {
      pub result: String,
      pub usage: Option<SubagentUsage>,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct SubagentUsage {
      pub input: u64,
      pub output: u64,
      pub cache_read: Option<u64>,
      pub cache_write: Option<u64>,
  }

  #[derive(Debug, thiserror::Error, PartialEq)]
  pub enum SubagentError {
      #[error("subagent host unavailable")]
      Unavailable,
      #[error("invalid resume/subagent_type combination")]
      InvalidCombination,
      #[error("background agent unavailable")]
      BackgroundUnavailable,
      #[error("{0}")]
      Message(String),
  }

  #[async_trait::async_trait]
  pub trait SubagentHost: Send + Sync {
      async fn spawn(&self, profile: &str, options: SubagentOptions) -> Result<SubagentHandle, SubagentError>;
      async fn resume(&self, agent_id: &str, options: SubagentOptions) -> Result<SubagentHandle, SubagentError>;
      fn get_profile_name(&self, agent_id: &str) -> Option<String>;
      fn background_task_timeout_ms(&self) -> u64;
      fn cancel_all(&self, reason: &str);
  }

  // ---------------------------------------------------------------------------
  // Background registrar
  // ---------------------------------------------------------------------------

  pub type QuestionRunFn = Arc<
      dyn Fn() -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>> + Send + Sync,
  >;

  pub type AgentCompletion = Pin<Box<dyn Future<Output = Result<SubagentResult, SubagentError>> + Send>>;

  #[derive(Debug, Clone, PartialEq)]
  pub struct QuestionTaskOptions {
      pub question_count: u32,
      pub tool_call_id: String,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct AgentTaskOptions {
      pub timeout_ms: Option<u64>,
      pub agent_id: String,
      pub subagent_type: String,
      pub abort: Arc<dyn Fn() + Send + Sync>,
  }

  #[derive(Debug, thiserror::Error, PartialEq)]
  pub enum BackgroundError {
      #[error("background manager unavailable")]
      Unavailable,
      #[error("{0}")]
      Message(String),
  }

  #[async_trait::async_trait]
  pub trait BackgroundRegistrar: Send + Sync {
      async fn register_question_task(
          &self,
          description: String,
          run: QuestionRunFn,
          options: QuestionTaskOptions,
      ) -> Result<String, BackgroundError>;

      async fn register_agent_task(
          &self,
          completion: AgentCompletion,
          description: String,
          options: AgentTaskOptions,
      ) -> Result<String, BackgroundError>;
  }
  ```
  Add `pub mod collaboration;` to `tools-rs/src/builtin/mod.rs` after the existing re-exports.

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo test -p tools-rs collaboration_trait_object_safe
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs \
         rust-ody/crates/tools-rs/src/builtin/mod.rs
  git commit -m "feat(tools-rs): define collaboration tool trait boundaries"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage table: Task 1 covers context extension; Task 2 covers tool bridge; Task 3 covers `SkillProvider`/`QuestionProvider`/`SubagentHost`/`BackgroundRegistrar`. No GAPs.
- [ ] 2. Placeholder scan: no TODO/TBD; every code block is real and compilable (modulo crate-local names that already exist).
- [ ] 3. No phantom tasks: each task creates/modifies files and ends with a verifiable test + commit.
- [ ] 4. Dependency soundness: Task 1 → Task 2 → (Task 3 parallel with Task 2). Task 3 only depends on Task 1 for context field names.
- [ ] 5. Caller & build soundness: Task 1 is the shared-signature task; it searches `ExecutableToolContext {` across both crates and updates every caller, ending with `cargo test -p tools-rs` and `cargo test -p agent-rs` (whole-workspace for Rust). The `AbortSignal` mapping is explicitly called out and resolved in Task 2.
- [ ] 6. Test-the-risk: Task 1 tests identity propagation (a state-mutation risk: wrong turn/tool-call id breaks telemetry/lineage). Task 2 tests that the bridge forwards ids without loss. Task 3 tests trait object safety (the compile-time risk for dependency injection).
- [ ] 7. Type consistency: `turn_id`/`tool_call_id` names match between crates; `AbortSignal` mapping is handled in the bridge; `ExecutableToolResult` conversion is explicitly verified.
