---
"@odysseythink/agent-core": patch
---

Make E2E auto-testing actually fire in real plan→implement workflows.

Two gaps prevented E2E from triggering even after the project-aware enrichment
fix: the change detection only looked at uncommitted `git status` (so per-task
commit workflows showed nothing), and enrichment only ran at plan-exit — before
any implementation exists.

- Add `detectChangedFiles`: unions uncommitted changes with everything committed
  since the merge-base with the default branch (preferring remote-tracking refs),
  so work committed per task is still detected. Used by the plan enricher and the
  `RunE2ETests` tool.
- Broaden the plan's declared-file extraction to recognize any user-project
  language (`.go`, `.rs`, `.java`, …), not just `packages/|apps/*.ts|.py`, so a
  plan that names the files it will create enriches correctly at plan-exit even
  when the tree is clean.
- Add a post-implementation trigger (option C) in the normal-mode task
  checkpoint: when the final task completes and source files changed, nudge
  `RunE2ETests` once — even when no E2E task was injected (e.g. normal-mode work
  or plans that don't name files).
