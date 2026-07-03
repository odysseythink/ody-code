# 4.3.0 Records & persistence foundation Implementation Plan

**Goal:** 在 Rust 中建立 `agent-rs` crate，完整复刻 `packages/agent-core/src/agent/records` 的 WAL（write-ahead log）记录格式与持久化语义，使 TS 与 Rust 能够交叉读写同一份 records 文件，为 4.3.1–4.3.9 的 Agent 子系统迁移提供不可变基础。

**Architecture:** 新增 `rust-ody/crates/agent-rs`，内部以 `records` 模块为中心：顶层 `AgentRecord` 是一个由 `type` 字段 tag 的 serde 枚举，完整镜像 TS `AgentRecordEvents` 中约 30 种事件类型；嵌套类型（`ContextMessage`、`LoopRecordedEvent`、`PromptOrigin`、权限/配置/目标/用量等）在同一模块定义，并复用 `kosong-rs` 的 `Message`、`ContentPart`、`TokenUsage`。持久化层提供 `InMemoryAgentRecordPersistence` 与 `FileSystemAgentRecordPersistence`，后者按 JSONL 行格式追加写入，支持 truncate 尾行容错与目录 fsync。`BlobStore` 负责将大 data URI 卸载为 `blobref:<mime>;<sha256>` 并在读取时水化。`AgentRecords` 统一 `logRecord`、`replay`、`restore` 生命周期，订阅者模式与 metadata 首行语义与 TS 一致。

**Tech Stack:** Rust 2021 edition, `serde` + `serde_json`, `tokio::fs`, `sha2` + `base64`, `thiserror`; 依赖 `kosong-rs` 复用消息/用量类型；TS 侧用 vitest 构造同构 fixture 做 L4 交叉读写对照。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/Cargo.toml` | 加入 `crates/agent-rs` workspace member |
| `rust-ody/crates/agent-rs/Cargo.toml` | crate 元数据 + 依赖 (`kosong-rs`, `serde`, `serde_json`, `tokio`, `sha2`, `base64`, `thiserror`, `anyhow`, `async-stream`, `indexmap`, `lazy_static`) |
| `rust-ody/crates/agent-rs/src/lib.rs` | crate 根，导出 `records` 模块与公开类型 |
| `rust-ody/crates/agent-rs/src/records/mod.rs` | `records` 模块入口，组合 types / persistence / blobstore / migration / records |
| `rust-ody/crates/agent-rs/src/records/types.rs` | `AgentRecord`、`AgentRecordEvents` 枚举 + `AgentRecordPersistence` trait + 辅助方法 |
| `rust-ody/crates/agent-rs/src/records/nested.rs` | `ContextMessage`、`PromptOrigin`、`LoopRecordedEvent` 等嵌套类型 |
| `rust-ody/crates/agent-rs/src/records/persistence.rs` | `InMemoryAgentRecordPersistence` + `FileSystemAgentRecordPersistence` + `RecordBlobStore` trait |
| `rust-ody/crates/agent-rs/src/records/blobstore.rs` | `BlobStore`：data URI 卸载 / 水化 / LRU / 去重 |
| `rust-ody/crates/agent-rs/src/records/migration.rs` | wire 版本比较 + `v1.0→v1.1→v1.2→v1.3` 迁移链 |
| `rust-ody/crates/agent-rs/src/records/records.rs` | `AgentRecords`（log / replay / restore / subscribe / flush） |
| `rust-ody/crates/agent-rs/src/bin/generate_fixtures.rs` | 生成 Rust-written fixtures 的 dev binary |
| `rust-ody/crates/agent-rs/tests/filesystem_persistence.rs` | 文件系统持久化 JSONL 测试 |
| `rust-ody/crates/agent-rs/tests/blob_offload.rs` | BlobStore offload 集成测试 |
| `rust-ody/crates/agent-rs/tests/blobstore_persistence_integration.rs` | `BlobStore` + `FileSystemAgentRecordPersistence` 集成测试 |
| `rust-ody/crates/agent-rs/tests/agent_records_replay.rs` | `AgentRecords::replay` 迁移与 rewrite 测试 |
| `rust-ody/crates/agent-rs/tests/fixture_parity.rs` | Rust 读取 TS/v1.0 fixture 的 L4 测试 |
| `rust-ody/crates/agent-rs/tests/fixtures/ts-written/` | TS 生成的 fixture（含 blobref） |
| `rust-ody/crates/agent-rs/tests/fixtures/v1.0/records.jsonl` | v1.0 旧格式手写 fixture |
| `rust-ody/crates/agent-rs/tests/fixtures/rust-written/` | Rust 生成的 fixture（TS 反向读取） |
| `scripts/generate-record-fixtures.ts` | 用 TS 实现生成 `ts-written` fixture 的脚本 |
| `packages/agent-core/src/agent/records/records.parity.test.ts` | TS 读取 Rust-written fixture 的 vitest 测试 |

---

## Dependency Overview

```text
[schema.md Task 1: crate scaffold + design-lite decision]
        │
        ▼
[schema.md Task 2: AgentRecordEvents enum + AgentRecordPersistence trait]
        │
        ▼
[schema.md Task 3: nested types]
        │
        ├──▶ [persistence.md Task 1: InMemoryAgentRecordPersistence]
        │         │
        │         ▼
        │    [persistence.md Task 2: FileSystemAgentRecordPersistence]
        │         │
        │         ▼
        │    [persistence.md Task 3: RecordBlobStore offload interface]
        │         │
        │         ▼
        ├──▶ [blobstore.md Task 1/2/3: BlobStore implementation + trait impl]
        │
        ├──▶ [migration.md Task 1/2/3: wire migration chain]
        │
        ▼
[records.md Task 1: persistence raw-read extension]
        │
        ▼
[records.md Task 2: AgentRecords core]
        │
        ▼
[records.md Task 3: AgentRecords replay]
        │
        ▼
[parity.md Task 1/2/3: L4 fixtures + cross-read tests]
```

- **可并行任务**：`persistence.md` / `blobstore.md` / `migration.md` 都只在 `schema.md` 完成后才可开始，但彼此独立，可并行开发（建议仍按 persistence → blobstore → migration 顺序合入，因为 FileSystem 是主路径）。
- **共享签名变更**：
  - `rust-ody/Cargo.toml` workspace members 列表（`schema.md` Task 1）。
  - `AgentRecordPersistence` trait 在 `records.md` Task 1 增加 `read_raw` 方法，需同步更新 `InMemoryAgentRecordPersistence` 与 `FileSystemAgentRecordPersistence`。
- **硬前置**：`records.md` 依赖 persistence + migration；`parity.md` 依赖 records + blobstore。无 TS 共享签名变更。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `AgentRecordEvents` 中大量嵌套类型导致 serde 枚举定义冗长且易错 | 每个变体都写 round-trip JSON 测试；L4 fixture 覆盖每个事件类型 |
| 时间戳字段在 TS 为 `number`（ms），Rust 中若用 `DateTime` 会引入 serde 格式差异 | 统一用 `i64` milliseconds；写入时与 TS `Date.now()` 一致 |
| `FileSystemAgentRecordPersistence` 的 rewrite 语义（append 转 truncate）与 TS 不完全一致 | 严格复刻 TS：rewrite 时先清 `should_clear` 标志，下一次 flush 以 `w` 模式打开文件 |
| `BlobStore` 的 LRU 缓存与去重需与 TS 逐字节一致 | L4 fixture 含大图片 data URI，交叉验证 blobref 格式与水化结果 |
| Windows 目录 fsync 语义差异 | 与 TS 一致：Windows 上 `sync_dir` 为 no-op |
| v1.2→v1.3 是 bump-only，但 blobref 可能在 v1.3 记录中出现 | 迁移代码透传，blobref 解析由 `BlobStore.rehydrate` 在 replay 末尾统一处理 |

**已做 design-lite 决策（详情见 Part 1 Task 1）：**
- 采用 **候选方案 A**：完全镜像 TS 的 JSONL wire 格式与字段命名，不做任何跨语言重命名或二进制化。原因是 4.3.0 的唯一正确性标准是 TS↔Rust 交叉读写逐字节/逐值一致，任何格式转换都会引入不可逆的兼容风险。

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-28-backend-architecture-evolution-phase4-3-0/schema.md` | 设计决策 + `AgentRecord`/`AgentRecordEvents` 枚举 + 嵌套类型 | done |
| 2 | `2026-06-28-backend-architecture-evolution-phase4-3-0/persistence.md` | `InMemoryAgentRecordPersistence` + `FileSystemAgentRecordPersistence` | done |
| 3 | `2026-06-28-backend-architecture-evolution-phase4-3-0/blobstore.md` | `BlobStore`：data URI 卸载 / 水化 / LRU / 去重 | done |
| 4 | `2026-06-28-backend-architecture-evolution-phase4-3-0/migration.md` | wire 版本比较 + `v1.0→v1.1→v1.2→v1.3` 迁移链 | done |
| 5 | `2026-06-28-backend-architecture-evolution-phase4-3-0/records.md` | `AgentRecords`（log / replay / restore / subscribe / flush） | done |
| 6 | `2026-06-28-backend-architecture-evolution-phase4-3-0/parity.md` | L4 golden fixtures + TS-Rust 交叉读写 vitest 测试 | done |

---

## Spec-Coverage Table

| Roadmap 4.3.0 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.3.0.1 新建 `agent-rs` crate 并加入 workspace | `schema.md` Task 1 | covered |
| 4.3.0.2 迁移 `AgentRecord` 类型与 `AgentRecordEvents` schema | `schema.md` Task 2, Task 3 | covered |
| 4.3.0.3 实现 `InMemoryAgentRecordPersistence` + `FileSystemAgentRecordPersistence` | `persistence.md` Task 1, Task 2 | covered |
| 4.3.0.4 实现 `BlobStore` | `blobstore.md` Task 1, Task 2 | covered |
| 4.3.0.5 实现 wire migration (v1.1/v1.2/v1.3) | `migration.md` Task 1, Task 2, Task 3 | covered |
| 4.3.0.6 实现 `AgentRecords` (log/replay/restore) | `records.md` Task 2, Task 3 | covered |
| 4.3.0.7 L4 records 互读 fixture | `parity.md` Task 1, Task 2, Task 3 | covered |

---

## Global Self-Review

- [ ] 1. Spec-coverage：上表覆盖 Roadmap 4.3.0 全部 7 个条目，无 GAP。
- [ ] 2. Placeholder scan：所有 part 文件均无 TODO/TBD；`records.md` 中的 `with_time` 已替换为完整变体枚举，无 `_ => self` fallback。
- [ ] 3. No phantom tasks：每个 task 都产生可验证的代码/测试/fixture 变更；无 `--allow-empty` 或 "already done" 类型任务。
- [ ] 4. Dependency soundness：跨 part 依赖均从早到晚：`persistence.md` 依赖 `schema.md`；`blobstore.md` 依赖 `persistence.md`；`migration.md` 依赖 `schema.md`；`records.md` 依赖 `persistence.md` + `migration.md`；`parity.md` 依赖 `records.md` + `blobstore.md`。无反向依赖。
- [ ] 5. Caller & build soundness：
  - `schema.md` Task 1 修改 `rust-ody/Cargo.toml` workspace members，仅影响构建，无其他调用方。
  - `records.md` Task 1 修改 `AgentRecordPersistence` trait（新增 `read_raw`），同任务内更新 `InMemoryAgentRecordPersistence` 与 `FileSystemAgentRecordPersistence`，并以 `cargo check -p agent-rs --workspace --tests` 验证。
  - 无 TS 共享签名变更。
- [ ] 6. Test-the-risk：每个状态变更任务都附带行为断言——`FileSystemAgentRecordPersistence` 重写后文件为空、`BlobStore` 去重后 blob 文件唯一、`AgentRecords` 非 metadata 记录前自动补 metadata、`AgentRecords::replay` 对 v1.0 fixture 迁移并 rewrite、L4 fixture 交叉验证 blobref 与水化。
- [ ] 7. Type consistency：`AgentRecord`、`AgentRecordPersistence`、`RecordBlobStore`、`WireMigration`、`AgentRecords` 等类型/签名在各 part 中保持一致；`time` 统一为 `Option<i64>`，与 TS `time?: number` 对应；`protocol_version` 常量统一为 `"1.3"`。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/scenarios (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

