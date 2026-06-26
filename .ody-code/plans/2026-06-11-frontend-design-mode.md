# Frontend-Design Mode Implementation Plan

**Goal:** Add a new `frontend-design` session mode that equips the agent with a frontend-design skill, generates a DESIGN.md document, and writes runnable frontend code files, with dedicated entry via `/frontend-design` and handoff support from design mode.

**Architecture:** The new mode plugs into the existing three-mode (`normal`/`plan`/`design`) partition system. `SessionModeKind` is extended to `'frontend-design'`; the Agent gains a fourth context partition; a new `FrontendDesignInjector` parallels `PlanModeInjector`/`DesignModeInjector`; permission policies are refactored from a binary `plan|design` guard into a mode-aware guard that allows writes and commands in `frontend-design` mode; a minimal appendix-selector ranks skill appendices by trigger-signal matching; TUI gets a `/frontend-design` slash command and footer badge.

**Tech Stack:** TypeScript, pnpm monorepo (`packages/agent-core`, `apps/ody-code`), vitest, zod, pi-tui.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use `- [ ]` checkboxes for tracking.

---

## File Structure

| # | File | Responsibility |
|---|---|---|
| 1 | `packages/agent-core/src/agent/session-mode/index.ts` | Extend `SessionModeKind`, `resolveSessionModeDirectory`, `handoffTo`, `isWritableSessionModePath` |
| 2 | `packages/agent-core/src/agent/index.ts` | Extend `ModeKey`, add `frontend-design` context partition |
| 3 | `packages/agent-core/src/config/schema.ts` | Add `'frontend-design'` to `modeModels` schema |
| 4 | `packages/agent-core/src/agent/permission/policies/session-mode-guard.ts` | **NEW** — refactor `PlanModeGuardDenyPermissionPolicy` into mode-aware guard |
| 5 | `packages/agent-core/src/agent/permission/policies/exit-plan-mode-review-ask.ts` | Handle `ExitFrontendDesignMode` in exit review |
| 6 | `packages/agent-core/src/agent/permission/policies/plan-mode-tool-approve.ts` | Approve `EnterFrontendDesignMode` / `ExitFrontendDesignMode` |
| 7 | `packages/agent-core/src/tools/builtin/planning/exit-frontend-design-mode.ts` | **NEW** — exit tool for frontend-design mode |
| 8 | `packages/agent-core/src/agent/injection/plan-mode.ts` | Fix `kind !== 'design'` → `kind === 'plan'` |
| 9 | `packages/agent-core/src/agent/injection/frontend-design-mode.ts` | **NEW** — `FrontendDesignInjector` |
| 10 | `packages/agent-core/src/agent/injection/frontend-design-mode-contract.ts` | **NEW** — reminder text helpers |
| 11 | `packages/agent-core/src/agent/injection/manager.ts` | Register `FrontendDesignInjector` |
| 12 | `packages/agent-core/src/agent/frontend-design/appendix-selector.ts` | **NEW** — trigger-signal matcher |
| 13 | `packages/agent-core/src/skill/builtin/frontend-design.ts` | **NEW** — skill registration stub |
| 14 | `packages/agent-core/src/skill/builtin/index.ts` | Register frontend-design skill |
| 15 | `apps/ody-code/src/tui/commands/types.ts` | Extend `SessionMode` type |
| 16 | `apps/ody-code/src/tui/commands/registry.ts` | Add `/frontend-design` command |
| 17 | `apps/ody-code/src/tui/commands/dispatch.ts` | Wire command handler |
| 18 | `apps/ody-code/src/tui/commands/config.ts` | Add `handleFrontendDesignCommand` |
| 19 | `apps/ody-code/src/tui/components/chrome/footer.ts` | Add `frontend-design` badge rendering |
| 20 | `packages/agent-core/src/tools/builtin/planning/enter-frontend-design-mode.ts` | **NEW** — entry tool |
| 21 | `packages/agent-core/src/tools/builtin/index.ts` | Re-export new tools |
| 22 | `packages/agent-core/src/agent/tool/index.ts` | Register new built-in tools |
| 23 | `packages/agent-core/src/skill/registry.ts` | Extend `listInvocableSkills` / `getUnavailableSkillsReminder` signatures |
| 24 | `packages/agent-core/src/rpc/events.ts` | Extend `AgentStatusUpdatedEvent.sessionMode` |

---

## Dependency Overview

```
Phase A: Core Types & Models (core.md)
  ├── SessionModeKind + ModeKey + AppState.sessionMode
  ├── Agent context partitions
  ├── resolveSessionModeDirectory + isWritableSessionModePath
  ├── handoffTo extension
  └── modeModels config schema

Phase B: Permission & Policies (permission.md) ── depends on Phase A
  ├── Refactor PlanModeGuardDeny → SessionModeGuardPermissionPolicy
  ├── Update ExitPlanModeReviewAskPermissionPolicy
  ├── Update PlanModeToolApprovePermissionPolicy
  └── Create ExitFrontendDesignModeTool

Phase C: Injection System (injection.md) ── depends on Phase A
  ├── Fix PlanModeInjector plan-mode detection
  ├── Create FrontendDesignInjector + contract
  └── Register in InjectionManager

Phase D: Skill & Appendix System (skill.md) ── depends on Phase A
  ├── Create AppendixSelector
  ├── Create FrontendDesignSkill registration
  └── Extend SkillRegistry signatures

Phase E: TUI & Commands (tui.md) ── depends on Phase A–D
  ├── Add /frontend-design slash command
  ├── Create EnterFrontendDesignModeTool
  ├── Update TUI footer badge
  └── TUI trigger detection (design → frontend-design handoff)
```

**Parallelism:** Phases B, C, and D are independent of each other after Phase A completes. Phase E must wait for all prior phases.

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| 1 | `SessionModeKind` expansion is a shared signature; any missed caller causes a compile error. | Task in Phase A ends with a full-workspace `pnpm -r typecheck`. |
| 2 | `PlanModeInjector` currently uses `kind !== 'design'`; adding a third mode would misclassify it as plan mode. | Task in Phase C explicitly fixes this to `kind === 'plan'`. |
| 3 | Permission policy refactor touches the security boundary. | New policy keeps `plan`/`design` behavior identical; only `frontend-design` gets additional permissions. Other policies (`CwdGuard`, `SensitiveFile`, `BashPermission`) remain in effect. |
| 4 | 1231-line skill injection may approach token limits. | The skill content is loaded via the same `DynamicInjector` cadence as plan/design mode; if tokens are tight, the model can use the PAUSED mechanism. No change to injector architecture. |
| 5 | Upstream skill files (appendices) may not be present at build time. | Phase D registers a stub skill with inline content; appendix files are loaded at runtime from a well-known path or gracefully skipped. |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-11-frontend-design-mode/core.md` | SessionModeKind, ModeKey, Agent partitions, directory/handoff/config extensions, tests | done |
| 2 | `2026-06-11-frontend-design-mode/permission.md` | Permission policy refactor, exit review, tool approve, ExitFrontendDesignModeTool, tests | done |
| 3 | `2026-06-11-frontend-design-mode/injection.md` | PlanModeInjector fix, FrontendDesignInjector + contract, InjectionManager registration, tests | done |
| 4 | `2026-06-11-frontend-design-mode/skill.md` | AppendixSelector, FrontendDesignSkill registration, SkillRegistry signature updates, tests | done |
| 5 | `2026-06-11-frontend-design-mode/tui.md` | Slash command, EnterFrontendDesignModeTool, footer, command resolution, trigger detection, tests | done |
