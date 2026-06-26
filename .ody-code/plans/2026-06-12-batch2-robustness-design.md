# Design Mode 鲁棒性改造 Batch 2 — Implementation Plan

**Goal:** 修复 Design Mode 三个根本问题：不完整设计被错误通过、会话导出遗漏内容、Session 恢复时 tool_call_id 断裂，通过完整性门控、事件驱动持久化和分层恢复机制实现鲁棒性。

**Architecture:** 三层改造：(1) ExitDesignModeTool 增加 7 项完整度检查作为门控，配合 System Reminder 指导 AI；(2) 新增 SessionCheckpoint 系统，在 5 个关键触发点异步保存 JSON 快照到 `.ody-code/session-state/`，支持版本回退；(3) 实时 Markdown 追加流，消息进入内存即追加到 `.ody-code/session-exports/`。恢复时优先 JSON checkpoint，失败则降级到前一版本，最终回退到 wire.jsonl 重放。

**Tech Stack:** TypeScript, vitest, Node.js `fs/promises`, 已有 `atomicWrite` (`packages/agent-core/src/utils/fs.ts`), 已有 `AgentRecords` wire.jsonl 持久化。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

```
packages/agent-core/src/
├── agent/
│   ├── session-mode/
│   │   └── index.ts                          # [Modify] add handoff artifact persistence
│   └── index.ts                               # [Modify] wire checkpoint triggers
├── tools/builtin/planning/
│   └── exit-design-mode.ts                    # [Modify] integrate completeness gate
├── injection/
│   └── design-mode.ts                         # [Modify] update reminder with completeness checklist
├── checkpoint/                                # [NEW] session checkpoint module
│   ├── completeness.ts                        # completeness checker (pure function)
│   ├── session-checkpoint.ts                  # SessionCheckpoint save/load
│   ├── checkpoint-index.ts                    # CheckpointIndex version management
│   ├── triggers.ts                            # 5 trigger-point orchestrator
│   └── recovery.ts                            # Session recovery with fallback chain
├── export/                                    # [NEW] real-time markdown export
│   └── markdown-stream.ts                     # append-only markdown stream
├── utils/
│   ├── fs.ts                                  # [Modify] add fileLock utilities
│   └── file-lock.ts                           # [NEW] flock-based file locking
└── session/
    └── index.ts                               # [Modify] integrate checkpoint on session lifecycle

packages/agent-core/test/
├── tools/
│   └── exit-design-mode.test.ts               # [Modify] add completeness test cases
├── agent/injection/
│   └── design-mode.test.ts                    # [Modify] verify completeness checklist in reminder
├── checkpoint/                                # [NEW]
│   ├── completeness.test.ts
│   ├── session-checkpoint.test.ts
│   ├── checkpoint-index.test.ts
│   ├── triggers.test.ts
│   └── recovery.test.ts
├── export/
│   └── markdown-stream.test.ts                # [NEW]
└── utils/
    └── file-lock.test.ts                      # [NEW]
```

## Dependency Overview

```
Phase A: Completeness Gate (独立，无外部依赖)
  Task A1: Design completeness checker → 纯函数模块 + 测试
  Task A2: Integrate checker into ExitDesignModeTool + System Reminder
    Depends on: A1

Phase B: Persistence Infrastructure (独立于 A，可并行)
  Task B1: File lock utilities (flock wrapper)
  Task B2: SessionCheckpoint data model + save/load
    Depends on: B1
  Task B3: CheckpointIndex version management + rotation
    Depends on: B2

Phase C: Integration & Wiring (依赖 A + B)
  Task C1: Wire 5 checkpoint triggers into agent/session lifecycle
    Depends on: B3
  Task C2: Real-time Markdown append stream
    Depends on: B1
  Task C3: Session recovery with checkpoint fallback → wire.jsonl
    Depends on: B3, C1

Phase D: Error Handling & Verification (依赖 C)
  Task D1: Error handling for all 8 error types + disk-full scenarios
    Depends on: C1, C2, C3
  Task D2: E2E verification + manual checklist
    Depends on: D1
```

**可并行执行：** Phase A 和 Phase B 互不依赖，可同时进行。Phase C 需要 A 和 B 都完成。

## Risks & Open Questions

| Risk | Mitigation |
|------|-----------|
| File lock 在 macOS 上 `flock` 行为与 Linux 不同 | 使用 `fs.open()` +  advisory lock；macOS 上 `flock` 是 whole-file advisory lock |
| Checkpoint JSON 体积随消息数增长 | 只保存最近 N 条消息的摘要 + 完整消息引用 wire.jsonl；设计文档 < 1MB 时全量保存 |
| 并发 ody 进程写同一 session | 文件锁保护；每个 session 绑定单进程（现有约束） |
| Markdown 追加性能影响 turn 延迟 | 异步写入，失败不阻塞；使用 `fs.appendFile` 批量 flush |
| Recovery Index 重建需要扫描整个 backups/ 目录 | backups/ 最多 10 个文件，扫描成本可忽略 |

---

## Spec Coverage Table

| Design Section | Task(s) | Status |
|---|---|---|
| Part 1: 系统架构与数据分层 (C1-C7 完整度检查) | A1, A2 | covered |
| Part 1: 5 checkpoint 触发点 | C1 | covered |
| Part 1: 分层存储 (Hot/Cold/Cold2) | B2, C2 | covered |
| Part 1: 恢复流程 (JSON → Markdown → wire.jsonl) | C3 | covered |
| Part 2: JSON Checkpoint 结构 | B2 | covered |
| Part 2: Recovery Index | B3 | covered |
| Part 2: Backups 目录 (10 版本) | B3 | covered |
| Part 2: Markdown 导出（仅追加） | C2 | covered |
| Part 3: ExitDesignMode 完整度检查 | A1, A2 | covered |
| Part 3: System Reminder 指导 | A2 | covered |
| Part 3: 恢复路径 (引导用户回相应 Part) | A2 | covered |
| Part 4: 实时 Markdown 追加 | C2 | covered |
| Part 4: JSON Checkpoint 异步保存 | C1 | covered |
| Part 4: Batch 2 时间线问题 | C2 | covered |
| Part 4: 完整性验证 (CheckpointIntegrity) | C3 | covered |
| Part 5: E1-E8 错误处理 | D1 | covered |
| Part 6: 单元测试 T1-T8 | A1 | covered |
| Part 6: 集成测试 I1-I5 | B2, B3, C1, D1 | covered |
| Part 6: Markdown 导出测试 M1-M4 | C2 | covered |
| Part 6: 恢复测试 R1-R5 | C3, D1 | covered |
| Part 6: E2E 测试 + 手工测试 | D2 | covered |

---

## Parts (generate one per invocation, in order)

| # | File | Scope | Status |
|---|---|---|---|
| 1 | 2026-06-12-batch2-robustness-design/completeness.md | Completeness checker + ExitDesignModeTool integration | pending |
| 2 | 2026-06-12-batch2-robustness-design/persistence.md | File lock + SessionCheckpoint + CheckpointIndex | pending |
| 3 | 2026-06-12-batch2-robustness-design/integration.md | Trigger wiring + Markdown stream + Recovery | pending |
| 4 | 2026-06-12-batch2-robustness-design/verification.md | Error handling + E2E verification | pending |
