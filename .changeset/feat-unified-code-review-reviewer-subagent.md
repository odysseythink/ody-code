---
"@odysseythink/agent-core": minor
"ody-code": minor
---

Unify code review into one "roams + structured + independent model" engine.

`/request-code-review` and the `requesting-code-review` skill previously used two
different mechanisms (a deterministic diff-only RPC engine vs. an ad-hoc
general-purpose subagent). They now converge on a single engine:

- New `RequestCodeReview` builtin tool spawns a read-only `reviewer` subagent that
  reads the diff AND the surrounding codebase (callers, invariants, tests,
  duplication) and returns structured findings (Critical/Important/Minor). Added
  to the `agent` and `coder` profiles.
- Subagents can now run on an explicit model override (`spawn(..., { modelAlias })`)
  instead of always inheriting the parent's model — so review can use a dedicated
  reviewer model resolved from `[mode_models] code_review`, independent of the
  model that wrote the code.
- The `requesting-code-review` skill now routes to the tool, and the
  `/request-code-review` TUI command activates the skill (mirroring
  `/receive-code-review`). The deterministic diff-only engine remains available via
  the CLI `request-code-review` subcommand for scripting.
