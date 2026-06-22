---
"@odysseythink/agent-core": patch
---

Harden the `requesting-code-review` skill to force calling the `RequestCodeReview`
tool by name and forbid spawning the reviewer via the generic `Agent`/`Task`
subagent. Only `RequestCodeReview` resolves the dedicated `[mode_models] code_review`
model; the generic path makes the reviewer inherit the parent model, silently
defeating second-model review.
