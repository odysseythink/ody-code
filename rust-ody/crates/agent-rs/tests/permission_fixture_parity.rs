use agent_rs::agent_loop::tool_access::ToolAccesses;
use agent_rs::agent_loop::types::RunnableToolExecution;
use agent_rs::permission::policies::create_permission_decision_policies;
use agent_rs::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution, PermissionRule,
    PermissionRuleDecision, PermissionRuleScope,
};
use agent_rs::records::nested::{ApprovalResponse, PermissionMode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Permission scenario fixture types
// ---------------------------------------------------------------------------
#[derive(Debug, Default, Serialize, Deserialize)]
struct ScenarioInput {
    mode: String,
    #[serde(rename = "toolName")]
    tool_name: String,
    #[serde(rename = "toolArgs", default)]
    tool_args: JsonValue,
    #[serde(default)]
    rules: Vec<ScenarioRule>,
    #[serde(default, rename = "sessionApprovalPatterns")]
    session_approval_patterns: Vec<String>,
    #[serde(default, rename = "fileAccesses")]
    file_accesses: Vec<ScenarioFileAccess>,
    #[serde(default, rename = "sessionModeActive")]
    session_mode_active: bool,
    #[serde(default, rename = "sessionModeKind")]
    session_mode_kind: Option<String>,
    #[serde(default, rename = "sessionModeFilePath")]
    session_mode_file_path: Option<String>,
    #[serde(default, rename = "preToolHookReason")]
    pre_tool_hook_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScenarioRule {
    decision: String,
    scope: String,
    pattern: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScenarioFileAccess {
    operation: String,
    path: String,
    #[serde(default)]
    recursive: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScenarioExpected {
    decision: String,
    #[serde(default, rename = "messageContains")]
    message_contains: Option<String>,
    #[serde(default, rename = "reasonKeys")]
    reason_keys: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PermissionScenario {
    name: String,
    description: String,
    input: ScenarioInput,
    expected: ScenarioExpected,
}

// ---------------------------------------------------------------------------
// Minimal mock context for policy chain evaluation
// ---------------------------------------------------------------------------
struct PolicyTestContext {
    mode_val: PermissionMode,
    rules_val: Vec<PermissionRule>,
    session_patterns: Vec<String>,
    cwd_val: String,
    path_class_val: String,
    agent_type_val: String,
    session_mode_active_val: bool,
    session_mode_kind_val: Option<String>,
    session_mode_file_path_val: Option<String>,
    sensitive_paths: Vec<String>,
    git_work_tree_marker: Option<(String, String)>,
    pre_tool_hook_reason: Option<String>,
    approval_decision: Arc<Mutex<String>>,
    telemetry_events: Arc<Mutex<Vec<(String, JsonValue)>>>,
}

impl PolicyTestContext {
    fn new() -> Self {
        Self {
            mode_val: PermissionMode::Manual,
            rules_val: vec![],
            session_patterns: vec![],
            cwd_val: "/home/user/project".to_string(),
            path_class_val: "posix".to_string(),
            agent_type_val: "primary".to_string(),
            session_mode_active_val: false,
            session_mode_kind_val: None,
            session_mode_file_path_val: None,
            sensitive_paths: vec![],
            git_work_tree_marker: None,
            pre_tool_hook_reason: None,
            approval_decision: Arc::new(Mutex::new("approved".to_string())),
            telemetry_events: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl agent_rs::permission::manager::PermissionManagerContext for PolicyTestContext {
    fn mode(&self) -> PermissionMode {
        self.mode_val
    }
    fn rules(&self) -> Vec<PermissionRule> {
        self.rules_val.clone()
    }
    fn session_approval_rule_patterns(&self) -> Vec<String> {
        self.session_patterns.clone()
    }
    fn add_session_approval_rule_pattern(&self, _pattern: String) {}
    fn log_record(&self, _record: agent_rs::records::AgentRecord) {}
    fn emit_status_updated(&self) {}
    fn push_approval_result_replay(
        &self,
        _record: &agent_rs::records::nested::PermissionApprovalResultRecord,
    ) {
    }
    fn track_telemetry(&self, event: &str, data: JsonValue) {
        self.telemetry_events
            .lock()
            .unwrap()
            .push((event.to_string(), data));
    }
    fn cwd(&self) -> String {
        self.cwd_val.clone()
    }
    fn path_class(&self) -> &str {
        &self.path_class_val
    }
    fn agent_type(&self) -> &str {
        &self.agent_type_val
    }
    fn is_sensitive_file(&self, path: &str) -> bool {
        self.sensitive_paths.iter().any(|sp| path.contains(sp))
    }
    fn is_session_mode_active(&self) -> bool {
        self.session_mode_active_val
    }
    fn session_mode_kind(&self) -> Option<&str> {
        self.session_mode_kind_val.as_deref()
    }
    fn session_mode_file_path(&self) -> Option<String> {
        self.session_mode_file_path_val.clone()
    }
    fn is_writable_session_mode_path(&self, path: &str) -> bool {
        self.session_mode_file_path_val.as_deref() == Some(path)
            || (self.session_mode_file_path_val.is_some()
                && path.ends_with(".md")
                && path.contains("/"))
    }
    fn exit_session_mode(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }
    fn find_git_work_tree_marker(&self) -> Option<(String, String)> {
        self.git_work_tree_marker.clone()
    }
    fn fire_hook_pre_tool_use(
        &self,
        _tool_name: &str,
        _tool_input: JsonValue,
        _tool_call_id: &str,
        _signal: kosong_rs::provider::AbortSignal,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>,
    > {
        let result = self.pre_tool_hook_reason.clone();
        Box::pin(async move { Ok(result) })
    }
    fn fire_hook_permission_request(&self, _tool_name: &str, _data: JsonValue) {}
    fn fire_hook_permission_result(&self, _tool_name: &str, _data: JsonValue) {}
    fn request_approval(
        &self,
        _req: &agent_rs::permission::types::ApprovalRequest,
        _signal: kosong_rs::provider::AbortSignal,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ApprovalResponse, anyhow::Error>> + Send + '_>,
    > {
        let d = self.approval_decision.lock().unwrap().clone();
        Box::pin(async move {
            Ok(ApprovalResponse {
                decision: d,
                scope: None,
                feedback: None,
                selected_label: None,
            })
        })
    }
    fn is_plan_review_display(&self, display: &JsonValue) -> bool {
        display.get("kind").and_then(|v| v.as_str()) == Some("plan_review")
    }
    fn writes_only_plan_file(&self, execution: &RunnableToolExecution, plan_path: &str) -> bool {
        execution
            .accesses
            .as_ref()
            .map(|a| {
                a.0.iter().all(|r| match r {
                    agent_rs::agent_loop::tool_access::ToolResourceAccess::File {
                        operation,
                        path,
                        ..
                    } => (operation == "write" || operation == "readwrite") && path == plan_path,
                    _ => true,
                })
            })
            .unwrap_or(false)
    }
}

// Helper to build ToolAccesses from fixture file accesses
fn build_accesses(fa: &[ScenarioFileAccess]) -> Option<ToolAccesses> {
    if fa.is_empty() {
        return None;
    }
    Some(ToolAccesses(
        fa.iter()
            .map(
                |f| agent_rs::agent_loop::tool_access::ToolResourceAccess::File {
                    operation: f.operation.clone(),
                    path: f.path.clone(),
                    recursive: f.recursive,
                },
            )
            .collect(),
    ))
}

fn build_execution(scenario: &ScenarioInput) -> RunnableToolExecution {
    let approval_rule = "test".to_string();
    RunnableToolExecution {
        is_error: None,
        accesses: build_accesses(&scenario.file_accesses),
        display: None,
        description: Some(format!("Call {}", scenario.tool_name)),
        stop_batch_after_this: None,
        approval_rule,
        matches_rule: Some(Box::new(|_arg_glob: &str| true)),
        execute: Box::new(|_ctx| {
            let result = agent_rs::records::nested::ExecutableToolResult::Success(
                agent_rs::records::nested::ExecutableToolSuccessResult {
                    output: agent_rs::records::nested::ExecutableToolOutput::Text(String::new()),
                    is_error: None,
                    stop_turn: None,
                    message: None,
                },
            );
            Box::pin(async { Ok(result) })
        }),
    }
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------
fn permission_scenarios() -> Vec<PermissionScenario> {
    vec![
        // --- yolo mode ---
        PermissionScenario {
            name: "yolo_mode_write_approve".into(),
            description: "Yolo mode approves any Write tool".into(),
            input: ScenarioInput {
                mode: "yolo".into(),
                tool_name: "Write".into(),
                tool_args: json!({"path": "/tmp/test.txt"}),
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "approve".into(),
                message_contains: None,
                reason_keys: vec![],
            },
        },
        // --- auto mode ---
        PermissionScenario {
            name: "auto_mode_bash_approve".into(),
            description: "Auto mode approves Bash tool".into(),
            input: ScenarioInput {
                mode: "auto".into(),
                tool_name: "Bash".into(),
                tool_args: json!({"command": "echo hello"}),
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "approve".into(),
                message_contains: None,
                reason_keys: vec![],
            },
        },
        // --- auto mode + AskUserQuestion → deny ---
        PermissionScenario {
            name: "auto_mode_ask_user_question_deny".into(),
            description: "Auto mode denies AskUserQuestion".into(),
            input: ScenarioInput {
                mode: "auto".into(),
                tool_name: "AskUserQuestion".into(),
                tool_args: json!({}),
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "deny".into(),
                message_contains: Some("AskUserQuestion is disabled".into()),
                reason_keys: vec![],
            },
        },
        // --- manual mode + no rules → fallback ask ---
        PermissionScenario {
            name: "manual_mode_fallback_ask".into(),
            description: "Manual mode with no rules asks for Write".into(),
            input: ScenarioInput {
                mode: "manual".into(),
                tool_name: "Write".into(),
                tool_args: json!({"path": "/tmp/test.txt"}),
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "ask".into(),
                message_contains: None,
                reason_keys: vec![],
            },
        },
        // --- manual mode + deny rule matches → deny ---
        PermissionScenario {
            name: "manual_mode_deny_rule".into(),
            description: "Manual mode deny rule blocks Bash".into(),
            input: ScenarioInput {
                mode: "manual".into(),
                tool_name: "Bash".into(),
                tool_args: json!({"command": "rm -rf /"}),
                rules: vec![ScenarioRule {
                    decision: "deny".into(),
                    scope: "user".into(),
                    pattern: "Bash".into(),
                    reason: Some("no destructive commands".into()),
                }],
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "deny".into(),
                message_contains: Some("denied by permission rule".into()),
                reason_keys: vec![],
            },
        },
        // --- session approval history approve ---
        PermissionScenario {
            name: "session_approval_history_approve".into(),
            description: "Session approval pattern matches → approve".into(),
            input: ScenarioInput {
                mode: "manual".into(),
                tool_name: "Write".into(),
                tool_args: json!({"path": "/tmp/test.txt"}),
                session_approval_patterns: vec!["Write(*)".to_string()],
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "approve".into(),
                message_contains: None,
                reason_keys: vec![],
            },
        },
        // --- sensitive file → ask ---
        PermissionScenario {
            name: "sensitive_file_access_ask".into(),
            description: "Access to .env triggers ask".into(),
            input: ScenarioInput {
                mode: "manual".into(),
                tool_name: "Read".into(),
                tool_args: json!({"path": "/home/user/project/.env"}),
                file_accesses: vec![ScenarioFileAccess {
                    operation: "read".into(),
                    path: "/home/user/project/.env".into(),
                    recursive: None,
                }],
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "ask".into(),
                message_contains: None,
                reason_keys: vec!["sensitive_path".into()],
            },
        },
        // --- default approve (Read) ---
        PermissionScenario {
            name: "default_approve_read".into(),
            description: "Read tool is in default approve list".into(),
            input: ScenarioInput {
                mode: "manual".into(),
                tool_name: "Read".into(),
                tool_args: json!({"path": "/tmp/test.txt"}),
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "approve".into(),
                message_contains: None,
                reason_keys: vec![],
            },
        },
        // --- plan mode guard: Write outside plan file → deny ---
        PermissionScenario {
            name: "plan_mode_write_outside_plan_deny".into(),
            description: "Plan mode denies Write outside plan file".into(),
            input: ScenarioInput {
                mode: "manual".into(),
                tool_name: "Write".into(),
                tool_args: json!({"path": "/tmp/other.txt"}),
                session_mode_active: true,
                session_mode_kind: Some("plan".into()),
                session_mode_file_path: Some("/home/user/project/.ody-code/plans/plan.md".into()),
                file_accesses: vec![ScenarioFileAccess {
                    operation: "write".into(),
                    path: "/tmp/other.txt".into(),
                    recursive: None,
                }],
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "deny".into(),
                message_contains: Some("Plan mode is active".into()),
                reason_keys: vec![],
            },
        },
        // --- idea tool directory approve ---
        PermissionScenario {
            name: "idea_tool_directory_approve".into(),
            description: "SaveIdeaReport under .ody-code/ideas/ approves".into(),
            input: ScenarioInput {
                mode: "manual".into(),
                tool_name: "SaveIdeaReport".into(),
                tool_args: json!({"title": "test", "content": "# test"}),
                file_accesses: vec![ScenarioFileAccess {
                    operation: "write".into(),
                    path: "/home/user/project/.ody-code/ideas/test.md".into(),
                    recursive: None,
                }],
                ..Default::default()
            },
            expected: ScenarioExpected {
                decision: "approve".into(),
                message_contains: None,
                reason_keys: vec![],
            },
        },
    ]
}

#[tokio::test]
async fn all_permission_scenarios_produce_expected_decision() {
    for scenario in permission_scenarios() {
        let mut ctx = PolicyTestContext::new();
        ctx.mode_val = match scenario.input.mode.as_str() {
            "manual" => PermissionMode::Manual,
            "yolo" => PermissionMode::Yolo,
            "auto" => PermissionMode::Auto,
            _ => PermissionMode::Manual,
        };
        ctx.rules_val = scenario
            .input
            .rules
            .iter()
            .map(|r| PermissionRule {
                decision: match r.decision.as_str() {
                    "allow" => PermissionRuleDecision::Allow,
                    "deny" => PermissionRuleDecision::Deny,
                    _ => PermissionRuleDecision::Ask,
                },
                scope: match r.scope.as_str() {
                    "user" => PermissionRuleScope::User,
                    "project" => PermissionRuleScope::Project,
                    "turn-override" => PermissionRuleScope::TurnOverride,
                    _ => PermissionRuleScope::SessionRuntime,
                },
                pattern: r.pattern.clone(),
                reason: r.reason.clone(),
            })
            .collect();
        ctx.session_patterns = scenario.input.session_approval_patterns.clone();
        ctx.session_mode_active_val = scenario.input.session_mode_active;
        ctx.session_mode_kind_val = scenario.input.session_mode_kind.clone();
        ctx.session_mode_file_path_val = scenario.input.session_mode_file_path.clone();
        ctx.sensitive_paths = vec![".env".to_string(), ".git".to_string()];

        let execution = build_execution(&scenario.input);
        let tool_call = kosong_rs::message::ToolCall {
            call_type: "function".to_string(),
            id: "tc-1".to_string(),
            name: scenario.input.tool_name.clone(),
            arguments: Some(scenario.input.tool_args.to_string()),
            extras: None,
            stream_index: None,
        };
        let _signal = kosong_rs::provider::AbortSignal::new();

        let pctx = PermissionPolicyContext {
            turn_id: "turn-1",
            step_number: 1,
            signal: _signal.clone(),
            tool_call: &tool_call,
            tool: None,
            args: scenario.input.tool_args.clone(),
            execution: &execution,
        };

        // Run the policy chain
        let policies = create_permission_decision_policies(&ctx);
        let mut result: Option<PermissionPolicyResolution> = None;
        for policy in &policies {
            if let Some(res) = policy.evaluate(&pctx) {
                result = Some(res);
                break;
            }
        }

        let actual_decision = match &result {
            Some(PermissionPolicyResolution::Approve { .. }) => "approve",
            Some(PermissionPolicyResolution::Deny { .. }) => "deny",
            Some(PermissionPolicyResolution::Ask { .. }) => "ask",
            Some(PermissionPolicyResolution::Result { .. }) => "result",
            None => "none",
        };

        let msg = match &result {
            Some(PermissionPolicyResolution::Deny { message, .. }) => message.clone(),
            _ => None,
        };

        assert_eq!(
            actual_decision, scenario.expected.decision,
            "Scenario '{}' ({}): expected decision '{}', got '{}'",
            scenario.name, scenario.description, scenario.expected.decision, actual_decision
        );

        if let Some(expected_msg) = &scenario.expected.message_contains {
            assert!(
                msg.as_ref().map_or(false, |m| m.contains(expected_msg)),
                "Scenario '{}': expected message containing '{}', got {:?}",
                scenario.name,
                expected_msg,
                msg
            );
        }
    }
}

#[test]
fn fixture_round_trip_serde() {
    // Verify scenario JSON round-trips correctly
    let scenario = PermissionScenario {
        name: "test".into(),
        description: "test".into(),
        input: ScenarioInput {
            mode: "manual".into(),
            tool_name: "Write".into(),
            tool_args: json!({"path": "/tmp/x"}),
            ..Default::default()
        },
        expected: ScenarioExpected {
            decision: "deny".into(),
            message_contains: Some("denied".into()),
            reason_keys: vec!["file".into()],
        },
    };
    let json = serde_json::to_string_pretty(&scenario).unwrap();
    let round: PermissionScenario = serde_json::from_str(&json).unwrap();
    assert_eq!(round.name, "test");
    assert_eq!(round.expected.decision, "deny");
}
