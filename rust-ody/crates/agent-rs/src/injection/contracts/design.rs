pub const DESIGN_ENTRY_REMINDER: &str = "\
Design mode is active. You are in a design/brainstorming session. Clarify assumptions, \
explore alternatives, and produce a design document. Do NOT implement code.";

pub const DESIGN_REENTRY_REMINDER: &str = "\
Design mode is still active. The design document already has content. Review and continue.";

pub const DESIGN_FULL_REMINDER: &str = "\
Design mode active. Remember: explore 2-3 genuinely different approaches. Tag decisions \
[C:USER] or [C:INFERRED]. Include an Assumptions section.";

pub const DESIGN_SPARSE_REMINDER: &str = "\
Design mode active. Continue exploring design alternatives. No implementation yet.";

pub const DESIGN_EXIT_REMINDER: &str = "\
Design mode has ended. You are now in normal mode. Suggest /plan to turn the design \
into a concrete implementation plan.";

/// Design→plan handoff template.
pub fn design_to_plan_handoff_reminder(path: &str, filename: &str) -> String {
    format!(
        "\
The design session has concluded. Here is the design document:

File: {path}

Please review the design. If approved, run /plan to create an implementation plan \
based on `{filename}`."
    )
}
