use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::builtin::collaboration::{
    AgentTaskOptions, BackgroundError, BackgroundRegistrar, SubagentError, SubagentHandle,
    SubagentHost, SubagentOptions,
};
use crate::builtin::{AbortSignal, BuiltinTool, ExecutableToolResult, ToolError, ToolExecution};
use crate::policies::rule_match::matches_glob_rule_subject;
use crate::tool_accesses::ToolAccesses;

const BACKGROUND_AGENT_UNAVAILABLE: &str =
    "Background agent execution is not available for this agent because TaskList, TaskOutput, and TaskStop are not enabled.";

#[derive(Debug, Clone, Default)]
pub struct AgentToolOptions {
    pub subagent_profiles: Option<std::collections::HashMap<String, String>>,
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
    fn name(&self) -> &str {
        "Agent"
    }

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
                format!(
                    "\n\nAvailable agent types (pass via subagent_type):\n{}",
                    lines.join("\n")
                )
            })
            .unwrap_or_default();
        Box::leak(format!("{}{}{}", base, background_note, type_lines).into_boxed_str())
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Full task prompt for the subagent"
                },
                "description": {
                    "type": "string",
                    "description": "Short task description (3-5 words) for UI display"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "One of the available agent types. Defaults to \"coder\" when omitted."
                },
                "resume": {
                    "type": "string",
                    "description": "Optional agent ID to resume instead of creating a new instance"
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, return immediately without waiting for completion."
                },
                "timeout": {
                    "type": "integer",
                    "minimum": 30,
                    "maximum": 3600,
                    "description": "Timeout in seconds for the agent task (min 30s, max 3600s)."
                }
            },
            "required": ["prompt", "description"],
            "additionalProperties": false
        })
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
        let requested_profile = args
            .get("subagent_type")
            .and_then(Value::as_str)
            .map(String::from);
        let resume = args
            .get("resume")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let run_in_background = args
            .get("run_in_background")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let timeout_secs = args.get("timeout").and_then(Value::as_u64);

        let profile_name = if let Some(resume_id) = &resume {
            self.host
                .get_profile_name(resume_id)
                .unwrap_or_else(|| "subagent".into())
        } else {
            requested_profile.clone().unwrap_or_else(|| "coder".into())
        };
        let prefix = if run_in_background {
            "Launching background"
        } else {
            "Launching"
        };

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
            display: None,
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
            "Cannot set subagent_type when resuming an existing agent. Resume by agent id only."
                .into(),
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
        let abort = Arc::new(move || {
            background_signal.abort();
        });
        let completion: crate::builtin::collaboration::AgentCompletion = handle.completion;
        match registrar
            .register_agent_task(
                completion,
                description.clone(),
                AgentTaskOptions {
                    timeout_ms: timeout_secs
                        .map(|s| s * 1000)
                        .or(Some(host.background_task_timeout_ms())),
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
            Err(BackgroundError::Message(m)) => {
                ExecutableToolResult::error_text(m.clone(), m)
            }
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

fn deadline_signal(
    parent: AbortSignal,
    timeout_ms: u64,
) -> (AbortSignal, tokio::task::JoinHandle<()>) {
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
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    use crate::builtin::collaboration::{
        AgentCompletion, AgentTaskOptions, BackgroundError, BackgroundRegistrar, QuestionRunFn,
        QuestionTaskOptions, SubagentError, SubagentHandle, SubagentHost, SubagentOptions,
        SubagentResult, SubagentUsage,
    };
    use crate::builtin::{AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolResult};

    struct TestSubagentHost {
        behavior: Mutex<HostBehavior>,
    }

    enum HostBehavior {
        Success {
            agent_id: String,
            profile_name: String,
            result: String,
        },
        Fail {
            error: SubagentError,
        },
        Timeout,
    }

    #[async_trait::async_trait]
    impl SubagentHost for TestSubagentHost {
        async fn spawn(
            &self,
            profile: &str,
            options: SubagentOptions,
        ) -> Result<SubagentHandle, SubagentError> {
            self.make_handle(profile, options.signal)
        }
        async fn resume(
            &self,
            _agent_id: &str,
            options: SubagentOptions,
        ) -> Result<SubagentHandle, SubagentError> {
            self.make_handle("subagent", options.signal)
        }
        fn get_profile_name(&self, _agent_id: &str) -> Option<String> {
            None
        }
        fn background_task_timeout_ms(&self) -> u64 {
            600_000
        }
        fn cancel_all(&self, _reason: &str) {}
    }

    impl TestSubagentHost {
        fn make_handle(
            &self,
            profile_name: &str,
            signal: AbortSignal,
        ) -> Result<SubagentHandle, SubagentError> {
            let behavior = self.behavior.lock().unwrap();
            match &*behavior {
                HostBehavior::Success {
                    agent_id, result, ..
                } => {
                    let agent_id = agent_id.clone();
                    let result = result.clone();
                    Ok(SubagentHandle {
                        agent_id,
                        profile_name: profile_name.into(),
                        completion: Box::pin(async move {
                            Ok(SubagentResult {
                                result,
                                usage: Some(SubagentUsage {
                                    input: 10,
                                    output: 20,
                                    cache_read: None,
                                    cache_write: None,
                                }),
                            })
                        }),
                    })
                }
                HostBehavior::Fail { error } => Err(error.clone()),
                HostBehavior::Timeout => Ok(SubagentHandle {
                    agent_id: "agent-timeout".into(),
                    profile_name: profile_name.into(),
                    completion: Box::pin(async move {
                        // Race a never-resolving future against the abort signal.
                        // When the signal fires, the completion returns an error
                        // so the caller can detect the timeout.
                        tokio::select! {
                            _ = futures::future::pending::<
                                Result<SubagentResult, SubagentError>,
                            >() => unreachable!(),
                            _ = wait_abort_signal(signal) => {
                                Err(SubagentError::Message("aborted".into()))
                            }
                        }
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
            _run: QuestionRunFn,
            _options: QuestionTaskOptions,
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
            self.registrations
                .lock()
                .unwrap()
                .push((options.agent_id.clone(), options));
            Ok(self.next_id.lock().unwrap().clone())
        }
    }

    fn ctx() -> ExecutableToolContext {
        ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_a".into(),
            signal: AbortSignal::new(),
            metadata: None,
        }
    }

    async fn wait_abort_signal(signal: AbortSignal) {
        while !signal.aborted() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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
        let tool = AgentTool::new(
            host,
            None::<Arc<dyn BackgroundRegistrar>>,
            AgentToolOptions::default(),
        );
        let result = run(
            &tool,
            json!({"prompt":"Do something","description":"Test agent","subagent_type":"coder"}),
        )
        .await;
        assert!(!result.is_error, "{:?}", result);
        assert!(result.to_text().contains("agent_id: agent-123"));
        assert!(result.to_text().contains("status: completed"));
    }

    #[tokio::test]
    async fn foreground_failure_returns_error() {
        let host = Arc::new(TestSubagentHost {
            behavior: Mutex::new(HostBehavior::Fail {
                error: SubagentError::Message("boom".into()),
            }),
        });
        let tool = AgentTool::new(
            host,
            None::<Arc<dyn BackgroundRegistrar>>,
            AgentToolOptions::default(),
        );
        let result = run(&tool, json!({"prompt":"do","description":"test"})).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("boom"));
    }

    #[tokio::test]
    async fn foreground_timeout_returns_timeout_error() {
        let host = Arc::new(TestSubagentHost {
            behavior: Mutex::new(HostBehavior::Timeout),
        });
        let tool = AgentTool::new(
            host,
            None::<Arc<dyn BackgroundRegistrar>>,
            AgentToolOptions::default(),
        );
        let result = run(
            &tool,
            json!({"prompt":"do","description":"test","timeout":1}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.to_text().contains("timed out"));
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
        let tool = AgentTool::new(
            host,
            None::<Arc<dyn BackgroundRegistrar>>,
            AgentToolOptions::default(),
        );
        let result = run(
            &tool,
            json!({"prompt":"do","description":"test","resume":"agent-123","subagent_type":"coder"}),
        )
        .await;
        assert!(result.is_error);
        assert!(result
            .to_text()
            .contains("Cannot set subagent_type when resuming"));
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
        let tool = AgentTool::new(
            host,
            Some(registrar.clone() as Arc<dyn BackgroundRegistrar>),
            AgentToolOptions::default(),
        );
        let result = run(
            &tool,
            json!({"prompt":"do","description":"test","run_in_background":true}),
        )
        .await;
        assert!(!result.is_error, "{:?}", result);
        assert!(result.to_text().contains("task_id: agent-00000001"));
        assert!(result.to_text().contains("agent_id: agent-123"));
        assert_eq!(registrar.registrations.lock().unwrap().len(), 1);
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
        let tool = AgentTool::new(
            host,
            None::<Arc<dyn BackgroundRegistrar>>,
            AgentToolOptions::default(),
        );
        let result = run(
            &tool,
            json!({"prompt":"do","description":"test","run_in_background":true}),
        )
        .await;
        assert!(result.is_error);
        assert!(result.to_text().contains("not available"));
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
        let tool = AgentTool::new(
            host,
            None::<Arc<dyn BackgroundRegistrar>>,
            AgentToolOptions::default(),
        );
        let exec = tool
            .resolve_execution(
                json!({"prompt":"do","description":"test","subagent_type":"reviewer"}),
            )
            .unwrap();
        let matches = exec.matches_rule.expect("agent should have matches_rule");
        assert!(matches("reviewer"));
        assert!(matches("review*"));
        assert!(!matches("coder"));
    }
}
