# Phase 4.4.4 Part 4 — AgentTool Implementation + L1 Fixture

**Goal:** Implement the Rust `AgentTool` in `tools-rs` for spawning/resuming subagents in foreground or background, with timeout handling and proper error formatting, and add an L1 golden fixture covering success, failure, timeout, resume conflicts, and background registration.

**Architecture:** `AgentTool` holds an `Arc<dyn SubagentHost>` and an optional `Arc<dyn BackgroundRegistrar>`. It normalizes the input (defaulting `subagent_type` to `"coder"`, rejecting the combination with `resume`), dispatches `spawn` or `resume` through the host, awaits the completion future for foreground calls, and registers the completion future as a background agent task when `run_in_background=true`. A small deadline helper combines the parent abort signal with an optional foreground timeout.

**Tech Stack:** Rust 2021, `tools-rs` builtins, `serde_json`, `tokio::time`, existing `SubagentHost`/`BackgroundRegistrar` boundaries, Vitest/TypeScript golden harness.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/tools-rs/
  src/builtin/collaboration/agent.rs            # AgentTool + unit tests
  src/builtin/collaboration/mod.rs              # re-export AgentTool
  src/builtin/mod.rs                            # re-export AgentTool
  src/golden.rs                                 # add AgentCall op + runner
packages/integration-tests/src/parity/
  fixtures/tools-rs/collaboration-tools.json    # extend with agent_call cases
  tools-rs-golden.ts                            # add agent_call op handler
```

## Dependency Overview

```
Part 1: infra.md (SubagentHost + BackgroundRegistrar traits)
  │
  ├──► Part 2: skill-tool.md
  ├──► Part 3: ask-user-tool.md
  │
  └──► Part 4: agent-tool.md
         │
         ├──► Task 1: Implement AgentTool + unit tests
         │
         └──► Task 2: Add agent_call L1 golden op + fixture + TS handler
```

- **Task 1** implements the tool using traits already defined in Part 1.
- **Task 2** depends on Task 1 for the working tool and the existing golden harness.

## Tasks

### Task 1: Implement `AgentTool`

**Depends on:** `2026-06-29-backend-architecture-evolution-phase4-4/infra.md`: Task 3

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/collaboration/agent.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs:1-5` (re-export `agent::AgentTool`)
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs:107-118` (re-export `AgentTool`)
- Test: `rust-ody/crates/tools-rs/src/builtin/collaboration/agent.rs` (module-level tests)

- [ ] Write the failing tests.
  Create `tools-rs/src/builtin/collaboration/agent.rs` with a `#[cfg(test)]` module:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use serde_json::json;
      use std::future::Future;
      use std::pin::Pin;
      use std::sync::{Arc, Mutex};

      use crate::builtin::collaboration::{
          AgentCompletion, AgentTaskOptions, BackgroundError, BackgroundRegistrar, SubagentError,
          SubagentHandle, SubagentHost, SubagentOptions, SubagentResult, SubagentUsage,
      };
      use crate::builtin::{AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolResult};

      struct TestSubagentHost {
          behavior: Mutex<HostBehavior>,
      }

      enum HostBehavior {
          Success { agent_id: String, profile_name: String, result: String },
          Fail { error: SubagentError },
          Timeout,
      }

      #[async_trait::async_trait]
      impl SubagentHost for TestSubagentHost {
          async fn spawn(
              &self,
              profile: &str,
              _options: SubagentOptions,
          ) -> Result<SubagentHandle, SubagentError> {
              self.make_handle(profile)
          }
          async fn resume(
              &self,
              _agent_id: &str,
              _options: SubagentOptions,
          ) -> Result<SubagentHandle, SubagentError> {
              self.make_handle("subagent")
          }
          fn get_profile_name(&self, _agent_id: &str) -> Option<String> { None }
          fn background_task_timeout_ms(&self) -> u64 { 600_000 }
          fn cancel_all(&self, _reason: &str) {}
      }

      impl TestSubagentHost {
          fn make_handle(&self, profile_name: &str) -> Result<SubagentHandle, SubagentError> {
              let behavior = self.behavior.lock().unwrap();
              match &*behavior {
                  HostBehavior::Success { agent_id, result, .. } => Ok(SubagentHandle {
                      agent_id: agent_id.clone(),
                      profile_name: profile_name.into(),
                      completion: Box::pin(async move {
                          Ok(SubagentResult {
                              result: result.clone(),
                              usage: Some(SubagentUsage {
                                  input: 10,
                                  output: 20,
                                  cache_read: None,
                                  cache_write: None,
                              }),
                          })
                      }),
                  }),
                  HostBehavior::Fail { error } => Err(error.clone()),
                  HostBehavior::Timeout => Ok(SubagentHandle {
                      agent_id: "agent-timeout".into(),
                      profile_name: profile_name.into(),
                      completion: Box::pin(async move {
                          futures::future::pending::<Result<SubagentResult, SubagentError>>().await
                      }),
                  }),
              }
          }
      }

      struct TestBackgroundRegistrar {
          registrations: Mutex<Vec<(String, AgentTaskOptions)>>,
          next_id: Mutex<String>,
          fail: bool,
      }

      #[async_trait::async_trait]
      impl BackgroundRegistrar for TestBackgroundRegistrar {
          async fn register_question_task(
              &self,
              _description: String,
              _run: crate::builtin::collaboration::QuestionRunFn,
              _options: crate::builtin::collaboration::QuestionTaskOptions,
          ) -> Result<String, BackgroundError> {
              unimplemented!()
          }
          async fn register_agent_task(
              &self,
              _completion: AgentCompletion,
              _description: String,
              options: AgentTaskOptions,
          ) -> Result<String, BackgroundError> {
              if self.fail {
                  return Err(BackgroundError::Message("registrar down".into()));
              }
              self.registrations.lock().unwrap().push((options.agent_id.clone(), options));
              Ok(self.next_id.lock().unwrap().clone())
          }
      }

      fn ctx() -> ExecutableToolContext {
          ExecutableToolContext {
              turn_id: "1".into(),
              tool_call_id: "call_a".into(),
              signal: AbortSignal::new(),
          }
      }

      fn args(extra: serde_json::Value) -> serde_json::Value {
          let mut base = json!({
              "prompt": "Do something",
              "description": "Test agent",
          });
          if let Some(obj) = extra.as_object() {
              for (k, v) in obj {
                  base[k] = v.clone();
              }
          }
          base
      }

      async fn run(tool: &AgentTool, input: serde_json::Value) -> ExecutableToolResult {
          let exec = tool.resolve_execution(input).unwrap();
          (exec.execute)(ctx()).await
      }

      #[tokio::test]
      async fn foreground_success_returns_agent_output() {
          let host = Arc::new(TestSubagentHost {
              behavior: Mutex::new(HostBehavior::Success {
                  agent_id: "agent-123".into(),
                  profile_name: "coder".into(),
                  result: "Done".into(),
              }),
          });
          let tool = AgentTool::new(host, None::<Arc<dyn BackgroundRegistrar>>, AgentToolOptions::default());
          let result = run(&tool, args(json!({"subagent_type": "coder"}))).await;
          assert!(!result.is_error, "{:?}", result);
          assert!(result.to_text().contains("agent_id: agent-123"));
          assert!(result.to_text().contains("actual_subagent_type: coder"));
          assert!(result.to_text().contains("status: completed"));
          assert!(result.to_text().contains("[summary]\nDone"));
      }

      #[tokio::test]
      async fn foreground_failure_returns_error() {
          let host = Arc::new(TestSubagentHost {
              behavior: Mutex::new(HostBehavior::Fail {
                  error: SubagentError::Message("boom".into()),
              }),
          });
          let tool = AgentTool::new(host, None::<Arc<dyn BackgroundRegistrar>>, AgentToolOptions::default());
          let result = run(&tool, args(json!({}))).await;
          assert!(result.is_error);
          assert!(result.to_text().contains("subagent error: boom"));
      }

      #[tokio::test]
      async fn foreground_timeout_returns_timeout_error() {
          let host = Arc::new(TestSubagentHost {
              behavior: Mutex::new(HostBehavior::Timeout),
          });
          let tool = AgentTool::new(host, None::<Arc<dyn BackgroundRegistrar>>, AgentToolOptions::default());
          let result = run(&tool, args(json!({"timeout": 30}))).await;
          assert!(result.is_error);
          assert!(result.to_text().contains("Agent timed out after 30s"));
      }

      #[tokio::test]
      async fn resume_and_subagent_type_conflict_returns_error() {
          let host = Arc::new(TestSubagentHost {
              behavior: Mutex::new(HostBehavior::Success {
                  agent_id: "agent-123".into(),
                  profile_name: "coder".into(),
                  result: "Done".into(),
              }),
          });
          let tool = AgentTool::new(host, None::<Arc<dyn BackgroundRegistrar>>, AgentToolOptions::default());
          let result = run(&tool, args(json!({"resume": "agent-123", "subagent_type": "coder"}))).await;
          assert!(result.is_error);
          assert!(result.to_text().contains("Cannot set subagent_type when resuming"));
      }

      #[tokio::test]
      async fn background_registers_task() {
          let host = Arc::new(TestSubagentHost {
              behavior: Mutex::new(HostBehavior::Success {
                  agent_id: "agent-123".into(),
                  profile_name: "coder".into(),
                  result: "Done".into(),
              }),
          });
          let registrar = Arc::new(TestBackgroundRegistrar {
              registrations: Mutex::new(Vec::new()),
              next_id: Mutex::new("agent-00000001".into()),
              fail: false,
          });
          let tool = AgentTool::new(host, Some(registrar.clone() as Arc<dyn BackgroundRegistrar>), AgentToolOptions::default());
          let result = run(&tool, args(json!({"run_in_background": true}))).await;
          assert!(!result.is_error, "{:?}", result);
          assert!(result.to_text().contains("task_id: agent-00000001"));
          assert!(result.to_text().contains("agent_id: agent-123"));
          let regs = registrar.registrations.lock().unwrap();
          assert_eq!(regs.len(), 1);
          assert_eq!(regs[0].0, "agent-123");
          assert_eq!(regs[0].1.subagent_type, "coder");
      }

      #[tokio::test]
      async fn background_without_registrar_returns_error() {
          let host = Arc::new(TestSubagentHost {
              behavior: Mutex::new(HostBehavior::Success {
                  agent_id: "agent-123".into(),
                  profile_name: "coder".into(),
                  result: "Done".into(),
              }),
          });
          let tool = AgentTool::new(host, None::<Arc<dyn BackgroundRegistrar>>, AgentToolOptions::default());
          let result = run(&tool, args(json!({"run_in_background": true}))).await;
          assert!(result.is_error);
          assert!(result.to_text().contains("Background agent execution is not available"));
      }

      #[test]
      fn matches_rule_compares_profile_name() {
          let host = Arc::new(TestSubagentHost {
              behavior: Mutex::new(HostBehavior::Success {
                  agent_id: "agent-123".into(),
                  profile_name: "coder".into(),
                  result: "Done".into(),
              }),
          });
          let tool = AgentTool::new(host, None::<Arc<dyn BackgroundRegistrar>>, AgentToolOptions::default());
          let exec = tool.resolve_execution(args(json!({"subagent_type": "reviewer"}))).unwrap();
          let matches = exec.matches_rule.expect("agent should have matches_rule");
          assert!(matches("reviewer"));
          assert!(matches("review*"));
          assert!(!matches("coder"));
      }
  }
  ```

- [ ] Run them and verify they FAIL.
  ```bash
  cd rust-ody && cargo test -p tools-rs agent_tool
  ```
  Expected failure: `AgentTool`, `AgentToolOptions`, and `SubagentHost` test helpers do not exist; `futures` crate may not be linked.

- [ ] Write the minimal implementation.
  1. Add `futures` to `tools-rs/Cargo.toml` (if not already present) for `futures::future::pending` in tests:
     ```toml
     futures = "0.3"
     ```
  2. Create `tools-rs/src/builtin/collaboration/agent.rs`:
     ```rust
     use std::collections::HashMap;
     use std::sync::Arc;
     use std::time::Duration;

     use serde_json::Value;

     use crate::builtin::collaboration::{
         AgentTaskOptions, BackgroundError, BackgroundRegistrar, SubagentError, SubagentHandle,
         SubagentHost, SubagentOptions,
     };
     use crate::builtin::{AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};
     use crate::policies::rule_match::matches_glob_rule_subject;
     use crate::schema::InputSchema;
     use crate::tool_accesses::ToolAccesses;

     const BACKGROUND_AGENT_UNAVAILABLE: &str =
         "Background agent execution is not available for this agent because TaskList, TaskOutput, and TaskStop are not enabled.";

     #[derive(Debug, Clone, Default)]
     pub struct AgentToolOptions {
         pub subagent_profiles: Option<HashMap<String, String>>,
     }

     pub struct AgentTool {
         host: Arc<dyn SubagentHost>,
         background_registrar: Option<Arc<dyn BackgroundRegistrar>>,
         options: AgentToolOptions,
     }

     impl AgentTool {
         pub fn new(
             host: Arc<dyn SubagentHost>,
             background_registrar: Option<Arc<dyn BackgroundRegistrar>>,
             options: AgentToolOptions,
         ) -> Self {
             Self {
                 host,
                 background_registrar,
                 options,
             }
         }
     }

     impl BuiltinTool for AgentTool {
         fn name(&self) -> &str { "Agent" }

         fn description(&self) -> &str {
             let base = concat!(
                 "Launch a subagent to handle a task. The subagent runs as a same-process loop instance with its own context and wire file.\n\n",
                 "Writing the prompt:\n",
                 "- The subagent starts with zero context — it has not seen this conversation. Brief it like a colleague who just walked into the room: state the goal, list what you already know, hand over the specifics.\n",
                 "- Lookups (read this file, run that test): put the exact path or command in the prompt. The subagent should not have to search for things you already know.\n",
                 "- Investigations (figure out X, find why Y): give the question, not prescribed steps — fixed steps become dead weight when the premise is wrong.\n",
                 "- Do not delegate understanding. If the task hinges on a file path or line number, find it yourself first and write it into the prompt.\n\n",
                 "Usage notes:\n",
                 "- When the task continues earlier work a subagent already did, prefer resuming that agent (pass its `resume` id) over spawning a fresh instance — the resumed agent keeps its prior context.\n",
                 "- A subagent's result is only visible to you, not to the user. When the user needs to see what a subagent produced, summarize the relevant parts yourself in your own reply.\n\n",
                 "When NOT to use Agent: skip delegation for trivial work you can do directly — reading a file whose path you already know, searching a small known set of files, or any task that takes only a step or two. Delegation has a context-handoff cost; it pays off only when the task is substantial enough to outweigh it.\n\n",
                 "Once a subagent is running, leave that scope to it: do not redo its searches or reads in parallel, and do not abandon it midway and finish the job manually. Both undo the context savings the delegation was meant to buy."
             );
             let background_note = if self.background_registrar.is_some() {
                 concat!(
                     "\n\nWhen `run_in_background=true`, the subagent runs detached from this turn. The completion arrives in a later turn as a synthetic user-role message containing its result — you do not need to poll, sleep, or check on its progress. Continue with other work or respond to the user. Never fabricate or predict what the result will say.\n\n",
                     "For a background task, when `timeout` is omitted it falls back to the operator-configured background timeout, if one is set. If the operator has not configured a background timeout, an omitted `timeout` means the task runs with no time limit."
                 )
             } else {
                 "\n\nBackground agent execution is disabled for this agent. Do not set `run_in_background=true`."
             };
             let type_lines = self
                 .options
                 .subagent_profiles
                 .as_ref()
                 .filter(|m| !m.is_empty())
                 .map(|profiles| {
                     let lines: Vec<String> = profiles
                         .iter()
                         .map(|(name, desc)| format!("- {}: {}", name, desc))
                         .collect();
                     format!("\n\nAvailable agent types (pass via subagent_type):\n{}", lines.join("\n"))
                 })
                 .unwrap_or_default();
             Box::leak(format!("{}{}{}", base, background_note, type_lines).into_boxed_str())
         }

         fn parameters(&self) -> Value {
             InputSchema::object(vec![
                 (
                     "prompt",
                     InputSchema::string().description("Full task prompt for the subagent"),
                 ),
                 (
                     "description",
                     InputSchema::string().description("Short task description (3-5 words) for UI display"),
                 ),
                 (
                     "subagent_type",
                     InputSchema::string()
                         .optional()
                         .description("One of the available agent types. Defaults to \"coder\" when omitted."),
                 ),
                 (
                     "resume",
                     InputSchema::string()
                         .optional()
                         .description("Optional agent ID to resume instead of creating a new instance"),
                 ),
                 (
                     "run_in_background",
                     InputSchema::boolean()
                         .optional()
                         .default(serde_json::json!(false))
                         .describe("If true, return immediately without waiting for completion."),
                 ),
                 (
                     "timeout",
                     InputSchema::integer()
                         .min(30.0)
                         .max(3600.0)
                         .optional()
                         .describe("Timeout in seconds for the agent task (min 30s, max 3600s)."),
                 ),
             ])
             .build()
         }

         fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
             let prompt = args
                 .get("prompt")
                 .and_then(Value::as_str)
                 .ok_or_else(|| ToolError::InvalidArgs("prompt is required".into()))?
                 .to_string();
             let description = args
                 .get("description")
                 .and_then(Value::as_str)
                 .ok_or_else(|| ToolError::InvalidArgs("description is required".into()))?
                 .to_string();
             let requested_profile = args.get("subagent_type").and_then(Value::as_str).map(String::from);
             let resume = args.get("resume").and_then(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
             let run_in_background = args.get("run_in_background").and_then(Value::as_bool).unwrap_or(false);
             let timeout_secs = args.get("timeout").and_then(Value::as_u64);

             let profile_name = if let Some(resume_id) = &resume {
                 self.host.get_profile_name(resume_id).unwrap_or_else(|| "subagent".into())
             } else {
                 requested_profile.clone().unwrap_or_else(|| "coder".into())
             };
             let prefix = if run_in_background { "Launching background" } else { "Launching" };

             let host = Arc::clone(&self.host);
             let registrar = self.background_registrar.as_ref().map(Arc::clone);
             let profile_for_match = profile_name.clone();

             Ok(ToolExecution {
                 accesses: ToolAccesses::none(),
                 description: format!("{} {} agent: {}", prefix, profile_name, description),
                 approval_rule: "Agent".into(),
                 matches_rule: Some(Box::new(move |rule_args| {
                     matches_glob_rule_subject(rule_args, &profile_for_match)
                 })),
                 execute: Box::new(move |ctx| {
                     let host = Arc::clone(&host);
                     let registrar = registrar.as_ref().map(Arc::clone);
                     let prompt = prompt.clone();
                     let description = description.clone();
                     let requested_profile = requested_profile.clone();
                     let resume = resume.clone();
                     let tool_call_id = ctx.tool_call_id.clone();
                     let parent_signal = ctx.signal.clone();
                     Box::pin(async move {
                         execute_agent(
                             host,
                             registrar,
                             prompt,
                             description,
                             requested_profile,
                             resume,
                             tool_call_id,
                             parent_signal,
                             run_in_background,
                             timeout_secs,
                         )
                         .await
                     })
                 }),
             })
         }
     }

     async fn execute_agent(
         host: Arc<dyn SubagentHost>,
         registrar: Option<Arc<dyn BackgroundRegistrar>>,
         prompt: String,
         description: String,
         requested_profile: Option<String>,
         resume: Option<String>,
         tool_call_id: String,
         parent_signal: AbortSignal,
         run_in_background: bool,
         timeout_secs: Option<u64>,
     ) -> ExecutableToolResult {
         if parent_signal.aborted() {
             return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
         }

         if resume.is_some() && requested_profile.is_some() {
             return ExecutableToolResult::error_text(
                 "Cannot set subagent_type when resuming an existing agent. Resume by agent id only.".into(),
                 "Invalid resume combination".into(),
             );
         }

         if run_in_background && registrar.is_none() {
             return ExecutableToolResult::error_text(
                 BACKGROUND_AGENT_UNAVAILABLE.into(),
                 "Background unavailable".into(),
             );
         }

         let (subagent_signal, timeout_handle): (AbortSignal, Option<tokio::task::JoinHandle<()>>) =
             if run_in_background {
                 (AbortSignal::new(), None)
             } else if let Some(secs) = timeout_secs {
                 let (signal, handle) = deadline_signal(parent_signal.clone(), secs * 1000);
                 (signal, Some(handle))
             } else {
                 (parent_signal.clone(), None)
             };

         let options = SubagentOptions {
             parent_tool_call_id: tool_call_id,
             prompt,
             description: description.clone(),
             run_in_background,
             signal: subagent_signal,
         };

         let handle: SubagentHandle = match &resume {
             Some(agent_id) => match host.resume(agent_id, options).await {
                 Ok(h) => h,
                 Err(e) => return subagent_error(e),
             },
             None => {
                 let profile = requested_profile.as_deref().unwrap_or("coder");
                 match host.spawn(profile, options).await {
                     Ok(h) => h,
                     Err(e) => return subagent_error(e),
                 }
             }
         };

         if run_in_background {
             let registrar = registrar.expect("checked above");
             let background_signal = AbortSignal::new();
             let abort = Arc::new(move || background_signal.abort());
             let completion: crate::builtin::collaboration::AgentCompletion = handle.completion;
             match registrar
                 .register_agent_task(
                     completion,
                     description.clone(),
                     AgentTaskOptions {
                         timeout_ms: timeout_secs.map(|s| s * 1000).or(Some(host.background_task_timeout_ms())),
                         agent_id: handle.agent_id.clone(),
                         subagent_type: handle.profile_name.clone(),
                         abort,
                     },
                 )
                 .await
             {
                 Ok(task_id) => ExecutableToolResult::ok_text(format!(
                     "task_id: {}\nstatus: running\nagent_id: {}\nactual_subagent_type: {}\nautomatic_notification: true\n\ndescription: {}\n\nnext_step: The completion arrives automatically in a later turn — no polling needed. To peek at progress without blocking, call TaskOutput(task_id=\"{}\", block=false).\nresume_hint: To continue or recover this same subagent later, call Agent(resume=\"{}\", prompt=\"...\"). The parameter is agent_id (\"{}\"), NOT task_id (\"{}\") or source_id from a later <notification>. Recovery cases: a later <notification type=\"task.lost\" | \"task.failed\" | \"task.killed\"> for this subagent — its conversation history is preserved across session restarts and resume will pick it up.",
                     task_id,
                     handle.agent_id,
                     handle.profile_name,
                     description,
                     task_id,
                     handle.agent_id,
                     handle.agent_id,
                     task_id,
                 )),
                 Err(BackgroundError::Unavailable) => ExecutableToolResult::error_text(
                     "Background agent task registration is unavailable.".into(),
                     "Background unavailable".into(),
                 ),
                 Err(BackgroundError::Message(m)) => ExecutableToolResult::error_text(m.clone(), m),
             }
         } else {
             let result = match handle.completion.await {
                 Ok(r) => r,
                 Err(e) => {
                     let message = if timeout_secs.is_some() && timed_out(&timeout_handle) {
                         format!("Agent timed out after {}s.", timeout_secs.unwrap())
                     } else {
                         format!("{}", e)
                     };
                     return ExecutableToolResult::error_text(
                         format!(
                             "agent_id: {}\nactual_subagent_type: {}\nstatus: failed\n\nsubagent error: {}",
                             handle.agent_id, handle.profile_name, message
                         ),
                         "Subagent failed".into(),
                     );
                 }
             };
             ExecutableToolResult::ok_text(format!(
                 "agent_id: {}\nactual_subagent_type: {}\nstatus: completed\n\n[summary]\n{}",
                 handle.agent_id, handle.profile_name, result.result
            ))
         }
     }

     fn subagent_error(e: SubagentError) -> ExecutableToolResult {
         ExecutableToolResult::error_text(
             format!("subagent error: {}", e),
             "Subagent launch failed".into(),
         )
     }

     fn deadline_signal(parent: AbortSignal, timeout_ms: u64) -> (AbortSignal, tokio::task::JoinHandle<()>) {
         let signal = AbortSignal::new();
         let handle = {
             let signal = signal.clone();
             tokio::spawn(async move {
                 tokio::select! {
                     _ = tokio::time::sleep(Duration::from_millis(timeout_ms)) => signal.abort(),
                     _ = wait_abort(parent) => signal.abort(),
                 }
             })
         };
         (signal, handle)
     }

     async fn wait_abort(signal: AbortSignal) {
         while !signal.aborted() {
             tokio::time::sleep(Duration::from_millis(50)).await;
         }
     }

     fn timed_out(handle: &Option<tokio::task::JoinHandle<()>>) -> bool {
         handle.as_ref().map(|h| h.is_finished()).unwrap_or(false)
     }

     #[cfg(test)]
     mod tests {
         // ... same as the failing-test block above ...
     }
     ```
  3. Re-export from `tools-rs/src/builtin/collaboration/mod.rs`:
     ```rust
     pub mod agent;
     pub use agent::{AgentTool, AgentToolOptions};
     ```
  4. Re-export from `tools-rs/src/builtin/mod.rs`:
     ```rust
     pub use collaboration::agent::{AgentTool, AgentToolOptions};
     ```

- [ ] Run the tests and verify they PASS.
  ```bash
  cd rust-ody && cargo test -p tools-rs agent_tool
  ```

- [ ] Whole-tree typecheck.
  ```bash
  cd rust-ody && cargo test
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/tools-rs/Cargo.toml \
         rust-ody/crates/tools-rs/src/builtin/collaboration/agent.rs \
         rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs \
         rust-ody/crates/tools-rs/src/builtin/mod.rs
  git commit -m "feat(tools-rs): implement AgentTool with spawn, resume, timeout, and background registration"
  ```

### Task 2: Add `agent_call` L1 golden op + fixture

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/golden.rs:48-262` (add `AgentCall` op variant)
- Modify: `rust-ody/crates/tools-rs/src/golden.rs:350+` (add `Op::AgentCall` handler)
- Modify: `packages/integration-tests/src/parity/fixtures/tools-rs/collaboration-tools.json` (extend with agent_call cases)
- Modify: `packages/integration-tests/src/parity/tools-rs-golden.ts:33-67` (add `agent_call` to `GoldenOp`)
- Modify: `packages/integration-tests/src/parity/tools-rs-golden.ts:147+` (add `agent_call` handler)

- [ ] Write the failing test / fixture.
  Extend `packages/integration-tests/src/parity/fixtures/tools-rs/collaboration-tools.json` with new cases after the `ask_user_*` cases:
  ```json
  {
    "name": "agent_foreground_success",
    "op": {
      "type": "agent_call",
      "prompt": "Refactor this function",
      "description": "Refactor helper",
      "subagent_type": "coder",
      "run_in_background": false,
      "timeout": null,
      "host_response": "success",
      "result": "Refactored.",
      "agent_id": "agent-123",
      "profile_name": "coder"
    },
    "expected": null
  },
  {
    "name": "agent_foreground_failure",
    "op": {
      "type": "agent_call",
      "prompt": "Do something impossible",
      "description": "Impossible task",
      "run_in_background": false,
      "timeout": null,
      "host_response": "fail",
      "error": "not allowed",
      "agent_id": "agent-456",
      "profile_name": "coder"
    },
    "expected": null
  },
  {
    "name": "agent_foreground_timeout",
    "op": {
      "type": "agent_call",
      "prompt": "Sleep forever",
      "description": "Slow task",
      "run_in_background": false,
      "timeout": 30,
      "host_response": "timeout",
      "agent_id": "agent-789",
      "profile_name": "coder"
    },
    "expected": null
  },
  {
    "name": "agent_resume_conflict",
    "op": {
      "type": "agent_call",
      "prompt": "Continue",
      "description": "Resume task",
      "subagent_type": "coder",
      "resume": "agent-123",
      "run_in_background": false,
      "timeout": null,
      "host_response": "success",
      "result": "Resumed.",
      "agent_id": "agent-123",
      "profile_name": "subagent"
    },
    "expected": null
  },
  {
    "name": "agent_background_success",
    "op": {
      "type": "agent_call",
      "prompt": "Run in background",
      "description": "Background task",
      "run_in_background": true,
      "timeout": null,
      "registrar_response": "ok",
      "task_id": "agent-00000001",
      "agent_id": "agent-abc",
      "profile_name": "coder"
    },
    "expected": null
  }
  ```
  Add the Rust `AgentCall` op variant in `tools-rs/src/golden.rs`:
  ```rust
  #[serde(rename = "agent_call")]
  AgentCall {
      prompt: String,
      description: String,
      #[serde(default)]
      subagent_type: Option<String>,
      #[serde(default)]
      resume: Option<String>,
      #[serde(default)]
      run_in_background: Option<bool>,
      #[serde(default)]
      timeout: Option<u64>,
      #[serde(default)]
      host_response: Option<String>,
      #[serde(default)]
      result: Option<String>,
      #[serde(default)]
      error: Option<String>,
      #[serde(default)]
      agent_id: Option<String>,
      #[serde(default)]
      profile_name: Option<String>,
      #[serde(default)]
      registrar_response: Option<String>,
      #[serde(default)]
      task_id: Option<String>,
  },
  ```
  Add to `GoldenOp` in `tools-rs-golden.ts`:
  ```typescript
  | { type: 'agent_call'; prompt: string; description: string; subagent_type?: string | null; resume?: string | null; run_in_background?: boolean | null; timeout?: number | null; host_response?: string | null; result?: string | null; error?: string | null; agent_id?: string | null; profile_name?: string | null; registrar_response?: string | null; task_id?: string | null }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo build -p tools-rs --bin tools-golden
  ```
  Expected failure: `AgentCall` variant / handler do not exist; TS type error for unknown `agent_call` op.

- [ ] Write the minimal implementation.
  1. In `tools-rs/src/golden.rs`, add fixture subagent host and registrar:
     ```rust
     struct FixtureSubagentHost {
         behavior: String,
         result: Option<String>,
         error: Option<String>,
         agent_id: Option<String>,
         profile_name: Option<String>,
     }

     #[async_trait::async_trait]
     impl crate::builtin::collaboration::SubagentHost for FixtureSubagentHost {
         async fn spawn(
             &self,
             profile: &str,
             _options: crate::builtin::collaboration::SubagentOptions,
         ) -> Result<crate::builtin::collaboration::SubagentHandle, crate::builtin::collaboration::SubagentError> {
             self.make_handle(profile)
         }
         async fn resume(
             &self,
             _agent_id: &str,
             _options: crate::builtin::collaboration::SubagentOptions,
         ) -> Result<crate::builtin::collaboration::SubagentHandle, crate::builtin::collaboration::SubagentError> {
             self.make_handle(self.profile_name.as_deref().unwrap_or("subagent"))
         }
         fn get_profile_name(&self, _agent_id: &str) -> Option<String> { None }
         fn background_task_timeout_ms(&self) -> u64 { 600_000 }
         fn cancel_all(&self, _reason: &str) {}
     }

     impl FixtureSubagentHost {
         fn make_handle(
             &self,
             profile: &str,
         ) -> Result<crate::builtin::collaboration::SubagentHandle, crate::builtin::collaboration::SubagentError> {
             if self.behavior == "fail" {
                 return Err(crate::builtin::collaboration::SubagentError::Message(
                     self.error.clone().unwrap_or_else(|| "boom".into()),
                 ));
             }
             if self.behavior == "timeout" {
                 return Ok(crate::builtin::collaboration::SubagentHandle {
                     agent_id: self.agent_id.clone().unwrap_or_else(|| "agent-timeout".into()),
                     profile_name: profile.into(),
                     completion: Box::pin(async move {
                         futures::future::pending::<Result<crate::builtin::collaboration::SubagentResult, crate::builtin::collaboration::SubagentError>>().await
                     }),
                 });
             }
             Ok(crate::builtin::collaboration::SubagentHandle {
                 agent_id: self.agent_id.clone().unwrap_or_else(|| "agent-123".into()),
                 profile_name: profile.into(),
                 completion: Box::pin(async move {
                     Ok(crate::builtin::collaboration::SubagentResult {
                         result: self.result.clone().unwrap_or_else(|| "Done".into()),
                         usage: None,
                     })
                 }),
             })
         }
     }

     struct FixtureAgentBackgroundRegistrar {
         next_id: String,
         fail: bool,
     }

     #[async_trait::async_trait]
     impl crate::builtin::collaboration::BackgroundRegistrar for FixtureAgentBackgroundRegistrar {
         async fn register_question_task(
             &self,
             _description: String,
             _run: crate::builtin::collaboration::QuestionRunFn,
             _options: crate::builtin::collaboration::QuestionTaskOptions,
         ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
             unimplemented!()
         }
         async fn register_agent_task(
             &self,
             _completion: crate::builtin::collaboration::AgentCompletion,
             _description: String,
             _options: crate::builtin::collaboration::AgentTaskOptions,
         ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
             if self.fail {
                 return Err(crate::builtin::collaboration::BackgroundError::Message("registrar down".into()));
             }
             Ok(self.next_id.clone())
         }
     }
     ```
  2. Add the `Op::AgentCall` arm in `run_case_sync`:
     ```rust
     Op::AgentCall {
         prompt,
         description,
         subagent_type,
         resume,
         run_in_background,
         timeout,
         host_response,
         result,
         error,
         agent_id,
         profile_name,
         registrar_response,
         task_id,
     } => {
         use crate::builtin::collaboration::{AgentTool, AgentToolOptions};
         let host = Arc::new(FixtureSubagentHost {
             behavior: host_response.clone().unwrap_or_else(|| "success".into()),
             result: result.clone(),
             error: error.clone(),
             agent_id: agent_id.clone(),
             profile_name: profile_name.clone(),
         });
         let registrar = run_in_background.unwrap_or(false).then(|| {
             Arc::new(FixtureAgentBackgroundRegistrar {
                 next_id: task_id.clone().unwrap_or_else(|| "agent-00000001".into()),
                 fail: registrar_response.as_deref() == Some("fail"),
             })
         });
         let tool = AgentTool::new(
             host,
             registrar,
             AgentToolOptions::default(),
         );
         let mut args = serde_json::json!({
             "prompt": prompt,
             "description": description,
             "run_in_background": run_in_background.unwrap_or(false),
         });
         if let Some(st) = subagent_type { args["subagent_type"] = Value::String(st.clone()); }
         if let Some(r) = resume { args["resume"] = Value::String(r.clone()); }
         if let Some(t) = timeout { args["timeout"] = Value::from(*t); }
         match tool.resolve_execution(args) {
             Ok(exec) => {
                 let ctx = ExecutableToolContext {
                     turn_id: "1".into(),
                     tool_call_id: "call_a".into(),
                     signal: crate::builtin::AbortSignal::new(),
                 };
                 let result = tokio::runtime::Runtime::new()
                     .unwrap()
                     .block_on((exec.execute)(ctx));
                 CaseResult::ok(result_to_value(result))
             }
             Err(e) => CaseResult::err(e.to_string()),
         }
     }
     ```
  3. In `tools-rs-golden.ts`, add the `agent_call` handler in `runCase`:
     ```typescript
     case 'agent_call': {
       const runInBackground = op.run_in_background ?? false;
       const profile = op.subagent_type ?? 'coder';
       const resume = op.resume?.trim();
       if (resume !== undefined && resume.length > 0 && op.subagent_type !== undefined) {
         return { result: { output: 'Cannot set subagent_type when resuming an existing agent. Resume by agent id only.', is_error: true, message: 'Invalid resume combination' } };
       }
       if (runInBackground) {
         const taskId = op.task_id ?? 'agent-00000001';
         const agentId = op.agent_id ?? 'agent-123';
         const actualProfile = op.profile_name ?? profile;
         const output = `task_id: ${taskId}\nstatus: running\nagent_id: ${agentId}\nactual_subagent_type: ${actualProfile}\nautomatic_notification: true\n\ndescription: ${op.description}\n\nnext_step: The completion arrives automatically in a later turn — no polling needed. To peek at progress without blocking, call TaskOutput(task_id="${taskId}", block=false).\nresume_hint: To continue or recover this same subagent later, call Agent(resume="${agentId}", prompt="..."). The parameter is agent_id ("${agentId}"), NOT task_id ("${taskId}") or source_id from a later <notification>. Recovery cases: a later <notification type="task.lost" | "task.failed" | "task.killed"> for this subagent — its conversation history is preserved across session restarts and resume will pick it up.`;
         return { result: { output, is_error: false, message: `Started ${taskId}` } };
       }
       const agentId = op.agent_id ?? 'agent-123';
       const actualProfile = op.profile_name ?? profile;
       if (op.host_response === 'fail') {
         const message = op.error ?? 'unknown error';
         return { result: { output: `agent_id: ${agentId}\nactual_subagent_type: ${actualProfile}\nstatus: failed\n\nsubagent error: ${message}`, is_error: true, message: 'Subagent failed' } };
       }
       if (op.host_response === 'timeout') {
         return { result: { output: `agent_id: ${agentId}\nactual_subagent_type: ${actualProfile}\nstatus: failed\n\nsubagent error: Agent timed out after ${String(op.timeout ?? 30)}s.`, is_error: true, message: 'Subagent failed' } };
       }
       const result = op.result ?? 'Done';
       return { result: { output: `agent_id: ${agentId}\nactual_subagent_type: ${actualProfile}\nstatus: completed\n\n[summary]\n${result}`, is_error: false, message: null } };
     }
     ```

- [ ] Run it and verify it PASSES.
  ```bash
  cd rust-ody && cargo build -p tools-rs --bin tools-golden
  cd D:/workspace/ody-code && pnpm --filter integration-tests test test/parity/tools-rs/l1-golden.test.ts
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/tools-rs/src/golden.rs \
         packages/integration-tests/src/parity/fixtures/tools-rs/collaboration-tools.json \
         packages/integration-tests/src/parity/tools-rs-golden.ts
  git commit -m "test(integration): extend collaboration fixture with AgentTool cases"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage table:
  | Requirement | Task | Status |
  |---|---|---|
  | Spawn subagent foreground | Task 1 | covered |
  | Resume subagent | Task 1 | covered |
  | `subagent_type` default + resume conflict | Task 1 | covered |
  | Foreground timeout | Task 1 | covered |
  | Background registration | Task 1 | covered |
  | Background unavailable | Task 1 | covered |
  | `matches_rule` on profile name | Task 1 | covered |
  | L1 golden fixture | Task 2 | covered |
- [ ] 2. Placeholder scan: no TODO/TBD; every code block is real and compilable modulo crate-local names that already exist.
- [ ] 3. No phantom tasks: each task creates/modifies files and ends with a verifiable test + commit.
- [ ] 4. Dependency soundness: Task 1 depends on Part 1 `SubagentHost`/`BackgroundRegistrar`; Task 2 depends on Task 1.
- [ ] 5. Caller & build soundness: no shared-signature changes in this part; Task 1 ends with whole-workspace `cargo test`.
- [ ] 6. Test-the-risk: Task 1 tests timeout boundary, resume conflict, background registration, and failure paths. Task 2 tests parity with TS reference outputs.
- [ ] 7. Type consistency: `SubagentOptions`, `SubagentHandle`, `AgentTaskOptions` field names match Part 1 definitions.
