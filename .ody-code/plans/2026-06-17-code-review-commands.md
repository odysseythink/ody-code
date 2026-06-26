# Code Review 命令化与可配置模型 Implementation Plan

**Goal:** 将内置的 `requesting-code-review` / `receiving-code-review` skill 暴露为 CLI/TUI 命令，并在 `config.toml` 的 `[mode_models]` 中增加 code-review 专用模型配置，支持 fallback 链。

**Architecture:**
- `packages/agent-core` 新增 code-review 执行器、模型解析器与类型定义，并通过 raw-text-loader 复用 `requesting-code-review.md` 的 prompt 模板。
- `packages/node-sdk` 通过 `KimiHarness.requestCodeReview()` 暴露执行器能力，使 `apps/ody-code` 无需直接依赖 `@odysseythink/agent-core`。
- `apps/ody-code` 仅保留薄胶着层：CLI 子命令 `ody request-code-review`、TUI slash `/request-code-review` 与 `/receive-code-review`。

**Tech Stack:** TypeScript, Vitest, Commander, `@odysseythink/kosong`, raw-text-loader, `gh` CLI（仅 `--pr` 路径）。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Part | 新建/修改文件 | 职责 |
|---|---|---|
| config | `packages/agent-core/src/config/schema.ts:315-320` | `modeModels` 增加 `codeReview` / `codeReviewRequest` / `codeReviewReceive` |
| config | `packages/agent-core/src/config/toml.ts:535-544` | `modeModelsToToml` 将 camelCase key 写回 snake_case |
| config | `packages/agent-core/src/code-review/model-resolver.ts` | code-review 模型 fallback 链解析器 |
| core | `packages/agent-core/src/code-review/types.ts` | `CodeReviewRequestInput` / `CodeReviewReport` / `CodeReviewFinding` |
| core | `packages/agent-core/src/code-review/diff.ts` | `fetchDiff` / `parsePrNumber` / `buildDiffSource` |
| core | `packages/agent-core/src/code-review/prompt.ts` | prompt 构造与报告解析 |
| core | `packages/agent-core/src/code-review/executor.ts` | 通用 `CodeReviewExecutor`（直接 LLM + 可选 deep runner） |
| core | `packages/agent-core/src/code-review/report.ts` | `renderCodeReviewReportToMarkdown` |
| sdk | `packages/agent-core/src/rpc/core-api.ts` | `RequestCodeReviewPayload` / `CodeReviewReportData` / CoreAPI 方法 |
| sdk | `packages/agent-core/src/rpc/core-impl.ts` | `requestCodeReview` RPC 实现 |
| sdk | `packages/node-sdk/src/types.ts` | SDK 导出 `CodeReviewRequestInput` / `CodeReviewReport` |
| sdk | `packages/node-sdk/src/rpc.ts` | SDK RPC 转发 `requestCodeReview` |
| sdk | `packages/node-sdk/src/kimi-harness.ts` | `KimiHarness.requestCodeReview()` |
| sdk | `packages/node-sdk/src/index.ts` | 导出 `renderCodeReviewReportToMarkdown` |
| cli | `apps/ody-code/src/cli/sub/request-code-review.ts` | `ody request-code-review` 子命令 |
| cli | `apps/ody-code/src/cli/commands.ts:93-94` | 注册子命令 |
| tui | `apps/ody-code/src/tui/commands/request-code-review.ts` | `/request-code-review` 处理函数 |
| tui | `apps/ody-code/src/tui/commands/receive-code-review.ts` | `/receive-code-review` 处理函数 |
| tui | `apps/ody-code/src/tui/commands/registry.ts:24-298` | 注册两个 slash 命令 |
| tui | `apps/ody-code/src/tui/commands/dispatch.ts:213-325` | switch 增加 case |
| tui | `apps/ody-code/src/tui/types.ts:15-43` | `AppState` 增加 `receiveCodeReview` 状态 |
| tui | `apps/ody-code/src/tui/ody-tui.ts:149-181, 683-706` | 初始化状态、发送普通消息前恢复模型 |

## Dependency Overview

```
Task 1 (config schema) ─┐
Task 2 (model resolver)─┤
Task 3 (diff/prompt)    │
Task 4 (executor)       ├─► Task 5 (types/RPC) ──► Task 6 (SDK harness) ──► Task 7 (CLI)
                        │                                                    │
                        └────────────────────────────────────────────────────┘
Task 6 (SDK harness) ──► Task 8 (TUI /request-code-review)
Task 1 (config) ───────► Task 9 (TUI /receive-code-review + restore)
Task 2 (resolver) ─────► Task 8 / Task 9
```

- **Phase A — Config**: Task 1 + Task 2。输出可解析的 `[mode_models]` 配置与模型 fallback 算法。
- **Phase B — Core + SDK**: Task 3 + Task 4 + Task 5 + Task 6。输出 `KimiHarness.requestCodeReview()` 可调用接口。
- **Phase C — CLI**: Task 7。输出 `ody request-code-review`。
- **Phase D — TUI**: Task 8 + Task 9。输出 `/request-code-review` 与 `/receive-code-review`。

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| 1 | `modeModelsToToml` 之前未对多词 camelCase key 做 snake_case 回写，新增字段会破坏 TOML 风格 | Task 1 同步修改 `modeModelsToToml` 并加 round-trip 测试 |
| 2 | CLI `--deep` 需要临时 session 才能派发 subagent，可能留下临时会话记录 | 在 `requestCodeReview` RPC 内部创建/关闭临时 session，不暴露给用户；如失败返回 `ok=false` |
| 3 | `/receive-code-review` 模型切换后若用户直接退出，原模型不会恢复 | 该状态仅影响当前会话；下次启动重新读取默认模型；仍尽量在普通消息发送前恢复 |
| 4 | `apps/ody-code` 直接依赖 agent-core | 通过 SDK `KimiHarness.requestCodeReview()` 封装，CLI/TUI 只导入 `@odysseythink/ody-code-sdk` |

## Spec-Coverage Table

| 设计需求 | 覆盖任务 | 状态 |
|---|---|---|
| `modeModels` 增加 codeReview / codeReviewRequest / codeReviewReceive | Task 1 | covered |
| 模型 fallback 链（显式参数 → 专用 → 通用 → review → 当前会话模型 → default） | Task 2 | covered |
| diff 来源：`--base/--head`、`--pr`、`working-tree` | Task 3, 7, 8 | covered |
| 直接 LLM 生成审查报告 | Task 4, 6 | covered |
| `--deep` subagent 深度审查 | Task 4, 6 | covered |
| `ody request-code-review` CLI 子命令 | Task 7 | covered |
| `/request-code-review` slash 命令 | Task 8 | covered |
| `/receive-code-review` 临时切换模型并注入 skill | Task 9 | covered |
| 发送下一条普通消息后恢复模型 | Task 9 | covered |
| 在 plan/design/office-hours 模式隐藏新 slash 命令 | Task 8, 9 | covered |
| 复用现有 telemetry 埋点 | 无新增事件 | no-op |

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-17-code-review-commands/config.md` | Config schema + model resolver | done |
| 2 | `2026-06-17-code-review-commands/core.md` | Diff/prompt/executor/report | done |
| 3 | `2026-06-17-code-review-commands/sdk.md` | RPC + SDK harness 封装 | done |
| 4 | `2026-06-17-code-review-commands/cli.md` | `ody request-code-review` | done |
| 5 | `2026-06-17-code-review-commands/tui.md` | `/request-code-review` + `/receive-code-review` | done |

## Global Self-Review

- [ ] 1. **Spec-coverage table**: 上方表格完整映射了设计文档中所有 11 项需求到具体 Task，无 GAP。唯一 `no-op` 是 telemetry（复用现有埋点无需新增事件）。✅
- [ ] 2. **Placeholder scan**: 全 5 个 Part 文件中无 `TODO`/`TBD`/"implement later"/"write tests for the above"。所有步骤含完整可执行代码与确切命令。✅
- [ ] 3. **No phantom tasks**: 每个 Task 产出明确的可编译/可测试变更（9 个任务，9 次 commit）。无 `--allow-empty`。✅
- [ ] 4. **Dependency soundness**: 
  - Task 1 → Task 2 依赖 schema 类型定义 ✅
  - Task 3 ∥ Task 4（core 纯函数与执行器）→ Task 5（RPC 类型+handler）→ Task 6（SDK 封装）→ Task 7（CLI）/ Task 8（TUI request）✅
  - Task 1/2 → Task 9（TUI receive 使用 resolveCodeReviewModel）✅
  - 无循环依赖，无后置引用。✅
- [ ] 5. **Caller & build soundness**:
  - `modeModelsToToml` 签名未变，内部实现改为 `camelToSnake(key)`，调用方不受影响。✅
  - `KimiConfigSchema` / `KimiConfigPatchSchema` `modeModels` 增加字段是超集（`.optional()`），`parseConfigString` / `mergeConfigPatch` 无需改动。✅
  - `CoreAPI` 新增 `requestCodeReview` 方法→`KimiCore` 实现→`SDKRpcClient` 转发→`KimiHarness` 暴露，全链路类型一致。✅
  - `AppState` 新增可选字段 `receiveCodeReview`，`Object.assign` 不删除已有字段。✅
  - `BUILTIN_SLASH_COMMANDS` 数组追加新命令，类型推断自动扩展 `BuiltinSlashCommandName`。✅
  - 每个 Part 末尾均要求 `pnpm -r typecheck` + `pnpm test`（全 workspace）。✅
- [ ] 6. **Test-the-risk**:
  - Config round-trip 验证 snake_case 写回。✅
  - 模型 fallback 链：显式 override → 专用 → 通用 → review → sessionModel → default → 全部无效抛错，含 invalid 跳过测试。✅
  - `parsePrNumber`：合法 URL / 纯数字 / 非 GitHub URL / 不完整 URL 均覆盖。✅
  - Executor：diff 失败、token 超限、LLM 成功、deep true 有/无 runner 四种路径。✅
  - CLI：参数冲突所有组合 + 默认行为断言。✅
  - TUI handler：无 session、正常调用、report 失败、模型恢复 active/noop。✅
- [ ] 7. **Type consistency**: 跨文件类型（`CodeReviewDiffSource` → `RequestCodeReviewPayload.source` → `CodeReviewRequestInput.source` → `buildDiffSource` 返回值）在 5 个 Part 间保持严格一致；`CodeReviewReport` → `CodeReviewReportData` 字段一一对应；`resolveCodeReviewModel` 的 `validate: (alias: string) => boolean` 回调在 core-impl 和 TUI handler 中一致实现。✅
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following tools:
- ExitPlanModeTool (priority: critical)

Use the RunE2ETests tool after completing the implementation tasks above.

