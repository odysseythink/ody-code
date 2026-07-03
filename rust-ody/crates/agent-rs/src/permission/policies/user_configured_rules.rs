use std::collections::HashSet;

use crate::permission::manager::format_permission_rule_deny_message;
use crate::permission::matches_rule::match_permission_rule;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution, PermissionRule,
    PermissionRuleDecision, PermissionRuleMatch, PermissionRuleScope,
};

const USER_CONFIGURED_SCOPES: &[PermissionRuleScope] = &[
    PermissionRuleScope::TurnOverride,
    PermissionRuleScope::Project,
    PermissionRuleScope::User,
];

fn first_matching_rule(
    context: &PermissionPolicyContext<'_>,
    rules: &[PermissionRule],
    decision: PermissionRuleDecision,
) -> Option<PermissionRuleMatch> {
    let scopes: HashSet<_> = USER_CONFIGURED_SCOPES.iter().collect();
    for rule in rules
        .iter()
        .filter(|r| scopes.contains(&r.scope) && r.decision == decision)
    {
        let m = match_permission_rule(rule, &context.tool_call.name, context.execution);
        if m.is_some() {
            return m;
        }
    }
    None
}

pub struct UserConfiguredDeny;

impl PermissionPolicy for UserConfiguredDeny {
    fn name(&self) -> &str {
        "user-configured-deny"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        // Rules are provided by the factory from PermissionManagerContext::rules().
        // When isolated, this policy returns None; the factory injects rules.
        None
    }
}

pub struct UserConfiguredAllow;

impl PermissionPolicy for UserConfiguredAllow {
    fn name(&self) -> &str {
        "user-configured-allow"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub struct UserConfiguredAsk;

impl PermissionPolicy for UserConfiguredAsk {
    fn name(&self) -> &str {
        "user-configured-ask"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}

/// Public helpers used by the factory
pub fn evaluate_user_configured_deny(
    context: &PermissionPolicyContext<'_>,
    rules: &[PermissionRule],
    agent_type: &str,
) -> Option<PermissionPolicyResolution> {
    let m = first_matching_rule(context, rules, PermissionRuleDecision::Deny)?;
    let mut reason = std::collections::HashMap::new();
    reason.insert("rule_decision".to_string(), serde_json::json!("deny"));
    reason.insert(
        "has_rule_args".to_string(),
        serde_json::json!(m.has_rule_args),
    );
    reason.insert(
        "match_strategy".to_string(),
        serde_json::to_value(&m.strategy).unwrap(),
    );
    Some(PermissionPolicyResolution::Deny {
        reason: Some(reason),
        message: Some(format_permission_rule_deny_message(
            &context.tool_call.name,
            m.rule.reason.as_deref(),
            agent_type,
        )),
    })
}

pub fn evaluate_user_configured_allow(
    context: &PermissionPolicyContext<'_>,
    rules: &[PermissionRule],
) -> Option<PermissionPolicyResolution> {
    let m = first_matching_rule(context, rules, PermissionRuleDecision::Allow)?;
    let mut reason = std::collections::HashMap::new();
    reason.insert("rule_decision".to_string(), serde_json::json!("allow"));
    reason.insert(
        "has_rule_args".to_string(),
        serde_json::json!(m.has_rule_args),
    );
    reason.insert(
        "match_strategy".to_string(),
        serde_json::to_value(&m.strategy).unwrap(),
    );
    Some(PermissionPolicyResolution::Approve {
        reason: Some(reason),
        execution_metadata: None,
    })
}

pub fn evaluate_user_configured_ask(
    context: &PermissionPolicyContext<'_>,
    rules: &[PermissionRule],
) -> Option<PermissionPolicyResolution> {
    let m = first_matching_rule(context, rules, PermissionRuleDecision::Ask)?;
    let mut reason = std::collections::HashMap::new();
    reason.insert("rule_decision".to_string(), serde_json::json!("ask"));
    reason.insert(
        "has_rule_args".to_string(),
        serde_json::json!(m.has_rule_args),
    );
    reason.insert(
        "match_strategy".to_string(),
        serde_json::to_value(&m.strategy).unwrap(),
    );
    Some(PermissionPolicyResolution::Ask {
        reason: Some(reason),
        resolve_approval: None,
        resolve_error: None,
    })
}
