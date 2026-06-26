# Idea Generator / Evaluator Skills & SaveIdeaReport Implementation Plan

**Goal:** Port the upstream `idea-generator` and `idea-evaluator` skills into `packages/agent-core` as built-in skills, add a `SaveIdeaReport` tool that persists their reports under `.ody-code/ideas/`, and auto-approve those writes through the permission system.

**Architecture:** Two new built-in skills inject their methodology as system reminders only in `normal` mode; they instruct the model to call `SaveIdeaReport` when a report is ready. The tool verifies that an idea skill is active in the recent conversation context, sanitizes the input, generates a unique dated filename under `.ody-code/ideas/`, and writes a Markdown file with YAML frontmatter. A dedicated permission policy auto-approves write accesses that land inside `.ody-code/ideas/`, while all other write paths continue through the normal policy chain.

**Tech Stack:** TypeScript, `zod` for input schemas, `js-yaml` for frontmatter, `pathe` for path math, the existing `Kaos` abstraction for I/O, `vitest` for tests.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

### New files

| File | Responsibility |
|------|----------------|
| `packages/agent-core/src/skill/builtin/idea-generator.md` | Skill body (upstream SKILL.md content minus zip wrapper). |
| `packages/agent-core/src/skill/builtin/idea-generator.ts` | Skill wrapper: parses frontmatter, exposes `IDEA_GENERATOR_SKILL`. |
| `packages/agent-core/src/skill/builtin/idea-evaluator.md` | Skill body (upstream SKILL.md content minus zip wrapper). |
| `packages/agent-core/src/skill/builtin/idea-evaluator.ts` | Skill wrapper: parses frontmatter, exposes `IDEA_EVALUATOR_SKILL`. |
| `packages/agent-core/src/tools/builtin/idea/save-idea-report.ts` | `SaveIdeaReportTool` implementation + input/output Zod schemas. |
| `packages/agent-core/src/tools/builtin/idea/save-idea-report.md` | Tool description surfaced to the model. |
| `packages/agent-core/src/agent/permission/policies/idea-tool-directory.ts` | Auto-approve writes under `.ody-code/ideas/`. |
| `packages/agent-core/test/tools/idea/save-idea-report.test.ts` | Unit tests for filename generation, validation, frontmatter, context guard, I/O. |
| `packages/agent-core/test/agent/permission/idea-tool-directory.test.ts` | Unit tests for the auto-approve policy scope. |

### Modified files

| File | Responsibility |
|------|----------------|
| `packages/agent-core/src/skill/builtin/index.ts` | Import and register the two idea skills. |
| `packages/agent-core/src/tools/builtin/index.ts` | Export `SaveIdeaReportTool`. |
| `packages/agent-core/src/tools/tool-manager.ts` | Instantiate `SaveIdeaReportTool` in `initializeBuiltinTools`. |
| `packages/agent-core/src/agent/permission/policies/index.ts` | Add `IdeaToolDirectoryApprovePermissionPolicy` to the chain. |
| `packages/agent-core/test/skill/builtin-skills.test.ts` | Extend the skill list to 15 and assert idea skill metadata. |

## Dependency Overview

The work is split into three phases. Each phase produces testable software on its own.

### Phase A — Core runtime (`2026-06-22-idea-skills/core.md`)

1. Extract reusable idea-report helpers (slug → filename, frontmatter builder, context guard, directory creation).
2. Implement `SaveIdeaReportTool` using the helpers.
3. Implement `IdeaToolDirectoryApprovePermissionPolicy`.

Phase A has no dependencies on Phase B or C; it only uses existing agent-core abstractions.

### Phase B — Skills (`2026-06-22-idea-skills/skills.md`)

4. Add `idea-generator` Markdown + wrapper.
5. Add `idea-evaluator` Markdown + wrapper.
6. Register both skills and update the builtin-skills test.

Phase B depends on nothing from Phase A (skills are pure data). It can run in parallel with Phase A.

### Phase C — Wiring & verification (`2026-06-22-idea-skills/wiring.md`)

7. Export `SaveIdeaReportTool` from `tools/builtin/index.ts` and instantiate it in `ToolManager`.
8. Register the permission policy in the policy chain.
9. Update both skill Markdown files to reference `SaveIdeaReport` in their output sections.
10. Run whole-tree typecheck, lint, tests, and a manual end-to-end check.

Phase C depends on Phase A (tool + policy exist) and Phase B (skills exist). It must run last.

## Risks & Open Questions

| # | Risk / Open Question | Mitigation in plan |
|---|----------------------|--------------------|
| 1 | Upstream `.skill` files are zip archives; extracting and porting the Markdown must preserve frontmatter/content exactly. | Phase B tasks include a verification step comparing the imported skill body length and frontmatter fields against the upstream files. |
| 2 | `SaveIdeaReport` could be called when no idea skill is active, leading to unrelated files in `.ody-code/ideas/`. | Task A2 implements a runtime context guard (`isIdeaSkillActive`) with adversarial tests. |
| 3 | Title may contain sensitive words (`key`, `token`, etc.) that leak into filenames. | Task A1 validates titles against `DEFAULT_SENSITIVE_WORDS` and rejects; tests include must-reject cases. |
| 4 | Filename collisions on repeated saves with the same title. | Task A1 implements `findUniqueStemInDir`-style suffix increment; tests assert `-1`, `-2` behavior. |
| 5 | Path traversal (`../`) could escape `.ody-code/ideas/`. | Task A3 normalizes paths before the prefix check; tests assert `.ody-code/ideas/../plans/foo.md` is not approved. |
| 6 | `hiddenInModes` must hide skills in `plan` / `design` / `office-hours` / `game-design`. | Task B6 asserts `hiddenInModes` includes all four modes; the existing registry filter is reused. |

## Spec-Coverage Table

| Design section / requirement | Covered by task(s) | Status |
|------------------------------|--------------------|--------|
| Scope In #1: add idea-generator / idea-evaluator as built-in skills | B4, B5, B6 | covered |
| Scope In #2: skills only visible in `normal` mode | B4, B5, B6 | covered |
| Scope In #3: add `SaveIdeaReport` tool | A2, C7 | covered |
| Scope In #4: auto-approve writes under `.ody-code/ideas/` | A3, C8 | covered |
| Scope In #5: filename `YYYY-MM-DD-<slug>.md`, suffix on conflict | A1, A2 | covered |
| Scope In #6: input `title`/`content`/`type`, optional `score`/`tags`, frontmatter header | A1, A2 | covered |
| Scope In #7: save failure does not block conversation | A2 | covered |
| Scope In #8: auto-create `.ody-code/ideas/` and ensure `.ody-code/` is gitignored | A1, A2, C10 | covered |
| Scope Out #1: no centralized index | — | no-op |
| Scope Out #2: no experimental flag | — | no-op |
| Scope Out #3: not auto-triggered | — | no-op |
| Scope Out #4: no cross-session state machine | — | no-op |
| Scope Out #5: preserve upstream methodology | B4, B5 | covered |
| Error handling: skill-not-active, empty title, sensitive title, invalid score, I/O errors | A2 | covered |
| Data models: `IdeaReportType`, `SaveIdeaReportInput`, output file format | A1, A2 | covered |
| Algorithms: unique filename, context guard, directory creation, body assembly | A1, A2, A3 | covered |
| Call-site integration: skill registration, tool export, ToolManager, permission chain | B6, C7, C8 | covered |
| Skill content references the tool | C9 | covered |
| Tests: builtin-skills, save-idea-report, permission policy | A2, A3, B6 | covered |

## Parts

| # | File | Scope | Status |
|---|------|-------|--------|
| 1 | `2026-06-22-idea-skills/core.md` | Helpers + `SaveIdeaReportTool` + permission policy | done |
| 2 | `2026-06-22-idea-skills/skills.md` | Skill Markdown wrappers + registration + skill tests | done |
| 3 | `2026-06-22-idea-skills/wiring.md` | Tool/policy wiring + skill content update + whole-tree verification | done |

---

## Final Self-Review

- [x] 1. **Spec-coverage table**: every design requirement is mapped to one or more tasks; no `GAP` rows remain. Scope-out items are explicitly marked `no-op`.
- [x] 2. **Placeholder scan**: no `TODO`/`TBD`/`implement later` remains in `index.md`, `core.md`, `skills.md`, or `wiring.md`. Every step contains real file paths, real code, and real commands.
- [x] 3. **No phantom tasks**: every task produces a verifiable change (new file, modified file, or passing test command). C10 uses `--allow-empty` only to record the verification state; the preceding sub-steps create real artifacts and run real checks.
- [x] 4. **Dependency soundness**: Phase A has no dependencies; Phase B has no dependencies; Phase C depends on Phase A and Phase B. Within each part, every `Depends on:` points to an earlier task in the same part. No task references a symbol defined only in a later task.
- [x] 5. **Caller & build soundness**: shared-signature changes are consolidated into single tasks (A1 refactors `ensureGitignore` and updates `SessionMode`; C7 updates the tool barrel and `ToolManager`; C8 updates the policy barrel). C10 ends with `pnpm -r typecheck` covering the whole workspace including tests.
- [x] 6. **Test-the-risk**: A1/A2 test filename collision, sensitive-word rejection with must-survive cases, and context guard behavior; A3 tests path-traversal rejection and `.ody-code/ideas/` approval; B6 tests skill `hiddenInModes`; C7 tests tool registration; C8 tests policy registration.
- [x] 7. **Type consistency**: `SaveIdeaReportTool` and `IdeaToolDirectoryApprovePermissionPolicy` names match across their definition files, re-export barrels, registration sites, and tests.
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/compaction (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/config (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/context (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/goal (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/injection (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission/policies (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/records (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/replay (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/session-mode (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/tool (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/turn (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/usage (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/rpc (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/checkpoint (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/export (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/collaboration (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/file (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/game-design (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/goal (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/office-hours (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/planning (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/shell (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/state (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/visual (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/web (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/cron/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/harness (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

