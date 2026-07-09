use agent_rs::agent_loop::types::RunnableToolExecution;
use agent_rs::permission::policies::*;
use agent_rs::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution, PermissionRule,
    PermissionRuleDecision, PermissionRuleScope,
};
use agent_rs::records::nested::PermissionMode;
use serde_json::json;

fn make_simple_execution(name: &str) -> RunnableToolExecution {
    RunnableToolExecution {
        is_error: None,
        accesses: None,
        display: None,
        description: Some(format!("Call {}", name)),
        stop_batch_after_this: None,
        approval_rule: format!("{}(*)", name),
        matches_rule: None,
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

#[test]
fn yolo_mode_approve_name() {
    assert_eq!(YoloModeApprove.name(), "yolo-mode-approve");
}

#[test]
fn auto_mode_approve_name() {
    assert_eq!(AutoModeApprove.name(), "auto-mode-approve");
}

#[test]
fn auto_mode_ask_user_question_deny_name() {
    assert_eq!(
        AutoModeAskUserQuestionDeny.name(),
        "auto-mode-ask-user-question-deny"
    );
}

#[test]
fn default_tool_approve_contains_read_grep_etc() {
    let approved = default_approve_tools_set();
    assert!(approved.contains("Read"));
    assert!(approved.contains("Grep"));
    assert!(approved.contains("WebSearch"));
    assert!(!approved.contains("Write")); // Write is NOT in default approve list
    assert!(!approved.contains("Bash"));
}

#[test]
fn fallback_ask_always_returns_ask() {
    assert_eq!(FallbackAsk.name(), "fallback-ask");
}

#[test]
fn browser_tool_ask_name() {
    assert_eq!(BrowserToolAsk.name(), "browser-tool-ask");
}

#[test]
fn pre_tool_call_hook_name() {
    assert_eq!(PreToolCallHook.name(), "pre-tool-call-hook");
}

#[test]
fn fallback_ask_evaluates_to_ask() {
    let execution = make_simple_execution("Write");
    let tool_call = kosong_rs::message::ToolCall {
        call_type: "function".to_string(),
        id: "tc-1".to_string(),
        name: "Write".to_string(),
        arguments: Some("{}".to_string()),
        extras: None,
        stream_index: None,
    };
    let signal = kosong_rs::provider::AbortSignal::new();
    let ctx = PermissionPolicyContext {
        turn_id: "turn-1",
        step_number: 1,
        signal,
        tool_call: &tool_call,
        tool: None,
        args: json!({}),
        execution: &execution,
    };

    let result = FallbackAsk.evaluate(&ctx);
    assert!(result.is_some());
    match result.unwrap() {
        PermissionPolicyResolution::Ask { .. } => {}
        _ => panic!("FallbackAsk should return Ask"),
    }
}

// Task 5: Rule-based policies
#[test]
fn user_configured_deny_policy_name() {
    use agent_rs::permission::policies::user_configured_rules::UserConfiguredDeny;
    assert_eq!(UserConfiguredDeny.name(), "user-configured-deny");
}

#[test]
fn user_configured_allow_policy_name() {
    use agent_rs::permission::policies::user_configured_rules::UserConfiguredAllow;
    assert_eq!(UserConfiguredAllow.name(), "user-configured-allow");
}

#[test]
fn user_configured_ask_policy_name() {
    use agent_rs::permission::policies::user_configured_rules::UserConfiguredAsk;
    assert_eq!(UserConfiguredAsk.name(), "user-configured-ask");
}

#[test]
fn session_approval_history_policy_name() {
    use agent_rs::permission::policies::session_approval_history::SessionApprovalHistory;
    assert_eq!(SessionApprovalHistory.name(), "session-approval-history");
}

// Task 6: File-access policies
#[test]
fn sensitive_file_access_ask_name() {
    use agent_rs::permission::policies::file_access_ask::SensitiveFileAccessAsk;
    assert_eq!(SensitiveFileAccessAsk.name(), "sensitive-file-access-ask");
}

#[test]
fn git_control_path_access_ask_name() {
    use agent_rs::permission::policies::file_access_ask::GitControlPathAccessAsk;
    assert_eq!(
        GitControlPathAccessAsk.name(),
        "git-control-path-access-ask"
    );
}

#[test]
fn cwd_outside_file_write_ask_name() {
    use agent_rs::permission::policies::file_access_ask::CwdOutsideFileWriteAsk;
    assert_eq!(CwdOutsideFileWriteAsk.name(), "cwd-outside-file-write-ask");
}

// Task 7: Plan/design/idea/git-cwd policies
#[test]
fn plan_mode_guard_deny_name() {
    use agent_rs::permission::policies::plan_mode_guard_deny::PlanModeGuardDeny;
    assert_eq!(PlanModeGuardDeny.name(), "plan-mode-guard-deny");
}

#[test]
fn plan_mode_tool_approve_name() {
    use agent_rs::permission::policies::plan_mode_tool_approve::PlanModeToolApprove;
    assert_eq!(PlanModeToolApprove.name(), "plan-mode-tool-approve");
}

#[test]
fn exit_plan_mode_review_ask_name() {
    use agent_rs::permission::policies::exit_plan_mode_review_ask::ExitPlanModeReviewAsk;
    assert_eq!(ExitPlanModeReviewAsk.name(), "exit-plan-mode-review-ask");
}

#[test]
fn idea_tool_directory_approve_name() {
    use agent_rs::permission::policies::idea_tool_directory::IdeaToolDirectory;
    assert_eq!(IdeaToolDirectory.name(), "idea-tool-directory-approve");
}

#[test]
fn git_cwd_write_approve_name() {
    use agent_rs::permission::policies::git_cwd_write_approve::GitCwdWriteApprove;
    assert_eq!(GitCwdWriteApprove.name(), "git-cwd-write-approve");
}
