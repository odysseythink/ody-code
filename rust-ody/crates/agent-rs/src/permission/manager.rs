use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::agent_loop::types::{PrepareToolExecutionResult, RunnableToolExecution};
use crate::records::nested::{ApprovalResponse, PermissionApprovalResultRecord, PermissionMode};
use crate::records::AgentRecord;

use super::types::{ApprovalRequest, PermissionData, PermissionPolicyResolution, PermissionRule};

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
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>,
    >;

    fn fire_hook_permission_request(&self, tool_name: &str, data: serde_json::Value);
    fn fire_hook_permission_result(&self, tool_name: &str, data: serde_json::Value);

    // --- RPC ---
    fn request_approval(
        &self,
        req: &ApprovalRequest,
        signal: kosong_rs::provider::AbortSignal,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ApprovalResponse, anyhow::Error>> + Send + '_>,
    >;

    // --- Plan/design file helpers ---
    fn is_plan_review_display(&self, display: &serde_json::Value) -> bool;
    fn writes_only_plan_file(
        &self,
        execution: &RunnableToolExecution,
        session_mode_file_path: &str,
    ) -> bool;
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
        self.context
            .log_record(AgentRecord::PermissionSetMode { time: None, mode });
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
        self.context
            .log_record(AgentRecord::PermissionRecordApprovalResult {
                time: None,
                record: record.clone(),
            });
        self.context.push_approval_result_replay(&record);

        if record.result.decision == "approved" && record.result.scope.as_deref() == Some("session")
        {
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
            format!(
                "Tool \"{}\" was not run because the approval request was cancelled.",
                tool_name
            )
        } else {
            format!(
                "Tool \"{}\" was not run because the user rejected the approval request.",
                tool_name
            )
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
        self.context.fire_hook_permission_request(
            "Setup Script",
            serde_json::json!({
                "turnId": 0,
                "toolCallId": "setup-script",
                "toolName": "Setup Script",
                "action": format!("Run {}", script_path),
                "toolInput": {},
                "display": req.display,
            }),
        );
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
                self.context.fire_hook_permission_result(
                    "Setup Script",
                    serde_json::json!({
                        "turnId": 0,
                        "toolCallId": "setup-script",
                        "toolName": "Setup Script",
                        "action": format!("Run {}", script_path),
                        "decision": response.decision,
                        "scope": response.scope,
                        "feedback": response.feedback,
                        "selectedLabel": response.selected_label,
                    }),
                );
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
            PermissionPolicyResolution::Approve {
                execution_metadata, ..
            } => PrepareToolExecutionResult {
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
pub fn format_permission_rule_deny_message(
    tool: &str,
    reason: Option<&str>,
    agent_type: &str,
) -> String {
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
