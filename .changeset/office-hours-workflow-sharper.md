---
"@odysseythink/agent-core": patch
---

Sharpen the office-hours workflow after observing weak runs (front-loaded detail
questions on the wrong target, startup/builder track waffling, forced Distribution
Plans on internal features, and confusion about where to write the doc):

- **Confirm the subject first**: Phase 1 now requires restating and confirming the
  exact page/file/feature before any detail question — a URL or route is treated as
  a guess until confirmed.
- **Lock the track early**: one explicit AskUserQuestion picks startup vs builder and
  routes the phases. Builder track skips the startup-demand diagnostics (Phase 2A /
  2.25) and omits the Distribution Plan for internal features instead of inventing an
  acquisition channel.
- **Question economy**: infer non-load-bearing details, tag them `[C:INFERRED]`, and
  list them under Open Questions rather than spending one-at-a-time turns on them.
- **Premise challenge with teeth**: the most load-bearing premise must resolve to a
  concrete cheapest-falsification test, not a rubber stamp.
- **Doc-emit clarity**: the contract now tells the model to Write under
  `.ody-code/products/` (host redirects to the canonical path) and to NOT call
  EnterDesignMode / EnterPlanMode while already in a writing mode.

Also add `modeModels.officeHours` and `modeModels.gameDesign` config slots so those
session modes can be pinned to their own model (e.g. a stronger reasoning model),
matching how `plan` / `design` already work.
