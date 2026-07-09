use serde_json::Value;

/// One user-selectable option surfaced by ExitPlanMode / ExitDesignMode.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExitModeOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

const RESERVED_OPTION_LABELS: &[&str] = &["approve", "reject", "reject and exit", "revise"];

pub fn normalize_option_label(label: &str) -> String {
    label.trim().to_lowercase()
}

pub fn has_unique_option_labels(options: &[ExitModeOption]) -> bool {
    let mut seen = std::collections::HashSet::new();
    for opt in options {
        if !seen.insert(normalize_option_label(&opt.label)) {
            return false;
        }
    }
    true
}

pub fn has_no_reserved_option_labels(options: &[ExitModeOption]) -> bool {
    let reserved: std::collections::HashSet<_> = RESERVED_OPTION_LABELS.iter().copied().collect();
    options
        .iter()
        .all(|opt| !reserved.contains(normalize_option_label(&opt.label).as_str()))
}

/// Whether the approval policy marked this execution as user-approved via the review surface.
pub fn is_via_approval(metadata: Option<&Value>) -> bool {
    metadata
        .and_then(|m| m.get("viaApproval"))
        .and_then(|v| v.as_bool())
        == Some(true)
}

/// Raw `selectedLabel` returned by approval, if any. May be a reserved label such as "Approve".
pub fn selected_label_of(metadata: Option<&Value>) -> Option<String> {
    metadata
        .and_then(|m| m.get("selectedLabel"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// The label only when it matches a declared option, so reserved approval labels never surface as a chosen approach.
pub fn declared_option_label(
    options: Option<&[ExitModeOption]>,
    label: Option<&str>,
) -> Option<String> {
    let label = label?;
    options?
        .iter()
        .find(|opt| opt.label == label)
        .map(|opt| opt.label.clone())
}

/// The "Selected approach: …" directive prefix, or an empty string when none.
pub fn selected_approach_prefix(label: Option<&str>) -> String {
    match label {
        Some(l) if !l.is_empty() => format!(
            "Selected approach: {}\nExecute ONLY the selected approach. Do not execute any unselected alternatives.\n\n",
            l
        ),
        _ => String::new(),
    }
}

pub fn plan_mode_entry_message(file_path: Option<&str>) -> String {
    let file_line = match file_path {
        Some(p) if !p.is_empty() => format!(
            "Plan file: {}\nWrite the plan to EXACTLY this path (a split plan's parts go in the matching `<stem>/` subdirectory). Do NOT invent your own path, directory, or filename.",
            p
        ),
        _ => "No plan file path is assigned yet. Invent your own filename under `.ody-code/plans/` (format: `YYYY-MM-DD-<topic>.md`). The host will normalize and deduplicate it on first write.".into(),
    };

    format!(
        "Plan mode is now active. This is an implementation-planning session: investigate with read-only tools, then write a plan an engineer with zero context for this codebase can execute task-by-task. You may only write the current plan file(s).\n\n**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.\n\n{}\n\n{}",
        file_line,
        PLAN_MODE_CONTRACT_BODY
    )
}

pub fn design_mode_entry_message(file_path: Option<&str>, mockup_available: bool) -> String {
    let file_line = match file_path {
        Some(p) if !p.is_empty() => format!(
            "Design file: {}\nWrite the design to EXACTLY this path (its split parts go in the matching `<stem>/` subdirectory). Do NOT invent your own path, directory, or filename.",
            p
        ),
        _ => "No design file path is assigned yet. Invent your own filename under `.ody-code/designs/` (format: `YYYY-MM-DD-<topic>.md`). The host will normalize and deduplicate it on first write.".into(),
    };

    let mockup_line = if mockup_available {
        "ShowDesignMockup is available — use ONLY for UI/visual appearance comparisons."
    } else {
        "ShowDesignMockup is NOT available in this host; describe visuals in text and skip any browser-render offer."
    };

    format!(
        "Design mode is now active. This is a brainstorming / spec-exploration session — NOT an implementation session. Do NOT write or edit code until the user approves a design via ExitDesignMode. You may only write the current design file(s).\n\n**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.\n\n{}\n\nFollow this workflow. Your VERY FIRST action is the Step 0 audit-strategy gate.\n\n{}\n\n{}",
        file_line,
        DESIGN_MODE_CONTRACT_BODY,
        mockup_line
    )
}

const PLAN_MODE_CONTRACT_BODY: &str = "## Workflow
1. Understand — explore with Read/Grep/Glob; actively find existing functions, utilities and patterns to reuse instead of inventing new ones.
2. File Structure — list the files each task creates/modifies, one clear responsibility each.
3. Dependency Overview — order the tasks as a graph; group into phases when work is independent or separately shippable.
4. Write the plan — incrementally; every task follows the Task skeleton.
5. Self-review — run the seven-item checklist against the spec.
6. Exit — call ExitPlanMode for user approval.

## Task skeleton
Header: `### Task N: <name>`, then `Depends on: Task M` and `Files:` listing Create/Modify/Test paths.
Testable code is TEST-FIRST, with the test and implementation in the SAME task.

## Dependencies & phases
Every task's `Depends on:` must be satisfied by an EARLIER task.

## Shared-signature changes
If a task changes a shared signature, that SAME task must update every caller and end with a whole-tree typecheck.

## No placeholders
Every step contains the real content an engineer needs.";

const DESIGN_MODE_CONTRACT_BODY: &str = "<HARD-GATE>
Do NOT write code, scaffold, refactor, or take ANY implementation action until you have presented a design AND the user has approved it via ExitDesignMode.
</HARD-GATE>

## Step 0 — Audit strategy gate (BLOCKING, ask ONCE)
Ask ONE AskUserQuestion to choose Basic / Standard / Deep assumption checking.

## Step 0.5 — Upstream inventory / prior art search (conditional)
(A) Upstream inventory — only for ports/adaptations.
(B) Prior art search — for new tools with open-source parallels.

## Step 0.6 — Internal reuse scan
Before designing new components, scan the existing codebase for reusable code.

## Step 1 — Clarify, ONE question per turn
Resolve Scope, Data & State, Integration, Error & Degradation, Security, Observability, Operations.

## Step 2 — Propose approaches
Present 2-3 genuinely different approaches with trade-offs.

## Step 3 — Present the design incrementally
Present sections and ask for approval before moving on.

## Step 4 — Write the design file
Write the design to the assigned path with [C:USER]/[C:INFERRED]/[C:DEFERRED]/[C:UPSTREAM] tags and an ## Assumptions chapter.

## Step 4.5 — Adversarial self-review + consolidated audit gate
Run four-lens review and list each [C:INFERRED] assumption verbatim before ExitDesignMode.

## Step 5 — Exit for approval
Ensure C1-C8 checklist is complete, then call ExitDesignMode.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_reserved_labels_case_insensitive() {
        let opts = vec![ExitModeOption {
            label: "ReJect".into(),
            description: "".into(),
        }];
        assert!(!has_no_reserved_option_labels(&opts));
    }

    #[test]
    fn rejects_duplicate_labels() {
        let opts = vec![
            ExitModeOption {
                label: "Fast".into(),
                description: "".into(),
            },
            ExitModeOption {
                label: "fast ".into(),
                description: "".into(),
            },
        ];
        assert!(!has_unique_option_labels(&opts));
    }

    #[test]
    fn approves_unique_non_reserved_labels() {
        let opts = vec![
            ExitModeOption {
                label: "Fast path (Recommended)".into(),
                description: "".into(),
            },
            ExitModeOption {
                label: "Safe path".into(),
                description: "".into(),
            },
        ];
        assert!(has_unique_option_labels(&opts));
        assert!(has_no_reserved_option_labels(&opts));
    }

    #[test]
    fn parses_approval_metadata() {
        let metadata = serde_json::json!({"viaApproval": true, "selectedLabel": "Fast path"});
        assert!(is_via_approval(Some(&metadata)));
        assert_eq!(
            selected_label_of(Some(&metadata)).as_deref(),
            Some("Fast path")
        );
        let opts = &[ExitModeOption {
            label: "Fast path".into(),
            description: "".into(),
        }];
        assert_eq!(
            declared_option_label(Some(opts), Some("Fast path")).as_deref(),
            Some("Fast path")
        );
        assert_eq!(
            declared_option_label(Some(opts), Some("Approve")).as_deref(),
            None
        );
    }

    #[test]
    fn selected_approach_prefix_formats_declared_option() {
        let prefix = selected_approach_prefix(Some("Fast path"));
        assert!(prefix.contains("Selected approach: Fast path"));
        assert!(prefix.contains("Execute ONLY the selected approach"));
        assert!(selected_approach_prefix(None).is_empty());
    }

    #[test]
    fn plan_entry_message_contains_path() {
        let msg = plan_mode_entry_message(Some(".ody-code/plans/2026-06-29-foo.md"));
        assert!(msg.contains("Plan file: .ody-code/plans/2026-06-29-foo.md"));
        assert!(msg.contains("Plan mode is now active"));
        assert!(msg.contains("## Workflow"));
    }

    #[test]
    fn plan_entry_message_handles_missing_path() {
        let msg = plan_mode_entry_message(None);
        assert!(msg.contains("No plan file path is assigned yet"));
    }

    #[test]
    fn design_entry_message_contains_hard_gate() {
        let msg = design_mode_entry_message(Some(".ody-code/designs/2026-06-29-foo.md"), true);
        assert!(msg.contains("Design file: .ody-code/designs/2026-06-29-foo.md"));
        assert!(msg.contains("<HARD-GATE>"));
        assert!(msg.contains("ShowDesignMockup is available"));
    }

    #[test]
    fn design_entry_message_when_mockup_unavailable() {
        let msg = design_mode_entry_message(None, false);
        assert!(msg.contains("ShowDesignMockup is NOT available"));
    }
}
