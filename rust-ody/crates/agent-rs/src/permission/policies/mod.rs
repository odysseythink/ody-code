pub mod auto_mode_approve;
pub mod auto_mode_ask_user_question_deny;
pub mod browser_tool_ask;
pub mod default_tool_approve;
pub mod fallback_ask;
pub mod pre_tool_call_hook;
pub mod yolo_mode_approve;

pub use auto_mode_approve::AutoModeApprove;
pub use auto_mode_ask_user_question_deny::AutoModeAskUserQuestionDeny;
pub use browser_tool_ask::BrowserToolAsk;
pub use default_tool_approve::{default_approve_tools_set, DefaultToolApprove};
pub use fallback_ask::FallbackAsk;
pub use pre_tool_call_hook::PreToolCallHook;
pub use session_approval_history::SessionApprovalHistory;
pub use user_configured_rules::{UserConfiguredAllow, UserConfiguredAsk, UserConfiguredDeny};
pub use yolo_mode_approve::YoloModeApprove;

pub mod file_access_ask;

pub use file_access_ask::{
    evaluate_cwd_outside_file_write_ask, evaluate_git_control_path_access_ask,
    evaluate_sensitive_file_access_ask, write_file_accesses, CwdOutsideFileWriteAsk,
    GitControlPathAccessAsk, SensitiveFileAccessAsk,
};
pub mod exit_plan_mode_review_ask;
pub mod git_cwd_write_approve;
pub mod idea_tool_directory;
pub mod plan_mode_guard_deny;
pub mod plan_mode_tool_approve;
pub mod session_approval_history;
pub mod user_configured_rules;

pub use exit_plan_mode_review_ask::{evaluate_exit_plan_mode_review_ask, ExitPlanModeReviewAsk};
pub use git_cwd_write_approve::{evaluate_git_cwd_write_approve, GitCwdWriteApprove};
pub use idea_tool_directory::{evaluate_idea_tool_directory_approve, IdeaToolDirectory};
pub use plan_mode_guard_deny::{evaluate_plan_mode_guard_deny, PlanModeGuardDeny};
pub use plan_mode_tool_approve::{evaluate_plan_mode_tool_approve, PlanModeToolApprove};

use crate::permission::manager::PermissionManagerContext;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};
use crate::records::nested::PermissionMode;

/// Erased wrapper: a PermissionPolicy backed by a closure.
struct WrappedPolicy {
    name: &'static str,
    eval: Box<
        dyn Fn(&PermissionPolicyContext<'_>) -> Option<PermissionPolicyResolution> + Send + Sync,
    >,
}

impl PermissionPolicy for WrappedPolicy {
    fn name(&self) -> &str {
        self.name
    }
    fn evaluate(
        &self,
        context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        (self.eval)(context)
    }
}

// ---------------------------------------------------------------------------
// Factory: assembles all policies in TS order
// ---------------------------------------------------------------------------
pub fn create_permission_decision_policies<C: PermissionManagerContext>(
    _ctx: &C,
) -> Vec<Box<dyn PermissionPolicy>> {
    let mode = _ctx.mode();
    let rules = _ctx.rules();
    let session_patterns = _ctx.session_approval_rule_patterns();
    let cwd = _ctx.cwd();
    let path_class = _ctx.path_class().to_string();
    let agent_type = _ctx.agent_type().to_string();
    let is_session_mode_active = _ctx.is_session_mode_active();
    let session_mode_kind = _ctx.session_mode_kind().map(|s| s.to_string());
    let session_mode_file_path = _ctx.session_mode_file_path();
    let git_marker = _ctx.find_git_work_tree_marker();

    // Capture clones for closures that need the same value more than once.
    let rules_for_deny = rules.clone();
    let rules_for_ask = rules.clone();
    let rules_for_allow = rules;
    let agent_type_for_deny = agent_type;
    let session_mode_file_path_for_guard = session_mode_file_path.clone();
    let session_mode_file_path_for_approve = session_mode_file_path;
    let cwd_for_git = cwd.clone();
    let cwd_for_cwd = cwd.clone();
    let cwd_for_idea = cwd.clone();
    let cwd_for_git_cwd = cwd;
    let git_marker_for_git = git_marker.clone();
    let git_marker_for_git_cwd = git_marker;

    vec![
        // 1. PreToolCallHook — hook returned a block → deny
        Box::new(WrappedPolicy {
            name: "pre-tool-call-hook",
            eval: Box::new(
                move |_pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    None
                },
            ),
        }),
        // 2. AutoMode + AskUserQuestion → deny
        Box::new(WrappedPolicy {
            name: "auto-mode-ask-user-question-deny",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    if mode != PermissionMode::Auto {
                        return None;
                    }
                    if pctx.tool_call.name != "AskUserQuestion" {
                        return None;
                    }
                    Some(PermissionPolicyResolution::Deny {
                    reason: None,
                    message: Some("AskUserQuestion is disabled while auto permission mode is active. Make a reasonable decision and continue without asking the user.".to_string()),
                })
                },
            ),
        }),
        // 3. PlanModeGuardDeny — plan-mode write/exit/edit guard
        Box::new(WrappedPolicy {
            name: "plan-mode-guard-deny",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    if !is_session_mode_active {
                        return None;
                    }
                    let kind = session_mode_kind.as_deref().unwrap_or("plan");
                    let is_office_hours = kind == "office-hours";
                    let is_game_design = kind == "game-design";
                    let is_design = kind == "design";
                    let mode_label = if is_office_hours {
                        "office-hours"
                    } else if is_game_design {
                        "game-design"
                    } else if is_design {
                        "design"
                    } else {
                        "plan"
                    };
                    let exit_tool = if is_office_hours {
                        "ExitOfficeHoursMode"
                    } else if is_game_design {
                        "ExitGameDesignMode"
                    } else if is_design {
                        "ExitDesignMode"
                    } else {
                        "ExitPlanMode"
                    };

                    evaluate_plan_mode_guard_deny(
                        pctx,
                        mode_label,
                        exit_tool,
                        session_mode_file_path_for_guard.as_deref(),
                        |path: &str| {
                            session_mode_file_path_for_guard.as_deref() == Some(path)
                                || (session_mode_file_path_for_guard.is_some()
                                    && path.ends_with(".md")
                                    && path.contains('/'))
                        },
                    )
                },
            ),
        }),
        // 4. UserConfiguredDeny
        Box::new(WrappedPolicy {
            name: "user-configured-deny",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    let a = agent_type_for_deny.clone();
                    crate::permission::policies::user_configured_rules::evaluate_user_configured_deny(pctx, &rules_for_deny, &a)
                },
            ),
        }),
        // 5. AutoModeApprove
        Box::new(WrappedPolicy {
            name: "auto-mode-approve",
            eval: Box::new(
                move |_pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    if mode != PermissionMode::Auto {
                        return None;
                    }
                    Some(PermissionPolicyResolution::Approve {
                        reason: None,
                        execution_metadata: None,
                    })
                },
            ),
        }),
        // 6. SessionApprovalHistory
        Box::new(WrappedPolicy {
            name: "session-approval-history",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    crate::permission::policies::session_approval_history::evaluate_session_approval_history(pctx, &session_patterns)
                },
            ),
        }),
        // 7. UserConfiguredAsk
        Box::new(WrappedPolicy {
            name: "user-configured-ask",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    crate::permission::policies::user_configured_rules::evaluate_user_configured_ask(
                        pctx,
                        &rules_for_ask,
                    )
                },
            ),
        }),
        // 8. UserConfiguredAllow
        Box::new(WrappedPolicy {
            name: "user-configured-allow",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    crate::permission::policies::user_configured_rules::evaluate_user_configured_allow(pctx, &rules_for_allow)
                },
            ),
        }),
        // 9. BrowserToolAsk
        Box::new(BrowserToolAsk),
        // 10. ExitPlanModeReviewAsk — stub for L3
        Box::new(WrappedPolicy {
            name: "exit-plan-mode-review-ask",
            eval: Box::new(
                |_pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    None // full impl in 4.3.7
                },
            ),
        }),
        // 11. PlanModeToolApprove
        Box::new(WrappedPolicy {
            name: "plan-mode-tool-approve",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    evaluate_plan_mode_tool_approve(
                        pctx,
                        is_session_mode_active,
                        session_mode_file_path_for_approve.as_deref(),
                    )
                },
            ),
        }),
        // 12. SensitiveFileAccessAsk
        Box::new(WrappedPolicy {
            name: "sensitive-file-access-ask",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    evaluate_sensitive_file_access_ask(pctx, |path: &str| {
                        path.contains(".env") || path.contains(".git")
                    })
                },
            ),
        }),
        // 13. GitControlPathAccessAsk — stub
        Box::new(WrappedPolicy {
            name: "git-control-path-access-ask",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    let m = git_marker_for_git.clone();
                    evaluate_git_control_path_access_ask(
                        pctx,
                        &cwd_for_git,
                        m.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
                    )
                },
            ),
        }),
        // 14. CwdOutsideFileWriteAsk
        Box::new(WrappedPolicy {
            name: "cwd-outside-file-write-ask",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    evaluate_cwd_outside_file_write_ask(pctx, &cwd_for_cwd)
                },
            ),
        }),
        // 15. YoloModeApprove
        Box::new(WrappedPolicy {
            name: "yolo-mode-approve",
            eval: Box::new(
                move |_pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    if mode != PermissionMode::Yolo {
                        return None;
                    }
                    Some(PermissionPolicyResolution::Approve {
                        reason: None,
                        execution_metadata: None,
                    })
                },
            ),
        }),
        // 16. DefaultToolApprove
        Box::new(DefaultToolApprove),
        // 17. IdeaToolDirectory
        Box::new(WrappedPolicy {
            name: "idea-tool-directory-approve",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    evaluate_idea_tool_directory_approve(pctx, &cwd_for_idea)
                },
            ),
        }),
        // 18. GitCwdWriteApprove — stub
        Box::new(WrappedPolicy {
            name: "git-cwd-write-approve",
            eval: Box::new(
                move |pctx: &PermissionPolicyContext<'_>| -> Option<PermissionPolicyResolution> {
                    let pc = path_class.clone();
                    evaluate_git_cwd_write_approve(
                        pctx,
                        &cwd_for_git_cwd,
                        &pc,
                        git_marker_for_git_cwd.is_some(),
                    )
                },
            ),
        }),
        // 19. FallbackAsk
        Box::new(FallbackAsk),
    ]
}
