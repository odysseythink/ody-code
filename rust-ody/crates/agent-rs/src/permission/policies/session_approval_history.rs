use crate::permission::matches_rule::match_permission_rule;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution, PermissionRule,
    PermissionRuleDecision, PermissionRuleScope,
};

pub struct SessionApprovalHistory;

impl PermissionPolicy for SessionApprovalHistory {
    fn name(&self) -> &str {
        "session-approval-history"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub fn evaluate_session_approval_history(
    context: &PermissionPolicyContext<'_>,
    session_approval_patterns: &[String],
) -> Option<PermissionPolicyResolution> {
    for pattern in session_approval_patterns {
        let rule = PermissionRule {
            decision: PermissionRuleDecision::Allow,
            scope: PermissionRuleScope::SessionRuntime,
            pattern: pattern.clone(),
            reason: Some("approve for session".to_string()),
        };
        let m = match_permission_rule(&rule, &context.tool_call.name, context.execution);
        if let Some(m) = m {
            let mut reason = std::collections::HashMap::new();
            reason.insert(
                "has_rule_args".to_string(),
                serde_json::json!(m.has_rule_args),
            );
            reason.insert(
                "match_strategy".to_string(),
                serde_json::to_value(&m.strategy).unwrap(),
            );
            return Some(PermissionPolicyResolution::Approve {
                reason: Some(reason),
                execution_metadata: None,
            });
        }
    }
    None
}
