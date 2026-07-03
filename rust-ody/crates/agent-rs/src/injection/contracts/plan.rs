/// Entry reminder when first entering plan mode (empty file).
pub const PLAN_ENTRY_REMINDER: &str = "\
Plan mode is active. This is an implementation-planning session. You MUST NOT make \
any edits except the current plan file(s) — prefer read-only tools. Goal: produce \
a plan a skilled engineer can execute task-by-task. DRY, YAGNI, TDD, frequent commits.";

/// Re-entry reminder when re-entering plan mode (file already has content).
pub const PLAN_REENTRY_REMINDER: &str = "\
Plan mode is still active. The plan file already has content. Review it and continue \
from where you left off.";

/// Full reminder (every 5 assistant turns).
pub const PLAN_FULL_REMINDER: &str = "\
Plan mode is active. You are writing an implementation plan. Remember: each task \
should be bite-sized with test-first steps. Do NOT make edits outside the plan file(s).";

/// Sparse reminder (every 2-4 assistant turns).
pub const PLAN_SPARSE_REMINDER: &str = "\
Plan mode active. Continue writing the plan. Remember: test-first, bite-sized tasks.";

/// Exit reminder when plan mode ends.
pub const PLAN_EXIT_REMINDER: &str = "\
Plan mode has ended. You are now in normal mode. All tools are available.";

/// Plan→normal handoff template when exiting plan mode with artifacts.
pub fn plan_to_normal_handoff_reminder(content: &str, path: &str) -> String {
    format!(
        "\
Here is the implementation plan that was just completed:

File: {path}

{content}

Please proceed to implement this plan task-by-task."
    )
}

/// Skills unavailable in plan mode.
pub const PLAN_UNAVAILABLE_SKILLS: &str = "\
Some skills are not available in plan mode: executing-plans, finishing-a-development-branch, \
systematic-debugging, test-driven-development, verification-before-completion, and others.";
