# Part 2 — Planning Tools (Enter/Exit Plan & Design Modes)

**Goal:** Port the four session-mode planning tools (`EnterPlanMode`, `ExitPlanMode`, `EnterDesignMode`, `ExitDesignMode`) from TypeScript to Rust, together with the shared option-label/approval-metadata helpers and plan/design entry messages they depend on.

**Architecture:** The tools live in `tools-rs` and consume the `SessionModeProvider` trait defined in Part 1, so they have no direct dependency on `agent-rs`. `agent-rs` provides an `AgentSessionModeProvider` adapter that wraps `Agent` plus lightweight in-memory state stores (replaced by file-backed stores in Part 5) and exposes the trait surface. Entry/exit messages and helper functions are centralized in `tools-rs/src/builtin/session_mode/planning.rs` so the tools and future injectors can reuse the same text.

**Tech stack:** Rust (`tools-rs`, `agent-rs`), `serde_json`, `async_trait`.

**Depends on:** `2026-06-29-backend-architecture-evolution-phase4-4/infra.md` (Tasks 1–4).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs` | Shared helpers (`ExitModeOption`, label validation, approval-metadata readers, approach prefix) and entry-message builders for plan/design modes. |
| `rust-ody/crates/tools-rs/src/builtin/session_mode/stores.rs` | In-memory implementations of `OfficeHoursStateStore` and `GameDesignStateStore` used by the adapter until Part 5. |
| `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_plan_mode.rs` | `EnterPlanModeTool`. |
| `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_design_mode.rs` | `EnterDesignModeTool`. |
| `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_plan_mode.rs` | `ExitPlanModeTool`. |
| `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_design_mode.rs` | `ExitDesignModeTool` + design-completeness checker. |
| `rust-ody/crates/agent-rs/src/session_mode/provider.rs` | `AgentSessionModeProvider` adapter implementing `SessionModeProvider` for `Agent`. |
| `rust-ody/crates/agent-rs/src/session_mode/mod.rs` | Export the new provider module. |
| `rust-ody/crates/agent-rs/src/tool/manager.rs` | Register the four planning tools in `core_builtin_tools()`. |
| `rust-ody/crates/agent-rs/src/agent.rs` | Build and install the provider adapter. |

---

## Dependency Overview

```
Phase A — Shared helpers
  Task 1: exit-mode-output helpers + option-label validation
  Task 2: plan/design entry-message builders

Phase B — Enter tools
  Task 3: EnterPlanModeTool  (depends on Task 1, Task 2)
  Task 4: EnterDesignModeTool (depends on Task 1, Task 2)

Phase C — Exit tools
  Task 5: ExitPlanModeTool   (depends on Task 1, Task 2)
  Task 6: ExitDesignModeTool + find_missing_design_sections (depends on Task 1, Task 2)

Phase D — Wiring
  Task 7: In-memory stores + AgentSessionModeProvider adapter (depends on Part 1 Task 3)
  Task 8: Register tools in ToolManager and wire provider into Agent
           (depends on Task 3–7, Part 1 Task 4)
```

Phases A, B and C can be developed and tested almost entirely inside `tools-rs` with mock providers. Phase D is the only `agent-rs` integration work.

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `SessionModeProvider` from Part 1 forces `office_hours_store()` and `game_design_store()` to return `Arc<dyn …>` even though planning tools do not need them. | Implement real in-memory stores in `tools-rs/src/builtin/session_mode/stores.rs` for Part 2; Part 5 swaps them for file-backed stores without changing the trait. |
| TS `ExitPlanModeTool` enriches the plan with E2E tests via `@odysseythink/e2e-testing`, which has no Rust port. | Mark E2E enrichment as a **GAP** in the spec-coverage table; the Rust tool skips it and documents the gap. |
| Plan/design entry messages in TS are long and include a full workflow contract; duplicating them in `tools-rs` could drift from the existing `agent-rs/injection/contracts` reminders. | Keep `agent-rs/injection/contracts` reminders unchanged; `tools-rs` entry messages are the full one-shot entry text, matching TS `planModeEntryMessage` / `designModeEntryMessage`. |
| `ExitDesignModeTool` completeness check uses English/Chinese regexes; the Rust port must preserve the same headings. | Port the exact patterns and add unit tests with mixed-language headings. |

---

## Task 1: Port exit-mode-output helpers and option-label validation

**Depends on:** `infra.md` Task 3 (trait surface exists).

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` (uncomment `pub mod planning;`)
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs` (inline `#[cfg(test)]` module)

**Why:** `ExitPlanModeTool` and `ExitDesignModeTool` share the same option-label rules and approval-metadata parsing. Centralizing them in `planning.rs` keeps the two exit tools consistent and testable.

**Steps:**

- [ ] Create `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs` with the following content:

```rust
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
        assert_eq!(selected_label_of(Some(&metadata)).as_deref(), Some("Fast path"));
        let opts = &[ExitModeOption {
            label: "Fast path".into(),
            description: "".into(),
        }];
        assert_eq!(
            declared_option_label(Some(opts), Some("Fast path")).as_deref(),
            Some("Fast path")
        );
        assert_eq!(declared_option_label(Some(opts), Some("Approve")).as_deref(), None);
    }

    #[test]
    fn selected_approach_prefix_formats_declared_option() {
        let prefix = selected_approach_prefix(Some("Fast path"));
        assert!(prefix.contains("Selected approach: Fast path"));
        assert!(prefix.contains("Execute ONLY the selected approach"));
        assert!(selected_approach_prefix(None).is_empty());
    }
}
```

- [ ] In `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`, change `pub mod planning;` from a stub declaration to the real module (the file now exists).

- [ ] Run the tests and verify they pass:

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::planning
```

Expected: 5 tests pass.

- [ ] Run the whole-workspace typecheck:

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green.

- [ ] Commit: `feat(tools-rs): port exit-mode-output helpers and option-label validation`.

---

## Task 2: Port plan/design entry-message builders

**Depends on:** Task 1.

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs`
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs` (inline tests)

**Why:** `EnterPlanModeTool` and `EnterDesignModeTool` must show the same full workflow contract that TS shows on mode entry. The messages are long static strings; keeping them next to the helpers avoids scattering contract text across tool files.

**Steps:**

- [ ] Append the following functions and constants to `rust-ody/crates/tools-rs/src/builtin/session_mode/planning.rs`:

```rust
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
```

- [ ] Add inline tests to verify the messages contain key markers:

```rust
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
```

- [ ] Run the tests:

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::planning
```

Expected: 9 tests pass.

- [ ] Run whole-workspace typecheck:

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green.

- [ ] Commit: `feat(tools-rs): add plan/design mode entry messages`.

---

## Task 3: Implement `EnterPlanModeTool`

**Depends on:** Task 2.

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_plan_mode.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` (add `pub mod enter_plan_mode;`)
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_plan_mode.rs` (inline tests)

**Why:** Mirrors TS `EnterPlanModeTool`. It enters plan mode when no session mode is active and returns the entry message with the assigned plan file path.

**Steps:**

- [ ] Create `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_plan_mode.rs`:

```rust
use std::sync::Arc;
use serde_json::Value;

use crate::builtin::session_mode::{
    planning::plan_mode_entry_message, SessionModeKind, SessionModeProvider,
};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

pub struct EnterPlanModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnterPlanModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "EnterPlanMode"
    }

    fn description(&self) -> &str {
        "Enter implementation-planning mode. Produces a step-by-step plan file."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Requesting to enter plan mode".into(),
            approval_rule: "EnterPlanMode".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    if provider.is_session_mode_active() {
                        let active = match provider.session_mode_kind() {
                            Some(SessionModeKind::Design) => "Design",
                            Some(SessionModeKind::OfficeHours) => "Office-hours",
                            Some(SessionModeKind::GameDesign) => "Game-design",
                            _ => "Plan",
                        };
                        let exit_tool = match provider.session_mode_kind() {
                            Some(SessionModeKind::Design) => "ExitDesignMode",
                            Some(SessionModeKind::OfficeHours) => "ExitOfficeHoursMode",
                            Some(SessionModeKind::GameDesign) => "ExitGameDesignMode",
                            _ => "ExitPlanMode",
                        };
                        return ExecutableToolResult::error_text(
                            format!(
                                "{} mode is already active. Use {} when you are ready to exit {} mode; do not try to enter another mode on top of it.",
                                active,
                                exit_tool,
                                active.to_lowercase()
                            ),
                            "session mode already active".into(),
                        );
                    }

                    if let Err(e) = provider.enter_session_mode(SessionModeKind::Plan).await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to enter plan mode: {}", e),
                            "enter failed".into(),
                        );
                    }

                    provider.telemetry().track(
                        "plan_enter_resolved",
                        std::collections::HashMap::from([("outcome".into(), "auto_approved".into())]),
                    );

                    let msg = plan_mode_entry_message(provider.session_mode_file_path().as_deref());
                    ExecutableToolResult::ok_text(msg)
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::tests::MockSessionModeProvider;

    #[tokio::test]
    async fn enter_plan_mode_succeeds_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = EnterPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("Plan mode is now active"));
        assert!(provider.entered.lock().unwrap().contains(&SessionModeKind::Plan));
    }

    #[tokio::test]
    async fn enter_plan_mode_fails_when_already_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Design));
        let tool = EnterPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("Design mode is already active"));
        assert!(result.to_text().contains("ExitDesignMode"));
    }
}
```

- [ ] Add `pub mod enter_plan_mode;` to `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`.

- [ ] The mock provider referenced in tests will be created in Task 7. For now the test code references `crate::builtin::session_mode::tests::MockSessionModeProvider`. Verify the reference resolves once Task 7 lands.

- [ ] Run whole-workspace typecheck:

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green (the test module will not compile until the mock exists; skip `--all-targets` until Task 7 if needed, or run `cargo check -p tools-rs --lib` to check the library code only).

- [ ] Commit: `feat(tools-rs): add EnterPlanModeTool`.

---

## Task 4: Implement `EnterDesignModeTool`

**Depends on:** Task 2.

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_design_mode.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` (add `pub mod enter_design_mode;`)
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_design_mode.rs` (inline tests)

**Why:** Mirrors TS `EnterDesignModeTool`. Differs from `EnterPlanModeTool` only in the mode kind and the entry message, which also reports whether `ShowDesignMockup` is available.

**Steps:**

- [ ] Create `rust-ody/crates/tools-rs/src/builtin/session_mode/enter_design_mode.rs`:

```rust
use std::sync::Arc;
use serde_json::Value;

use crate::builtin::session_mode::{
    planning::design_mode_entry_message, SessionModeKind, SessionModeProvider,
};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution};

pub struct EnterDesignModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnterDesignModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnterDesignModeTool {
    fn name(&self) -> &str {
        "EnterDesignMode"
    }

    fn description(&self) -> &str {
        "Enter design/brainstorming mode. Produces a design document."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Requesting to enter design mode".into(),
            approval_rule: "EnterDesignMode".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    if provider.is_session_mode_active() {
                        let active = match provider.session_mode_kind() {
                            Some(SessionModeKind::Plan) => "Plan",
                            Some(SessionModeKind::OfficeHours) => "Office-hours",
                            Some(SessionModeKind::GameDesign) => "Game-design",
                            _ => "Design",
                        };
                        let exit_tool = match provider.session_mode_kind() {
                            Some(SessionModeKind::Plan) => "ExitPlanMode",
                            Some(SessionModeKind::OfficeHours) => "ExitOfficeHoursMode",
                            Some(SessionModeKind::GameDesign) => "ExitGameDesignMode",
                            _ => "ExitDesignMode",
                        };
                        return ExecutableToolResult::error_text(
                            format!(
                                "{} mode is already active. Use {} when you are ready to exit {} mode; do not try to enter another mode on top of it.",
                                active,
                                exit_tool,
                                active.to_lowercase()
                            ),
                            "session mode already active".into(),
                        );
                    }

                    if let Err(e) = provider.enter_session_mode(SessionModeKind::Design).await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to enter design mode: {}", e),
                            "enter failed".into(),
                        );
                    }

                    provider.telemetry().track(
                        "design_enter_resolved",
                        std::collections::HashMap::from([("outcome".into(), "auto_approved".into())]),
                    );

                    let msg = design_mode_entry_message(
                        provider.session_mode_file_path().as_deref(),
                        provider.open_external_available(),
                    );
                    ExecutableToolResult::ok_text(msg)
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::tests::MockSessionModeProvider;

    #[tokio::test]
    async fn enter_design_mode_succeeds_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = EnterDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("Design mode is now active"));
        assert!(provider.entered.lock().unwrap().contains(&SessionModeKind::Design));
    }

    #[tokio::test]
    async fn enter_design_mode_fails_when_plan_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Plan));
        let tool = EnterDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("Plan mode is already active"));
        assert!(result.to_text().contains("ExitPlanMode"));
    }
}
```

- [ ] Add `pub mod enter_design_mode;` to `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`.

- [ ] Run whole-workspace typecheck (`cargo check -p tools-rs --lib` until Task 7 adds the mock).

- [ ] Commit: `feat(tools-rs): add EnterDesignModeTool`.

---

## Task 5: Add `stop_turn` to `tools-rs::ExecutableToolResult` and implement `ExitPlanModeTool`

**Depends on:** Task 1, Task 2.

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/mod.rs` (add `stop_turn` field and helpers)
- Modify: `rust-ody/crates/tools-rs/src/builtin/*.rs` (every direct `ExecutableToolResult { ... }` literal, see list)
- Modify: `rust-ody/crates/agent-rs/src/tool/bridge.rs` (forward `stop_turn`)
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_plan_mode.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` (add `pub mod exit_plan_mode;`)
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_plan_mode.rs` (inline tests)

**Why:** TS `ExitPlanModeTool` returns `stopTurn: true` so the turn ends after the plan is handed off. `tools-rs::ExecutableToolResult` currently has no `stop_turn` field, so the bridge always forwards `None`. This task adds the field, updates every construction site, and then implements the exit tool.

**Steps:**

- [ ] Write a failing test in `rust-ody/crates/agent-rs/src/tool/bridge.rs`:

```rust
#[tokio::test]
async fn bridge_forwards_stop_turn_from_tool_execution() {
    struct StopTurnTool;
    impl tools_rs::builtin::BuiltinTool for StopTurnTool {
        fn name(&self) -> &str { "StopTurn" }
        fn description(&self) -> &str { "stop" }
        fn parameters(&self) -> serde_json::Value { json!({"type":"object"}) }
        fn resolve_execution(
            &self,
            _args: serde_json::Value,
        ) -> Result<tools_rs::builtin::ToolExecution, tools_rs::builtin::ToolError> {
            Ok(tools_rs::builtin::ToolExecution {
                accesses: tools_rs::tool_accesses::ToolAccesses::none(),
                description: "stop".into(),
                approval_rule: "StopTurn".into(),
                matches_rule: None,
                display: None,
                execute: Box::new(|_ctx| Box::pin(async {
                    tools_rs::builtin::ExecutableToolResult {
                        output: tools_rs::builtin::ExecutableToolOutput::Text("ok".into()),
                        message: None,
                        is_error: false,
                        stop_turn: Some(true),
                    }
                })),
            })
        }
    }

    let bridge = ToolBridge::new(Arc::new(StopTurnTool));
    let exec = bridge.resolve_execution(json!({})).await.unwrap();
    match exec {
        crate::agent_loop::types::ToolExecution::Runnable(r) => {
            let result = (r.execute)(LoopContext {
                turn_id: "1".into(),
                tool_call_id: "call_1".into(),
                metadata: None,
                signal: kosong_rs::provider::AbortSignal::new(),
                on_update: None,
            }).await.unwrap();
            match result {
                crate::records::nested::ExecutableToolResult::Success(s) => {
                    assert_eq!(s.stop_turn, Some(true));
                }
                _ => panic!("expected success"),
            }
        }
        _ => panic!("expected Runnable"),
    }
}
```

Run it:

```bash
cd rust-ody && cargo test -p agent-rs tool::bridge::tests::bridge_forwards_stop_turn_from_tool_execution
```

Expected failure: `no field stop_turn on type ExecutableToolResult`.

- [ ] Extend `tools-rs::ExecutableToolResult` in `rust-ody/crates/tools-rs/src/builtin/mod.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableToolResult {
    pub output: ExecutableToolOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_turn: Option<bool>,
}
```

Update the helpers:

```rust
impl ExecutableToolResult {
    pub fn ok_text(output: String) -> Self {
        Self {
            output: ExecutableToolOutput::Text(output),
            message: None,
            is_error: false,
            stop_turn: None,
        }
    }
    pub fn error_text(output: String, message: String) -> Self {
        Self {
            output: ExecutableToolOutput::Text(output),
            message: Some(message),
            is_error: true,
            stop_turn: None,
        }
    }
}
```

- [ ] Forward `stop_turn` in `rust-ody/crates/agent-rs/src/tool/bridge.rs` `From<ToolsResult> for AgentResult`:

For `Success`:

```rust
AgentResult::Success(ExecutableToolSuccessResult {
    output,
    is_error: None,
    stop_turn: r.stop_turn,
    message: r.message,
})
```

For `Error`:

```rust
AgentResult::Error(ExecutableToolErrorResult {
    output,
    is_error: true,
    stop_turn: r.stop_turn,
    message: r.message,
})
```

- [ ] Find and update every direct `ExecutableToolResult { ... }` literal in `tools-rs` to include `stop_turn: None`:

```bash
cd rust-ody && rg -n "ExecutableToolResult \{" crates/tools-rs/src | rg -v "mod.rs|test"
```

Affected production files:
- `crates/tools-rs/src/builtin/cron/cron_list.rs`
- `crates/tools-rs/src/builtin/cron/cron_create.rs`
- `crates/tools-rs/src/builtin/cron/cron_delete.rs`
- `crates/tools-rs/src/builtin/bash.rs`
- `crates/tools-rs/src/builtin/media.rs`
- `crates/tools-rs/src/builtin/background/task_list.rs`
- `crates/tools-rs/src/builtin/background/task_stop.rs`
- `crates/tools-rs/src/builtin/background/task_output.rs`
- `crates/tools-rs/src/builtin/collaboration/ask_user.rs`
- `crates/tools-rs/src/builtin/collaboration/agent.rs`

For each literal add `stop_turn: None,`. Test helper literals (e.g. `run_bash`) must also be updated.

- [ ] Run the bridge test and whole-workspace typecheck:

```bash
cd rust-ody && cargo test -p agent-rs tool::bridge::tests::bridge_forwards_stop_turn_from_tool_execution
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green.

- [ ] Create `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_plan_mode.rs`:

```rust
use std::sync::Arc;
use serde_json::Value;

use crate::builtin::session_mode::planning::{
    declared_option_label, is_via_approval, selected_approach_prefix, selected_label_of,
    ExitModeOption,
};
use crate::builtin::session_mode::{SessionModeKind, SessionModeProvider};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError, ToolExecution};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExitPlanModeInput {
    #[serde(default)]
    pub options: Vec<ExitModeOption>,
}

pub struct ExitPlanModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitPlanModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "ExitPlanMode"
    }

    fn description(&self) -> &str {
        "Present the finalized plan to the user and exit plan mode."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "options": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 3,
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string", "minLength": 1, "maxLength": 80 },
                            "description": { "type": "string" }
                        },
                        "required": ["label"]
                    },
                    "description": "When the plan contains multiple alternative approaches, list them here so the user can choose which one to execute."
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: ExitPlanModeInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let provider = Arc::clone(&self.provider);
        let display = build_plan_review_display(&*provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Presenting plan and exiting plan mode".into(),
            approval_rule: "ExitPlanMode".into(),
            matches_rule: None,
            display,
            execute: Box::new(move |ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    execute_exit_plan_mode(provider, input, ctx).await
                })
            }),
        })
    }
}

fn build_plan_review_display(provider: &dyn SessionModeProvider) -> Option<Value> {
    if !provider.is_session_mode_active() {
        return None;
    }
    let path = provider.session_mode_file_path()?;
    let content = match read_session_mode_file_sync(provider.kaos().as_ref(), &path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let display = serde_json::json!({
        "kind": "plan_review",
        "plan": trimmed,
        "path": path,
    });
    Some(display)
}

fn read_session_mode_file_sync(kaos: &dyn SessionModeContext, path: &str) -> anyhow::Result<String> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(kaos.read_text(path))
    })
}

async fn execute_exit_plan_mode(
    provider: Arc<dyn SessionModeProvider>,
    input: ExitPlanModeInput,
    _ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if !provider.is_session_mode_active() {
        return ExecutableToolResult::error_text(
            "ExitPlanMode can only be called while plan mode is active. Use EnterPlanMode (or /plan) first.".into(),
            "not in plan mode".into(),
        );
    }

    let (plan, path) = match resolve_plan(&*provider).await {
        Ok(v) => v,
        Err(e) => return e,
    };

    let option_label = declared_option_label(
        Some(&input.options),
        selected_label_of(_ctx.metadata.as_ref()).as_deref(),
    );

    // Telemetry: plan_submitted only when not via approval.
    // Note: execution metadata is not available in this closure signature; we pass it through ctx.
    let metadata = _ctx.metadata.as_ref();
    if !is_via_approval(metadata) {
        provider.telemetry().track(
            "plan_submitted",
            std::collections::HashMap::from([(
                "has_options".into(),
                serde_json::Value::Bool(input.options.len() >= 2),
            )]),
        );
    }

    if let Err(e) = provider.handoff_to("normal", option_label.clone()).await {
        return ExecutableToolResult::error_text(
            format!("Failed to exit plan mode: {}", e),
            "handoff failed".into(),
        );
    }

    if is_via_approval(metadata) {
        let raw_label = selected_label_of(metadata);
        let props = if let Some(l) = raw_label {
            std::collections::HashMap::from([
                ("outcome".into(), "approved".into()),
                ("chosen_option".into(), l.into()),
            ])
        } else {
            std::collections::HashMap::from([("outcome".into(), "approved".into())])
        };
        provider.telemetry().track("plan_resolved", props);
    } else {
        provider.telemetry().track(
            "plan_resolved",
            std::collections::HashMap::from([("outcome".into(), "auto_approved".into())]),
        );
    }

    let output = format!(
        "{}Exited plan mode. {}Plan mode deactivated. The approved plan has been handed off to the main conversation context.\n{}\n## Approved Plan:\n{}\n\nSTOP — do NOT begin executing now. This turn ends here. The user will start implementation themselves — the plan is now available in their main conversation context.",
        selected_approach_prefix(option_label.as_deref()),
        if path.is_some() { "Plan saved to: ".to_string() + path.as_ref().unwrap().as_str() + "\n\n" } else { String::new() },
        // path line is included above when present
        plan
    );

    ExecutableToolResult {
        output: ExecutableToolOutput::Text(output),
        message: None,
        is_error: false,
        stop_turn: Some(true),
    }
}

async fn resolve_plan(provider: &dyn SessionModeProvider) -> Result<(String, Option<String>), ExecutableToolResult> {
    let path = provider.session_mode_file_path();
    let content = match path.as_ref() {
        Some(p) => match provider.kaos().read_text(p).await {
            Ok(c) => c,
            Err(e) => {
                return Err(ExecutableToolResult::error_text(
                    format!("Failed to read plan file: {}", e),
                    "read failed".into(),
                ));
            }
        },
        None => String::new(),
    };

    if content.trim().is_empty() {
        return Err(ExecutableToolResult::error_text(
            match path {
                Some(p) => format!("No plan file found. Write your plan to {} first, then call ExitPlanMode.", p),
                None => "No plan file found. Write the plan to the current plan file first, then call ExitPlanMode.".into(),
            },
            "empty plan".into(),
        ));
    }

    Ok((content, path))
}
```

Wait — `build_plan_review_display` is called synchronously in `resolve_execution`, but it needs to read the file asynchronously. The `SessionModeContext::read_text` is async. We cannot `block_on` inside an async context if the runtime is already running. This is a design problem.

Fix: move display construction into the execute closure (which is async) and return `ToolExecution` with `display: None` from `resolve_execution`, then set the display at execution time? No, `ToolExecution.display` is immutable once constructed.

Alternative: change `SessionModeContext::read_text` to a synchronous `fn read_text(&self, path: &str) -> anyhow::Result<String>` like `agent-rs::SessionModeContext::read_file`. Looking at Part 1 infra.md Task 3, `SessionModeContext` has `async fn read_text`. That was my design choice, but it conflicts with synchronous display construction.

- [ ] Add `pub mod exit_plan_mode;` to `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`.

- [ ] Add inline tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::tests::MockSessionModeProvider;

    #[tokio::test]
    async fn exit_plan_mode_hands_off_to_normal() {
        let provider = Arc::new(MockSessionModeProvider::plan_mode_with_content("## Plan\n\nDo X."));
        let tool = ExitPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        assert!(exec.display.is_some());
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(!result.is_error);
        assert_eq!(result.stop_turn, Some(true));
        let text = result.to_text();
        assert!(text.contains("Plan mode deactivated"));
        assert!(provider.handed_off_to.lock().unwrap().contains(&("normal".to_string(), None)));
    }

    #[tokio::test]
    async fn exit_plan_mode_preserves_selected_label() {
        let provider = Arc::new(MockSessionModeProvider::plan_mode_with_content("## Plan\n\nDo X."));
        let tool = ExitPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({
            "options": [{"label": "Fast", "description": ""}]
        })).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(!result.is_error);
        // Mock provider records selected_label from handoff; verify it is None when no metadata.
        assert!(provider.handed_off_to.lock().unwrap().contains(&("normal".to_string(), None)));
    }

    #[tokio::test]
    async fn exit_plan_mode_errors_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = ExitPlanModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("ExitPlanMode can only be called while plan mode is active"));
    }
}
```

- [ ] Run tests and whole-workspace typecheck:

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::exit_plan_mode
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green.

- [ ] Commit: `feat(tools-rs): add stop_turn to ExecutableToolResult and ExitPlanModeTool`.

---

## Task 6: Implement design completeness checker and `ExitDesignModeTool`

**Depends on:** Task 5.

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_design_mode.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` (add `pub mod exit_design_mode;`)
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_design_mode.rs` (inline tests)

**Why:** Mirrors TS `ExitDesignModeTool`. Before presenting the design it validates that required sections exist; on approval it hands off to plan mode.

**Steps:**

- [ ] Create `rust-ody/crates/tools-rs/src/builtin/session_mode/exit_design_mode.rs`:

```rust
use std::sync::Arc;
use serde_json::Value;

use crate::builtin::session_mode::planning::{
    declared_option_label, selected_approach_prefix, selected_label_of, ExitModeOption,
};
use crate::builtin::session_mode::{SessionModeKind, SessionModeProvider};
use crate::builtin::{BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError, ToolExecution};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExitDesignModeInput {
    #[serde(default)]
    pub options: Vec<ExitModeOption>,
}

pub struct ExitDesignModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitDesignModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitDesignModeTool {
    fn name(&self) -> &str {
        "ExitDesignMode"
    }

    fn description(&self) -> &str {
        "Present the finalized design document to the user and exit design mode."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "options": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 3,
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string", "minLength": 1, "maxLength": 80 },
                            "description": { "type": "string" }
                        },
                        "required": ["label"]
                    },
                    "description": "When the design presents multiple alternative directions, list them here so the user can choose which one to pursue."
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: ExitDesignModeInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let provider = Arc::clone(&self.provider);

        // If active, run completeness check before building the review display.
        if provider.is_session_mode_active() {
            if let Some(path) = provider.session_mode_file_path() {
                if let Ok(content) = provider.kaos().read_text(&path) {
                    let missing = find_missing_design_sections(&content);
                    if !missing.is_empty() {
                        let list = missing.iter().map(|m| format!("- {}", m)).collect::<Vec<_>>().join("\n");
                        return Ok(ToolExecution {
                            accesses: Default::default(),
                            description: "Design is incomplete".into(),
                            approval_rule: "ExitDesignMode".into(),
                            matches_rule: None,
                            display: None,
                            execute: Box::new(move |_ctx| Box::pin(async move {
                                ExecutableToolResult::error_text(
                                    format!("Design is incomplete. Missing:\n{}\n\nPlease add the missing sections to the design file, then call ExitDesignMode again.", list),
                                    "incomplete design".into(),
                                )
                            })),
                        });
                    }
                }
            }
        }

        let display = build_design_review_display(&*provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Presenting design and exiting design mode".into(),
            approval_rule: "ExitDesignMode".into(),
            matches_rule: None,
            display,
            execute: Box::new(move |ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    execute_exit_design_mode(provider, input, ctx).await
                })
            }),
        })
    }
}

pub fn find_missing_design_sections(content: &str) -> Vec<String> {
    let mut missing = Vec::new();
    let trimmed = content.trim();

    if trimmed.len() < 300 {
        missing.push("sufficient content (design appears incomplete or empty)".into());
    }

    let heading_count = trimmed.matches("\n## ").count()
        + if trimmed.starts_with("## ") { 1 } else { 0 };
    if heading_count < 3 {
        missing.push(format!("at least 3 design sections (found {})", heading_count));
    }

    let checks: Vec<(&str, regex::Regex)> = vec![
        ("Scope or Scope In/Out section", regex::Regex::new(r"(?im)^#{1,3}\s+(scope|in/out|范围|scope\s+in)\b").unwrap()),
        ("Architecture or Design section", regex::Regex::new(r"(?im)^#{1,3}\s+(architecture|design|approach|overview|架构|设计方案)\b").unwrap()),
        ("Data Models section", regex::Regex::new(r"(?im)^#{1,3}\s+(data\s*models?|数据模型|models?|data\s+&?\s*state)\b").unwrap()),
        ("Algorithms section", regex::Regex::new(r"(?im)^#{1,3}\s+(algorithms?|算法|pseudocode|implementation\s+notes?)\b").unwrap()),
        ("Error Handling section", regex::Regex::new(r"(?im)^#{1,3}\s+(error\s*handling|错误处理|errors?|degradation|failure\s+scenarios?)\b").unwrap()),
        ("Self-Review section", regex::Regex::new(r"(?im)^#{1,3}\s+(self[- ]?review|自检|review|audit)\b").unwrap()),
        ("User Approval", regex::Regex::new(r"(?im)^#{1,3}\s+(user\s+(final\s+)?approval|用户批准|批准状态|approved?)\b").unwrap()),
        ("Reuse Analysis section", regex::Regex::new(r"(?im)^#{1,3}\s+(reuse\s+analysis|复用分析|component\s+reuse|existing\s+components?)\b").unwrap()),
    ];

    for (name, re) in checks {
        if !re.is_match(trimmed) {
            missing.push(name.into());
        }
    }

    missing
}

fn build_design_review_display(provider: &dyn SessionModeProvider) -> Option<Value> {
    if !provider.is_session_mode_active() {
        return None;
    }
    let path = provider.session_mode_file_path()?;
    let content = read_session_mode_file_sync(provider.kaos().as_ref(), &path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "kind": "plan_review",
        "plan": trimmed,
        "path": path,
    }))
}

async fn execute_exit_design_mode(
    provider: Arc<dyn SessionModeProvider>,
    input: ExitDesignModeInput,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if !provider.is_session_mode_active() {
        return ExecutableToolResult::error_text(
            "ExitDesignMode can only be called while design mode is active. Use EnterDesignMode (or /design) first.".into(),
            "not in design mode".into(),
        );
    }

    let path = provider.session_mode_file_path();
    if path.is_none() {
        return ExecutableToolResult::error_text(
            "No design file found. Write the design to the current design file first, then call ExitDesignMode.".into(),
            "no design file".into(),
        );
    }

    let option_label = declared_option_label(
        Some(&input.options),
        selected_label_of(ctx.metadata.as_ref()).as_deref(),
    );

    if let Err(e) = provider.handoff_to("plan", option_label.clone()).await {
        return ExecutableToolResult::error_text(
            format!("Failed to exit design mode: {}", e),
            "handoff failed".into(),
        );
    }

    let saved_to = path.as_ref().map(|p| format!("Design saved to: {}\n\n", p)).unwrap_or_default();
    let output = format!(
        "{}Design mode deactivated. Now in plan mode.\n\n{}Create a concrete, step-by-step implementation plan based on the approved design document.",
        selected_approach_prefix(option_label.as_deref()),
        saved_to,
    );

    ExecutableToolResult {
        output: ExecutableToolOutput::Text(output),
        message: None,
        is_error: false,
        stop_turn: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::tests::MockSessionModeProvider;

    fn complete_design() -> String {
        "# Design\n\n## Scope\nIn.\n\n## Architecture\nDiagram.\n\n## Data Models\nFoo.\n\n## Algorithms\nBar.\n\n## Error Handling\nBaz.\n\n## Self-Review\nOk.\n\n## User Final Approval\nPending.\n\n## Reuse Analysis\nNone.\n".into()
    }

    #[test]
    fn find_missing_sections_flags_short_content() {
        let missing = find_missing_design_sections("Too short.");
        assert!(missing.iter().any(|m| m.contains("sufficient content")));
    }

    #[test]
    fn find_missing_sections_flags_missing_architecture() {
        let mut content = complete_design();
        content = content.replace("## Architecture\nDiagram.\n", "");
        let missing = find_missing_design_sections(&content);
        assert!(missing.iter().any(|m| m.contains("Architecture")));
    }

    #[test]
    fn complete_design_has_no_missing_sections() {
        let missing = find_missing_design_sections(&complete_design());
        assert!(missing.is_empty(), "missing: {:?}", missing);
    }

    #[tokio::test]
    async fn exit_design_mode_hands_off_to_plan() {
        let provider = Arc::new(MockSessionModeProvider::design_mode_with_content(&complete_design()));
        let tool = ExitDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        assert!(exec.display.is_some());
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(!result.is_error);
        assert_eq!(result.stop_turn, Some(true));
        assert!(provider.handed_off_to.lock().unwrap().contains(&("plan".to_string(), None)));
    }

    #[tokio::test]
    async fn exit_design_mode_rejects_incomplete_design() {
        let provider = Arc::new(MockSessionModeProvider::design_mode_with_content("## Scope\nOnly."));
        let tool = ExitDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("Design is incomplete"));
    }
}
```

- [ ] Add `pub mod exit_design_mode;` to `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`.

- [ ] Add `regex` to `tools-rs` dependencies if not already present. Check `rust-ody/crates/tools-rs/Cargo.toml` and add `regex = "1"` under `[dependencies]` if missing.

- [ ] Run tests and whole-workspace typecheck:

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::exit_design_mode
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green.

- [ ] Commit: `feat(tools-rs): add ExitDesignModeTool and design completeness checker`.

---

## Task 7: Implement in-memory state stores and `AgentSessionModeProvider` adapter

**Depends on:** `infra.md` Task 3 (trait surface), Task 5 (tools exist, need provider for tests).

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/stores.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` (add `pub mod stores;` and test mock)
- Create: `rust-ody/crates/agent-rs/src/session_mode/provider.rs`
- Modify: `rust-ody/crates/agent-rs/src/session_mode/mod.rs` (add `pub mod provider;`)
- Test: `rust-ody/crates/agent-rs/src/session_mode/provider.rs` (inline tests)

**Why:** `SessionModeProvider` is the boundary between `tools-rs` and `agent-rs`. The adapter lets planning tools call into the real `Agent` without a circular dependency. In-memory stores satisfy the trait today; Part 5 replaces them with file-backed stores.

**Steps:**

- [ ] Create `rust-ody/crates/tools-rs/src/builtin/session_mode/stores.rs`:

```rust
use std::sync::Mutex;
use crate::builtin::session_mode::{
    BuilderProfileEntry, GameDesignProfileEntry, GameDesignStateStore, LearningEntry,
    OfficeHoursStateStore,
};

pub struct InMemoryOfficeHoursStateStore {
    profiles: Mutex<Vec<BuilderProfileEntry>>,
    learnings: Mutex<Vec<LearningEntry>>,
}

impl InMemoryOfficeHoursStateStore {
    pub fn new() -> Self {
        Self {
            profiles: Mutex::new(Vec::new()),
            learnings: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl OfficeHoursStateStore for InMemoryOfficeHoursStateStore {
    async fn append_profile(&self, entry: BuilderProfileEntry) -> anyhow::Result<()> {
        self.profiles.lock().unwrap().push(entry);
        Ok(())
    }

    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()> {
        self.learnings.lock().unwrap().push(entry);
        Ok(())
    }

    async fn search_learnings(
        &self,
        limit: usize,
        _cross_project: bool,
    ) -> anyhow::Result<Vec<LearningEntry>> {
        let items = self.learnings.lock().unwrap();
        Ok(items.iter().rev().take(limit).cloned().collect())
    }
}

pub struct InMemoryGameDesignStateStore {
    profiles: Mutex<Vec<GameDesignProfileEntry>>,
    learnings: Mutex<Vec<LearningEntry>>,
}

impl InMemoryGameDesignStateStore {
    pub fn new() -> Self {
        Self {
            profiles: Mutex::new(Vec::new()),
            learnings: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl GameDesignStateStore for InMemoryGameDesignStateStore {
    async fn append_profile(&self, entry: GameDesignProfileEntry) -> anyhow::Result<()> {
        self.profiles.lock().unwrap().push(entry);
        Ok(())
    }

    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()> {
        self.learnings.lock().unwrap().push(entry);
        Ok(())
    }

    async fn search_learnings(
        &self,
        limit: usize,
        _branch: Option<String>,
    ) -> anyhow::Result<Vec<LearningEntry>> {
        let items = self.learnings.lock().unwrap();
        Ok(items.iter().rev().take(limit).cloned().collect())
    }
}
```

- [ ] Add `pub mod stores;` to `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`.

- [ ] Add the test mock to `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` inside `#[cfg(test)] pub mod tests`:

```rust
#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use crate::builtin::session_mode::stores::{
        InMemoryGameDesignStateStore, InMemoryOfficeHoursStateStore,
    };

    pub struct MockTelemetryClient {
        pub events: Mutex<Vec<(String, std::collections::HashMap<String, serde_json::Value>)>>,
    }

    impl MockTelemetryClient {
        pub fn new() -> Self {
            Self { events: Mutex::new(Vec::new()) }
        }
    }

    impl TelemetryClient for MockTelemetryClient {
        fn track(&self, event: &str, properties: std::collections::HashMap<String, serde_json::Value>) {
            self.events.lock().unwrap().push((event.into(), properties));
        }
    }

    pub struct MockMcpProvider;

    #[async_trait::async_trait]
    impl McpProvider for MockMcpProvider {
        async fn gbrain_available(&self) -> bool { false }
    }

    pub struct MockKaosContext {
        files: Mutex<std::collections::HashMap<String, String>>,
    }

    impl MockKaosContext {
        pub fn new() -> Self {
            Self { files: Mutex::new(std::collections::HashMap::new()) }
        }
        pub fn with_file(self, path: &str, content: &str) -> Self {
            self.files.lock().unwrap().insert(path.into(), content.into());
            self
        }
    }

    #[async_trait::async_trait]
    impl SessionModeContext for MockKaosContext {
        fn cwd(&self) -> String { "/".into() }
        fn project_root(&self) -> Option<String> { None }
        async fn read_text(&self, path: &str) -> anyhow::Result<String> {
            self.files.lock().unwrap().get(path).cloned().ok_or_else(|| anyhow::anyhow!("not found"))
        }
        async fn write_text(&self, _path: &str, _content: &str) -> anyhow::Result<()> { Ok(()) }
        async fn stat(&self, _path: &str) -> anyhow::Result<()> { Ok(()) }
    }

    pub struct MockSessionModeProvider {
        pub active: Mutex<bool>,
        pub kind: Mutex<Option<SessionModeKind>>,
        pub file_path: Mutex<Option<String>>,
        pub entered: Mutex<Vec<SessionModeKind>>,
        pub exited: Mutex<bool>,
        pub handed_off_to: Mutex<Vec<(String, Option<String>)>>,
        pub kaos: Arc<dyn SessionModeContext>,
        pub telemetry: Arc<dyn TelemetryClient>,
        pub office_hours_store: Arc<dyn OfficeHoursStateStore>,
        pub game_design_store: Arc<dyn GameDesignStateStore>,
        pub mcp: Arc<dyn McpProvider>,
    }

    impl MockSessionModeProvider {
        pub fn inactive() -> Self {
            Self {
                active: Mutex::new(false),
                kind: Mutex::new(None),
                file_path: Mutex::new(None),
                entered: Mutex::new(Vec::new()),
                exited: Mutex::new(false),
                handed_off_to: Mutex::new(Vec::new()),
                kaos: Arc::new(MockKaosContext::new()),
                telemetry: Arc::new(MockTelemetryClient::new()),
                office_hours_store: Arc::new(InMemoryOfficeHoursStateStore::new()),
                game_design_store: Arc::new(InMemoryGameDesignStateStore::new()),
                mcp: Arc::new(MockMcpProvider),
            }
        }

        pub fn active(kind: SessionModeKind) -> Self {
            let mut s = Self::inactive();
            *s.active.lock().unwrap() = true;
            *s.kind.lock().unwrap() = Some(kind);
            s
        }

        pub fn plan_mode_with_content(content: &str) -> Self {
            let mut s = Self::active(SessionModeKind::Plan);
            let path = "/plan.md".to_string();
            *s.file_path.lock().unwrap() = Some(path.clone());
            s.kaos = Arc::new(MockKaosContext::new().with_file(&path, content));
            s
        }

        pub fn design_mode_with_content(content: &str) -> Self {
            let mut s = Self::active(SessionModeKind::Design);
            let path = "/design.md".to_string();
            *s.file_path.lock().unwrap() = Some(path.clone());
            s.kaos = Arc::new(MockKaosContext::new().with_file(&path, content));
            s
        }
    }

    #[async_trait::async_trait]
    impl SessionModeProvider for MockSessionModeProvider {
        fn is_session_mode_active(&self) -> bool { *self.active.lock().unwrap() }
        fn session_mode_kind(&self) -> Option<SessionModeKind> { *self.kind.lock().unwrap() }
        fn session_mode_file_path(&self) -> Option<String> { self.file_path.lock().unwrap().clone() }

        async fn enter_session_mode(&self, kind: SessionModeKind) -> anyhow::Result<()> {
            self.entered.lock().unwrap().push(kind);
            *self.active.lock().unwrap() = true;
            *self.kind.lock().unwrap() = Some(kind);
            Ok(())
        }

        async fn exit_session_mode(&self) -> anyhow::Result<()> {
            *self.exited.lock().unwrap() = true;
            *self.active.lock().unwrap() = false;
            *self.kind.lock().unwrap() = None;
            Ok(())
        }

        async fn handoff_to(&self, target: &str, selected_label: Option<String>) -> anyhow::Result<()> {
            self.handed_off_to.lock().unwrap().push((target.into(), selected_label));
            *self.active.lock().unwrap() = false;
            *self.kind.lock().unwrap() = None;
            Ok(())
        }

        fn user_language(&self) -> Language { Language::En }
        fn set_user_language(&self, _lang: Language) {}
        fn open_external_available(&self) -> bool { false }
        fn telemetry(&self) -> Arc<dyn TelemetryClient> { self.telemetry.clone() }
        fn kaos(&self) -> Arc<dyn SessionModeContext> { self.kaos.clone() }
        fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore> { self.office_hours_store.clone() }
        fn game_design_store(&self) -> Arc<dyn GameDesignStateStore> { self.game_design_store.clone() }
        fn mcp(&self) -> Arc<dyn McpProvider> { self.mcp.clone() }
    }
}
```

- [ ] Create `rust-ody/crates/agent-rs/src/session_mode/provider.rs`:

```rust
use std::sync::{Arc, Mutex, Weak};
use async_trait::async_trait;

use crate::agent::Agent;
use crate::records::nested::SessionModeKind;
use crate::session_mode::manager::SessionModeManager;
use crate::session_mode::types::HandoffOptions;
use tools_rs::builtin::session_mode::{
    GameDesignStateStore, Language, McpProvider, OfficeHoursStateStore, SessionModeContext,
    SessionModeProvider, TelemetryClient,
};

pub struct AgentSessionModeProvider {
    agent: Weak<Agent>,
    language: Mutex<Language>,
    office_hours_store: Arc<dyn OfficeHoursStateStore>,
    game_design_store: Arc<dyn GameDesignStateStore>,
    telemetry: Arc<dyn TelemetryClient>,
    mcp: Arc<dyn McpProvider>,
}

impl AgentSessionModeProvider {
    pub fn new(
        agent: Weak<Agent>,
        office_hours_store: Arc<dyn OfficeHoursStateStore>,
        game_design_store: Arc<dyn GameDesignStateStore>,
        telemetry: Arc<dyn TelemetryClient>,
        mcp: Arc<dyn McpProvider>,
    ) -> Self {
        Self {
            agent,
            language: Mutex::new(Language::En),
            office_hours_store,
            game_design_store,
            telemetry,
            mcp,
        }
    }

    fn upgrade(&self) -> Option<Arc<Agent>> {
        self.agent.upgrade()
    }
}

#[async_trait]
impl SessionModeProvider for AgentSessionModeProvider {
    fn is_session_mode_active(&self) -> bool {
        self.upgrade().map(|a| a.session_mode.lock().unwrap().is_active()).unwrap_or(false)
    }

    fn session_mode_kind(&self) -> Option<SessionModeKind> {
        self.upgrade().and_then(|a| a.session_mode.lock().unwrap().kind())
    }

    fn session_mode_file_path(&self) -> Option<String> {
        self.upgrade().and_then(|a| a.session_mode.lock().unwrap().session_mode_file_path())
    }

    async fn enter_session_mode(&self, kind: SessionModeKind) -> anyhow::Result<()> {
        if let Some(agent) = self.upgrade() {
            agent.enter_session_mode(kind, None).await?;
        }
        Ok(())
    }

    async fn exit_session_mode(&self) -> anyhow::Result<()> {
        if let Some(agent) = self.upgrade() {
            agent.exit_session_mode().await?;
        }
        Ok(())
    }

    async fn handoff_to(&self, target: &str, selected_label: Option<String>) -> anyhow::Result<()> {
        if let Some(agent) = self.upgrade() {
            agent.session_mode.lock().unwrap().handoff_to(target, HandoffOptions { selected_label }).await?;
        }
        Ok(())
    }

    fn user_language(&self) -> Language {
        *self.language.lock().unwrap()
    }

    fn set_user_language(&self, lang: Language) {
        *self.language.lock().unwrap() = lang;
    }

    fn open_external_available(&self) -> bool {
        // The host sets `agent.rpc` after construction; default to false until wired.
        false
    }

    fn telemetry(&self) -> Arc<dyn TelemetryClient> {
        self.telemetry.clone()
    }

    fn kaos(&self) -> Arc<dyn SessionModeContext> {
        self.upgrade()
            .map(|a| Arc::new(KaosSessionModeContext { kaos: a.kaos.clone() }) as Arc<dyn SessionModeContext>)
            .unwrap_or_else(|| Arc::new(EmptySessionModeContext))
    }

    fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore> {
        self.office_hours_store.clone()
    }

    fn game_design_store(&self) -> Arc<dyn GameDesignStateStore> {
        self.game_design_store.clone()
    }

    fn mcp(&self) -> Arc<dyn McpProvider> {
        self.mcp.clone()
    }
}

struct KaosSessionModeContext {
    kaos: Arc<kaos_rs::kaos::Kaos>,
}

#[async_trait]
impl SessionModeContext for KaosSessionModeContext {
    fn cwd(&self) -> String {
        self.kaos.getcwd()
    }

    fn project_root(&self) -> Option<String> {
        None
    }

    async fn read_text(&self, path: &str) -> anyhow::Result<String> {
        Ok(self.kaos.read_text(path, None, None).await?)
    }

    async fn write_text(&self, path: &str, content: &str) -> anyhow::Result<()> {
        self.kaos.write_text(path, content, None, None).await?;
        Ok(())
    }

    async fn stat(&self, path: &str) -> anyhow::Result<()> {
        self.kaos.stat(path, true).await?;
        Ok(())
    }
}

struct EmptySessionModeContext;

#[async_trait]
impl SessionModeContext for EmptySessionModeContext {
    fn cwd(&self) -> String { "/".into() }
    fn project_root(&self) -> Option<String> { None }
    async fn read_text(&self, _path: &str) -> anyhow::Result<String> { Ok(String::new()) }
    async fn write_text(&self, _path: &str, _content: &str) -> anyhow::Result<()> { Ok(()) }
    async fn stat(&self, _path: &str) -> anyhow::Result<()> { Ok(()) }
}
```

- [ ] Add `pub mod provider;` to `rust-ody/crates/agent-rs/src/session_mode/mod.rs`.

- [ ] Add a test in `rust-ody/crates/agent-rs/src/session_mode/provider.rs` that creates a minimal `Agent`, builds an `AgentSessionModeProvider`, and asserts `enter_session_mode(Plan)` makes `is_session_mode_active()` true:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentBuilder, NoopEnv};
    use std::sync::Arc;

    #[tokio::test]
    async fn adapter_enters_plan_mode() {
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
            kaos_rs::environment::Environment {
                os_kind: "macOS".into(),
                os_arch: "arm64".into(),
                os_version: "23.0.0".into(),
                shell_name: "bash".into(),
                shell_path: "/bin/bash".into(),
            },
            std::env::current_dir().unwrap(),
        ));
        let env = Arc::new(NoopEnv);
        let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
        let provider = AgentSessionModeProvider::new(
            Arc::downgrade(&agent),
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryOfficeHoursStateStore::new()),
            Arc::new(tools_rs::builtin::session_mode::stores::InMemoryGameDesignStateStore::new()),
            Arc::new(tools_rs::builtin::session_mode::MockTelemetryClient::new()),
            Arc::new(tools_rs::builtin::session_mode::MockMcpProvider),
        );
        provider.enter_session_mode(SessionModeKind::Plan).await.unwrap();
        assert!(provider.is_session_mode_active());
        assert_eq!(provider.session_mode_kind(), Some(SessionModeKind::Plan));
    }
}
```

Wait — `MockTelemetryClient` and `MockMcpProvider` are in `tools-rs` under `#[cfg(test)]`, so they are not exported for `agent-rs` to use. Create simple local structs in the agent-rs test instead, or make the mocks public in tools-rs. For the plan, use local test structs in `provider.rs`.

- [ ] Run tests and typecheck:

```bash
cd rust-ody && cargo test -p agent-rs session_mode::provider
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green.

- [ ] Commit: `feat(agent-rs): add AgentSessionModeProvider adapter and in-memory stores`.

---

## Task 8: Wire planning tools into `Agent::loop_tools`

**Depends on:** Task 7.

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs` (`build_tools` signature + session-mode tool registration)
- Modify: `rust-ody/crates/agent-rs/src/agent.rs` (add `session_mode_provider` field, build and inject provider)
- Modify: `rust-ody/crates/agent-rs/src/tool/collaboration/subagent_host.rs` (update `build_tools` call)
- Test: `rust-ody/crates/agent-rs/tests/session_mode_tools_integration.rs`

**Why:** The agent loop consumes tools through `Agent::loop_tools`, which delegates to `CollaborationToolkit::build_tools`. Session-mode tools must be added there with the provider adapter created in Task 7.

**Steps:**

- [ ] Modify `rust-ody/crates/agent-rs/src/tool/collaboration/mod.rs`:

```rust
use tools_rs::builtin::session_mode::{
    enter_design_mode::EnterDesignModeTool,
    enter_plan_mode::EnterPlanModeTool,
    exit_design_mode::ExitDesignModeTool,
    exit_plan_mode::ExitPlanModeTool,
    SessionModeProvider,
};

pub struct CollaborationToolkit;

impl CollaborationToolkit {
    pub fn build_tools(
        context: AgentContext,
        skill_registry: Option<Arc<dyn crate::skill::registry::SkillRegistry>>,
        question_callback: Option<QuestionCallback>,
        subagent_host: Option<Arc<dyn tools_rs::builtin::collaboration::SubagentHost>>,
        background_manager: Option<Arc<BackgroundManager>>,
        session_mode_provider: Option<Arc<dyn SessionModeProvider>>,
    ) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
        let mut tools: Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> = Vec::new();

        // ... existing skill / ask_user / agent tool blocks unchanged ...

        if let Some(provider) = session_mode_provider {
            tools.push(Arc::new(ToolBridge::new(Arc::new(EnterPlanModeTool::new(provider.clone())))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(ExitPlanModeTool::new(provider.clone())))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(EnterDesignModeTool::new(provider.clone())))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(ExitDesignModeTool::new(provider.clone())))));
        }

        tools
    }
}
```

- [ ] Add a `session_mode_provider` field to the `Agent` struct in `rust-ody/crates/agent-rs/src/agent.rs`:

```rust
session_mode_provider: Mutex<Option<Arc<dyn tools_rs::builtin::session_mode::SessionModeProvider>>>,
```

- [ ] In `AgentBuilder::build` in `rust-ody/crates/agent-rs/src/agent.rs`, after the `Agent` `Arc::new_cyclic` block completes, create the provider and store it:

```rust
let agent = Arc::new_cyclic(|weak| {
    // ... existing construction ...
    Agent {
        // ... existing fields ...
        session_mode_provider: Mutex::new(None),
        // ...
    }
})?;

let session_mode_provider = Arc::new(crate::session_mode::provider::AgentSessionModeProvider::new(
    Arc::downgrade(&agent),
    Arc::new(tools_rs::builtin::session_mode::stores::InMemoryOfficeHoursStateStore::new()),
    Arc::new(tools_rs::builtin::session_mode::stores::InMemoryGameDesignStateStore::new()),
    Arc::new(AgentTelemetryClient { agent: Arc::downgrade(&agent) }),
    Arc::new(AgentMcpProvider { agent: Arc::downgrade(&agent) }),
));
*agent.session_mode_provider.lock().unwrap() = Some(session_mode_provider);

*records_holder.lock().unwrap() = Some(Arc::downgrade(&agent));
Ok(agent)
```

- [ ] Define the helper structs `AgentTelemetryClient` and `AgentMcpProvider` in `rust-ody/crates/agent-rs/src/session_mode/provider.rs`:

```rust
pub struct AgentTelemetryClient {
    agent: Weak<Agent>,
}

impl TelemetryClient for AgentTelemetryClient {
    fn track(&self, event: &str, properties: std::collections::HashMap<String, serde_json::Value>) {
        if let Some(agent) = self.agent.upgrade() {
            agent.track_telemetry(event, serde_json::to_value(properties).unwrap_or_default());
        }
    }
}

pub struct AgentMcpProvider {
    agent: Weak<Agent>,
}

#[async_trait]
impl McpProvider for AgentMcpProvider {
    async fn gbrain_available(&self) -> bool {
        // MCP host is not yet ported; always report unavailable for now.
        false
    }
}
```

- [ ] Update `Agent::loop_tools` in `rust-ody/crates/agent-rs/src/agent.rs` to pass the provider:

```rust
impl TurnTools for Agent {
    fn loop_tools(&self) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
        let context = self.agent_context();
        let background = self.background.lock().unwrap().clone();
        let session_mode_provider = self.session_mode_provider.lock().unwrap().clone();
        crate::tool::collaboration::CollaborationToolkit::build_tools(
            context,
            self.skill_registry.lock().unwrap().clone(),
            self.question_callback.lock().unwrap().clone(),
            self.subagent_host.lock().unwrap().clone(),
            background,
            session_mode_provider,
        )
    }
    // ...
}
```

- [ ] Update the `build_tools` call in `rust-ody/crates/agent-rs/src/tool/collaboration/subagent_host.rs`:

```rust
tools: Some(child.tools().loop_tools()),
```

This line already calls `child.loop_tools()`, so it will automatically pick up the new signature once `Agent::loop_tools` is updated. Verify no other direct `build_tools` call exists.

- [ ] Add an integration test `rust-ody/crates/agent-rs/tests/session_mode_tools_integration.rs`:

```rust
use std::sync::Arc;
use agent_rs::agent::{AgentBuilder, NoopEnv};
use agent_rs::records::nested::SessionModeKind;

#[tokio::test]
async fn agent_exposes_planning_tools() {
    let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
        kaos_rs::environment::Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        },
        std::env::current_dir().unwrap(),
    ));
    let env = Arc::new(NoopEnv);
    let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
    let tools = agent.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name().to_string()).collect();
    assert!(names.contains(&"EnterPlanMode".into()));
    assert!(names.contains(&"ExitPlanMode".into()));
    assert!(names.contains(&"EnterDesignMode".into()));
    assert!(names.contains(&"ExitDesignMode".into()));
}

#[tokio::test]
async fn agent_enters_and_exits_plan_mode() {
    let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
        kaos_rs::environment::Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        },
        std::env::current_dir().unwrap(),
    ));
    let env = Arc::new(NoopEnv);
    let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
    agent.enter_session_mode(SessionModeKind::Plan, None).await.unwrap();
    assert!(agent.session_mode.lock().unwrap().is_active());
}
```

- [ ] Run tests and whole-workspace typecheck:

```bash
cd rust-ody && cargo test -p agent-rs session_mode_tools_integration
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green.

- [ ] Commit: `feat(agent-rs): wire Enter/Exit Plan/Design tools into Agent loop tools`.

---

## Local Self-Review

- [ ] 1. Spec-coverage table:
  - `EnterPlanModeTool` → Task 3 covered.
  - `EnterDesignModeTool` → Task 4 covered.
  - `ExitPlanModeTool` (option validation, display, telemetry, handoff, stop_turn) → Task 1 + Task 5 covered.
  - `ExitDesignModeTool` (completeness check, display, handoff) → Task 1 + Task 6 covered.
  - Plan/design entry messages → Task 2 covered.
  - `SessionModeProvider` adapter + in-memory stores → Task 7 covered.
  - Tool registration in agent loop → Task 8 covered.
  - E2E enrichment on plan exit → **GAP**: `@odysseythink/e2e-testing` has no Rust port; explicitly deferred.
- [ ] 2. Placeholder scan: no TODO/TBD in task bodies; the E2E gap is documented as a GAP, not a placeholder.
- [ ] 3. No phantom tasks: every task creates files, tests, and a commit.
- [ ] 4. Dependency soundness: Task 1/2 → Task 3/4/5/6 → Task 7 → Task 8; every `Depends on:` is satisfied earlier.
- [ ] 5. Caller & build soundness: Task 5 changes `ExecutableToolResult` (shared signature) and updates all direct struct literals plus `ToolBridge`; ends with `cargo check --workspace --all-targets`.
- [ ] 6. Test-the-risk: option-label validation tests reserved/duplicate/surviving labels; design completeness tests missing sections; exit tools test handoff target and stop_turn.
- [ ] 7. Type consistency: `SessionModeProvider` trait from Part 1 is implemented by `AgentSessionModeProvider`; `HandoffOptions` from Part 1 Task 2 is used; `ToolExecution.display` from Part 1 Task 1 is populated by exit tools.
