---
name: requesting-code-review
description: Use when completing tasks, implementing major features, or before merging to verify work meets requirements
namespace: core
upstream: superpowers@v5.1.0
---

# Requesting Code Review

Get an independent review to catch issues before they cascade. In ody-code this is
a single tool — **`RequestCodeReview`** — that runs a read-only reviewer subagent
on a dedicated reviewer model (when configured). The reviewer reads the diff AND
the surrounding codebase (callers, invariants, tests, duplication), so it catches
the high-value issues that live *outside* the changed lines, and returns
structured findings (Critical / Important / Minor) with locations and fixes.

**Core principle:** Review early, review often.

## When to Request Review

**Mandatory:**
- After each task in subagent-driven development
- After completing a major feature
- Before merge to main

**Optional but valuable:**
- When stuck (fresh perspective)
- After fixing a complex bug

## How to Request

Prefer the **`/request-code-review`** slash command (or `/requesting-code-review`).
It invokes the same review engine directly and works regardless of whether the
`RequestCodeReview` tool is exposed in the current agent profile.

If the **`RequestCodeReview`** tool **is** available in your tool list, you may also
call it by name. It handles everything — choosing the reviewer model, fetching the
diff, spawning the read-only reviewer subagent, and returning structured findings.
You do not need to dispatch a subagent or craft a review prompt yourself.

> **Do NOT spawn the reviewer yourself.** Do not use the generic `Agent` / `Task`
> tool with `subagent_type: reviewer` (or any hand-written review prompt) to run
> the review. Only the dedicated review path resolves and applies the
> code-review model (`[mode_models] code_review`); the generic `Agent` path makes
> the reviewer inherit *your* model, silently defeating second-model review. If you
> find yourself writing a review prompt, stop and use `/request-code-review` or
> call `RequestCodeReview` instead.

Parameters (all optional):
- `description` — short summary of what you built.
- `requirements` — what the change is supposed to do (the plan/spec).
- `model` — override the reviewer model alias (defaults to the configured
  `[mode_models] code_review` model, else the default model).
- `base` + `head` — review a commit range (e.g. `origin/main`..`HEAD`). Omit both
  to review the working tree.
- `pr` — a GitHub PR URL/number to review instead of local changes.

Example: `RequestCodeReview(description: "add /preferences endpoint", requirements: "GET returns the saved prefs as JSON")`.

## Act on Feedback

- Fix **Critical** issues immediately.
- Fix **Important** issues before proceeding.
- Note **Minor** issues for later.
- Push back if the reviewer is wrong — with technical reasoning, showing the
  code/tests that prove it.

## Red Flags

**Never:**
- Skip review because "it's simple"
- Spawn the reviewer via the generic `Agent` / `Task` tool — always call `RequestCodeReview`
- Ignore Critical issues
- Proceed with unfixed Important issues
- Argue with valid technical feedback
