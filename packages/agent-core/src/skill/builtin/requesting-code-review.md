---
name: requesting-code-review
description: Use when completing tasks, implementing major features, or before merging to verify work meets requirements
namespace: core
upstream: superpowers@v5.1.0
---

# Requesting Code Review

Dispatch a code reviewer subagent to catch issues before they cascade. The reviewer gets precisely crafted context for evaluation — never your session's history. This keeps the reviewer focused on the work product, not your thought process, and preserves your own context for continued work.

**Core principle:** Review early, review often.

## When to Request Review

**Mandatory:**
- After each task in subagent-driven development
- After completing major feature
- Before merge to main

**Optional but valuable:**
- When stuck (fresh perspective)
- Before refactoring (baseline check)
- After fixing complex bug

## How to Request

**1. Get git SHAs:**
```bash
BASE_SHA=$(git rev-parse HEAD~1)  # or origin/main
HEAD_SHA=$(git rev-parse HEAD)
```

**2. Dispatch code reviewer subagent:**

Use Task tool with `general-purpose` type. Use the following prompt, replacing placeholders with actual values:

```
You are a code reviewer. Review the changes from {BASE_SHA} to {HEAD_SHA}.

## Context

**What was built:** {DESCRIPTION}

**Requirements:** {PLAN_OR_REQUIREMENTS}

## Your Task

1. Run `git diff {BASE_SHA} {HEAD_SHA}` to see all changes
2. Review the code against the requirements
3. Evaluate:
   - **Strengths:** What's done well
   - **Issues:**
     - Critical: Bugs, security issues, broken functionality, missing error handling
     - Important: Edge cases not covered, test gaps, unclear logic, API design problems
     - Minor: Style issues, naming, documentation gaps, magic numbers
4. **Assessment:** Ready to proceed / Needs fixes

## Output Format

```
Strengths:
- [strength 1]

Issues:
Critical:
- [issue 1] (if any)

Important:
- [issue 1] (if any)

Minor:
- [issue 1] (if any)

Assessment: [Ready to proceed / Needs fixes]
```
```

**Placeholders:**
- `{DESCRIPTION}` - Brief summary of what you built
- `{PLAN_OR_REQUIREMENTS}` - What it should do
- `{BASE_SHA}` - Starting commit
- `{HEAD_SHA}` - Ending commit

**3. Act on feedback:**
- Fix Critical issues immediately
- Fix Important issues before proceeding
- Note Minor issues for later
- Push back if reviewer is wrong (with reasoning)

## Example

```
[Just completed Task 2: Add verification function]

You: Let me request code review before proceeding.

BASE_SHA=$(git log --oneline | grep "Task 1" | head -1 | awk '{print $1}')
HEAD_SHA=$(git rev-parse HEAD)

[Dispatch code reviewer subagent]
  DESCRIPTION: Added verifyIndex() and repairIndex() with 4 issue types
  PLAN_OR_REQUIREMENTS: Task 2 from $(gpowers-path project)/plans/deployment-plan.md
  BASE_SHA: a7981ec
  HEAD_SHA: 3df7661

[Subagent returns]:
  Strengths: Clean architecture, real tests
  Issues:
    Important: Missing progress indicators
    Minor: Magic number (100) for reporting interval
  Assessment: Ready to proceed

You: [Fix progress indicators]
[Continue to Task 3]
```

## Integration with Workflows

**Subagent-Driven Development:**
- Review after EACH task
- Catch issues before they compound
- Fix before moving to next task

**Executing Plans:**
- Review after each task or at natural checkpoints
- Get feedback, apply, continue

**Ad-Hoc Development:**
- Review before merge
- Review when stuck

## Red Flags

**Never:**
- Skip review because "it's simple"
- Ignore Critical issues
- Proceed with unfixed Important issues
- Argue with valid technical feedback

**If reviewer wrong:**
- Push back with technical reasoning
- Show code/tests that prove it works
- Request clarification

Use the prompt template in the "How to Request" section above.
