use agent_rs::agent_loop::types::RunnableToolExecution;
use agent_rs::permission::matches_rule::{match_permission_rule, parse_pattern};
use agent_rs::permission::types::{PermissionRule, PermissionRuleDecision, PermissionRuleScope};

fn make_execution(
    matches_rule: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
) -> RunnableToolExecution {
    RunnableToolExecution {
        is_error: None,
        accesses: None,
        display: None,
        description: None,
        stop_batch_after_this: None,
        approval_rule: "test".to_string(),
        matches_rule,
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
fn parse_pattern_tool_name_only() {
    let p = parse_pattern("Read").unwrap();
    assert_eq!(p.tool_name, "Read");
    assert_eq!(p.arg_pattern, None);
}

#[test]
fn parse_pattern_tool_name_with_glob() {
    let p = parse_pattern("mcp__*").unwrap();
    assert_eq!(p.tool_name, "mcp__*");
    assert_eq!(p.arg_pattern, None);
}

#[test]
fn parse_pattern_with_arg_glob() {
    let p = parse_pattern("Read(/etc/**)").unwrap();
    assert_eq!(p.tool_name, "Read");
    assert_eq!(p.arg_pattern, Some("/etc/**".to_string()));

    let p2 = parse_pattern("Bash(rm *)").unwrap();
    assert_eq!(p2.tool_name, "Bash");
    assert_eq!(p2.arg_pattern, Some("rm *".to_string()));
}

#[test]
fn parse_pattern_invalid_tool_name_placeholder() {
    // TS parsePattern: pattern is just "*" → toolName = "*", no arg
    let p = parse_pattern("*").unwrap();
    assert_eq!(p.tool_name, "*");
    assert_eq!(p.arg_pattern, None);
}

#[test]
fn parse_pattern_invalid_brackets() {
    // Unmatched parens → error
    assert!(parse_pattern("Read(").is_err());
    assert!(parse_pattern(")").is_err());
}

#[test]
fn match_tool_name_exact() {
    let rule = PermissionRule {
        decision: PermissionRuleDecision::Allow,
        scope: PermissionRuleScope::User,
        pattern: "Read".to_string(),
        reason: None,
    };
    let execution = make_execution(None);
    let m = match_permission_rule(&rule, "Read", &execution);
    assert!(m.is_some());
    let m = m.unwrap();
    assert!(!m.has_rule_args);

    // Non-matching tool name
    assert!(match_permission_rule(&rule, "Write", &execution).is_none());
}

#[test]
fn match_tool_name_glob() {
    let rule = PermissionRule {
        decision: PermissionRuleDecision::Deny,
        scope: PermissionRuleScope::User,
        pattern: "mcp__*".to_string(),
        reason: None,
    };
    let execution = make_execution(None);
    assert!(match_permission_rule(&rule, "mcp__chrome__navigate", &execution).is_some());
    assert!(match_permission_rule(&rule, "Read", &execution).is_none());
}

#[test]
fn match_with_arg_glob() {
    let rule = PermissionRule {
        decision: PermissionRuleDecision::Ask,
        scope: PermissionRuleScope::User,
        pattern: "Read(/etc/**)".to_string(),
        reason: None,
    };
    // matches_rule returns true for "/etc/**"
    let execution = make_execution(Some(Box::new(|arg_glob| arg_glob == "/etc/**")));
    let m = match_permission_rule(&rule, "Read", &execution);
    assert!(m.is_some());
    let m = m.unwrap();
    assert!(m.has_rule_args);

    // matches_rule returns false
    let execution2 = make_execution(Some(Box::new(|_arg_glob| false)));
    assert!(match_permission_rule(&rule, "Read", &execution2).is_none());
}

#[test]
fn match_without_matches_rule_fn_on_arg_rule() {
    // Rule has arg pattern but execution has no matches_rule fn → no match
    let rule = PermissionRule {
        decision: PermissionRuleDecision::Ask,
        scope: PermissionRuleScope::User,
        pattern: "Bash(rm *)".to_string(),
        reason: None,
    };
    let execution = make_execution(None);
    assert!(match_permission_rule(&rule, "Bash", &execution).is_none());
}

#[test]
fn match_wildcard_tool_name() {
    let rule = PermissionRule {
        decision: PermissionRuleDecision::Deny,
        scope: PermissionRuleScope::User,
        pattern: "*".to_string(),
        reason: None,
    };
    let execution = make_execution(None);
    assert!(match_permission_rule(&rule, "Write", &execution).is_some());
    assert!(match_permission_rule(&rule, "Bash", &execution).is_some());
}
