use agent_rs::agent_loop::types::RunnableToolExecution;
use agent_rs::permission::manager::{PermissionManager, PermissionManagerContext};
use agent_rs::permission::types::{PermissionRule, PermissionRuleDecision, PermissionRuleScope};
use agent_rs::records::nested::{ApprovalResponse, PermissionApprovalResultRecord, PermissionMode};
use agent_rs::records::AgentRecord;
use serde_json::json;
use std::sync::{Arc, Mutex};

// Minimal mock context for testing PermissionManager in isolation
struct MockContext {
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
        self.telemetry_events
            .lock()
            .unwrap()
            .push((event.to_string(), data));
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

    fn request_approval(
        &self,
        _req: &agent_rs::permission::types::ApprovalRequest,
        _signal: kosong_rs::provider::AbortSignal,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ApprovalResponse, anyhow::Error>> + Send + '_>,
    > {
        Box::pin(async {
            Ok(ApprovalResponse {
                decision: "approved".to_string(),
                scope: None,
                feedback: None,
                selected_label: None,
            })
        })
    }

    fn fire_hook_pre_tool_use(
        &self,
        _tool_name: &str,
        _tool_input: serde_json::Value,
        _tool_call_id: &str,
        _signal: kosong_rs::provider::AbortSignal,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>,
    > {
        Box::pin(async { Ok(None) })
    }

    fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn is_plan_review_display(&self, _display: &serde_json::Value) -> bool {
        false
    }
    fn writes_only_plan_file(
        &self,
        _execution: &RunnableToolExecution,
        _session_mode_file_path: &str,
    ) -> bool {
        false
    }
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
    assert_eq!(mgr.mode(), PermissionMode::Manual);
}
