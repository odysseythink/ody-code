use agent_rs::records::nested::PermissionMode;
use serde_json::json;

#[test]
fn permission_mode_serde_round_trip() {
    let modes = vec![
        (PermissionMode::Manual, "\"manual\""),
        (PermissionMode::Yolo, "\"yolo\""),
        (PermissionMode::Auto, "\"auto\""),
    ];
    for (mode, expected_json) in &modes {
        let json = serde_json::to_string(mode).unwrap();
        assert_eq!(&json, expected_json);
        let round: PermissionMode = serde_json::from_str(expected_json).unwrap();
        assert_eq!(round, *mode);
    }
}

#[test]
fn permission_rule_serde_round_trip() {
    use agent_rs::permission::types::{
        PermissionRule, PermissionRuleDecision, PermissionRuleScope,
    };
    let rule = PermissionRule {
        decision: PermissionRuleDecision::Allow,
        scope: PermissionRuleScope::User,
        pattern: "Read(/etc/**)".to_string(),
        reason: Some("safe read".to_string()),
    };
    let json = serde_json::to_string(&rule).unwrap();
    // camelCase keys
    assert!(json.contains("\"pattern\""));
    assert!(json.contains("\"reason\""));
    let round: PermissionRule = serde_json::from_str(&json).unwrap();
    assert_eq!(round, rule);
}

#[test]
fn permission_data_serde() {
    use agent_rs::permission::types::{
        PermissionData, PermissionRule, PermissionRuleDecision, PermissionRuleScope,
    };
    use agent_rs::records::nested::PermissionMode;
    let data = PermissionData {
        mode: PermissionMode::Manual,
        rules: vec![PermissionRule {
            decision: PermissionRuleDecision::Deny,
            scope: PermissionRuleScope::Project,
            pattern: "Bash(rm *)".to_string(),
            reason: None,
        }],
    };
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("\"mode\":\"manual\""));
    assert!(json.contains("\"decision\":\"deny\""));
    let round: PermissionData = serde_json::from_str(&json).unwrap();
    assert_eq!(round.mode, PermissionMode::Manual);
    assert_eq!(round.rules.len(), 1);
}

#[test]
fn policy_result_kind_serialization() {
    use agent_rs::permission::types::{PermissionDecisionReason, PermissionPolicyResult};
    use std::collections::HashMap;

    let approve: PermissionPolicyResult = PermissionPolicyResult::Approve {
        reason: None,
        execution_metadata: None,
    };
    assert_eq!(
        serde_json::to_string(&approve).unwrap(),
        "{\"kind\":\"approve\"}"
    );

    let deny = PermissionPolicyResult::Deny {
        reason: None,
        message: Some("blocked by policy".to_string()),
    };
    let deny_json = serde_json::to_string(&deny).unwrap();
    assert!(deny_json.contains("\"kind\":\"deny\""));
    assert!(deny_json.contains("\"message\":\"blocked by policy\""));

    let ask = PermissionPolicyResult::Ask {
        reason: None,
        resolve_approval: None,
        resolve_error: None,
    };
    assert_eq!(serde_json::to_string(&ask).unwrap(), "{\"kind\":\"ask\"}");
}

#[test]
fn approval_request_serde() {
    use agent_rs::permission::types::ApprovalRequest;
    let req = ApprovalRequest {
        tool_call_id: "tc-1".to_string(),
        tool_name: "Write".to_string(),
        action: "Write file".to_string(),
        display: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"toolCallId\":\"tc-1\""));
    assert!(json.contains("\"toolName\":\"Write\""));
    let round: ApprovalRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(round.tool_call_id, "tc-1");
}
