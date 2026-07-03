# Phase 4.4.4 Part 3 — AskUserQuestionTool Implementation + L1 Fixture

**Goal:** Implement the Rust `AskUserQuestionTool` in `tools-rs`, fix the `QuestionRunFn` signature so background question tasks receive an abort signal, and add an L1 golden fixture covering foreground answers, dismissal, unsupported clients, and background registration.

**Architecture:** `AskUserQuestionTool` holds an `Arc<dyn QuestionProvider>` for foreground reverse-RPC questions and an `Arc<dyn BackgroundRegistrar>` for background question tasks. It validates the input, dispatches to the provider, normalizes the result into JSON answers (or the dismissed note), and maps `NotImplemented` to the unsupported-client message. Background mode registers a `QuestionRunFn` that re-runs the foreground question logic under the background task's own signal.

**Tech Stack:** Rust 2021, `tools-rs` builtins, `serde_json`, existing `QuestionProvider`/`BackgroundRegistrar` boundaries, Vitest/TypeScript golden harness.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/tools-rs/
  src/builtin/collaboration/mod.rs              # update QuestionRunFn signature + mock
  src/builtin/collaboration/ask_user.rs         # AskUserQuestionTool + unit tests
  src/builtin/mod.rs                            # re-export AskUserQuestionTool
  src/golden.rs                                 # add AskUser op + runner
packages/integration-tests/src/parity/
  fixtures/tools-rs/collaboration-tools.json    # extend with ask_user cases
  tools-rs-golden.ts                            # add ask_user op handler
```

## Dependency Overview

```
Part 1: infra.md (trait boundaries)
  │
  ├──► Part 2: skill-tool.md (SkillTool)
  │
  └──► Part 3: ask-user-tool.md
         │
         ├──► Task 1: Fix QuestionRunFn signature + implement AskUserQuestionTool
         │
         └──► Task 2: Add ask_user L1 golden op + fixture + TS handler
```

- **Task 1** is a shared-signature change: `QuestionRunFn` changes shape, so the Part 1 mock `BackgroundRegistrar` and any new code must be updated. It ends with a whole-workspace Rust typecheck.
- **Task 2** depends on Task 1 for the working tool and the updated registrar signature.

## Tasks

### Task 1: Fix `QuestionRunFn` signature and implement `AskUserQuestionTool`

**Depends on:** `2026-06-29-backend-architecture-evolution-phase4-4/infra.md`: Task 3; `2026-06-29-backend-architecture-evolution-phase4-4/skill-tool.md`: Task 1

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs:540-542` (change `QuestionRunFn` to take `AbortSignal`)
- Modify: `rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs:349-358` (update `MockBackgroundRegistrar` test impl)
- Create: `rust-ody/crates/tools-rs/src/builtin/collaboration/ask_user.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs:1-5` (re-export `ask_user::AskUserQuestionTool`)
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs:107-118` (re-export `AskUserQuestionTool`)
- Test: `rust-ody/crates/tools-rs/src/builtin/collaboration/ask_user.rs` (module-level tests)

- [ ] Write the failing tests.
  Create `tools-rs/src/builtin/collaboration/ask_user.rs` with a `#[cfg(test)]` module:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use serde_json::json;
      use std::collections::HashMap;
      use std::sync::{Arc, Mutex};

      use crate::builtin::collaboration::{
          BackgroundError, BackgroundRegistrar, QuestionAnswers, QuestionError, QuestionItem,
          QuestionOption, QuestionProvider, QuestionRequest, QuestionResult, QuestionTaskOptions,
      };
      use crate::builtin::{AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolResult};

      struct TestQuestionProvider {
          response: Mutex<Option<Result<QuestionResult, QuestionError>>>,
      }

      #[async_trait::async_trait]
      impl QuestionProvider for TestQuestionProvider {
          async fn request_question(
              &self,
              _req: QuestionRequest,
              _signal: &AbortSignal,
          ) -> Result<QuestionResult, QuestionError> {
              self.response.lock().unwrap().take().unwrap_or(Ok(QuestionResult::Dismissed))
          }
      }

      struct TestBackgroundRegistrar {
          registrations: Mutex<Vec<(String, QuestionTaskOptions)>>,
          next_id: Mutex<String>,
          fail: bool,
      }

      #[async_trait::async_trait]
      impl BackgroundRegistrar for TestBackgroundRegistrar {
          async fn register_question_task(
              &self,
              description: String,
              _run: crate::builtin::collaboration::QuestionRunFn,
              options: QuestionTaskOptions,
          ) -> Result<String, BackgroundError> {
              if self.fail {
                  return Err(BackgroundError::Message("registrar down".into()));
              }
              self.registrations.lock().unwrap().push((description, options));
              Ok(self.next_id.lock().unwrap().clone())
          }

          async fn register_agent_task(
              &self,
              _completion: crate::builtin::collaboration::AgentCompletion,
              _description: String,
              _options: crate::builtin::collaboration::AgentTaskOptions,
          ) -> Result<String, BackgroundError> {
              unimplemented!()
          }
      }

      fn ctx() -> ExecutableToolContext {
          ExecutableToolContext {
              turn_id: "7".into(),
              tool_call_id: "call_q".into(),
              signal: AbortSignal::new(),
          }
      }

      fn sample_questions() -> Vec<QuestionItem> {
          vec![QuestionItem {
              question: "Pick a color?".into(),
              header: "Style".into(),
              options: vec![
                  QuestionOption { label: "Red".into(), description: "warm".into() },
                  QuestionOption { label: "Blue".into(), description: "cool".into() },
              ],
              multi_select: false,
          }]
      }

      async fn run_foreground(provider_response: Result<QuestionResult, QuestionError>) -> ExecutableToolResult {
          let provider = Arc::new(TestQuestionProvider {
              response: Mutex::new(Some(provider_response)),
          });
          let registrar = Arc::new(TestBackgroundRegistrar {
              registrations: Mutex::new(Vec::new()),
              next_id: Mutex::new("question-00000001".into()),
              fail: false,
          });
          let tool = AskUserQuestionTool::new(provider, registrar, AskUserQuestionOptions::default());
          let exec = tool
              .resolve_execution(json!({"questions": sample_questions()}))
              .unwrap();
          (exec.execute)(ctx()).await
      }

      #[tokio::test]
      async fn foreground_answers_return_json() {
          let mut answers = HashMap::new();
          answers.insert("Pick a color?".into(), json!("Red"));
          let result = run_foreground(Ok(QuestionResult::Answers(QuestionAnswers {
              answers,
              method: Some("enter".into()),
          })))
          .await;
          assert!(!result.is_error, "{:?}", result);
          let parsed: serde_json::Value = serde_json::from_str(&result.to_text()).unwrap();
          assert_eq!(parsed["answers"]["Pick a color?"], "Red");
      }

      #[tokio::test]
      async fn foreground_dismissed_returns_note() {
          let result = run_foreground(Ok(QuestionResult::Dismissed)).await;
          assert!(!result.is_error);
          let parsed: serde_json::Value = serde_json::from_str(&result.to_text()).unwrap();
          assert!(parsed["answers"].as_object().unwrap().is_empty());
          assert!(parsed["note"].as_str().unwrap().contains("dismissed"));
      }

      #[tokio::test]
      async fn unsupported_client_returns_error() {
          let result = run_foreground(Err(QuestionError::NotImplemented)).await;
          assert!(result.is_error);
          assert!(result.to_text().contains("does not support interactive questions"));
      }

      #[tokio::test]
      async fn background_mode_registers_task() {
          let provider = Arc::new(TestQuestionProvider {
              response: Mutex::new(None),
          });
          let registrar = Arc::new(TestBackgroundRegistrar {
              registrations: Mutex::new(Vec::new()),
              next_id: Mutex::new("question-00000001".into()),
              fail: false,
          });
          let tool = AskUserQuestionTool::new(
              provider,
              Arc::clone(&registrar) as Arc<dyn BackgroundRegistrar>,
              AskUserQuestionOptions { background_ask_enabled: true },
          );
          let exec = tool
              .resolve_execution(json!({
                  "questions": sample_questions(),
                  "background": true,
              }))
              .unwrap();
          let result = (exec.execute)(ctx()).await;
          assert!(!result.is_error, "{:?}", result);
          let regs = registrar.registrations.lock().unwrap();
          assert_eq!(regs.len(), 1);
          assert_eq!(regs[0].0, "Pick a color?");
          assert_eq!(regs[0].1.question_count, 1);
          assert_eq!(regs[0].1.tool_call_id, "call_q");
          assert!(result.to_text().contains("task_id: question-00000001"));
      }

      #[tokio::test]
      async fn background_disabled_ignores_flag() {
          let provider = Arc::new(TestQuestionProvider {
              response: Mutex::new(Some(Ok(QuestionResult::Dismissed))),
          });
          let registrar = Arc::new(TestBackgroundRegistrar {
              registrations: Mutex::new(Vec::new()),
              next_id: Mutex::new("question-00000001".into()),
              fail: false,
          });
          let tool = AskUserQuestionTool::new(
              provider,
              Arc::clone(&registrar) as Arc<dyn BackgroundRegistrar>,
              AskUserQuestionOptions { background_ask_enabled: false },
          );
          let exec = tool
              .resolve_execution(json!({
                  "questions": sample_questions(),
                  "background": true,
              }))
              .unwrap();
          let result = (exec.execute)(ctx()).await;
          assert!(!result.is_error);
          assert!(registrar.registrations.lock().unwrap().is_empty());
      }
  }
  ```

- [ ] Run them and verify they FAIL.
  ```bash
  cd rust-ody && cargo test -p tools-rs ask_user_question
  ```
  Expected failure: `AskUserQuestionTool`, `AskUserQuestionOptions`, updated `QuestionRunFn`, and `register_question_task` signature do not match.

- [ ] Write the minimal implementation.
  1. Update `QuestionRunFn` in `tools-rs/src/builtin/collaboration/mod.rs`:
     ```rust
     pub type QuestionRunFn = Arc<
         dyn Fn(AbortSignal) -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>> + Send + Sync,
     >;
     ```
  2. Update the `MockBackgroundRegistrar` test implementation in the same file to match the new `QuestionRunFn` type:
     ```rust
     struct MockBackgroundRegistrar;
     #[async_trait::async_trait]
     impl BackgroundRegistrar for MockBackgroundRegistrar {
         async fn register_question_task(
             &self,
             _description: String,
             _run: QuestionRunFn,
             _options: QuestionTaskOptions,
         ) -> Result<String, BackgroundError> {
             Ok("question-12345678".into())
         }
         async fn register_agent_task(
             &self,
             _completion: AgentCompletion,
             _description: String,
             _options: AgentTaskOptions,
         ) -> Result<String, BackgroundError> {
             Ok("agent-12345678".into())
         }
     }
     ```
  3. Create `tools-rs/src/builtin/collaboration/ask_user.rs`:
     ```rust
     use std::collections::HashMap;
     use std::sync::Arc;

     use serde_json::Value;

     use crate::builtin::collaboration::{
         BackgroundError, BackgroundRegistrar, QuestionAnswers, QuestionError, QuestionItem,
         QuestionOption, QuestionProvider, QuestionRequest, QuestionResult, QuestionRunFn,
         QuestionTaskOptions,
     };
     use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};
     use crate::schema::InputSchema;
     use crate::tool_accesses::ToolAccesses;

     const QUESTION_DISMISSED_MESSAGE: &str = "User dismissed the question without answering.";
     const QUESTION_UNSUPPORTED_FAILURE_MESSAGE: &str =
         "The connected client does not support interactive questions. Do NOT call this tool again. Ask the user directly in your text response instead.";

     #[derive(Debug, Clone, Default)]
     pub struct AskUserQuestionOptions {
         pub background_ask_enabled: bool,
     }

     pub struct AskUserQuestionTool {
         question_provider: Arc<dyn QuestionProvider>,
         background_registrar: Arc<dyn BackgroundRegistrar>,
         options: AskUserQuestionOptions,
     }

     impl AskUserQuestionTool {
         pub fn new(
             question_provider: Arc<dyn QuestionProvider>,
             background_registrar: Arc<dyn BackgroundRegistrar>,
             options: AskUserQuestionOptions,
         ) -> Self {
             Self {
                 question_provider,
                 background_registrar,
                 options,
             }
         }
     }

     impl BuiltinTool for AskUserQuestionTool {
         fn name(&self) -> &str {
             "AskUserQuestion"
         }

         fn description(&self) -> &str {
             let base = concat!(
                 "Use this tool when you need to ask the user questions with structured options during execution. This allows you to:\n",
                 "1. Collect user preferences or requirements before proceeding\n",
                 "2. Resolve ambiguous or underspecified instructions\n",
                 "3. Let the user decide between implementation approaches as you work\n",
                 "4. Present concrete options when multiple valid directions exist\n\n",
                 "**When NOT to use:**\n",
                 "- When you can infer the answer from context — be decisive and proceed\n",
                 "- Trivial decisions that don't materially affect the outcome\n\n",
                 "Overusing this tool interrupts the user's flow. Only use it when the user's input genuinely changes your next action.\n\n",
                 "**Usage notes:**\n",
                 "- Users always have an \"Other\" option for custom input — don't create one yourself\n",
                 "- Use multi_select to allow multiple answers to be selected for a question\n",
                 "- Keep option labels concise (1-5 words), use descriptions for trade-offs and details\n",
                 "- Each question should have 2-4 meaningful, distinct options\n",
                 "- You can ask 1-4 questions at a time; group related questions to minimize interruptions\n",
                 "- If you recommend a specific option, list it first and append \"(Recommended)\" to its label"
             );
             if self.options.background_ask_enabled {
                 return Box::leak(format!(
                     "{}- Set background=true when you can keep working without the answer. This starts a background question task and returns a task_id immediately. The answer arrives automatically in a later turn — you do not need to poll, sleep, or check on it. Continue with other work; never fabricate or predict the answer.",
                     base
                 ).into_boxed_str());
             }
             base
         }

         fn parameters(&self) -> Value {
             let mut questions_schema = InputSchema::array(
                 InputSchema::object(vec![
                     (
                         "question",
                         InputSchema::string().description("A specific, actionable question. End with '?'."),
                     ),
                     (
                         "header",
                         InputSchema::string()
                             .optional()
                             .description("Short category tag (max 12 chars, e.g. 'Auth', 'Style')."),
                     ),
                     (
                         "options",
                         InputSchema::array(
                             InputSchema::object(vec![
                                 (
                                     "label",
                                     InputSchema::string()
                                         .description("Concise display text (1-5 words). If recommended, append '(Recommended)'."),
                                 ),
                                 (
                                     "description",
                                     InputSchema::string()
                                         .optional()
                                         .description("Brief explanation of trade-offs or implications."),
                                 ),
                             ])
                             .build(),
                         )
                         .min(2.0)
                         .max(4.0)
                         .description("2-4 meaningful, distinct options. Do NOT include an 'Other' option — the system adds one automatically."),
                     ),
                     (
                         "multi_select",
                         InputSchema::boolean()
                             .optional()
                             .default(serde_json::json!(false))
                             .description("Whether the user can select multiple options."),
                     ),
                 ])
                 .build(),
             )
             .min(1.0)
             .max(4.0)
             .description("The questions to ask the user (1-4 questions).");

             if self.options.background_ask_enabled {
                 questions_schema = questions_schema.with_property(
                     "background",
                     InputSchema::boolean()
                         .optional()
                         .default(serde_json::json!(false))
                         .description("Set true to ask in the background and return immediately with a background task_id. Use TaskOutput to read the answer later."),
                 );
             }

             questions_schema.build()
         }

         fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
             let questions: Vec<QuestionItem> = serde_json::from_value(
                 args.get("questions").cloned().unwrap_or(Value::Array(vec![])),
             )
             .map_err(|e| ToolError::InvalidArgs(format!("invalid questions: {}", e)))?;

             if questions.is_empty() {
                 return Err(ToolError::InvalidArgs("at least one question is required".into()));
             }

             let background = self.options.background_ask_enabled
                 && args.get("background").and_then(Value::as_bool).unwrap_or(false);

             let provider = Arc::clone(&self.question_provider);
             let registrar = Arc::clone(&self.background_registrar);

             Ok(ToolExecution {
                 accesses: ToolAccesses::none(),
                 description: if background {
                     format!("Starting background question: {}", question_description(&questions))
                 } else {
                     "Asking user questions".into()
                 },
                 approval_rule: "AskUserQuestion".into(),
                 matches_rule: None,
                 execute: Box::new(move |ctx| {
                     let provider = Arc::clone(&provider);
                     let registrar = Arc::clone(&registrar);
                     let questions = questions.clone();
                     let tool_call_id = ctx.tool_call_id.clone();
                     let turn_id = ctx.turn_id.clone();
                     Box::pin(async move {
                         if background {
                             execute_background(
                                 registrar,
                                 provider,
                                 questions,
                                 tool_call_id,
                                 turn_id,
                                 ctx.signal,
                             )
                             .await
                         } else {
                             execute_foreground(provider, questions, tool_call_id, turn_id, ctx.signal).await
                         }
                     })
                 }),
             })
         }
     }

     async fn execute_foreground(
         provider: Arc<dyn QuestionProvider>,
         questions: Vec<QuestionItem>,
         tool_call_id: String,
         turn_id: String,
         signal: crate::builtin::AbortSignal,
     ) -> ExecutableToolResult {
         if signal.aborted() {
             return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
         }

         let req = QuestionRequest {
             turn_id: numeric_turn_id(&turn_id),
             tool_call_id,
             questions,
         };

         match provider.request_question(req, &signal).await {
             Ok(QuestionResult::Answers(answers)) => {
                 ExecutableToolResult::ok_text(serde_json::to_string(&serde_json::json!({
                     "answers": answers.answers,
                 })).unwrap())
             }
             Ok(QuestionResult::Dismissed) => dismissed_result(),
             Err(QuestionError::NotImplemented) => ExecutableToolResult::error_text(
                 QUESTION_UNSUPPORTED_FAILURE_MESSAGE.into(),
                 "Question unsupported".into(),
             ),
             Err(QuestionError::Aborted) => {
                 ExecutableToolResult::error_text("Aborted".into(), "Aborted".into())
             }
             Err(_) => dismissed_result(),
         }
     }

     async fn execute_background(
         registrar: Arc<dyn BackgroundRegistrar>,
         provider: Arc<dyn QuestionProvider>,
         questions: Vec<QuestionItem>,
         tool_call_id: String,
         turn_id: String,
         signal: crate::builtin::AbortSignal,
     ) -> ExecutableToolResult {
         if signal.aborted() {
             return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
         }

         let description = question_description(&questions);
         let run: QuestionRunFn = Arc::new(move |task_signal| {
             let provider = Arc::clone(&provider);
             let questions = questions.clone();
             let tool_call_id = tool_call_id.clone();
             let turn_id = turn_id.clone();
             Box::pin(async move {
                 Ok(execute_foreground(provider, questions, tool_call_id, turn_id, task_signal).await)
             })
         });

         match registrar
             .register_question_task(
                 description.clone(),
                 run,
                 QuestionTaskOptions {
                     question_count: questions.len() as u32,
                     tool_call_id: tool_call_id.clone(),
                 },
             )
             .await
         {
             Ok(task_id) => ExecutableToolResult::ok_text(format!(
                 "task_id: {}\ndescription: {}\nstatus: running\nautomatic_notification: true\nnext_step: Continue your current work; the answer will arrive automatically when the user responds.\nnext_step: Use TaskOutput with this task_id for a non-blocking status/answer snapshot.\nnext_step: Use TaskStop only if the question should be cancelled.\nhuman_shell_hint: The pending question is also visible in /tasks.",
                 task_id, description
             )),
             Err(BackgroundError::Unavailable) => ExecutableToolResult::error_text(
                 "Background question tasks are not available.".into(),
                 "Background unavailable".into(),
             ),
             Err(BackgroundError::Message(m)) => ExecutableToolResult::error_text(m.clone(), m),
         }
     }

     fn dismissed_result() -> ExecutableToolResult {
         ExecutableToolResult::ok_text(
             serde_json::to_string(&serde_json::json!({
                 "answers": {},
                 "note": QUESTION_DISMISSED_MESSAGE,
             }))
             .unwrap(),
         )
     }

     fn numeric_turn_id(turn_id: &str) -> Option<i64> {
         if turn_id.trim().is_empty() {
             return None;
         }
         turn_id.parse::<i64>().ok()
     }

     fn question_description(questions: &[QuestionItem]) -> String {
         let first = questions
             .first()
             .map(|q| q.question.trim())
             .filter(|s| !s.is_empty())
             .unwrap_or("Ask user question");
         if questions.len() <= 1 {
             first.to_string()
         } else {
             format!("{} (+{} more)", first, questions.len() - 1)
         }
     }

     #[cfg(test)]
     mod tests {
         // ... same as the failing-test block above ...
     }
     ```
     Note: `InputSchema::with_property` may not exist on the builder. If the real `InputSchema` builder does not support adding properties to an object schema after construction, build the object schema directly with the optional `background` property included conditionally.
  4. Re-export from `tools-rs/src/builtin/collaboration/mod.rs`:
     ```rust
     pub mod ask_user;
     pub use ask_user::{AskUserQuestionTool, AskUserQuestionOptions};
     ```
  5. Re-export from `tools-rs/src/builtin/mod.rs`:
     ```rust
     pub use collaboration::ask_user::{AskUserQuestionTool, AskUserQuestionOptions};
     ```

- [ ] Run the tests and verify they PASS.
  ```bash
  cd rust-ody && cargo test -p tools-rs ask_user_question
  ```

- [ ] Whole-tree typecheck (shared-signature task).
  ```bash
  cd rust-ody && cargo test
  ```

- [ ] Commit.
  ```bash
  git add rust-ody/crates/tools-rs/src/builtin/collaboration/mod.rs \
         rust-ody/crates/tools-rs/src/builtin/collaboration/ask_user.rs \
         rust-ody/crates/tools-rs/src/builtin/mod.rs
  git commit -m "feat(tools-rs): implement AskUserQuestionTool with foreground and background support"
  ```

### Task 2: Add `ask_user` L1 golden op + fixture

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/golden.rs:48-262` (add `AskUser` op variant)
- Modify: `rust-ody/crates/tools-rs/src/golden.rs:350+` (add `Op::AskUser` handler)
- Modify: `packages/integration-tests/src/parity/fixtures/tools-rs/collaboration-tools.json` (extend with ask_user cases)
- Modify: `packages/integration-tests/src/parity/tools-rs-golden.ts:33-67` (add `ask_user` to `GoldenOp`)
- Modify: `packages/integration-tests/src/parity/tools-rs-golden.ts:147+` (add `ask_user` handler)

- [ ] Write the failing test / fixture.
  Extend `packages/integration-tests/src/parity/fixtures/tools-rs/collaboration-tools.json` with new cases after the `skill_*` cases:
  ```json
  {
    "name": "ask_user_answered",
    "op": {
      "type": "ask_user",
      "questions": [
        {
          "question": "Pick a color?",
          "header": "Style",
          "options": [
            { "label": "Red", "description": "warm" },
            { "label": "Blue", "description": "cool" }
          ],
          "multi_select": false
        }
      ],
      "background": false,
      "provider_response": "answered",
      "answers": { "Pick a color?": "Red" },
      "method": "enter"
    },
    "expected": null
  },
  {
    "name": "ask_user_dismissed",
    "op": {
      "type": "ask_user",
      "questions": [
        {
          "question": "Pick a color?",
          "header": "Style",
          "options": [
            { "label": "Red", "description": "warm" },
            { "label": "Blue", "description": "cool" }
          ],
          "multi_select": false
        }
      ],
      "background": false,
      "provider_response": "dismissed"
    },
    "expected": null
  },
  {
    "name": "ask_user_unsupported",
    "op": {
      "type": "ask_user",
      "questions": [
        {
          "question": "Pick a color?",
          "header": "Style",
          "options": [
            { "label": "Red", "description": "warm" },
            { "label": "Blue", "description": "cool" }
          ],
          "multi_select": false
        }
      ],
      "background": false,
      "provider_response": "unsupported"
    },
    "expected": null
  },
  {
    "name": "ask_user_background",
    "op": {
      "type": "ask_user",
      "questions": [
        {
          "question": "Pick a color?",
          "header": "Style",
          "options": [
            { "label": "Red", "description": "warm" },
            { "label": "Blue", "description": "cool" }
          ],
          "multi_select": false
        }
      ],
      "background": true,
      "registrar_response": "ok",
      "task_id": "question-00000001"
    },
    "expected": null
  }
  ```
  Add the Rust `AskUser` op variant in `tools-rs/src/golden.rs`:
  ```rust
  #[serde(rename = "ask_user")]
  AskUser {
      questions: Vec<crate::builtin::collaboration::QuestionItem>,
      #[serde(default)]
      background: Option<bool>,
      #[serde(default)]
      provider_response: Option<String>,
      #[serde(default)]
      answers: Option<HashMap<String, serde_json::Value>>,
      #[serde(default)]
      method: Option<String>,
      #[serde(default)]
      registrar_response: Option<String>,
      #[serde(default)]
      task_id: Option<String>,
  },
  ```
  Add to `GoldenOp` in `tools-rs-golden.ts`:
  ```typescript
  | { type: 'ask_user'; questions: QuestionItemFixture[]; background?: boolean | null; provider_response?: string | null; answers?: Record<string, string> | null; method?: string | null; registrar_response?: string | null; task_id?: string | null }
  ```
  with helper type:
  ```typescript
  interface QuestionItemFixture {
    question: string;
    header?: string | null;
    options: { label: string; description?: string | null }[];
    multi_select?: boolean;
  }
  ```

- [ ] Run it and verify it FAILS.
  ```bash
  cd rust-ody && cargo build -p tools-rs --bin tools-golden
  ```
  Expected failure: `AskUser` variant / handler do not exist; TS type error for unknown `ask_user` op.

- [ ] Write the minimal implementation.
  1. In `tools-rs/src/golden.rs`, add mock question provider and background registrar for fixtures:
     ```rust
     struct FixtureQuestionProvider {
         response: QuestionResult,
     }

     #[async_trait::async_trait]
     impl crate::builtin::collaboration::QuestionProvider for FixtureQuestionProvider {
         async fn request_question(
             &self,
             _req: crate::builtin::collaboration::QuestionRequest,
             _signal: &crate::builtin::AbortSignal,
         ) -> Result<QuestionResult, crate::builtin::collaboration::QuestionError> {
             Ok(self.response.clone())
         }
     }

     struct FixtureBackgroundRegistrar {
         next_id: String,
         fail: bool,
     }

     #[async_trait::async_trait]
     impl crate::builtin::collaboration::BackgroundRegistrar for FixtureBackgroundRegistrar {
         async fn register_question_task(
             &self,
             _description: String,
             _run: crate::builtin::collaboration::QuestionRunFn,
             _options: crate::builtin::collaboration::QuestionTaskOptions,
         ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
             if self.fail {
                 return Err(crate::builtin::collaboration::BackgroundError::Message("registrar down".into()));
             }
             Ok(self.next_id.clone())
         }
         async fn register_agent_task(
             &self,
             _completion: crate::builtin::collaboration::AgentCompletion,
             _description: String,
             _options: crate::builtin::collaboration::AgentTaskOptions,
         ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
             unimplemented!()
         }
     }
     ```
  2. Add the `Op::AskUser` arm in `run_case_sync`:
     ```rust
     Op::AskUser {
         questions,
         background,
         provider_response,
         answers,
         method,
         registrar_response,
         task_id,
     } => {
         use crate::builtin::collaboration::{
             AskUserQuestionTool, AskUserQuestionOptions, QuestionAnswers, QuestionResult,
         };

         let result = match provider_response.as_deref() {
             Some("dismissed") => QuestionResult::Dismissed,
             _ => {
                 let answers = answers.clone().unwrap_or_default();
                 QuestionResult::Answers(QuestionAnswers { answers, method: method.clone() })
             }
         };
         let provider = std::sync::Arc::new(FixtureQuestionProvider { response: result });
         let registrar = std::sync::Arc::new(FixtureBackgroundRegistrar {
             next_id: task_id.clone().unwrap_or_else(|| "question-00000001".into()),
             fail: registrar_response.as_deref() == Some("fail"),
         });
         let background_enabled = background == Some(true);
         let tool = AskUserQuestionTool::new(
             provider,
             registrar,
             AskUserQuestionOptions { background_ask_enabled: background_enabled },
         );
         match tool.resolve_execution(serde_json::json!({
             "questions": questions,
             "background": background.unwrap_or(false),
         })) {
             Ok(exec) => {
                 let ctx = ExecutableToolContext {
                     turn_id: "7".into(),
                     tool_call_id: "call_q".into(),
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
  3. In `tools-rs-golden.ts`, add the `ask_user` handler in `runCase`:
     ```typescript
     case 'ask_user': {
       const questions = op.questions.map((q) => ({
         question: q.question,
         header: q.header ?? '',
         options: q.options.map((o) => ({
           label: o.label,
           description: o.description ?? '',
         })),
         multi_select: q.multi_select ?? false,
       }));
       const background = op.background ?? false;
       if (background) {
         const first = questions[0]?.question.trim() ?? 'Ask user question';
         const description = questions.length <= 1 ? first : `${first} (+${String(questions.length - 1)} more)`;
         const taskId = op.task_id ?? 'question-00000001';
         const output = `task_id: ${taskId}\ndescription: ${description}\nstatus: running\nautomatic_notification: true\nnext_step: Continue your current work; the answer will arrive automatically when the user responds.\nnext_step: Use TaskOutput with this task_id for a non-blocking status/answer snapshot.\nnext_step: Use TaskStop only if the question should be cancelled.\nhuman_shell_hint: The pending question is also visible in /tasks.`;
         return { result: { output, is_error: false, message: `Started ${taskId}` } };
       }
       const response = op.provider_response ?? 'dismissed';
       if (response === 'unsupported') {
         return { result: { output: 'The connected client does not support interactive questions. Do NOT call this tool again. Ask the user directly in your text response instead.', is_error: true, message: 'Question unsupported' } };
       }
       if (response === 'dismissed') {
         return { result: { output: JSON.stringify({ answers: {}, note: 'User dismissed the question without answering.' }), is_error: false, message: null } };
       }
       const answers = op.answers ?? {};
       return { result: { output: JSON.stringify({ answers }), is_error: false, message: null } };
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
  git commit -m "test(integration): extend collaboration fixture with AskUserQuestionTool cases"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage table:
  | Requirement | Task | Status |
  |---|---|---|
  | Foreground `requestQuestion` + answer normalization | Task 1 | covered |
  | Dismissed handling | Task 1 | covered |
  | Unsupported-client (`NotImplemented`) handling | Task 1 | covered |
  | Background question task registration | Task 1 | covered |
  | Background flag gating | Task 1 | covered |
  | `QuestionRunFn` receives abort signal | Task 1 | covered |
  | L1 golden fixture | Task 2 | covered |
- [ ] 2. Placeholder scan: no TODO/TBD; every code block is real and compilable modulo crate-local names that already exist.
- [ ] 3. No phantom tasks: each task creates/modifies files and ends with a verifiable test + commit.
- [ ] 4. Dependency soundness: Task 1 depends on Part 1 trait boundaries and Part 2 shared `matches_rule`/`ToolExecution`; Task 2 depends on Task 1.
- [ ] 5. Caller & build soundness: Task 1 is the shared-signature task; it updates `QuestionRunFn`, the Part 1 `MockBackgroundRegistrar`, and ends with `cargo test` across the Rust workspace.
- [ ] 6. Test-the-risk: Task 1 tests state mutations (background registration) and boundary conditions (unsupported, dismissed, background flag gating). Task 2 tests parity with TS reference outputs.
- [ ] 7. Type consistency: `QuestionRunFn` signature matches in trait definition, mock, `AskUserQuestionTool`, and golden fixture runner.
