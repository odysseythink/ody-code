# /request-code-review TUI 进度提示改进

## Scope

### In Scope [C:USER]

- 仅针对 TUI 的 `/request-code-review` slash 命令：把当前的静态文案 `Running code review on <model>…` 替换为可实时更新的 spinner 进度提示。
- 通过新增 `codeReview.progress` 事件，把 agent-core 代码审查执行器的真实阶段反馈到 TUI：
  - `preparing`：参数解析、模型解析完成
  - `fetching-diff`：获取 diff / PR diff
  - `audit-scanning`：`--scope repo` 时的仓库扫描
  - `deep-review`：`--deep` 时的深度审查（子 agent 派发/完成事件预留）
  - `generating`：调用 LLM 生成审查报告
  - `completed` / `failed`：审查结束
- TUI spinner 标签同时显示当前阶段与已运行秒数。
- 支持用户通过 Esc/Ctrl-C 取消正在进行的代码审查，复用现有的 `host.cancelInFlight` 机制。
- 进度文案允许显示文件路径等定位信息，但不显示代码片段或 diff 内容 [C:USER]。

### Out of Scope [C:USER]

- CLI 子命令 `ody request-code-review` 不展示实时进度：它是非交互式输出，且本次反馈仅针对 TUI 体验。
- 不为本次改动新增实验性 feature flag：直接随命令发布上线。
- 不新增 telemetry 事件：仅在 debug 日志中记录阶段切换。
- 不实现真正的百分比进度条：代码审查的阶段耗时不可预估，采用阶段文案 + 耗时计时器。
- `deep-review` 阶段暂不提供子 agent 内部 step 级进度；只在 core 中预留事件字段，待 `deepRunner` 实现后补充。

## Reuse Analysis

| # | 组件/文件 | 能力 | 复用方式 |
|---|---|---|---|
| 1 | `apps/ody-code/src/tui/components/chrome/moon-loader.ts` | `MoonLoader` 已支持 `setLabel(label: string)` 更新标签 | 在 `showProgressSpinner` 返回的 handle 上新增 `updateLabel` 方法并委托给 `MoonLoader.setLabel` |
| 2 | `apps/ody-code/src/tui/ody-tui.ts:1490-1505` | `showProgressSpinner` 创建 transcript spinner 并返回 `LoginProgressSpinnerHandle` | 扩展 handle 接口，新增 `updateLabel`；保持 `stop({ ok, label })` 不变 |
| 3 | `packages/agent-core/src/rpc/events.ts` | 已有 `AgentEvent` 联合类型和 `Event = AgentEvent & { agentId; sessionId }` | 新增 `CodeReviewProgressEvent` 到 `AgentEvent` 联合中 |
| 4 | `packages/node-sdk/src/rpc.ts` | `SDKRpcClient` 已有 `onEvent` / `receiveEvent` 事件总线 | 在 `receiveEvent` 中增加按 `requestId` 分发的内部 handler map；`requestCodeReview` 在调用前后注册/注销 handler |
| 5 | `packages/agent-core/src/code-review/executor.ts` | `review()` 方法内已区分 repo/diff/deep 路径 | 在路径关键节点调用可选的 `onProgress` 回调；把 `signal` 透传给 `fetchDiff` / `generate` / `deepRunner` / `auditScanner` |
| 6 | `apps/ody-code/src/tui/commands/request-code-review.ts` | 现有命令解析与 `host.showStatus` | 替换为 `host.showProgressSpinner`，传入 `onProgress` 回调并在回调中调用 `spinner.updateLabel` |
| 7 | 既有取消模式（`apps/ody-code/src/tui/commands/auth.ts`、`provider.ts`） | 使用 `AbortController` + `host.cancelInFlight` | 照搬到 `/request-code-review` 命令中 |

**结论**：无 greenfield 大组件，全部基于现有能力扩展。

## Architecture

```
TUI  /request-code-review handler
  ├─ 创建 AbortController，注册到 host.cancelInFlight
  ├─ spinner = host.showProgressSpinner("Preparing code review on <model>…")
  ├─ elapsed timer 每秒更新 spinner 标签
  │
  └─ await host.harness.requestCodeReview({
       source, modelAlias, ..., requestId,
       signal: controller.signal,
       onProgress: (p) => spinner.updateLabel(formatLabel(p, elapsed))
     })
       │
       ▼
KimiHarness.requestCodeReview(input, opts)
  ├─ 若 opts.onProgress 存在，生成 requestId
  ├─ 在 SDKRpcClient 注册 requestId → onProgress 的临时 handler
  │
  └─ await rpc.requestCodeReview({ ...input, requestId })
       │
       ▼
KimiCore.requestCodeReview(payload)
  ├─ 解析模型、构造 executor
  ├─ 把 payload.requestId 注入 onProgress 闭包
  │
  └─ await executor.review({ ...input, onProgress, signal })
       │
       ▼
createCodeReviewExecutor.review(input)
  ├─ onProgress({ stage: 'preparing', modelAlias })
  ├─ repo 路径：onProgress({ stage: 'audit-scanning', filePath? })
  ├─ diff 路径：onProgress({ stage: 'fetching-diff' }) → fetchDiff
  ├─ deep 路径：onProgress({ stage: 'deep-review' }) → deepRunner
  ├─ onProgress({ stage: 'generating' }) → generate
  └─ onProgress({ stage: completed|failed, ... })

onProgress 闭包
  └─ emitEvent({ type: 'codeReview.progress', requestId, stage, detail,
                  sessionId: CODE_REVIEW_PROGRESS_SESSION_ID,
                  agentId: CODE_REVIEW_PROGRESS_AGENT_ID })
       │
       ▼
SDKRpcClient.receiveEvent(event)
  └─ if event.type === 'codeReview.progress' and handlerMap.has(event.requestId):
       handlerMap.get(event.requestId)(event)
```

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `MoonLoader.setLabel` 可以安全地在运行时多次调用并触发重绘 | High | 标签无法更新 | 已读源码确认 |
| 2 | `SDKRpcClient.receiveEvent` 在 `requestCodeReview` 返回前会收到 core 发出的最终 progress 事件 | Medium | 用户看不到 `completed` 阶段，但不影响结果 | 实现后通过单元测试/手动验证事件顺序 |
| 3 | `AbortSignal.any` 在 Node ≥24.15.0 可用，用于合并用户取消与 timeout 信号 | High | 需要手写 signal 合并逻辑 | Node 24 已原生支持 |
| 4 | 代码审查执行器中的 `deepRunner` 与 `auditScanner` 当前未在 core-impl 外实现；进度事件预留字段即可，不阻塞本次发布 | Medium | deep/repo 路径只显示进入/退出阶段，没有中间进度 | 已读 `executor.ts` / `core-impl.ts` 确认 |
| 5 | TUI 使用 synthetic `sessionId`/`agentId` 的 progress 事件不会与真实 session 事件冲突 | High | session 事件处理可能误处理 | 选择常量值并让 `Session.onEvent` 按 sessionId 过滤；已确认 |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | progress 事件与 promise 返回竞态，导致 `completed` 事件丢失 | Medium | Low | TUI 在 `finally` 中停止 spinner；最终文案以 promise 结果为准 |
| 2 | `onProgress` 回调抛错导致整个请求失败 | Low | Medium | SDK 侧 dispatch 时 try/catch 吞掉回调错误，继续执行 |
| 3 | 大量 progress 事件（如 repo 扫描每文件一个）导致 UI 频繁重绘 | Medium | Medium | 对 repo 扫描事件做节流（≥200ms）或合并；TUI 定时器更新优先 |
| 4 | 取消信号传递不到 fetchDiff / deepRunner | Medium | Low | 扩展 `fetchDiff` 签名接受 signal；deepRunner/auditScanner 已支持 signal；实现单元测试验证 |
| 5 | 进度文案包含意外敏感信息 | Low | High | 设计约定：只传递 stage、modelAlias、token 数、文件路径；不传递 diff/代码 |

## Self-Review

（待设计完成后补充）

## User Final Approval

（待审批）

## Data Models

### `CodeReviewProgressStage` [C:USER]

```typescript
type CodeReviewProgressStage =
  | 'preparing'
  | 'fetching-diff'
  | 'audit-scanning'
  | 'deep-review'
  | 'generating'
  | 'completed'
  | 'failed';
```

### `CodeReviewProgress` [C:INFERRED]

TUI/SDK 侧看到的 progress 数据，不携带 synthetic session/agent id：

```typescript
interface CodeReviewProgress {
  readonly requestId: string;
  readonly stage: CodeReviewProgressStage;
  readonly modelAlias: string;
  readonly detail?: string | undefined;
  readonly meta?: {
    readonly estimatedTokens?: number | undefined;
    readonly filePath?: string | undefined;
    readonly fileCount?: number | undefined;
  } | undefined;
}
```

### `CodeReviewProgressEvent` [C:INFERRED]

加入到 `packages/agent-core/src/rpc/events.ts` 的 `AgentEvent` 联合中：

```typescript
export interface CodeReviewProgressEvent {
  readonly type: 'codeReview.progress';
  readonly requestId: string;
  readonly stage: CodeReviewProgressStage;
  readonly modelAlias: string;
  readonly detail?: string | undefined;
  readonly meta?: {
    readonly estimatedTokens?: number | undefined;
    readonly filePath?: string | undefined;
    readonly fileCount?: number | undefined;
  } | undefined;
}
```

被包装成 `Event` 时携带 synthetic id：

```typescript
const CODE_REVIEW_PROGRESS_SESSION_ID = '__code_review_progress__';
const CODE_REVIEW_PROGRESS_AGENT_ID = '__code_review_progress__';
```

### `RequestCodeReviewPayload` 扩展 [C:INFERRED]

```typescript
export interface RequestCodeReviewPayload {
  readonly modelAlias?: string | undefined;
  readonly source: CodeReviewDiffSource;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
  readonly workDir: string;
  readonly focus?: 'correctness' | 'simplicity' | undefined;
  readonly scope?: 'diff' | 'repo' | undefined;
  readonly requestId?: string | undefined;   // 新增
}
```

### `CodeReviewRequestInput` 扩展 [C:INFERRED]

```typescript
export interface CodeReviewRequestInput {
  readonly source: CodeReviewDiffSource;
  readonly modelAlias: string;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
  readonly focus?: 'correctness' | 'simplicity' | undefined;
  readonly scope?: 'diff' | 'repo' | undefined;
  readonly signal?: AbortSignal | undefined;   // 新增
  readonly onProgress?: (progress: CodeReviewProgress) => void | undefined; // 新增
}
```

### `CodeReviewExecutorDeps` 扩展 [C:INFERRED]

```typescript
export interface CodeReviewExecutorDeps {
  readonly cwd: string;
  readonly fetchDiff: (
    source: CodeReviewDiffSource,
    cwd: string,
    signal?: AbortSignal | undefined,
  ) => Promise<string>;
  readonly generate: (options: {
    readonly modelAlias: string;
    readonly systemPrompt: string;
    readonly userPrompt: string;
    readonly signal?: AbortSignal | undefined;
  }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> }; usage?: unknown }>;
  readonly resolveProviderConfig: (alias: string) => unknown;
  readonly estimateTokens: (text: string) => number;
  readonly deepRunner?: ((
    diff: string,
    input: CodeReviewRequestInput,
    signal?: AbortSignal,
  ) => Promise<CodeReviewReport>) | undefined;
  readonly auditScanner?: ((
    workspaceDir: string,
    signal?: AbortSignal,
  ) => Promise<RepoAuditDigest>) | undefined;
}
```

### `ProgressSpinnerHandle` 扩展 [C:INFERRED]

```typescript
export interface LoginProgressSpinnerHandle {
  updateLabel(label: string): void;           // 新增
  stop(opts: { ok: boolean; label: string }): void;
}
```

### `KimiHarness.requestCodeReview` 签名 [C:INFERRED]

```typescript
async requestCodeReview(
  input: {
    readonly source: ...;
    readonly modelAlias?: string | undefined;
    readonly description?: string | undefined;
    readonly requirements?: string | undefined;
    readonly deep?: boolean | undefined;
    readonly timeoutMs?: number | undefined;
    readonly workDir?: string | undefined;
    readonly focus?: 'correctness' | 'simplicity' | undefined;
    readonly scope?: 'diff' | 'repo' | undefined;
  },
  options?: {
    readonly signal?: AbortSignal | undefined;
    readonly onProgress?: (progress: CodeReviewProgress) => void;
  },
): Promise<CodeReviewReport>
```

## Algorithms

### A1. 核心执行器阶段上报 [C:INFERRED]

```
function review(input: CodeReviewRequestInput): Promise<CodeReviewReport>
  emit({ stage: 'preparing', modelAlias: input.modelAlias })
  signal = combineSignals(input.signal, input.timeoutMs)

  if input.scope === 'repo':
    if deps.auditScanner === undefined:
      return failReport('Repo audit is not available in this context.')
    emit({ stage: 'audit-scanning' })
    digest = await deps.auditScanner(deps.cwd, signal)
    emit({ stage: 'generating' })
    response = await deps.generate({ ..., signal })
    return parse(...)

  emit({ stage: 'fetching-diff' })
  diff = await deps.fetchDiff(input.source, deps.cwd, signal)

  estimatedTokens = deps.estimateTokens(diff)
  emit({ stage: 'fetching-diff', meta: { estimatedTokens } })
  if estimatedTokens > MAX_DIFF_TOKENS:
    emit({ stage: 'failed', detail: 'diff too large' })
    return failReport(...)

  if input.deep:
    if deps.deepRunner === undefined:
      return failReport('Deep review is not available...')
    emit({ stage: 'deep-review' })
    return await deps.deepRunner(diff, input, signal)

  emit({ stage: 'generating' })
  response = await deps.generate({ ..., signal })
  report = parse(...)
  emit({ stage: report.ok ? 'completed' : 'failed', detail: report.note })
  return report
```

### A2. 信号合并 [C:INFERRED]

```
function combineSignals(userSignal?: AbortSignal, timeoutMs?: number): AbortSignal | undefined
  timeoutSignal = timeoutMs !== undefined ? AbortSignal.timeout(timeoutMs) : undefined
  if userSignal === undefined and timeoutSignal === undefined: return undefined
  if userSignal === undefined: return timeoutSignal
  if timeoutSignal === undefined: return userSignal
  return AbortSignal.any([userSignal, timeoutSignal])
```

### A3. SDK 事件关联与分发 [C:INFERRED]

```
class SDKRpcClient:
  codeReviewProgressHandlers: Map<string, (p: CodeReviewProgress) => void> = new Map()

  async requestCodeReview(input, options = {}):
    rpc = await getRpc()
    if options.onProgress:
      requestId = generateRequestId()
      codeReviewProgressHandlers.set(requestId, options.onProgress)
      input = { ...input, requestId }
    try:
      return await rpc.requestCodeReview({ ...input, workDir: input.workDir ?? process.cwd() })
    finally:
      if requestId !== undefined:
        codeReviewProgressHandlers.delete(requestId)

  receiveEvent(event):
    for listener in eventListeners: listener(event)
    if event.type === 'codeReview.progress':
      handler = codeReviewProgressHandlers.get(event.requestId)
      if handler:
        try:
          handler(stripSyntheticIds(event))
        catch err:
          // 静默吞掉用户回调错误，避免破坏请求
```

### A4. TUI 标签格式化 [C:INFERRED]

```
function formatReviewProgressLabel(
  progress: CodeReviewProgress,
  elapsedSeconds: number,
): string
  stageText = mapStage(progress.stage)
  base = `Code review on ${progress.modelAlias} — ${stageText}`
  if progress.detail:
    base += ` (${truncate(progress.detail, 40)})`
  if progress.meta?.estimatedTokens !== undefined:
    base += ` · ~${progress.meta.estimatedTokens} tokens`
  if progress.meta?.filePath !== undefined:
    base += ` · ${basename(progress.meta.filePath)}`
  return `${base} (${elapsedSeconds}s)`
```

### A5. TUI 命令取消绑定 [C:USER]

```
handleRequestCodeReviewCommand(host, args):
  ...
  controller = new AbortController()
  cancel = () => controller.abort()
  host.cancelInFlight = cancel

  spinner = host.showProgressSpinner(formatLabel({ stage: 'preparing', modelAlias }, 0))
  elapsed = 0
  timer = setInterval(() => {
    elapsed += 1
    spinner.updateLabel(currentLabelWithElapsed(elapsed))
  }, 1000)

  try:
    report = await host.harness.requestCodeReview({ ... }, {
      signal: controller.signal,
      onProgress: (p) => {
        currentProgress = p
        spinner.updateLabel(formatReviewProgressLabel(p, elapsed))
      },
    })
    if report.ok:
      spinner.stop({ ok: true, label: `Code review complete (${report.reviewerAlias}).` })
      host.sendNormalUserInput(...)
    else:
      spinner.stop({ ok: false, label: report.note ?? 'Code review failed.' })
      host.showError(...)
  catch error:
    spinner.stop({ ok: false, label: `Code review failed: ${message(error)}` })
    host.showError(...)
  finally:
    clearInterval(timer)
    if host.cancelInFlight === cancel:
      host.cancelInFlight = undefined
```

## Error Handling

| 错误类 | 发生位置 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|---|
| `onProgress` 回调抛错 | `SDKRpcClient.receiveEvent` | try/catch 吞掉，继续 | 该次进度不显示，后续仍可能更新 | 用户代码修复后自动恢复 |
| progress 事件丢失/乱序 | SDK 事件通道 | TUI 依赖 `finally`/`promise` 结果停止 spinner | 最终标签用结果文案覆盖 | 无需恢复 |
| 用户取消 | TUI `AbortController.abort()` | signal 传播到 core generate / fetchDiff | 返回 `ok=false` 或抛 `AbortError` | 重试不使用取消 |
| `fetchDiff` 不支持 signal | `packages/agent-core/src/code-review/diff.ts` | 签名扩展后部分实现可能忽略 signal | 取消对 diff 获取无效，但 generate 仍可取消 | 逐步让 fetchDiff 使用 `exec` 的 abort 支持 |
| `deepRunner`/`auditScanner` 内部未转发 signal | `createCodeReviewExecutor` 外部注入 | 已要求签名带 signal；若实现忽略，则取消延迟 | 用户等待更久 | deep/repo 实现者补全 |
| diff 过大 | executor | emit `failed` 阶段并返回 `ok=false` | 提示用户缩小范围 | 用户换 `--base/--head` 后重试 |
| LLM 调用失败 | executor / generate | emit `failed` 阶段，返回 `ok=false` | 使用其它模型或重试 | 模型服务恢复 |

## Call-Site Integration

### 1. `packages/agent-core/src/rpc/events.ts`

在 `AgentEvent` 联合中追加 `CodeReviewProgressEvent`。

### 2. `packages/agent-core/src/code-review/types.ts:6-15`

为 `CodeReviewRequestInput` 增加 `signal?` 与 `onProgress?`。

### 3. `packages/agent-core/src/code-review/executor.ts`

关键节点（约第 30-130 行）插入 `input.onProgress?.(...)` 调用；`fetchDiff` 调用传入 `signal`；`generate` 调用已支持 `signal`。

### 4. `packages/agent-core/src/code-review/diff.ts`

```typescript
export async function fetchDiff(
  source: CodeReviewDiffSource,
  cwd: string,
  signal?: AbortSignal,
): Promise<string>
```

当前实现可忽略 `signal`，但签名必须兼容；后续若 `exec` 支持 abort，可直接接入。

### 5. `packages/agent-core/src/rpc/core-api.ts:348-358`

`RequestCodeReviewPayload` 增加 `readonly requestId?: string | undefined;`。

### 6. `packages/agent-core/src/rpc/core-impl.ts:468-586`

`requestCodeReview(payload)` 中：

```
const requestId = payload.requestId;
const emitProgress = (stage, detail?, meta?) => {
  if (requestId === undefined) return;
  void this.sdk.emitEvent({
    type: 'codeReview.progress',
    requestId,
    stage,
    modelAlias: resolvedModel,
    detail,
    meta,
    sessionId: CODE_REVIEW_PROGRESS_SESSION_ID,
    agentId: CODE_REVIEW_PROGRESS_AGENT_ID,
  });
};

const executor = createCodeReviewExecutor({
  ...,
  onProgress: emitProgress,
});
```

### 7. `packages/node-sdk/src/types.ts`

导出 `CodeReviewProgress` / `CodeReviewProgressStage`（从 `@odysseythink/agent-core` re-export）。

### 8. `packages/node-sdk/src/kimi-harness.ts:208-227`

```typescript
async requestCodeReview(
  input: ...,
  options?: { signal?: AbortSignal; onProgress?: (p: CodeReviewProgress) => void },
): Promise<CodeReviewReport> {
  return this.rpc.requestCodeReview({ ...input, ...options });
}
```

### 9. `packages/node-sdk/src/rpc.ts:346-349`

实现 `requestCodeReview` 的 requestId 生成、handler 注册与 `receiveEvent` 分发（见算法 A3）。

### 10. `apps/ody-code/src/tui/types.ts:208-210`

扩展 `LoginProgressSpinnerHandle` 接口，新增 `updateLabel`。

### 11. `apps/ody-code/src/tui/ody-tui.ts:1490-1505`

`showProgressSpinner` 返回的 handle 增加：

```typescript
updateLabel: (label: string) => {
  spinner.setLabel(label);
},
```

### 12. `apps/ody-code/src/tui/commands/request-code-review.ts:59-116`

按算法 A5 替换现有 `host.showStatus` 与直接 `await` 逻辑。

## Test Plan

### `packages/agent-core/test/code-review/executor.test.ts` [C:INFERRED]

1. **must pass**：给定 `onProgress` 时，`preparing` → `fetching-diff` → `generating` → `completed` 按顺序触发。
   - assert: `onProgress` 被调用至少 4 次；第一次 stage 为 `preparing`；最后一次 stage 为 `completed`。
2. **must pass**：diff 超过 token 上限时触发 `failed` 阶段并返回 `ok=false`。
   - assert: 最后一次 progress stage 为 `failed`；report.note 包含 `too large`。
3. **must pass**：`signal` 被传递到 `generate`。
   - assert: mock generate 收到的 `signal` 与输入 `signal` 相同。

### `packages/node-sdk/test/rpc.test.ts`（如不存在则新建或补充） [C:INFERRED]

4. **must pass**：`SDKRpcClient.requestCodeReview` 在传入 `onProgress` 时把 `requestId` 注入 payload。
   - assert: `rpc.requestCodeReview` 被调用时 payload 包含非空 `requestId`。
5. **must pass**：收到 `codeReview.progress` 事件时按 `requestId` 调用 `onProgress`。
   - assert: `onProgress` 被调用一次且参数 stage 为 `generating`。
6. **must reject**：不同 `requestId` 的事件不会触发当前请求的 `onProgress`。
   - assert: 当前请求的 `onProgress` 调用次数为 0。

### `apps/ody-code/test/tui/commands/request-code-review.test.ts` [C:USER]

7. **must pass**：命令启动时调用 `host.showProgressSpinner` 并传入初始 `preparing` 标签。
   - assert: `host.showProgressSpinner` 被调用，参数包含 `Code review on review-model` 和 `preparing`。
8. **must pass**：progress 回调更新 spinner 标签。
   - assert: mock spinner handle 的 `updateLabel` 被调用，参数包含阶段文本。
9. **must pass**：用户取消时 `controller.abort()` 被调用，且 `host.cancelInFlight` 在 finally 中被清空。
   - assert: 调用 cancel 后 `controller.signal.aborted` 为 true；finally 后 `host.cancelInFlight` 为 undefined。
10. **must pass**：请求成功时 spinner 以 `ok: true` 停止并发送结果消息。
    - assert: `spinner.stop({ ok: true, ... })`；`host.sendNormalUserInput` 被调用且包含 `Code review complete`。

### Done Criteria [C:INFERRED]

```bash
pnpm --filter @odysseythink/agent-core test
pnpm --filter @odysseythink/ody-code-sdk test
pnpm --filter @odysseythink/ody-code test test/tui/commands/request-code-review.test.ts
pnpm lint
```

（以项目实际命令为准；至少覆盖上述三个测试命令。）

## Self-Review

### 高代价决策审查

1. **按 `requestId` 分发 progress 事件**：使用 `crypto.randomUUID()` 生成 128-bit UUID，同一会话内并发代码审查概率极低；即使冲突，SDK 的 `codeReviewProgressHandlers` map 会覆盖旧 handler，旧请求的进度会进入新请求的回调——但 `requestCodeReview` 调用结束后会删除 handler，因此生命周期内不会泄漏。验证：`randomUUID()` 在本机 Node 环境下可生成唯一值。
2. **synthetic `sessionId`/`agentId` 不与真实 session 冲突**：常量 `__code_review_progress__` 不是合法 session id（session id 为 UUID），`Session.onEvent` 按 `event.sessionId === this.id` 过滤，因此不会进入任何真实 session 事件流。
3. **progress 文案不泄漏敏感上下文**：设计中 `detail` 仅用于阶段说明或文件路径，`meta` 仅用于 token 数/文件数/文件路径；不传递 diff 内容、代码片段、API key 或 provider 配置。

### 四镜扫描

- **Security**：检查了事件 payload 中可能进入 UI 的字段，确认只允许 `stage`、`modelAlias`、`detail`（文件路径/简短说明）、`meta.estimatedTokens/filePath/fileCount`。diff 内容、`userPrompt`、provider 配置、API key 均不进入 progress 事件。UI 侧 `formatReviewProgressLabel` 的 `truncate(detail, 40)` 仅影响显示长度，不改变允许字段集合。无新增 secret 泄漏点。
- **Test**：每个新增行为都有 must-pass / must-reject 断言：executor 阶段顺序、diff 过大触发 `failed`、SDK 按 requestId 分发、不同 requestId 不触发、TUI spinner 启动/更新/停止/取消。未发现断言与实现逻辑矛盾。
- **Ops**：progress 事件仅在 TUI 发起 `/request-code-review` 且传入 `onProgress` 时产生；单次审查的事件数受阶段数限制（repo 扫描场景若每文件一个事件，已建议在 TUI 侧节流）。`requestId` 使用 `randomUUID()`，无碰撞风险。取消通过 `AbortSignal.any` 合并 timeout 与用户 signal，实现幂等。
- **Integration**：已验证 `MoonLoader.setLabel`、`showProgressSpinner`、`SDKRpcClient.onEvent/receiveEvent`、`KimiCore.requestCodeReview`、`createCodeReviewExecutor`、事件类型定义、`Session.onEvent` 过滤逻辑均存在于代码库中。所有设计依赖的数据源/钩子点均存在。改动落在用户指定的 TUI `/request-code-review` 路径，未静默 retarget。
- **Scope**：本设计仍是一个连贯功能——“为 TUI `/request-code-review` 增加实时阶段进度提示”。虽然跨越 agent-core / node-sdk / apps/ody-code 三层，但属于同一功能在不同抽象层的改动，未裂变为多个独立产品。

## User Final Approval

- [ ] 设计已获用户批准
