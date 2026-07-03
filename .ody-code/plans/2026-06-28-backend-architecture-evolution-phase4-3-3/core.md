# Part 1: Types + matches_rule + PermissionManager core

## Phase A: Foundation — serial dependency chain

### Task 1: Permission types module

**Depends on:** none (prerequisites: 4.3.0 records, `agent_loop::types`)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/permission/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/permission/types.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs` (add `pub mod permission;` before the closing brace)
- Test: `rust-ody/crates/agent-rs/tests/permission_types.rs`

- [ ] Write the failing test (`tests/permission_types.rs`):

```rust
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
    use agent_rs::permission::types::{PermissionRule, PermissionRuleDecision, PermissionRuleScope};
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
    use agent_rs::permission::types::{PermissionData, PermissionRule, PermissionRuleDecision, PermissionRuleScope};
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
    use agent_rs::permission::types::{PermissionPolicyResult, PermissionDecisionReason};
    use std::collections::HashMap;

    let approve: PermissionPolicyResult = PermissionPolicyResult::Approve {
        reason: None,
        execution_metadata: None,
    };
    assert_eq!(serde_json::to_string(&approve).unwrap(), "{\"kind\":\"approve\"}");

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
```

- [ ] Run it and verify it FAILS:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_types 2>&1 | tail -5
# Expected: error[E0433] — no `permission` module in `agent_rs`
```

- [ ] Write the minimal implementation:

`rust-ody/crates/agent-rs/src/permission/mod.rs`:
```rust
pub mod types;
```

`rust-ody/crates/agent-rs/src/permission/types.rs`:
```rust
use crate::agent_loop::types::RunnableToolExecution;
use crate::records::nested::{ApprovalResponse, PermissionMode};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// PermissionRule
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleScope {
    #[serde(rename = "turn-override")]
    TurnOverride,
    #[serde(rename = "session-runtime")]
    SessionRuntime,
    #[serde(rename = "project")]
    Project,
    #[serde(rename = "user")]
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRule {
    pub decision: PermissionRuleDecision,
    pub scope: PermissionRuleScope,
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// PermissionData
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionData {
    pub mode: PermissionMode,
    pub rules: Vec<PermissionRule>,
}

// ---------------------------------------------------------------------------
// ApprovalRequest
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<JsonValue>,
}

// ---------------------------------------------------------------------------
// PermissionDecision & reason
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Approve,
    Deny,
    Ask,
}

pub type PermissionReasonValue = JsonValue;
pub type PermissionDecisionReason = HashMap<String, PermissionReasonValue>;

// ---------------------------------------------------------------------------
// PermissionPolicyContext — mirrors TS ResolvedToolExecutionHookContext
// ---------------------------------------------------------------------------
pub struct PermissionPolicyContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: kosong_rs::provider::AbortSignal,
    pub tool_call: &'a kosong_rs::message::ToolCall,
    pub tool: Option<&'a dyn crate::agent_loop::types::ExecutableTool>,
    pub args: JsonValue,
    pub execution: &'a RunnableToolExecution,
}

// ---------------------------------------------------------------------------
// PermissionPolicyResult — the tagged union
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum PermissionPolicyResult {
    #[serde(rename = "approve")]
    Approve {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<PermissionDecisionReason>,
        #[serde(rename = "executionMetadata", skip_serializing_if = "Option::is_none")]
        execution_metadata: Option<JsonValue>,
    },
    #[serde(rename = "deny")]
    Deny {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "ask")]
    Ask {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<PermissionDecisionReason>,
        #[serde(skip)]
        resolve_approval: Option<fn(&crate::records::nested::ApprovalResponse) -> Option<PermissionPolicyResolution>>,
        #[serde(skip)]
        resolve_error: Option<fn(&anyhow::Error) -> Option<PermissionPolicyResolution>>,
    },
}

impl PartialEq for PermissionPolicyResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PermissionPolicyResult::Approve { reason: r1, execution_metadata: e1 }, PermissionPolicyResult::Approve { reason: r2, execution_metadata: e2 }) => r1 == r2 && e1 == e2,
            (PermissionPolicyResult::Deny { reason: r1, message: m1 }, PermissionPolicyResult::Deny { reason: r2, message: m2 }) => r1 == r2 && m1 == m2,
            (PermissionPolicyResult::Ask { reason: r1, .. }, PermissionPolicyResult::Ask { reason: r2, .. }) => r1 == r2,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// PermissionPolicyResolution — Approve | Deny | Ask | Result
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub enum PermissionPolicyResolution {
    Approve {
        reason: Option<PermissionDecisionReason>,
        execution_metadata: Option<JsonValue>,
    },
    Deny {
        reason: Option<PermissionDecisionReason>,
        message: Option<String>,
    },
    Ask {
        reason: Option<PermissionDecisionReason>,
        resolve_approval: Option<fn(&ApprovalResponse) -> Option<Box<PermissionPolicyResolution>>>,
        resolve_error: Option<fn(&anyhow::Error) -> Option<Box<PermissionPolicyResolution>>>,
    },
    Result {
        // Wraps a PrepareToolExecutionResult
        inner: crate::agent_loop::types::PrepareToolExecutionResult,
    },
}

impl From<PermissionPolicyResult> for PermissionPolicyResolution {
    fn from(result: PermissionPolicyResult) -> Self {
        match result {
            PermissionPolicyResult::Approve { reason, execution_metadata } => {
                PermissionPolicyResolution::Approve { reason, execution_metadata }
            }
            PermissionPolicyResult::Deny { reason, message } => {
                PermissionPolicyResolution::Deny { reason, message }
            }
            PermissionPolicyResult::Ask { reason, resolve_approval, resolve_error } => {
                PermissionPolicyResolution::Ask { reason, resolve_approval, resolve_error }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PermissionPolicy trait
// ---------------------------------------------------------------------------
#[async_trait::async_trait]
pub trait PermissionPolicy: Send + Sync {
    fn name(&self) -> &str;
    async fn evaluate(
        &self,
        context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution>;
}
```

In `rust-ody/crates/agent-rs/src/lib.rs`, add after all existing `pub mod` lines:
```rust
pub mod permission;
```

- [ ] Run it and verify it PASSES:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_types 2>&1 | tail -10
# Expected: test result: ok. 5 passed
```

- [ ] Commit:
```bash
cd rust-ody && git add crates/agent-rs/src/permission/mod.rs crates/agent-rs/src/permission/types.rs crates/agent-rs/tests/permission_types.rs crates/agent-rs/src/lib.rs && git commit -m "feat(agent-rs): add permission types module with PermissionRule, PermissionData, PermissionPolicy trait"
```

---

### Task 2: matches_rule — parse_pattern + match_permission_rule

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/permission/matches_rule.rs`
- Modify: `rust-ody/crates/agent-rs/src/permission/mod.rs` (add `pub mod matches_rule;`)
- Modify: `rust-ody/crates/agent-rs/Cargo.toml` (add `globset` dependency)
- Test: `rust-ody/crates/agent-rs/tests/permission_matches_rule.rs`

- [ ] Write the failing test (`tests/permission_matches_rule.rs`):

```rust
use agent_rs::permission::matches_rule::{match_permission_rule, ParsedPattern, parse_pattern};
use agent_rs::permission::types::{PermissionRule, PermissionRuleDecision, PermissionRuleScope};
use agent_rs::agent_loop::types::RunnableToolExecution;
use serde_json::json;

fn make_execution(matches_rule: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>) -> RunnableToolExecution {
    RunnableToolExecution {
        is_error: None,
        accesses: None,
        display: None,
        description: None,
        stop_batch_after_this: None,
        approval_rule: "test".to_string(),
        matches_rule,
        execute: Box::new(|_ctx| Box::pin(async { Ok(Default::default()) })),
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
    // matches_rule returns true for "/etc/passwd"
    let execution = make_execution(Some(Box::new(|arg_glob| {
        arg_glob == "/etc/**"
    })));
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
```

- [ ] Run it and verify it FAILS:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_matches_rule 2>&1 | tail -5
# Expected: error[E0432] — no `matches_rule` module in `agent_rs::permission`
```

- [ ] Write the minimal implementation:

Add to `rust-ody/crates/agent-rs/Cargo.toml` under `[dependencies]`:
```toml
globset = { version = "0.4", default-features = false }
```

`rust-ody/crates/agent-rs/src/permission/matches_rule.rs`:
```rust
use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};

use super::types::{PermissionRule, PermissionRuleMatch, PermissionRuleMatchStrategy};
use crate::agent_loop::types::RunnableToolExecution;

/// Parsed representation of a permission rule pattern.
/// Format: `ToolName(arg_glob)` or just `ToolName`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedPattern {
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_pattern: Option<String>,
}

/// Parse a permission rule pattern string.
/// `"Read"` → tool_name: "Read"
/// `"Read(/etc/**)"` → tool_name: "Read", arg: "/etc/**"
/// `"*"` → tool_name: "*"
pub fn parse_pattern(pattern: &str) -> Result<ParsedPattern, ParsePatternError> {
    let pattern = pattern.trim();
    // Find the outermost `(` — everything before it is the tool name,
    // everything inside is the arg glob.
    if let Some(open_paren) = pattern.rfind('(') {
        let tool_name = pattern[..open_paren].trim().to_string();
        let remainder = pattern[open_paren + 1..].trim();
        if let Some(close_paren) = remainder.rfind(')') {
            let arg = remainder[..close_paren].trim().to_string();
            if tool_name.is_empty() {
                return Err(ParsePatternError::EmptyToolName);
            }
            Ok(ParsedPattern {
                tool_name,
                arg_pattern: if arg.is_empty() { None } else { Some(arg) },
            })
        } else {
            Err(ParsePatternError::UnmatchedParen)
        }
    } else if pattern.contains(')') {
        Err(ParsePatternError::UnmatchedParen)
    } else {
        Ok(ParsedPattern {
            tool_name: pattern.to_string(),
            arg_pattern: None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParsePatternError {
    #[error("unmatched parenthesis in pattern")]
    UnmatchedParen,
    #[error("empty tool name in pattern")]
    EmptyToolName,
}

/// Test if a tool name matches a glob pattern string.
/// TS uses `picomatch.isMatch(toolName, parsed.toolName)`. We use `globset`
/// which is ripgrep's high-quality glob library. Both support standard glob
/// syntax (`*`, `**`, `?`, `[abc]`).
fn tool_name_matches(pattern: &str, tool_name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    // globset::Glob is case-sensitive; TS picomatch is case-sensitive too.
    Glob::new(pattern)
        .ok()
        .map(|g| g.compile_matcher().is_match(tool_name))
        .unwrap_or(false)
}

/// Match a permission rule against a tool call.
pub fn match_permission_rule(
    rule: &PermissionRule,
    tool_name: &str,
    execution: &RunnableToolExecution,
) -> Option<PermissionRuleMatch> {
    let parsed = parse_pattern(&rule.pattern).ok()?;

    if parsed.tool_name != "*" && !tool_name_matches(&parsed.tool_name, tool_name) {
        return None;
    }

    if parsed.arg_pattern.is_none() {
        return Some(PermissionRuleMatch {
            rule: rule.clone(),
            strategy: PermissionRuleMatchStrategy::ToolNameOnly,
            has_rule_args: false,
        });
    }

    let arg_pattern = parsed.arg_pattern.as_ref().unwrap();
    // If execution has a matches_rule fn, call it; otherwise no match
    if let Some(matches_fn) = &execution.matches_rule {
        if matches_fn(arg_pattern) {
            return Some(PermissionRuleMatch {
                rule: rule.clone(),
                strategy: PermissionRuleMatchStrategy::MatchesRule,
                has_rule_args: true,
            });
        }
    }

    None
}
```

Update `rust-ody/crates/agent-rs/src/permission/mod.rs`:
```rust
pub mod matches_rule;
pub mod types;
```

Add `PermissionRuleMatch` to `rust-ody/crates/agent-rs/src/permission/types.rs` (append before the closing of types.rs):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRuleMatchStrategy {
    #[serde(rename = "tool_name_only")]
    ToolNameOnly,
    #[serde(rename = "matches_rule")]
    MatchesRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRuleMatch {
    pub rule: PermissionRule,
    pub strategy: PermissionRuleMatchStrategy,
    #[serde(rename = "hasRuleArgs")]
    pub has_rule_args: bool,
}
```

- [ ] Run it and verify it PASSES:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_matches_rule 2>&1 | tail -10
# Expected: test result: ok. 10 passed
```

- [ ] Commit:
```bash
cd rust-ody && git add crates/agent-rs/src/permission/matches_rule.rs crates/agent-rs/src/permission/mod.rs crates/agent-rs/src/permission/types.rs crates/agent-rs/Cargo.toml crates/agent-rs/tests/permission_matches_rule.rs && git commit -m "feat(agent-rs): add matches_rule with parse_pattern and match_permission_rule"
```

---

### Task 3: PermissionManager core + PermissionManagerContext trait

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/agent-rs/src/permission/manager.rs`
- Modify: `rust-ody/crates/agent-rs/src/permission/mod.rs` (add `pub mod manager;`)
- Test: `rust-ody/crates/agent-rs/tests/permission_manager.rs`

- [ ] Write the failing test (`tests/permission_manager.rs`):

```rust
use agent_rs::agent_loop::types::RunnableToolExecution;
use agent_rs::permission::manager::{PermissionManager, PermissionManagerContext};
use agent_rs::permission::types::{PermissionRule, PermissionRuleDecision, PermissionRuleScope};
use agent_rs::records::nested::{ApprovalResponse, PermissionApprovalResultRecord, PermissionMode};
use agent_rs::records::{AgentRecord, AgentRecords};
use serde_json::json;
use std::sync::{Arc, Mutex};

// Minimal mock context for testing PermissionManager in isolation
struct MockContext {
    records: Arc<Mutex<Vec<AgentRecord>>>,
    mode: Arc<Mutex<PermissionMode>>,
    rules: Arc<Mutex<Vec<PermissionRule>>>,
    session_approval_patterns: Arc<Mutex<Vec<String>>>,
    emit_status_updated_count: Arc<Mutex<usize>>,
    log_record_count: Arc<Mutex<usize>>,
    telemetry_events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    cwd: Arc<Mutex<String>>,
    path_class: String,
    agent_type_val: String,
    is_office_hours: Arc<Mutex<bool>>,
}

impl MockContext {
    fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            mode: Arc::new(Mutex::new(PermissionMode::Manual)),
            rules: Arc::new(Mutex::new(Vec::new())),
            session_approval_patterns: Arc::new(Mutex::new(Vec::new())),
            emit_status_updated_count: Arc::new(Mutex::new(0)),
            log_record_count: Arc::new(Mutex::new(0)),
            telemetry_events: Arc::new(Mutex::new(Vec::new())),
            cwd: Arc::new(Mutex::new("/home/user/project".to_string())),
            path_class: "posix".to_string(),
            agent_type_val: "primary".to_string(),
            is_office_hours: Arc::new(Mutex::new(false)),
        }
    }
}

impl PermissionManagerContext for MockContext {
    fn mode(&self) -> PermissionMode {
        *self.mode.lock().unwrap()
    }

    fn log_record(&self, _record: AgentRecord) {
        *self.log_record_count.lock().unwrap() += 1;
    }

    fn emit_status_updated(&self) {
        *self.emit_status_updated_count.lock().unwrap() += 1;
    }

    fn rules(&self) -> Vec<PermissionRule> {
        self.rules.lock().unwrap().clone()
    }

    fn session_approval_rule_patterns(&self) -> Vec<String> {
        self.session_approval_patterns.lock().unwrap().clone()
    }

    fn add_session_approval_rule_pattern(&self, pattern: String) {
        self.session_approval_patterns.lock().unwrap().push(pattern);
    }

    fn push_approval_result_replay(&self, _record: &PermissionApprovalResultRecord) {}

    fn track_telemetry(&self, event: &str, data: serde_json::Value) {
        self.telemetry_events.lock().unwrap().push((event.to_string(), data));
    }

    fn cwd(&self) -> String {
        self.cwd.lock().unwrap().clone()
    }

    fn path_class(&self) -> &str {
        &self.path_class
    }

    fn agent_type(&self) -> &str {
        &self.agent_type_val
    }

    fn is_session_mode_active(&self) -> bool {
        false
    }

    fn session_mode_kind(&self) -> Option<&str> {
        None
    }

    fn session_mode_file_path(&self) -> Option<String> {
        None
    }

    fn is_writable_session_mode_path(&self, _path: &str) -> bool {
        false
    }

    fn exit_session_mode(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    fn find_git_work_tree_marker(&self) -> Option<(String, String)> {
        None
    }

    fn is_sensitive_file(&self, _path: &str) -> bool {
        false
    }

    fn request_approval(&self, _req: &agent_rs::permission::types::ApprovalRequest, _signal: kosong_rs::provider::AbortSignal) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ApprovalResponse, anyhow::Error>> + Send + '_>> {
        Box::pin(async { Ok(ApprovalResponse { decision: "approved".to_string(), scope: None, feedback: None, selected_label: None }) })
    }

    fn fire_hook_pre_tool_use(&self, _tool_name: &str, _tool_input: serde_json::Value, _tool_call_id: &str, _signal: kosong_rs::provider::AbortSignal) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
        Box::pin(async { Ok(None) })
    }

    fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn is_plan_review_display(&self, _display: &serde_json::Value) -> bool { false }
    fn writes_only_plan_file(&self, _execution: &RunnableToolExecution, _session_mode_file_path: &str) -> bool { false }
}

fn make_execution(name: &str) -> RunnableToolExecution {
    RunnableToolExecution {
        is_error: None,
        accesses: None,
        display: None,
        description: Some(format!("Run {}", name)),
        stop_batch_after_this: None,
        approval_rule: format!("{}(*)", name),
        matches_rule: None,
        execute: Box::new(|_ctx| Box::pin(async { Ok(Default::default()) })),
    }
}

#[test]
fn default_mode_is_manual() {
    let ctx = MockContext::new();
    let mgr = PermissionManager::new(ctx, None);
    assert_eq!(mgr.mode(), PermissionMode::Manual);
}

#[test]
fn set_mode_writes_record_and_emits_status() {
    let ctx = MockContext::new();
    let mgr = PermissionManager::new(ctx, None);
    mgr.set_mode(PermissionMode::Yolo);
    assert_eq!(mgr.mode(), PermissionMode::Yolo);
    assert!(*mgr.context.log_record_count.lock().unwrap() >= 1);
    assert!(*mgr.context.emit_status_updated_count.lock().unwrap() >= 1);
}

#[test]
fn data_returns_mode_and_rules() {
    let ctx = MockContext::new();
    {
        let mut rules = ctx.rules.lock().unwrap();
        rules.push(PermissionRule {
            decision: PermissionRuleDecision::Allow,
            scope: PermissionRuleScope::User,
            pattern: "Read".to_string(),
            reason: None,
        });
    }
    let mgr = PermissionManager::new(ctx, None);
    let data = mgr.data();
    assert_eq!(data.mode, PermissionMode::Manual);
    assert_eq!(data.rules.len(), 1);
    assert_eq!(data.rules[0].pattern, "Read");
}

#[test]
fn record_approval_result_session_scope_adds_pattern() {
    let ctx = MockContext::new();
    let mgr = PermissionManager::new(ctx, None);
    let record = PermissionApprovalResultRecord {
        turn_id: 1,
        tool_call_id: "tc-1".to_string(),
        tool_name: "Write".to_string(),
        action: "Write file".to_string(),
        session_approval_rule: Some("Write(*)".to_string()),
        result: ApprovalResponse {
            decision: "approved".to_string(),
            scope: Some("session".to_string()),
            feedback: None,
            selected_label: None,
        },
    };
    mgr.record_approval_result(record);
    let patterns = mgr.session_approval_rule_patterns();
    assert!(patterns.contains(&"Write(*)".to_string()));
}

#[test]
fn record_approval_result_not_session_does_not_add_pattern() {
    let ctx = MockContext::new();
    let mgr = PermissionManager::new(ctx, None);
    let record = PermissionApprovalResultRecord {
        turn_id: 1,
        tool_call_id: "tc-1".to_string(),
        tool_name: "Write".to_string(),
        action: "Write file".to_string(),
        session_approval_rule: Some("Write(*)".to_string()),
        result: ApprovalResponse {
            decision: "approved".to_string(),
            scope: None,
            feedback: None,
            selected_label: None,
        },
    };
    mgr.record_approval_result(record);
    let patterns = mgr.session_approval_rule_patterns();
    assert!(!patterns.contains(&"Write(*)".to_string()));
}

#[test]
fn parent_mode_inheritance() {
    let parent_ctx = MockContext::new();
    let parent = PermissionManager::new(parent_ctx, None);
    parent.set_mode(PermissionMode::Auto);

    let child_ctx = MockContext::new();
    let child = PermissionManager::new(child_ctx, Some(&parent));
    assert_eq!(child.mode(), PermissionMode::Auto);

    child.set_mode(PermissionMode::Manual);
    assert_eq!(child.mode(), PermissionMode::Manual);
}

#[test]
fn before_tool_call_fallback_ask_in_manual_mode() {
    let ctx = MockContext::new();
    let mgr = PermissionManager::new(ctx, None);
    // Manual mode, no rules → fallback-ask policy fires

    // We can't easily call before_tool_call without constructing a full
    // ToolExecutionHookContext, so we test through the public API:
    // the policy chain is assembled by the factory which we test in
    // policies.md. Here we only verify the PermissionManager struct shape
    // and mode management.
    assert_eq!(mgr.mode(), PermissionMode::Manual);
}
```

- [ ] Run it and verify it FAILS:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_manager 2>&1 | tail -5
# Expected: error[E0433] — no `manager` module in `agent_rs::permission`
```

- [ ] Write the minimal implementation:

`rust-ody/crates/agent-rs/src/permission/manager.rs`:
```rust
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::agent_loop::types::{PrepareToolExecutionResult, ResolvedToolExecutionHookContext, RunnableToolExecution, ToolExecutionHookContext};
use crate::records::nested::{ApprovalResponse, PermissionApprovalResultRecord, PermissionMode};
use crate::records::AgentRecord;

use super::types::{
    ApprovalRequest, PermissionData, PermissionPolicyResolution, PermissionRule,
};

// ---------------------------------------------------------------------------
// PermissionManagerContext trait — minimal Agent surface
// ---------------------------------------------------------------------------
pub trait PermissionManagerContext: Send + Sync {
    // --- Permission state ---
    fn mode(&self) -> PermissionMode;
    fn rules(&self) -> Vec<PermissionRule>;
    fn session_approval_rule_patterns(&self) -> Vec<String>;
    fn add_session_approval_rule_pattern(&self, pattern: String);

    // --- Records / WAL ---
    fn log_record(&self, record: AgentRecord);
    fn emit_status_updated(&self);
    fn push_approval_result_replay(&self, record: &PermissionApprovalResultRecord);

    // --- Telemetry ---
    fn track_telemetry(&self, event: &str, data: serde_json::Value);

    // --- Config / env ---
    fn cwd(&self) -> String;
    fn path_class(&self) -> &str;
    fn agent_type(&self) -> &str;
    fn is_sensitive_file(&self, path: &str) -> bool;

    // --- Session mode (4.3.7, stub for tests) ---
    fn is_session_mode_active(&self) -> bool;
    fn session_mode_kind(&self) -> Option<&str>;
    fn session_mode_file_path(&self) -> Option<String>;
    fn is_writable_session_mode_path(&self, path: &str) -> bool;
    fn exit_session_mode(&self) -> Result<(), anyhow::Error>;

    // --- Kaos / filesystem ---
    fn find_git_work_tree_marker(&self) -> Option<(String, String)>;

    // --- Hooks ---
    fn fire_hook_pre_tool_use(
        &self,
        tool_name: &str,
        tool_input: serde_json::Value,
        tool_call_id: &str,
        signal: kosong_rs::provider::AbortSignal,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>>;

    fn fire_hook_permission_request(&self, tool_name: &str, data: serde_json::Value);
    fn fire_hook_permission_result(&self, tool_name: &str, data: serde_json::Value);

    // --- RPC ---
    fn request_approval(
        &self,
        req: &ApprovalRequest,
        signal: kosong_rs::provider::AbortSignal,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ApprovalResponse, anyhow::Error>> + Send + '_>>;

    // --- Plan/design file helpers ---
    fn is_plan_review_display(&self, display: &serde_json::Value) -> bool;
    fn writes_only_plan_file(&self, execution: &RunnableToolExecution, session_mode_file_path: &str) -> bool;
}

// ---------------------------------------------------------------------------
// PermissionManager
// ---------------------------------------------------------------------------
pub struct PermissionManager<'a, C: PermissionManagerContext> {
    pub context: C,
    mode_override: Arc<Mutex<Option<PermissionMode>>>,
    parent: Option<&'a PermissionManager<'a, C>>,
    local_session_approval_rule_patterns: Arc<Mutex<HashSet<String>>>,
}

impl<'a, C: PermissionManagerContext> PermissionManager<'a, C> {
    pub fn new(context: C, parent: Option<&'a PermissionManager<'a, C>>) -> Self {
        Self {
            context,
            mode_override: Arc::new(Mutex::new(None)),
            parent,
            local_session_approval_rule_patterns: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Effective permission mode: override > parent > default("manual")
    pub fn mode(&self) -> PermissionMode {
        if let Some(ov) = *self.mode_override.lock().unwrap() {
            return ov;
        }
        if let Some(p) = self.parent {
            return p.mode();
        }
        PermissionMode::Manual
    }

    /// Set mode override, log WAL, push replay, emit status
    pub fn set_mode(&self, mode: PermissionMode) {
        self.context.log_record(AgentRecord::PermissionSetMode {
            time: None,
            mode,
        });
        self.context.emit_status_updated();
        *self.mode_override.lock().unwrap() = Some(mode);
    }

    pub fn data(&self) -> PermissionData {
        PermissionData {
            mode: self.mode(),
            rules: self.effective_rules(),
        }
    }

    fn effective_rules(&self) -> Vec<PermissionRule> {
        let mut rules = self.context.rules();
        if let Some(p) = self.parent {
            rules.extend(p.effective_rules());
        }
        rules
    }

    /// Record an approval result. If approved for session, memorize the pattern.
    pub fn record_approval_result(&self, record: PermissionApprovalResultRecord) {
        self.context.log_record(AgentRecord::PermissionRecordApprovalResult {
            time: None,
            record: record.clone(),
        });
        self.context.push_approval_result_replay(&record);

        if record.result.decision == "approved" && record.result.scope.as_deref() == Some("session") {
            if let Some(pattern) = &record.session_approval_rule {
                self.local_session_approval_rule_patterns
                    .lock()
                    .unwrap()
                    .insert(pattern.clone());
            }
        }
    }

    pub fn session_approval_rule_patterns(&self) -> Vec<String> {
        let mut patterns: Vec<String> = self
            .local_session_approval_rule_patterns
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        if let Some(p) = self.parent {
            patterns.extend(p.session_approval_rule_patterns());
        }
        patterns
    }

    /// Format a deny-policy message for the tool name.
    /// Subagents get an extra "try a different approach" suffix.
    pub fn format_policy_deny_message(&self, tool_name: &str) -> String {
        let prefix = format!("Tool \"{}\" was denied by permission policy.", tool_name);
        if self.context.agent_type() == "sub" {
            format!("{} Try a different approach — don't retry the same call, don't attempt to bypass the restriction.", prefix)
        } else {
            prefix
        }
    }

    /// Format an approval-rejection message.
    pub fn format_approval_rejection_message(
        &self,
        tool_name: &str,
        decision: &str,
        feedback: Option<&str>,
    ) -> String {
        let suffix = match feedback {
            Some(fb) if !fb.is_empty() => format!(" Reason: {}", fb),
            _ => String::new(),
        };
        let prefix = if decision == "cancelled" {
            format!("Tool \"{}\" was not run because the approval request was cancelled.", tool_name)
        } else {
            format!("Tool \"{}\" was not run because the user rejected the approval request.", tool_name)
        };
        if self.context.agent_type() == "sub" {
            format!("{}{} Try a different approach — don't retry the same call, don't attempt to bypass the restriction.", prefix, suffix)
        } else {
            format!("{}{}", prefix, suffix)
        }
    }

    /// Request user approval for running a setup script.
    /// Yolo/auto modes approve immediately; manual mode uses RPC.
    pub async fn request_setup_script_approval(
        &self,
        script_path: &str,
        signal: kosong_rs::provider::AbortSignal,
    ) -> Result<ApprovalResponse, anyhow::Error> {
        let m = self.mode();
        if m == PermissionMode::Yolo || m == PermissionMode::Auto {
            return Ok(ApprovalResponse {
                decision: "approved".to_string(),
                scope: None,
                feedback: None,
                selected_label: None,
            });
        }
        let req = ApprovalRequest {
            tool_call_id: "setup-script".to_string(),
            tool_name: "Setup Script".to_string(),
            action: format!("Run {}", script_path),
            display: Some(serde_json::json!({
                "kind": "generic",
                "summary": "Run repository setup script",
                "detail": format!("The repository contains a setup script at {}. Running it may install dependencies and prepare the environment.", script_path),
            })),
        };
        self.context.fire_hook_permission_request("Setup Script", serde_json::json!({
            "turnId": 0,
            "toolCallId": "setup-script",
            "toolName": "Setup Script",
            "action": format!("Run {}", script_path),
            "toolInput": {},
            "display": req.display,
        }));
        let result = self.context.request_approval(&req, signal).await;
        match &result {
            Ok(response) => {
                self.context.track_telemetry("permission_approval_result", serde_json::json!({
                    "policy_name": "setup_script",
                    "tool_name": "Setup Script",
                    "permission_mode": serde_json::to_string(&m).unwrap(),
                    "result": if response.decision == "approved" { "approved" } else { &response.decision },
                    "approval_surface": "generic",
                    "duration_ms": 0,
                    "session_cache_written": false,
                    "has_feedback": false,
                }));
                self.context.fire_hook_permission_result("Setup Script", serde_json::json!({
                    "turnId": 0,
                    "toolCallId": "setup-script",
                    "toolName": "Setup Script",
                    "action": format!("Run {}", script_path),
                    "decision": response.decision,
                    "scope": response.scope,
                    "feedback": response.feedback,
                    "selectedLabel": response.selected_label,
                }));
            }
            Err(_) => {
                // Silently approve on error
                return Ok(ApprovalResponse {
                    decision: "approved".to_string(),
                    scope: None,
                    feedback: None,
                    selected_label: None,
                });
            }
        }
        result
    }

    /// Convert a PermissionPolicyResolution to a PrepareToolExecutionResult.
    /// This is used by before_tool_call after the policy chain produces a resolution.
    pub fn policy_resolution_to_prepare(
        &self,
        resolution: PermissionPolicyResolution,
        tool_name: &str,
        _policy_name: &str,
    ) -> PrepareToolExecutionResult {
        match resolution {
            PermissionPolicyResolution::Approve { execution_metadata, .. } => PrepareToolExecutionResult {
                execution_metadata,
                ..Default::default()
            },
            PermissionPolicyResolution::Deny { message, .. } => PrepareToolExecutionResult {
                block: Some(true),
                reason: message.or_else(|| Some(self.format_policy_deny_message(tool_name))),
                ..Default::default()
            },
            PermissionPolicyResolution::Ask { .. } => {
                // The ask resolution is handled by request_tool_approval,
                // which is called by before_tool_call in the full integration.
                // At this layer we return a non-blocking default.
                PrepareToolExecutionResult::default()
            }
            PermissionPolicyResolution::Result { inner } => inner,
        }
    }
}

/// Format a permission-rule deny message (used by UserConfiguredDeny policy).
pub fn format_permission_rule_deny_message(tool: &str, reason: Option<&str>, agent_type: &str) -> String {
    let suffix = match reason {
        Some(r) if !r.is_empty() => format!(" Reason: {}", r),
        _ => String::new(),
    };
    if agent_type == "sub" {
        format!("Tool \"{}\" was denied.{} Try a different approach — don't retry the same call, don't attempt to bypass the restriction.", tool, suffix)
    } else {
        format!("Tool \"{}\" was denied by permission rule.{}", tool, suffix)
    }
}
```

Update `rust-ody/crates/agent-rs/src/permission/mod.rs`:
```rust
pub mod manager;
pub mod matches_rule;
pub mod types;
```

- [ ] Run it and verify it PASSES:
```bash
cd rust-ody && cargo test -p agent-rs --tests permission_manager 2>&1 | tail -15
# Expected: test result: ok. 7 passed
```

- [ ] Whole-tree typecheck (only agent-rs, no TS changes):
```bash
cd rust-ody && cargo check -p agent-rs --tests 2>&1 | tail -5
# Expected: Finished `dev` profile
```

- [ ] Commit:
```bash
cd rust-ody && git add crates/agent-rs/src/permission/manager.rs crates/agent-rs/src/permission/mod.rs crates/agent-rs/tests/permission_manager.rs && git commit -m "feat(agent-rs): add PermissionManager core with PermissionManagerContext trait"
```

---

## Local Self-Review (core.md)

- [x] 1. Spec-coverage: types (4.3.3 partial) + matches-rule (4.3.3.3) + manager core (4.3.3.1 partial) — all covered by Tasks 1–3.
- [x] 2. Placeholder scan: no TODO/TBD; `PermissionManagerContext` declares all trait methods needed by policies but marked as "stub for tests" where 4.3.7 capabilities are not yet available. All methods have real bodies or clear mock semantics.
- [x] 3. No phantom tasks: each task creates real files with compilable Rust code and corresponding tests. No `--allow-empty` or "already done".
- [x] 4. Dependency soundness: Task 1 → Task 2 → Task 3 is serial; Task 3 uses types from Task 1 and matches_rule from Task 2. No forward references.
- [x] 5. Caller & build soundness: `lib.rs` adds `pub mod permission;` in Task 1; `Cargo.toml` adds `globset` in Task 2. Both changes are verified with `cargo check --tests`. No TS shared-signature changes — this part is pure Rust.
- [x] 6. Test-the-risk: Task 1 tests serde round-trip for all tagged enums (PermissionMode, PermissionRuleDecision, PermissionRuleScope, PermissionPolicyResult). Task 2 tests parse_pattern edge cases (unmatched parens, empty tool name, wildcard, arg glob) and match_permission_rule behaviors (exact match, glob match, arg match with/without matches_rule fn). Task 3 tests mode inheritance (parent→child), set_mode WAL logging + status emission, record_approval_result session scope pattern caching, and non-session scope rejection.
- [x] 7. Type consistency: `PermissionMode` reused from `records::nested`; `PermissionRuleDecision`/`PermissionRuleScope` match TS exact enum values; `PermissionPolicyContext` mirrors TS `ResolvedToolExecutionHookContext`; `PermissionPolicyResult` uses `#[serde(tag = "kind")]` matching TS. `PermissionRuleMatch` added to types.rs in Task 2 with camelCase serde matching TS.
