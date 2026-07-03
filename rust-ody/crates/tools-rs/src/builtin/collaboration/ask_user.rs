use std::sync::Arc;

use serde_json::Value;

use crate::builtin::collaboration::{
    BackgroundError, BackgroundRegistrar, QuestionError, QuestionItem, QuestionProvider,
    QuestionRequest, QuestionResult, QuestionRunFn, QuestionTaskOptions,
};
use crate::builtin::{AbortSignal, BuiltinTool, ExecutableToolResult, ToolError, ToolExecution};
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
        let base = "Use this tool when you need to ask the user questions with structured options during execution. This allows you to:\n1. Collect user preferences or requirements before proceeding\n2. Resolve ambiguous or underspecified instructions\n3. Let the user decide between implementation approaches as you work\n4. Present concrete options when multiple valid directions exist\n\n**When NOT to use:**\n- When you can infer the answer from context — be decisive and proceed\n- Trivial decisions that don't materially affect the outcome\n\nOverusing this tool interrupts the user's flow. Only use it when the user's input genuinely changes your next action.\n\n**Usage notes:**\n- Users always have an \"Other\" option for custom input — don't create one yourself\n- Use multi_select to allow multiple answers to be selected for a question\n- Keep option labels concise (1-5 words), use descriptions for trade-offs and details\n- Each question should have 2-4 meaningful, distinct options\n- You can ask 1-4 questions at a time; group related questions to minimize interruptions\n- If you recommend a specific option, list it first and append \"(Recommended)\" to its label";
        if self.options.background_ask_enabled {
            Box::leak(
                format!(
                    "{}- Set background=true when you can keep working without the answer. This starts a background question task and returns a task_id immediately.",
                    base
                )
                .into_boxed_str(),
            )
        } else {
            base
        }
    }

    fn parameters(&self) -> Value {
        let obj = InputSchema::object(vec![
            (
                "question",
                InputSchema::string()
                    .description("A specific, actionable question. End with '?'."),
            ),
            (
                "header",
                InputSchema::string()
                    .optional()
                    .description("Short category tag (max 12 chars, e.g. 'Auth', 'Style')."),
            ),
            (
                "options",
                InputSchema::array(InputSchema::object(vec![
                        (
                            "label",
                            InputSchema::string().description(
                                "Concise display text (1-5 words). If recommended, append '(Recommended)'.",
                            ),
                        ),
                        (
                            "description",
                            InputSchema::string()
                                .optional()
                                .description("Brief explanation of trade-offs or implications."),
                        ),
                    ])
                )
                .min(2.0)
                .max(4.0)
                .description(
                    "2-4 meaningful, distinct options. Do NOT include an 'Other' option — the system adds one automatically.",
                ),
            ),
            (
                "multi_select",
                InputSchema::boolean()
                    .optional()
                    .default(serde_json::json!(false))
                    .description("Whether the user can select multiple options."),
            ),
        ])
        .build();
        // Note: InputSchema may not support nested objects perfectly. If build fails, manually construct the JSON schema.
        let questions_schema = serde_json::json!({
            "type": "array",
            "minItems": 1,
            "maxItems": 4,
            "items": obj,
            "description": "The questions to ask the user (1-4 questions)."
        });
        if self.options.background_ask_enabled {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "questions": questions_schema,
                    "background": {
                        "type": "boolean",
                        "default": false,
                        "description": "Set true to ask in the background and return immediately with a background task_id."
                    }
                },
                "required": ["questions"]
            })
        } else {
            serde_json::json!({
                "type": "object",
                "properties": { "questions": questions_schema },
                "required": ["questions"]
            })
        }
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let questions: Vec<QuestionItem> = serde_json::from_value(
            args.get("questions")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        )
        .map_err(|e| ToolError::InvalidArgs(format!("invalid questions: {}", e)))?;
        if questions.is_empty() {
            return Err(ToolError::InvalidArgs(
                "at least one question is required".into(),
            ));
        }
        let background = self.options.background_ask_enabled
            && args
                .get("background")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let provider = Arc::clone(&self.question_provider);
        let registrar = Arc::clone(&self.background_registrar);

        Ok(ToolExecution {
            accesses: ToolAccesses::none(),
            description: if background {
                format!(
                    "Starting background question: {}",
                    question_description(&questions)
                )
            } else {
                "Asking user questions".into()
            },
            approval_rule: "AskUserQuestion".into(),
            matches_rule: None,
            display: None,
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
                        execute_foreground(provider, questions, tool_call_id, turn_id, ctx.signal)
                            .await
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
    signal: AbortSignal,
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
        Ok(QuestionResult::Answers(answers)) => ExecutableToolResult::ok_text(
            serde_json::to_string(&serde_json::json!({"answers": answers.answers})).unwrap(),
        ),
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
    signal: AbortSignal,
) -> ExecutableToolResult {
    if signal.aborted() {
        return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
    }
    let description = question_description(&questions);
    let questions_len = questions.len() as u32;
    let tool_call_id_for_opts = tool_call_id.clone();
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
                question_count: questions_len,
                tool_call_id: tool_call_id_for_opts,
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
    turn_id.trim().parse::<i64>().ok()
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
            self.response
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(QuestionResult::Dismissed))
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
            self.registrations
                .lock()
                .unwrap()
                .push((description, options));
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
            metadata: None,
        }
    }

    fn sample_questions() -> Vec<QuestionItem> {
        vec![QuestionItem {
            question: "Pick a color?".into(),
            header: "Style".into(),
            options: vec![
                QuestionOption {
                    label: "Red".into(),
                    description: "warm".into(),
                },
                QuestionOption {
                    label: "Blue".into(),
                    description: "cool".into(),
                },
            ],
            multi_select: false,
        }]
    }

    async fn run_foreground(
        provider_response: Result<QuestionResult, QuestionError>,
    ) -> ExecutableToolResult {
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
        let mut answers_map = HashMap::new();
        answers_map.insert("Pick a color?".into(), json!("Red"));
        let result = run_foreground(Ok(QuestionResult::Answers(QuestionAnswers {
            answers: answers_map,
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
        assert!(result
            .to_text()
            .contains("does not support interactive questions"));
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
            AskUserQuestionOptions {
                background_ask_enabled: true,
            },
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
            AskUserQuestionOptions {
                background_ask_enabled: false,
            },
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
