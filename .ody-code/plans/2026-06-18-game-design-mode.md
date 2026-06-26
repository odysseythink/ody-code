# Game Design Mode Implementation Plan

**Goal:** Add a `game-design` session mode (alongside `office-hours`) with a CLI entry `--game-design`, 33 embedded game design skills, dedicated tools/injection/i18n, and TUI integration.

**Architecture:** Mirrors the existing `office-hours` mode across all four layers: agent-core (SessionMode + StateStore + Injector + Tools + i18n), skills (build-time embedding + registration with `hiddenInModes`), CLI (`--game-design` flag + runner + telemetry), and TUI (badge + command visibility + status). The upstream game design skill library (skill.md + 22 modules + 11 companions) is embedded at build time via the existing `raw-text-plugin` infrastructure. All state is project-scoped under `.ody-code/game-design/`.

**Tech Stack:** TypeScript, zod, pathe, commander (CLI), tsdown (build), raw-text-plugin (md imports).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Task | Create | Modify |
|------|--------|--------|
| 1 | `packages/agent-core/src/office-hours/state.ts:export GameDesignStateStore` (add to existing file) | `packages/agent-core/src/agent/session-mode/index.ts:1-771` (SessionModeKind, resolveSessionModeDirectory), `packages/agent-core/src/agent/index.ts:1-300` (ModeKey, contexts, gameDesignStateStore), `packages/agent-core/src/agent/records/types.ts:41-46` (kind type), `packages/agent-core/src/rpc/core-api.ts:168` (kind type), `packages/agent-core/src/rpc/resumed.ts:18` (kind type) |
| 2 | — | `packages/agent-core/src/office-hours/state.ts` (add GameDesignStateStore + FileSystemGameDesignStateStore + NoopGameDesignStateStore + types), `packages/agent-core/src/agent/index.ts` (wire), `packages/agent-core/src/agent/office-hours/state.ts` test |
| 3 | `packages/agent-core/src/agent/injection/game-design.ts`, `packages/agent-core/src/agent/injection/game-design-contract.ts` | `packages/agent-core/src/agent/injection/manager.ts` (register injector) |
| 4 | `packages/agent-core/src/tools/builtin/game-design/enter-game-design.ts`, `.../exit-game-design.ts`, `.../append-game-design-profile.ts`, `.../append-game-design-learning.ts`, `.../search-game-design-learnings.ts`, `.../ensure-game-design-routing.ts`, `.../sync-game-design-artifact.ts`, `.../set-game-design-language.ts` | `packages/agent-core/src/tools/builtin/index.ts` (re-export), `packages/agent-core/src/agent/tool/index.ts` (register in ToolManager) |
| 5 | — | `packages/agent-core/src/i18n/translations.ts` (add gameDesign.* keys en+zh), `packages/agent-core/src/i18n/types.ts` (type keys) |
| 6 | `packages/agent-core/src/skill/builtin/game-design-skills.ts` (generated), `packages/agent-core/src/skill/builtin/generate-game-design-skills.ts` (script) | `packages/agent-core/src/skill/builtin/index.ts` (register) |
| 7 | — | `apps/ody-code/src/cli/commands.ts:77-80` (add --game-design flag), `apps/ody-code/src/cli/options.ts:9-10` (CLIOptions.gameDesign), `apps/ody-code/src/cli/options.ts:73-92` (validateOptions) |
| 8 | `apps/ody-code/src/cli/run-game-design.ts` | `apps/ody-code/src/main.ts` (dispatch) |
| 9 | — | `apps/ody-code/src/tui/commands/types.ts` (SessionMode), `apps/ody-code/src/tui/commands/registry.ts` (SPECIAL_MODE_HIDDEN), `apps/ody-code/src/tui/ody-tui.ts` (OdyTUIStartupInput, createInitialAppState, init, getSlashCommands, syncRuntimeState, showHelpPanel) |
| 10 | — | `apps/ody-code/src/tui/components/chrome/footer.ts` (EMOJIS, renderModeBadge), `apps/ody-code/src/tui/components/messages/status-panel.ts` |

## Dependency Overview

```
Phase A: agent-core basics (Tasks 1-2) — no external deps
  Task 1: SessionModeKind + ModeKey + directory + write guard
  Task 2: GameDesignStateStore (depends on Task 1)

Phase B: injection + tools + i18n (Tasks 3-5) — depends on A
  Task 3: GameDesignInjector + contract (depends on Task 1)
  Task 4: 8 game-design tools (depends on Tasks 1, 2, 3)
  Task 5: i18n translations (depends on Task 4 for key naming)

Phase C: skills + CLI (Tasks 6-8) — depends on A for types, independent of B
  Task 6: Skills build + register (depends on Task 1 for types)
  Task 7: CLI flag + validation (independent; just needs session-mode types)
  Task 8: runGameDesign + main dispatch (depends on Tasks 1, 7)

Phase D: TUI (Tasks 9-10) — depends on A, C
  Task 9: TUI types + commands + ody-tui (depends on Tasks 1, 7, 8)
  Task 10: Footer + status panel (depends on Task 9)
```

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | 2026-06-18-game-design-mode/core.md | SessionModeKind+ModeKey+StateStore (Tasks 1-2) | done |
| 2 | 2026-06-18-game-design-mode/injection-tools.md | Injector + 8 Tools + i18n (Tasks 3-5) | done |
| 3 | 2026-06-18-game-design-mode/skills-cli.md | Skills build + CLI flag + runner (Tasks 6-8) | done |
| 4 | 2026-06-18-game-design-mode/tui.md | TUI integration (Tasks 9-10) | done |

## Risks & Open Questions

| # | Risk | Mitigation |
|---|------|------------|
| 1 | `SkillDefinition.name` may reject `/` characters | Task 6 verifies with a test that `game-design/flow-state` registers and is invocable |
| 2 | 33 .md files inflate agent-core bundle | Monitor build output; if >1MB, defer companion files to runtime loading |
| 3 | Injected skill.md content exceeds context budget | Only inject core workflow (Phase 1-8 + index), not full 286 lines; sub-modules as on-demand Skills |
| 4 | `PlanModeGuardDeny` write guard needs `game-design` awareness | Task 1 updates `isWritableSessionModePath` — verify existing tests still pass |
| 5 | Default-enable without experimental flag violates team convention | Recorded in design as user decision [C:USER]; confirm with maintainer before merge |

## Spec-Coverage Table

| Design Item | Requirement | Task(s) | Status |
|---|---|---|---|
| 1 | CLI `--game-design` entry | Task 7, Task 8 | covered |
| 2 | Session mode `game-design` alongside `office-hours` | Task 1 | covered |
| 3 | Build-time embedding of 33 skill files | Task 6 | covered |
| 4 | skill.md core workflow injected into context | Task 3 | covered |
| 5 | 22 main + 11 companion files as `game-design/*` Skills | Task 6 | covered |
| 6 | Skills hidden in all modes except `game-design` | Task 6 | covered |
| 7 | Mode runs like office-hours: restricted, exit to normal | Task 1, Task 9 | covered |
| 8 | Output: `game-design.md` + companion .md files | Task 1 (write guard), Task 3 (injection reminder) | covered |
| 9 | Tools: enter/exit, language, profile, learnings, routing, sync | Task 4 | covered |
| 10 | State persistence in `.ody-code/game-design/` | Task 2 | covered |
| 11 | UI/i18n follows existing patterns | Task 5, Task 10 | covered |
| 12 | Telemetry: `game_design_started` / `game_design_completed` | Task 8 | covered |
| 13 | Default enabled, no experimental flag | Task 7 (no flag check) | covered |
| 14 | No `--session-mode game-design` entry | Task 7 (conflicts validation) | covered |
| 15 | No plan/design handoff | Task 1 (no handoffTo support) | covered |
| 16 | Project-scoped storage only | Task 2 (cwd-based path) | covered |
| 17 | config.toml `modeModels.game-design` | no-op (reuses existing `modeModels[kind]` read) | no-op |

## Final Self-Review

- [ ] 1. Spec-coverage table: All 17 design items mapped to tasks. 16 covered, 1 no-op (config.toml model reuse).
- [ ] 2. Placeholder scan: No TODO/TBD in any of the 4 part files. All code is exact with line numbers.
- [ ] 3. No phantom tasks: Every task (1-10) produces verifiable changes — type extensions, new classes, tests, CLI flags, UI badges. Zero `--allow-empty`.
- [ ] 4. Dependency soundness: Phase A (Tasks 1-2) → Phase B (Tasks 3-5) → Phase C (Tasks 6-8) → Phase D (Tasks 9-10). No forward references. Cross-file deps use `<part>: Task N` notation.
- [ ] 5. Caller & build soundness: Task 1 changes `SessionModeKind` (shared) — updated all 4 file locations + whole-tree typecheck. Task 7 changes `CLIOptions` (shared) — `gameDesign` field with default `false` ensures existing callers compile. Task 9 changes TUI `SessionMode` — all 5 consumer files updated in Tasks 9-10. `setSessionMode` type union extended in `node-sdk/session.ts`. Every shared-signature task ends with `pnpm -r typecheck`.
- [ ] 6. Test-the-risk: Task 1 tests write guard allow/deny. Task 2 tests append/search/noop. Task 3 tests injector entry/exit/reentry. Task 4 tests all 8 tools' guard conditions. Task 6 tests hiddenInModes for all 4 non-game-design modes + visibility in game-design. Task 7 tests 7 conflict combinations. Task 9 tests command visibility. Task 5 and Task 10 have implicit tests (tools fail if i18n keys missing). Task 8 has manual verification checklist.
- [ ] 7. Type consistency: `SessionModeKind` (= `'plan' | 'design' | 'office-hours' | 'game-design'`) used consistently across agent-core. TUI `SessionMode` (= `'normal' | 'plan' | 'design' | 'office-hours' | 'game-design'`) used in commands/footer/status-panel/ody-tui. `GameDesignProfileEntry` fields match `AppendGameDesignProfileTool` input schema. `CLIOptions.gameDesign: boolean` → `OdyTUIStartupInput.gameDesign: boolean`.
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following tools:
- ExitPlanModeTool (priority: critical)

Use the RunE2ETests tool after completing the implementation tasks above.

