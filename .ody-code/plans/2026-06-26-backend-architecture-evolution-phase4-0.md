# Phase 4.0 TS↔Rust 对照测试框架 + 双后端运行时开关 实施计划

**Goal:** 在 `packages/integration-tests/src/parity/` 建立可复用的 parity 框架，移植 3 个场景完成 TS-vs-TS 自比对与 TS-vs-Rust 对照，同时把 CLI `--host=inproc` 重命名为 `--host=ts` 并引入 `ODY_BACKEND` 运行时开关，最终在 CI 新增 parity job。

**Architecture:** 通过 `BackendFactory` 统一构造 TS 内存后端（`WorkerCoreAPI` + `createRPC`）与 Rust stdio 后端（`SDKRpcClient.connect`）；`ParityDriver` 收集每个 scenario 的响应、事件与记录，经 `Normalizer` 抹平时间戳/UUID/路径等非确定性字段后，由 `AssertParity` 做结构化 diff。CLI 侧在参数解析阶段合并 `--host` 与 `ODY_BACKEND`，默认保持 TS backend 以兼容现有行为。

**Tech Stack:** TypeScript / Vitest / `packages/integration-tests`（ESM）/ `packages/agent-core` RPC / `packages/node-sdk` `SDKRpcClient` / Rust host binary (`ody-host`) / GitHub Actions。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

新文件全部落在 `packages/integration-tests/src/parity/` 与 `packages/integration-tests/test/parity/`，CLI 改动集中在 `apps/ody-code/src/cli/` 及其测试，CI 改动为 `.github/workflows/rust-host.yml`。

| Path | Responsibility |
|---|---|
| `packages/integration-tests/src/parity/types.ts` | `BackendKind`、`ParityBackend`、`Scenario`、`ScenarioSnapshot`、`NormalizedSnapshot`、`ParityDiff` 等共享类型 |
| `packages/integration-tests/src/parity/normalize.ts` | 非确定性字段归一化（UUID、时间戳、路径、pid、端口、流式分片等） |
| `packages/integration-tests/src/parity/assert-parity.ts` | 递归结构化 diff，输出 `ParityDiff` |
| `packages/integration-tests/src/parity/known-gaps.ts` / `known-gaps.md` | known-gap 解析、匹配、过期检测与登记表 |
| `packages/integration-tests/src/parity/backends.ts` | `makeTsBackend()` / `makeRustBackend()` / `createTempHome()` / `cleanupHome()` |
| `packages/integration-tests/src/parity/fixtures/mock-provider.ts` | 本地 `MockChatProvider`，支持单轮/多轮响应 |
| `packages/integration-tests/src/parity/driver.ts` | `ParityDriver.runScenario()` 执行与事件收集 |
| `packages/integration-tests/src/parity/scenarios/utils.ts` | `waitForEvent` / `waitForTurnEnded` 工具 |
| `packages/integration-tests/src/parity/scenarios/hello-world.ts` | hello-world scenario + mock LLM |
| `packages/integration-tests/src/parity/scenarios/file-edit.ts` | file-edit scenario + mock LLM |
| `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts` | multi-turn-tool scenario + mock LLM |
| `packages/integration-tests/src/parity/scenarios/index.ts` | scenario 统一导出 |
| `packages/integration-tests/src/parity/run-parity.ts` | `runParity()` / `runParityWithGaps()` |
| `packages/integration-tests/src/parity/rust-binary.ts` | Rust host 二进制路径解析 |
| `packages/integration-tests/test/parity/*.test.ts` | 各模块单元测试与 TS-vs-TS、TS-vs-Rust 测试 |
| `packages/integration-tests/test/parity/scenarios/*.test.ts` | 各 scenario 独立测试 |
| `apps/ody-code/src/cli/options.ts:19` / `:123-124` | `CLIOptions.host` 类型与校验逻辑 |
| `apps/ody-code/src/cli/commands.ts:100` / `:149` | `--host` option 与 `ODY_BACKEND` fallback |
| `apps/ody-code/test/cli/*.test.ts` / `apps/ody-code/test/tui/*.test.ts` | 所有 `host: 'inproc'` fixture 更新为 `host: 'ts'` |
| `packages/agent-core-shared/src/errors/codes.ts:450` | worker spawn 错误提示文本同步改为 `ts` |
| `packages/integration-tests/package.json` / `package.json` | parity npm scripts |
| `.github/workflows/rust-host.yml` | 新增 parity smoke 步骤 |

---

## Dependency Overview

按子系统分为 5 个 Part。Part 1–3 共同构成 parity 框架与测试；Part 4（CLI）可独立开发；Part 5（CI）依赖前面所有代码与脚本就位。

```
Part 1: Parity Framework Core
  A1 Normalizer + types
  A2 AssertParity
  A3 KnownGaps
  A4 BackendFactory.makeTsBackend
  A5 BackendFactory.makeRustBackend
  A6 ParityDriver

Part 2: Scenarios + TS-vs-TS
  B1 Scenario helpers + multi-turn MockChatProvider
  B2 hello-world scenario  ─┐
  B3 file-edit scenario     ├─► B5 TS-vs-TS parity harness
  B4 multi-turn-tool scenario ─┘

Part 3: TS-vs-Rust
  C1 Rust host binary resolver
  C2 runParityWithGaps (known-gap integration)
  C3 TS-vs-Rust parity tests

Part 4: CLI Backend Switch
  D1 Rename --host=inproc → --host=ts + ODY_BACKEND fallback + whole-tree typecheck

Part 5: CI Integration
  E1 Parity npm scripts
  E2 GitHub Actions parity smoke step
```

组内并行规则：
- Group 1：A1–A6 串行依赖（A6 依赖 A1/A4；A5 依赖 A4）。
- Group 2：B1 必须先完成；B2–B4 可并行；B5 依赖 B1–B4 与 Part 1。
- Group 3：C1 可独立；C2 依赖 Part 1 A3 与 Part 2 B5；C3 依赖 C1/C2 与 Part 2 scenarios。
- Group 4：D1 是单一原子任务，含 whole-tree typecheck。
- Group 5：E1 与 E2 串行在 Group 1–4 之后。

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| R1 | `SDKRpcClient` constructor 不接受裸 `Transport`，TS backend 需手动镜像 `createRPC` + `ClientAPI` 接线 | 参考 `node-sdk/src/core-server.ts` 与 `node-sdk/src/rpc.ts` 的非 worker 路径，用 `createRPC` + `ClientAPI` 构造可工作的内存后端 |
| R2 | Rust mock provider 与 TS `MockChatProvider` 事件顺序/字段不一致 | mock-prompt scenario 用 `known-gaps.md` 登记 L3 差异；测试要求「已登记 gap 实际通过时失败」以倒逼同步 |
| R3 | Normalizer 过度归一化掩盖真实差异 | 每条规则配 must-survive 测试；diff 失败时同时输出原始 snapshot 供人工复核 |
| R4 | `--host=inproc` 重命名破坏内部脚本/文档 | Task D2 用 `grep -rn "inproc"` 全局替换，并 whole-tree typecheck；changeset 标记 minor |
| R5 | CI parity job 因平台路径/行尾差异不稳定 | Normalizer 优先处理路径与行尾；矩阵覆盖 darwin-arm64/darwin-x64/linux-x64 |
| R6 | Rust host 二进制构建耗时导致 PR 检查过长 | CI 复用已有 `rust-host-smoke` job 的构建产物；parity job 依赖同一 artifact |

---

## Spec-Coverage Table

| 设计 § | Requirement | 覆盖 Task(s) | 状态 |
|---|---|---|---|
| In Scope #1 | `types.ts` 共享类型 | A1 | covered |
| In Scope #1 | `normalize.ts` 归一化 | A1 | covered |
| In Scope #1 | `assert-parity.ts` diff 断言 | A2 | covered |
| In Scope #1 | `known-gaps.md` / `known-gaps.ts` 登记与过期检测 | A3, C2 | covered |
| In Scope #1 | `backends.ts` TS/Rust 工厂 | A4, A5 | covered |
| In Scope #1 | `driver.ts` 场景执行器 | A6 | covered |
| In Scope #2 | 三个 scenario（hello-world / file-edit / multi-turn-tool） | B2, B3, B4 | covered |
| In Scope #2 | scenario 等待工具与多轮 mock provider | B1 | covered |
| In Scope #3 | TS-vs-TS 自比对 | B5 | covered |
| In Scope #4 | TS-vs-Rust 比对 + known gap | C1, C2, C3 | covered |
| In Scope #4 | Rust host 二进制解析 | C1 | covered |
| In Scope #5 | `ODY_BACKEND=ts\|rust` 开关 | D1 | covered |
| In Scope #5 | `--host=ts\|rust` 覆盖 env | D1 | covered |
| In Scope #5 | `--host=inproc` 重命名 `--host=ts` | D1 | covered |
| In Scope #6 | CI parity job | E2 | covered |
| In Scope #6 | parity npm scripts | E1 | covered |
| Error Handling | known-gap 过期检测 | A3, C2 | covered |
| Test Plan | Normalizer 单元测试 | A1 | covered |
| Test Plan | AssertParity 单元测试 | A2 | covered |
| Test Plan | KnownGaps 单元测试 | A3 | covered |
| Test Plan | scenario 独立测试 | B2, B3, B4 | covered |
| Test Plan | CLI 解析与 fixture 测试 | D1 | covered |

---

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-26-backend-architecture-evolution-phase4-0/core.md` | Parity 框架核心（Normalizer / AssertParity / KnownGaps / Backends / Driver） | done |
| 2 | `2026-06-26-backend-architecture-evolution-phase4-0/scenarios.md` | 三个 scenario + TS-vs-TS 自比对测试 | done |
| 3 | `2026-06-26-backend-architecture-evolution-phase4-0/rust.md` | TS-vs-Rust 对照测试 + 二进制解析 | done |
| 4 | `2026-06-26-backend-architecture-evolution-phase4-0/cli.md` | CLI `--host=ts` 重命名 + `ODY_BACKEND` + 全仓库 typecheck | done |
| 5 | `2026-06-26-backend-architecture-evolution-phase4-0/ci.md` | `package.json` scripts + GitHub Actions parity job | done |

---

## Global Self-Review

- [ ] 1. Spec-coverage table: 所有设计 § 已映射到实际 Task（A1–A6、B1–B5、C1–C3、D1、E1–E2），无 GAP。
- [ ] 2. Placeholder scan: 所有 Part 文件无 TODO/TBD/占位符；`makeRustBackend` 在 A4 中以 stub 抛出、A5 立即替换的写法已在 Part 1 本地 review 中说明。
- [ ] 3. No phantom tasks: 每个 Task 均产生可运行测试或可验证代码变更；无 `--allow-empty` 或「已在 Task N 完成」的虚假任务。
- [ ] 4. Dependency soundness: 跨 Part 依赖均为「前面 Part 的 Task」引用后面 Part 的 Task：B5 引用 `core.md` A4/A6；C2 引用 `core.md` A3 与 `scenarios.md` B5；C3 引用 C1/C2 与 Part 2；D1 独立；E1/E2 在所有功能之后。无反向依赖。
- [ ] 5. Caller & build soundness: 唯一共享签名变更是 D1 的 `CLIOptions.host`（`'inproc' | 'rust'` → `'ts' | 'rust'`）；同一任务内用 `rg -l` 找出所有 `host: 'inproc'` fixture 并替换，且以 `pnpm -r typecheck` 收尾。`known-gaps.md` 与 `run-parity.ts` 的修改由 C3 完成，未引入共享签名变更。
- [ ] 6. Test-the-risk: 状态变更类 Task 均含行为断言——A1 Normalizer 断言 UUID/路径替换与 must-survive 短文本保留；B2–B4 断言文件内容或 assistant 文本；C2 覆盖 diff+gap、无 gap、stale gap 三种状态；D1 通过 `options.test.ts` 断言默认值变更。
- [ ] 7. Type一致性: Part 2–5 复用 Part 1 定义的 `Scenario`、`ParityBackend`、`ParityDiff` 等类型；`runParity` 签名在 B5 定义，C2 以新增函数 `runParityWithGaps` 扩展而未改变原签名；`CLIOptions.host` 在所有 fixture 中同步更新。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/apps/ody-code/src/cli (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core-shared/src/errors (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

