---
"@odysseythink/agent-core": patch
---

Make E2E plan enrichment work for user projects, not just ody-code itself.

`ExitPlanMode` now picks the impact model from the project's own detected
generator (Go, Python, Node/Jest, TS/Vitest) instead of always using the
ody-code builtin-tool map, so a planned change in a user project gets an
"E2E tests" task injected. The TypeScript/Vitest generator's `analyzeImpact`
now groups a user project's changed sources by package directory (like the
other language generators) and only falls back to ody-code's builtin-tool map
when the changes actually hit it — fixing the case where a user's own
TypeScript+Vitest project silently produced no E2E coverage. Projects with no
matching generator are skipped (no spurious E2E task).
