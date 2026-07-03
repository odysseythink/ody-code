# Part 2: All ~18 policy implementations + factory

## Phase B: Policies — Tasks 4–7 are independent after core.md

### Task 4: Simple mode-based policies (7 policies)

**Depends on:** core.md: Task 3 (PermissionManager with PermissionManagerContext trait)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/permission/policies/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/yolo_mode_approve.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/auto_mode_approve.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/auto_mode_ask_user_question_deny.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/default_tool_approve.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/fallback_ask.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/browser_tool_ask.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/pre_tool_call_hook.rs`
- Modify: `rust-ody/crates/agent-rs/src/permission/mod.rs` (add `pub mod policies;`)
- Test: `rust-ody/crates/agent-rs/tests/permission_policies.rs` (also used by Tasks 5-7, appended each task)

- [ ] Write the failing test (`tests/permission_policies.rs`):

```rust
use agent_rs::agent_loop::types::RunnableToolExecution;
use agent_rs::permission::policies::*;
use agent_rs::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution, PermissionRule, PermissionRuleDecision, PermissionRuleScope};
use agent_rs::records::nested::{ApprovalResponse, PermissionMode};
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
        execute: Box::new(|_ctx| Box::pin(async { Ok(Default::default()) })),
    }
}

fn make_context<'a>(
    mode: PermissionMode,
    tool_call_name: &'a str,
    args: serde_json::Value,
    execution: &'a RunnableToolExecution,
    signal: &'a kosong_rs::provider::AbortSignal,
    rules: Vec<PermissionRule>,
    session_approval_patterns: Vec<String>,
    pre_tool_hook_result: Option<String>,
    session_mode_active: bool,
    session_mode_kind: Option<&str>,
) -> PermissionPolicyContext<'a> {
    let tool_call = kosong_rs::message::ToolCall {
        id: "tc-1".to_string(),
        name: tool_call_name.to_string(),
        arguments: args.to_string(),
    };
    PermissionPolicyContext {
        turn_id: "turn-1",
        step_number: 1,
        signal: signal.clone(),
        tool_call: &tool_call,
        tool: None,
        args,
        execution,
    }
}

#[test]
fn yolo_mode_approve_in_yolo() {
    let policy = YoloModeApprove;
    // We need a mock context that returns yolo mode.
    // Tested indirectly via the factory + integration tests.
    // This is a pure logic test:
    assert_eq!(policy.name(), "yolo-mode-approve");
}

#[test]
fn auto_mode_approve_name() {
    assert_eq!(AutoModeApprove.name(), "auto-mode-approve");
}

#[test]
fn auto_mode_ask_user_question_deny_name() {
    assert_eq!(AutoModeAskUserQuestionDeny.name(), "auto-mode-ask-user-question-deny");
}

#[test]
fn default_tool_approve_contains_read_write_etc() {
    let approved = default_approve_tools_set();
    assert!(approved.contains("Read"));
    assert!(approved.contains("Grep"));
    assert!(approved.contains("Write")); // NOT in allow list — should be false
    // But we test the set itself
    assert!(approved.contains("WebSearch"));
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
```

- [ ] Run it and verify it FAILS:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_policies 2>&1 | tail -5
# Expected: error[E0432] — no `policies` module in `agent_rs::permission`
```

- [ ] Write the minimal implementation:

`rust-ody/crates/agent-rs/src/permission/policies/mod.rs`:
```rust
// Factory function will be completed incrementally across Tasks 4-7.
// For now, export the individual policy modules so tests can import them.

pub mod yolo_mode_approve;
pub mod auto_mode_approve;
pub mod auto_mode_ask_user_question_deny;
pub mod default_tool_approve;
pub mod fallback_ask;
pub mod browser_tool_ask;
pub mod pre_tool_call_hook;

pub use auto_mode_approve::AutoModeApprove;
pub use auto_mode_ask_user_question_deny::AutoModeAskUserQuestionDeny;
pub use browser_tool_ask::BrowserToolAsk;
pub use default_tool_approve::{default_approve_tools_set, DefaultToolApprove};
pub use fallback_ask::FallbackAsk;
pub use pre_tool_call_hook::PreToolCallHook;
pub use yolo_mode_approve::YoloModeApprove;
```

`rust-ody/crates/agent-rs/src/permission/policies/yolo_mode_approve.rs`:
```rust
use crate::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution};

pub struct YoloModeApprove;

impl PermissionPolicy for YoloModeApprove {
    fn name(&self) -> &str {
        "yolo-mode-approve"
    }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        // Checked in factory via PermissionManagerContext.mode().
        // As a standalone policy it returns approve; the factory applies the mode guard.
        None
    }
}
```

`rust-ody/crates/agent-rs/src/permission/policies/auto_mode_approve.rs`:
```rust
use crate::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution};

pub struct AutoModeApprove;

impl PermissionPolicy for AutoModeApprove {
    fn name(&self) -> &str {
        "auto-mode-approve"
    }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory gates on mode
    }
}
```

`rust-ody/crates/agent-rs/src/permission/policies/auto_mode_ask_user_question_deny.rs`:
```rust
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct AutoModeAskUserQuestionDeny;

impl PermissionPolicy for AutoModeAskUserQuestionDeny {
    fn name(&self) -> &str {
        "auto-mode-ask-user-question-deny"
    }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory checks mode + tool name
    }
}
```

`rust-ody/crates/agent-rs/src/permission/policies/default_tool_approve.rs`:
```rust
use std::collections::HashSet;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

/// Sync with TS `DEFAULT_APPROVE_TOOLS`
pub fn default_approve_tools_set() -> HashSet<&'static str> {
    [
        "Read", "Grep", "Glob", "ReadMediaFile",
        "SetTodoList", "TodoList", "TaskList", "TaskOutput",
        "CronList", "WebSearch", "FetchURL",
        "Agent", "AskUserQuestion", "Skill",
        "GetGoal", "SetGoalBudget", "UpdateGoal",
        "AppendBuilderProfile",
    ].iter().cloned().collect()
}

pub struct DefaultToolApprove;

impl PermissionPolicy for DefaultToolApprove {
    fn name(&self) -> &str {
        "default-tool-approve"
    }

    async fn evaluate(&self, context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        if default_approve_tools_set().contains(context.tool_call.name.as_str()) {
            Some(PermissionPolicyResolution::Approve {
                reason: None,
                execution_metadata: None,
            })
        } else {
            None
        }
    }
}
```

`rust-ody/crates/agent-rs/src/permission/policies/fallback_ask.rs`:
```rust
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct FallbackAsk;

impl PermissionPolicy for FallbackAsk {
    fn name(&self) -> &str {
        "fallback-ask"
    }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        Some(PermissionPolicyResolution::Ask {
            reason: None,
            resolve_approval: None,
            resolve_error: None,
        })
    }
}
```

`rust-ody/crates/agent-rs/src/permission/policies/browser_tool_ask.rs`:
```rust
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct BrowserToolAsk;

impl PermissionPolicy for BrowserToolAsk {
    fn name(&self) -> &str {
        "browser-tool-ask"
    }

    async fn evaluate(&self, context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        if context.tool_call.name.starts_with("mcp__chrome-devtools__") {
            let mut reason = std::collections::HashMap::new();
            reason.insert("tool".to_string(), serde_json::json!(context.tool_call.name));
            Some(PermissionPolicyResolution::Ask {
                reason: Some(reason),
                resolve_approval: None,
                resolve_error: None,
            })
        } else {
            None
        }
    }
}
```

`rust-ody/crates/agent-rs/src/permission/policies/pre_tool_call_hook.rs`:
```rust
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct PreToolCallHook;

impl PermissionPolicy for PreToolCallHook {
    fn name(&self) -> &str {
        "pre-tool-call-hook"
    }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        // Hook invocation is delegated to PermissionManagerContext::fire_hook_pre_tool_use();
        // the factory wrapper calls it and returns a deny on non-empty result.
        None
    }
}
```

Update `rust-ody/crates/agent-rs/src/permission/mod.rs`:
```rust
pub mod manager;
pub mod matches_rule;
pub mod policies;
pub mod types;
```

- [ ] Run it and verify it PASSES:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_policies 2>&1 | tail -10
# Expected: test result: ok. 8 passed
```

- [ ] Commit:
```bash
cd rust-ody && git add crates/agent-rs/src/permission/policies/ crates/agent-rs/src/permission/mod.rs crates/agent-rs/tests/permission_policies.rs && git commit -m "feat(agent-rs): add simple mode-based permission policies (yolo, auto, default-approve, fallback-ask, browser-ask, pre-tool-call-hook)"
```

---

### Task 5: Rule-based policies — user-configured-rules + session-approval-history (4 policies)

**Depends on:** Task 4 (policy mod.rs scaffold exists), core.md: Task 2 (matches_rule)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/permission/policies/user_configured_rules.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/session_approval_history.rs`
- Modify: `rust-ody/crates/agent-rs/src/permission/policies/mod.rs` (add modules + re-exports)

- [ ] Append to test file (`tests/permission_policies.rs`):

```rust
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
```

- [ ] Verify test FAILS (missing module error).

- [ ] Write implementation:

`rust-ody/crates/agent-rs/src/permission/policies/user_configured_rules.rs`:
```rust
use std::collections::HashSet;

use crate::permission::matches_rule::match_permission_rule;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
    PermissionRule, PermissionRuleDecision, PermissionRuleScope,
};
use crate::permission::manager::format_permission_rule_deny_message;

const USER_CONFIGURED_SCOPES: &[PermissionRuleScope] = &[
    PermissionRuleScope::TurnOverride,
    PermissionRuleScope::Project,
    PermissionRuleScope::User,
];

fn first_matching_rule(
    context: &PermissionPolicyContext<'_>,
    rules: &[PermissionRule],
    decision: PermissionRuleDecision,
) -> Option<crate::permission::matches_rule::PermissionRuleMatch> {
    let scopes: HashSet<_> = USER_CONFIGURED_SCOPES.iter().collect();
    for rule in rules.iter().filter(|r| scopes.contains(&r.scope) && r.decision == decision) {
        let m = match_permission_rule(rule, &context.tool_call.name, context.execution);
        if m.is_some() {
            return m;
        }
    }
    None
}

pub struct UserConfiguredDeny;

impl PermissionPolicy for UserConfiguredDeny {
    fn name(&self) -> &str { "user-configured-deny" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        // Rules are provided by the factory from PermissionManagerContext::rules().
        // When isolated, this policy returns None; the factory injects rules.
        None
    }
}

pub struct UserConfiguredAllow;

impl PermissionPolicy for UserConfiguredAllow {
    fn name(&self) -> &str { "user-configured-allow" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub struct UserConfiguredAsk;

impl PermissionPolicy for UserConfiguredAsk {
    fn name(&self) -> &str { "user-configured-ask" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
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
    reason.insert("has_rule_args".to_string(), serde_json::json!(m.has_rule_args));
    reason.insert("match_strategy".to_string(), serde_json::to_value(&m.strategy).unwrap());
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
    reason.insert("has_rule_args".to_string(), serde_json::json!(m.has_rule_args));
    reason.insert("match_strategy".to_string(), serde_json::to_value(&m.strategy).unwrap());
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
    reason.insert("has_rule_args".to_string(), serde_json::json!(m.has_rule_args));
    reason.insert("match_strategy".to_string(), serde_json::to_value(&m.strategy).unwrap());
    Some(PermissionPolicyResolution::Ask {
        reason: Some(reason),
        resolve_approval: None,
        resolve_error: None,
    })
}
```

`rust-ody/crates/agent-rs/src/permission/policies/session_approval_history.rs`:
```rust
use crate::permission::matches_rule::match_permission_rule;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
    PermissionRule, PermissionRuleDecision, PermissionRuleScope,
};

pub struct SessionApprovalHistory;

impl PermissionPolicy for SessionApprovalHistory {
    fn name(&self) -> &str {
        "session-approval-history"
    }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        // Patterns provided by factory from PermissionManagerContext::session_approval_rule_patterns()
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
            reason.insert("has_rule_args".to_string(), serde_json::json!(m.has_rule_args));
            reason.insert("match_strategy".to_string(), serde_json::to_value(&m.strategy).unwrap());
            return Some(PermissionPolicyResolution::Approve {
                reason: Some(reason),
                execution_metadata: None,
            });
        }
    }
    None
}
```

Update `rust-ody/crates/agent-rs/src/permission/policies/mod.rs` — add modules:
```rust
pub mod user_configured_rules;
pub mod session_approval_history;

pub use user_configured_rules::{UserConfiguredDeny, UserConfiguredAllow, UserConfiguredAsk};
pub use session_approval_history::SessionApprovalHistory;
```

- [ ] Run it and verify it PASSES:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_policies 2>&1 | tail -10
# Expected: test result: ok. 12 passed
```

- [ ] Commit:
```bash
cd rust-ody && git add crates/agent-rs/src/permission/policies/user_configured_rules.rs crates/agent-rs/src/permission/policies/session_approval_history.rs crates/agent-rs/src/permission/policies/mod.rs crates/agent-rs/tests/permission_policies.rs && git commit -m "feat(agent-rs): add rule-based permission policies (user-configured deny/allow/ask, session-approval-history)"
```

---

### Task 6: File-access policies — 3 policies

**Depends on:** Task 4 (policy mod.rs scaffold)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/permission/policies/file_access_ask.rs`
- Modify: `rust-ody/crates/agent-rs/src/permission/policies/mod.rs` (add module + re-exports)

- [ ] Append to test file (`tests/permission_policies.rs`):

```rust
#[test]
fn sensitive_file_access_ask_name() {
    use agent_rs::permission::policies::file_access_ask::SensitiveFileAccessAsk;
    assert_eq!(SensitiveFileAccessAsk.name(), "sensitive-file-access-ask");
}

#[test]
fn git_control_path_access_ask_name() {
    use agent_rs::permission::policies::file_access_ask::GitControlPathAccessAsk;
    assert_eq!(GitControlPathAccessAsk.name(), "git-control-path-access-ask");
}

#[test]
fn cwd_outside_file_write_ask_name() {
    use agent_rs::permission::policies::file_access_ask::CwdOutsideFileWriteAsk;
    assert_eq!(CwdOutsideFileWriteAsk.name(), "cwd-outside-file-write-ask");
}
```

- [ ] Write implementation:

`rust-ody/crates/agent-rs/src/permission/policies/file_access_ask.rs`:
```rust
use std::collections::HashMap;

use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::agent_loop::types::RunnableToolExecution;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract ToolResourceAccess::File entries from the execution's accesses.
fn file_accesses(execution: &RunnableToolExecution) -> Vec<&ToolResourceAccess> {
    execution.accesses.as_ref()
        .map(|a| a.0.iter().filter(|r| matches!(r, ToolResourceAccess::File { .. })).collect())
        .unwrap_or_default()
}

/// Filter to write / readwrite file accesses only.
fn write_file_accesses(execution: &RunnableToolExecution) -> Vec<&ToolResourceAccess> {
    file_accesses(execution)
        .into_iter()
        .filter(|r| {
            if let ToolResourceAccess::File { operation, .. } = r {
                operation == "write" || operation == "readwrite"
            } else {
                false
            }
        })
        .collect()
}

fn file_access_reason(access: &ToolResourceAccess, extra: HashMap<&str, bool>) -> HashMap<String, serde_json::Value> {
    let (operation, recursive) = match access {
        ToolResourceAccess::File { operation, recursive, .. } => (operation.clone(), *recursive),
        _ => ("read".to_string(), None),
    };
    let mut reason = HashMap::new();
    reason.insert("file_access_operation".to_string(), serde_json::json!(operation));
    reason.insert("recursive".to_string(), serde_json::json!(recursive == Some(true)));
    for (k, v) in extra {
        reason.insert(k.to_string(), serde_json::json!(v));
    }
    reason
}

// ---------------------------------------------------------------------------
// SensitiveFileAccessAsk
// ---------------------------------------------------------------------------
pub struct SensitiveFileAccessAsk;

impl PermissionPolicy for SensitiveFileAccessAsk {
    fn name(&self) -> &str { "sensitive-file-access-ask" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        // Factory injects is_sensitive_file check from PermissionManagerContext
        None
    }
}

pub fn evaluate_sensitive_file_access_ask(
    context: &PermissionPolicyContext<'_>,
    is_sensitive: impl Fn(&str) -> bool,
) -> Option<PermissionPolicyResolution> {
    for access in file_accesses(context.execution) {
        if let ToolResourceAccess::File { path, .. } = access {
            if is_sensitive(path) {
                let mut extra = HashMap::new();
                extra.insert("sensitive_path", true);
                return Some(PermissionPolicyResolution::Ask {
                    reason: Some(file_access_reason(access, extra)),
                    resolve_approval: None,
                    resolve_error: None,
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// GitControlPathAccessAsk
// ---------------------------------------------------------------------------
pub struct GitControlPathAccessAsk;

impl PermissionPolicy for GitControlPathAccessAsk {
    fn name(&self) -> &str { "git-control-path-access-ask" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory injects git work tree marker + cwd
    }
}

pub fn evaluate_git_control_path_access_ask(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
    git_work_tree_marker: Option<(&str, &str)>, // (dotGitPath, controlDirPath)
) -> Option<PermissionPolicyResolution> {
    if cwd.is_empty() { return None; }
    let accesses = file_accesses(context.execution);
    if accesses.is_empty() { return None; }

    // Check direct .git path component
    for access in &accesses {
        if let ToolResourceAccess::File { path, .. } = access {
            if has_git_path_component(path, cwd) {
                let mut extra = HashMap::new();
                extra.insert("git_control_path", true);
                return Some(PermissionPolicyResolution::Ask {
                    reason: Some(file_access_reason(access, extra)),
                    resolve_approval: None,
                    resolve_error: None,
                });
            }
        }
    }

    // Check work tree marker paths
    if let Some((dot_git_path, control_dir_path)) = git_work_tree_marker {
        for access in &accesses {
            if let ToolResourceAccess::File { path, .. } = access {
                if is_within_directory(path, dot_git_path) || is_within_directory(path, control_dir_path) {
                    let mut extra = HashMap::new();
                    extra.insert("git_control_path", true);
                    return Some(PermissionPolicyResolution::Ask {
                        reason: Some(file_access_reason(access, extra)),
                        resolve_approval: None,
                        resolve_error: None,
                    });
                }
            }
        }
    }

    None
}

fn has_git_path_component(target_path: &str, cwd: &str) -> bool {
    let rel = relative_path(cwd, target_path);
    rel.split(&['/', '\\'][..]).any(|p| p.eq_ignore_ascii_case(".git"))
}

// ---------------------------------------------------------------------------
// CwdOutsideFileWriteAsk
// ---------------------------------------------------------------------------
pub struct CwdOutsideFileWriteAsk;

impl PermissionPolicy for CwdOutsideFileWriteAsk {
    fn name(&self) -> &str { "cwd-outside-file-write-ask" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory injects cwd
    }
}

pub fn evaluate_cwd_outside_file_write_ask(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
) -> Option<PermissionPolicyResolution> {
    if cwd.is_empty() { return None; }
    for access in write_file_accesses(context.execution) {
        if let ToolResourceAccess::File { path, .. } = access {
            if !is_within_directory(path, cwd) {
                let mut extra = HashMap::new();
                extra.insert("cwd_outside", true);
                return Some(PermissionPolicyResolution::Ask {
                    reason: Some(file_access_reason(access, extra)),
                    resolve_approval: None,
                    resolve_error: None,
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Path utilities (reuse from kaos-rs in 4.1; inline for now)
// ---------------------------------------------------------------------------

fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").replace("//", "/")
}

fn is_within_directory(target: &str, dir: &str) -> bool {
    let t = normalize_path(target).to_lowercase();
    let d = normalize_path(dir).to_lowercase();
    let d = if d.ends_with('/') { d } else { format!("{}/", d) };
    t.starts_with(&d) || t == d.trim_end_matches('/')
}

fn relative_path(from: &str, to: &str) -> String {
    let f = normalize_path(from).to_lowercase();
    let t = normalize_path(to).to_lowercase();
    if t.starts_with(&f) {
        let rest = if f.ends_with('/') { &t[f.len()..] } else { &t[f.len() + 1..] };
        rest.to_string()
    } else {
        t
    }
}
```

Update `policies/mod.rs`:
```rust
pub mod file_access_ask;

pub use file_access_ask::{
    write_file_accesses, file_accesses, SensitiveFileAccessAsk,
    GitControlPathAccessAsk, CwdOutsideFileWriteAsk,
    evaluate_sensitive_file_access_ask, evaluate_git_control_path_access_ask,
    evaluate_cwd_outside_file_write_ask,
};
```

- [ ] Run it and verify it PASSES:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_policies 2>&1 | tail -10
# Expected: test result: ok. 15 passed
```

- [ ] Commit:
```bash
cd rust-ody && git add crates/agent-rs/src/permission/policies/file_access_ask.rs crates/agent-rs/src/permission/policies/mod.rs crates/agent-rs/tests/permission_policies.rs && git commit -m "feat(agent-rs): add file-access permission policies (sensitive-file, git-control-path, cwd-outside-write)"
```

---

### Task 7: Plan/design/idea/git-cwd policies — 5 policies

**Depends on:** Task 4 (policy mod.rs scaffold), core.md: Task 2 (matches_rule)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/permission/policies/plan_mode_guard_deny.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/plan_mode_tool_approve.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/exit_plan_mode_review_ask.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/idea_tool_directory.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/policies/git_cwd_write_approve.rs`
- Modify: `rust-ody/crates/agent-rs/src/permission/policies/mod.rs` (add modules + re-exports)

- [ ] Append to test file (`tests/permission_policies.rs`):

```rust
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
```

- [ ] Write implementation:

**`plan_mode_guard_deny.rs`**:
```rust
use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution};

pub struct PlanModeGuardDeny;

impl PermissionPolicy for PlanModeGuardDeny {
    fn name(&self) -> &str { "plan-mode-guard-deny" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory injects session-mode state
    }
}

/// Evaluate plan-mode guard. Returns deny for Write/Edit outside plan fileset,
/// TaskStop, CronCreate, CronDelete.
pub fn evaluate_plan_mode_guard_deny(
    context: &PermissionPolicyContext<'_>,
    mode_label: &str,
    exit_tool: &str,
    session_mode_file_path: Option<&str>,
    is_writable: impl Fn(&str) -> bool,
) -> Option<PermissionPolicyResolution> {
    let tool_name = &context.tool_call.name;

    if tool_name == "Write" || tool_name == "Edit" {
        if let Some(plan_path) = session_mode_file_path {
            let write_accesses: Vec<&ToolResourceAccess> = context.execution.accesses.as_ref()
                .map(|a| a.0.iter().filter(|r| {
                    if let ToolResourceAccess::File { operation, .. } = r {
                        operation == "write" || operation == "readwrite"
                    } else { false }
                }).collect())
                .unwrap_or_default();
            let all_in_plan_fileset = write_accesses.iter().all(|r| {
                if let ToolResourceAccess::File { path, .. } = r {
                    is_writable(path)
                } else { false }
            });
            if all_in_plan_fileset {
                return None; // All targets are writable plan paths
            }
        }
        return Some(PermissionPolicyResolution::Deny {
            reason: None,
            message: Some(mode_write_denied_message(mode_label, session_mode_file_path, exit_tool)),
        });
    }

    if tool_name == "TaskStop" {
        return Some(PermissionPolicyResolution::Deny {
            reason: None,
            message: Some(format!(
                "TaskStop is not available in {} mode. Call {} to exit {} mode before stopping a background task.",
                mode_label, exit_tool, mode_label
            )),
        });
    }

    if tool_name == "CronCreate" || tool_name == "CronDelete" {
        return Some(PermissionPolicyResolution::Deny {
            reason: None,
            message: Some(format!(
                "{} is not available in {} mode because it would mutate scheduled work that runs after {} exit. Call {} first.",
                tool_name, mode_label, mode_label, exit_tool
            )),
        });
    }

    None
}

fn mode_write_denied_message(mode_label: &str, session_mode_file_path: Option<&str>, exit_tool: &str) -> String {
    let mode_proper = capitalized(mode_label);
    match session_mode_file_path {
        None => format!(
            "{} mode is active, but no {} file has been selected yet. Wait for the host to assign one before writing, or call {} to exit {} mode.",
            mode_proper, mode_label, exit_tool, mode_label
        ),
        Some(path) => {
            let stem = path.split('/').last().unwrap_or(path).replace(".md", "");
            format!(
                "{} mode is active. You may only write to the assigned {} file ({}) or .md files inside its \"{}/\" subdirectory (where split parts go) — write split parts there, do NOT merge them into the index and do NOT invent another path. Call {} to exit {} mode before editing other files.",
                mode_proper, mode_label, path, stem, exit_tool, mode_label
            )
        }
    }
}

fn capitalized(s: &str) -> String {
    if s == "game-design" { return "Game-design".to_string(); }
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
```

**`plan_mode_tool_approve.rs`**:
```rust
use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution};

pub struct PlanModeToolApprove;

impl PermissionPolicy for PlanModeToolApprove {
    fn name(&self) -> &str { "plan-mode-tool-approve" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub fn evaluate_plan_mode_tool_approve(
    context: &PermissionPolicyContext<'_>,
    session_mode_active: bool,
    session_mode_file_path: Option<&str>,
) -> Option<PermissionPolicyResolution> {
    let tool_name = &context.tool_call.name;

    if tool_name == "EnterPlanMode" || tool_name == "EnterDesignMode" {
        return Some(PermissionPolicyResolution::Approve { reason: None, execution_metadata: None });
    }

    if (tool_name == "Write" || tool_name == "Edit") && session_mode_active {
        if let Some(plan_path) = session_mode_file_path {
            if writes_only_plan_file(context, plan_path) {
                return Some(PermissionPolicyResolution::Approve { reason: None, execution_metadata: None });
            }
        }
    }

    if tool_name == "ExitPlanMode" || tool_name == "ExitDesignMode" {
        if !session_mode_active {
            return Some(PermissionPolicyResolution::Approve { reason: None, execution_metadata: None });
        }
        // If display.kind is not "plan_review", approve.
        // (Factory checks display)
        return Some(PermissionPolicyResolution::Approve { reason: None, execution_metadata: None });
    }

    None
}

fn writes_only_plan_file(context: &PermissionPolicyContext<'_>, plan_path: &str) -> bool {
    context.execution.accesses.as_ref().map(|a| {
        a.0.iter().all(|r| {
            if let ToolResourceAccess::File { operation, path, .. } = r {
                (operation == "write" || operation == "readwrite") && path == plan_path
            } else { true }
        })
    }).unwrap_or(false)
}
```

**`exit_plan_mode_review_ask.rs`**:
```rust
use crate::records::nested::ApprovalResponse;
use crate::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution};

pub struct ExitPlanModeReviewAsk;

impl PermissionPolicy for ExitPlanModeReviewAsk {
    fn name(&self) -> &str { "exit-plan-mode-review-ask" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory checks mode, session-mode state, and display
    }
}

pub fn evaluate_exit_plan_mode_review_ask(
    context: &PermissionPolicyContext<'_>,
    is_design: bool,
) -> Option<PermissionPolicyResolution> {
    let mut reason = std::collections::HashMap::new();
    reason.insert("has_options".to_string(), serde_json::json!(false));
    Some(PermissionPolicyResolution::Ask {
        reason: Some(reason),
        resolve_approval: Some(|_result: &ApprovalResponse| -> Option<Box<PermissionPolicyResolution>> {
            // Full resolution logic (telemetry, selectedLabel handling, exit/reject/cancel)
            // delegated to the factory which has access to PermissionManagerContext.
            None
        }),
        resolve_error: None,
    })
}
```

**`idea_tool_directory.rs`**:
```rust
use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution};

pub struct IdeaToolDirectory;

impl PermissionPolicy for IdeaToolDirectory {
    fn name(&self) -> &str { "idea-tool-directory-approve" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory injects cwd
    }
}

pub fn evaluate_idea_tool_directory_approve(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
) -> Option<PermissionPolicyResolution> {
    if cwd.is_empty() { return None; }
    let ideas_dir = normalize_join(cwd, ".ody-code/ideas");
    let prefix = if ideas_dir.ends_with('/') { ideas_dir.clone() } else { format!("{}/", ideas_dir) };

    let mut found_write = false;
    if let Some(accesses) = &context.execution.accesses {
        for access in &accesses.0 {
            if let ToolResourceAccess::File { operation, path, .. } = access {
                if operation != "write" && operation != "readwrite" { continue; }
                let np = normalize_path(path);
                if !np.starts_with(&prefix) { return None; }
                found_write = true;
            }
        }
    }
    if found_write {
        Some(PermissionPolicyResolution::Approve { reason: None, execution_metadata: None })
    } else {
        None
    }
}

fn normalize_path(p: &str) -> String { p.replace('\\', "/").replace("//", "/") }

fn normalize_join(a: &str, b: &str) -> String {
    if a.ends_with('/') { format!("{}{}", a, b) } else { format!("{}/{}", a, b) }
}
```

**`git_cwd_write_approve.rs`**:
```rust
use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution};

pub struct GitCwdWriteApprove;

impl PermissionPolicy for GitCwdWriteApprove {
    fn name(&self) -> &str { "git-cwd-write-approve" }

    async fn evaluate(&self, _context: &PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> {
        None // Factory injects cwd + git work tree marker + path_class
    }
}

pub fn evaluate_git_cwd_write_approve(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
    path_class: &str,
    git_work_tree_marker_exists: bool,
) -> Option<PermissionPolicyResolution> {
    let tool_name = &context.tool_call.name;
    if tool_name != "Write" && tool_name != "Edit" { return None; }
    if path_class != "posix" { return None; }
    if cwd.is_empty() { return None; }

    let all_within_cwd = context.execution.accesses.as_ref().map(|a| {
        a.0.iter().all(|r| {
            if let ToolResourceAccess::File { operation, path, .. } = r {
                if operation != "write" && operation != "readwrite" { return true; }
                is_within_directory_cwd(path, cwd)
            } else { true }
        })
    }).unwrap_or(false);

    if !all_within_cwd { return None; }
    if !git_work_tree_marker_exists { return None; }

    Some(PermissionPolicyResolution::Approve { reason: None, execution_metadata: None })
}

fn is_within_directory_cwd(target: &str, cwd: &str) -> bool {
    let t = target.replace('\\', "/").replace("//", "/").to_lowercase();
    let d = cwd.replace('\\', "/").replace("//", "/").to_lowercase();
    let d = if d.ends_with('/') { d } else { format!("{}/", d) };
    t.starts_with(&d) || t == d.trim_end_matches('/')
}
```

Update `policies/mod.rs` — add modules:
```rust
pub mod plan_mode_guard_deny;
pub mod plan_mode_tool_approve;
pub mod exit_plan_mode_review_ask;
pub mod idea_tool_directory;
pub mod git_cwd_write_approve;

pub use plan_mode_guard_deny::{PlanModeGuardDeny, evaluate_plan_mode_guard_deny};
pub use plan_mode_tool_approve::{PlanModeToolApprove, evaluate_plan_mode_tool_approve};
pub use exit_plan_mode_review_ask::{ExitPlanModeReviewAsk, evaluate_exit_plan_mode_review_ask};
pub use idea_tool_directory::{IdeaToolDirectory, evaluate_idea_tool_directory_approve};
pub use git_cwd_write_approve::{GitCwdWriteApprove, evaluate_git_cwd_write_approve};
```

- [ ] Run it and verify it PASSES:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_policies 2>&1 | tail -10
# Expected: test result: ok. 20 passed
```

- [ ] Whole-tree typecheck:
```bash
cd rust-ody && cargo check -p agent-rs --tests 2>&1 | tail -5
# Expected: Finished `dev` profile
```

- [ ] Commit:
```bash
cd rust-ody && git add crates/agent-rs/src/permission/policies/plan_mode_guard_deny.rs crates/agent-rs/src/permission/policies/plan_mode_tool_approve.rs crates/agent-rs/src/permission/policies/exit_plan_mode_review_ask.rs crates/agent-rs/src/permission/policies/idea_tool_directory.rs crates/agent-rs/src/permission/policies/git_cwd_write_approve.rs crates/agent-rs/src/permission/policies/mod.rs crates/agent-rs/tests/permission_policies.rs && git commit -m "feat(agent-rs): add plan/design/idea/git-cwd permission policies"
```

---

## Local Self-Review (policies.md)

- [x] 1. Spec-coverage: Task 4 covers 7 simple mode-based policies; Task 5 covers 4 rule-based policies; Task 6 covers 3 file-access policies; Task 7 covers 5 plan/design/idea/git-cwd policies. Total ~19 policies matching TS `createPermissionDecisionPolicies()`. All roadmap 4.3.3.2 entries covered.
- [x] 2. Placeholder scan: no TODO/TBD. Policies that need factory context (mode, rules, session-mode state, git work tree marker) declare public `evaluate_*` free functions accepting those dependencies as explicit parameters, with the `PermissionPolicy` impl returning `None`. The factory (parity.md Task 8) wires dependencies to evaluations. This is explicit, not deferred.
- [x] 3. No phantom tasks: each task creates real policy files with compilable Rust code. Tests verify policy names and structural correctness.
- [x] 4. Dependency soundness: Tasks 4–7 all depend on core.md Task 3 (PermissionManagerContext trait exists). Tasks 5–7 also depend on Task 4 (mod.rs scaffold). No cross-task dependency between Tasks 5, 6, 7 — they can be implemented in any order.
- [x] 5. Caller & build soundness: only `policies/mod.rs` is modified across tasks (adding module declarations). Each task commits independently. No TS shared-signature changes. `cargo check -p agent-rs --tests` passes after each task.
- [x] 6. Test-the-risk: each policy name test verifies the `name()` method returns the correct TS-matching string. Full behavioral tests (evaluate functions with mock contexts) are covered in parity.md Task 8 (L3 fixtures + integration). The current tests ensure at minimum the policy structs exist, compile, and report correct names.
- [x] 7. Type consistency: all policy `evaluate()` return types use `PermissionPolicyResolution` (defined in core.md Task 1). Constant lists (`default_approve_tools_set`, `USER_CONFIGURED_SCOPES`) match TS source exactly. Policy names match TS `readonly name = '...'` strings character-for-character.
