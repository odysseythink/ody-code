# Batch 2 鲁棒性改造实施计划

**Goal:** 修复 Batch 2 设计被跳过、会话导出缺失、Session 恢复不完善的问题，建立事件驱动的实时持久化与检查点恢复机制。

**Architecture:** 在 `packages/agent-core` 中扩展 `AgentRecords` 订阅能力，新增 `SessionMarkdownExport` 实时追加消息到 Markdown，新增 `CheckpointCoordinator` 监听 5 类事件并异步保存 JSON checkpoint，配合 `SessionCheckpoint`/`CheckpointIndex` 实现版本化恢复。恢复时从最新有效 checkpoint 加载会话元数据，并与 `wire.jsonl` 回放结果做一致性校验。

**Tech Stack:** TypeScript, Node.js `fs/promises` flock, vitest, pnpm workspace.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Path | Responsibility |
|---|---|
| `packages/agent-core/src/agent/records/index.ts` | Add `AgentRecords.subscribe()` hook |
| `packages/agent-core/src/session/export/markdown-export.ts` | Real-time Markdown append writer |
| `packages/agent-core/src/agent/session-mode/index.ts` | Design session tracking |
| `packages/agent-core/src/checkpoint/coordinator.ts` | Event-driven checkpoint coordinator |
| `packages/agent-core/src/checkpoint/integrity.ts` | Checkpoint integrity verifier |
| `packages/agent-core/src/session/index.ts` | Resume integration |
| `packages/agent-core/test/agent/records.test.ts` | Subscribe hook tests |
| `packages/agent-core/test/session/export/markdown-export.test.ts` | Markdown export tests |
| `packages/agent-core/test/checkpoint/coordinator.test.ts` | Coordinator tests |
| `packages/agent-core/test/checkpoint/integrity.test.ts` | Integrity tests |
| `packages/agent-core/test/session/resume.test.ts` | Resume integration tests |

## Dependency Overview

```text
Phase A: Completeness Gate       (completeness.md)
Phase B: Persistence Infrastructure (persistence.md)
Phase C: Event-Driven Sync + Recovery (events.md)  ← current
Phase D: Error Handling + Edge Cases  (errors.md)
```

Phase C depends on B (uses `SessionCheckpoint`/`CheckpointIndex`). Phase D depends on C (uses coordinator/integrity). Within Phase C, dependencies are:

```text
E1 (AgentRecords.subscribe)
  → E2 (MarkdownExport), E3 (Design tracking), E4 (Coordinator)
E3 → E4
E4 → E5 (Integrity), E6 (Resume)
E5 → E6
```

## Risks & Open Questions

1. `AgentRecords.subscribe` must not slow down the hot path — callbacks run synchronously after persistence append; subscriber errors are caught and logged.
2. Markdown file path needs to be stable per session — derive from session creation timestamp captured in `Session` constructor.
3. Checkpoint coordinator must not block turn execution — all saves are async and best-effort; failures are logged and surfaced via telemetry.
4. Integrity verification on resume must handle wire replay after checkpoint load; the checkpoint is a hint, wire is the source of truth.
5. Design session tracking must not break existing `SessionMode` behavior — track metadata in parallel without changing public methods.

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | 2026-06-12-completeness/completeness.md | Completeness gate (C1-C7) + System Reminder | done |
| 2 | 2026-06-12-completeness/persistence.md | File lock + SessionCheckpoint + CheckpointIndex | done |
| 3 | 2026-06-12-completeness/events.md | Event-driven sync + recovery | done |
| 4 | 2026-06-12-completeness/errors.md | Error handling + edge cases | done |

## Spec-Coverage Table

| Design doc Part / requirement | Part file | Task(s) | Status |
|---|---|---|---|
| Part 3: C1-C7 completeness gate | completeness.md | A1-A2 | covered |
| Part 3: System reminder checklist | completeness.md | A2 | covered |
| Part 2: JSON checkpoint data model | persistence.md | B2 | covered |
| Part 2: Recovery index + 10-version rotation | persistence.md | B3 | covered |
| Part 4: Real-time Markdown append | events.md | C2 | covered |
| Part 4: JSON checkpoint async save (5 triggers) | events.md C4 + errors.md D5 | C4 (4 auto) + D5 (manual) | covered |
| Part 4: Batch 2 timeline fix | events.md | C2 + C4 | covered |
| Part 4: Integrity verification | events.md | C5 | covered |
| Part 4: Recovery from checkpoint | events.md | C3 + C6 | covered |
| Part 5: E1 JSON write failure | errors.md | D2 | covered |
| Part 5: E2 Corrupt checkpoint fallback | errors.md | D3 | covered |
| Part 5: E3 Markdown append failure | errors.md | D4 | covered |
| Part 5: E4 Recovery index rebuild | errors.md | D6 | covered |
| Part 5: E5 Disk full | errors.md | D2 | covered |
| Part 5: E6 Concurrent write | persistence.md | B1 | covered |
| Part 5: E7 Unrecoverable checkpoint prompt | errors.md | D3 | covered |
| Part 5: E8 Broken version chain | errors.md | D3 | covered |

## Self-Review

- [ ] 1. Spec-coverage table: every design doc requirement maps to a part file and task; no GAP.
- [ ] 2. Placeholder scan: no TODO/TBD in any part file; all code, tests, and commands are concrete.
- [ ] 3. No phantom tasks: every task creates/modifies files and ends with a verifiable test or build step.
- [ ] 4. Dependency soundness: cross-file dependencies point only to earlier parts (completeness → persistence → events → errors); no forward references.
- [ ] 5. Caller & build soundness: shared-signature changes (`SessionOptions.odyHome`, `CheckpointIndex.update`, `Agent.session`, `SessionCheckpoint.save` return type) are each updated in all callers/tests within the task that introduces them, and each such task ends with `pnpm -r typecheck`.
- [ ] 6. Test-the-risk: every state-mutating task has behavioral tests asserting the mutation; regex/filter constants are verified against must-survive inputs in completeness.md A1.
- [ ] 7. Type consistency: types defined in persistence.md (`DesignSessionCheckpoint`, `SessionCheckpointPayload`) are reused in events.md/errors.md; `ModeKey` and `AgentRecord` types are reused consistently.
