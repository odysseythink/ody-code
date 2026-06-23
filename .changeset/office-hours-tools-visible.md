---
"@odysseythink/agent-core": patch
---

Fix office-hours (and game-design) session modes ordering the model to call
tools it could not see. The mode tools (`ExitOfficeHoursMode`,
`AppendBuilderProfile`, `SetOfficeHoursLanguage`, `AppendLearning`,
`SearchLearnings`, `EnsureClaudeMdRouting`, `SyncOfficeHoursArtifact`, plus the
game-design equivalents) were registered as builtins but were never in the
`agent` profile's tool allowlist, and entering the mode did not enable them — so
the contract's "MUST call AppendBuilderProfile" / "End with ExitOfficeHoursMode"
instructions pointed at invisible tools. They are now listed in
`profile/default/agent.yaml`.

Also fix `EnterDesignMode` / `EnterPlanMode` reporting "Plan mode is already
active" when the active mode is actually office-hours, game-design, or design.
The guard now names the real active mode and its matching exit tool, so the model
stops trying to recover as if it were in plan mode (looking for a plan file,
calling ExitPlanMode).
