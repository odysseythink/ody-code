---
"@odysseythink/agent-core": patch
---

Move office-hours session-mode design documents from `.ody-code/office-hours/` to
`.ody-code/products/` (project-scoped; home fallback `~/products/`). The Phase 2.5
"related design discovery" search path is updated to match. The office-hours state
store (builder profiles / learnings under `~/.ody-code/office-hours`) is unchanged —
only the produced document directory moved. Existing docs already written under the
old directory are not migrated.
