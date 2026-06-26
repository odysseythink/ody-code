# Code Review 命令 — TUI 层

## 设计目标

在 TUI 会话中提供两个 slash 命令：`/request-code-review` 复用核心执行器生成报告并注入会话；`/receive-code-review` 临时切换模型并注入 `receiving-code-review` skill prompt [C:USER]。

## Slash 命令注册

在 `apps/ody-code/src/tui/commands/registry.ts` 的 `BUILTIN_SLASH_COMMANDS` 数组中新增 [C:INFERRED]：

```typescript
{
  name: 'request-code-review',
  aliases: [],
  description: 'Request a code review of the current changes.',
  priority: 80,
  availability: 'idle-only',
  hiddenInModes: ['plan', 'design', 'office-hours'],
},
{
  name: 'receive-code-review',
  aliases: [],
  description: 'Enter receiving-code-review mode: switch model and load the receiving skill.',
  priority: 80,
  availability: 'idle-only',
  hiddenInModes: ['plan', 'design', 'office-hours'],
},
```

## `/request-code-review`

### 参数

```typescript
interface RequestCodeReviewSlashArgs {
  readonly base?: string;
  readonly head?: string;
  readonly pr?: string;
  readonly model?: string;
  readonly description?: string;
  readonly requirements?: string;
  readonly deep?: boolean;
}
```

参数解析逻辑与 CLI 相同 [C:INFERRED]。

### 处理流程

```typescript
export async function handleRequestCodeReviewCommand(
  host: SlashCommandHost,
  args: string,
): Promise<void> {
  session = host.requireSession()
  if (session === undefined) return

  const parsed = parseRequestCodeReviewArgs(args)
  const config = await host.harness.getConfig()
  const currentModel = host.state.appState.model

  const modelAlias = resolveCodeReviewModel(
    'request',
    config.modeModels,
    config.defaultModel,
    {
      explicit: parsed.model,
      sessionModel: currentModel.length > 0 ? currentModel : undefined,
    },
  )

  host.showStatus(`Running code review on ${modelAlias}…`)

  const source = buildDiffSource(parsed)
  const executor = createCodeReviewExecutor(host.harness)

  const report = await executor.review({
    source,
    modelAlias,
    description: parsed.description,
    requirements: parsed.requirements,
    deep: parsed.deep,
  })

  if (!report.ok) {
    host.showError(report.note ?? 'Code review failed.')
    return
  }

  const markdown = renderReportToMarkdown(report)
  host.sendNormalUserInput(
    `Code review complete (${report.reviewerAlias}). Findings:\n\n${markdown}\n\nPlease act on the findings.`
  )
}
```

## `/receive-code-review`

### 状态管理

```typescript
interface ReceiveCodeReviewState {
  readonly originalModelAlias: string;
  readonly reviewModelAlias: string;
  readonly active: boolean;
}
```

该状态挂载在 `TUIState` 或 `AppState` 上 [C:INFERRED]：

```typescript
interface AppState {
  // ... existing fields ...
  receiveCodeReview?: ReceiveCodeReviewState;
}
```

### 处理流程

```typescript
export async function handleReceiveCodeReviewCommand(
  host: SlashCommandHost,
  _args: string,
): Promise<void> {
  session = host.requireSession()
  if (session === undefined) return

  const config = await host.harness.getConfig()
  const currentModel = host.state.appState.model

  const reviewModelAlias = resolveCodeReviewModel(
    'receive',
    config.modeModels,
    config.defaultModel,
    {
      sessionModel: currentModel.length > 0 ? currentModel : undefined,
    },
  )

  // 保存原模型
  host.setAppState({
    receiveCodeReview: {
      originalModelAlias: currentModel,
      reviewModelAlias,
      active: true,
    },
  })

  // 切换模型
  await session.setModel(reviewModelAlias)

  // 注入 receiving skill
  await session.activateSkill('receiving-code-review')

  host.showStatus(
    `Switched to ${reviewModelAlias} and loaded receiving-code-review skill. Paste the review feedback and continue.`
  )
}
```

### 模型恢复

在 `host.sendNormalUserInput` 的发送路径中检查 `receiveCodeReview.active` [C:INFERRED]：

```typescript
function maybeRestoreModelAfterReceiveReview(host: SlashCommandHost): void {
  const state = host.state.appState.receiveCodeReview
  if (state?.active !== true) return

  const session = host.session
  if (session !== undefined && state.originalModelAlias.length > 0) {
    void session.setModel(state.originalModelAlias)
  }

  host.setAppState({
    receiveCodeReview: { ...state, active: false },
  })
}
```

触发位置：
- 用户发送下一条非 slash 消息前调用 `maybeRestoreModelAfterReceiveReview` [C:INFERRED]。
- 用户发送新的 slash 命令时同样调用，确保模型不会长期停留在 review 模型。

### Skill 注入方式

调用 `session.activateSkill('receiving-code-review')`，该 RPC 会把 skill 的 prompt 注入当前会话上下文 [C:INFERRED]。这与 dispatch.ts 中现有 skill 命令的调用路径一致。

## 调用位置

| 文件 | 行范围 | 说明 |
|---|---|---|
| `apps/ody-code/src/tui/commands/registry.ts` | 24-298 | 在 `BUILTIN_SLASH_COMMANDS` 中新增两个命令定义 [C:INFERRED] |
| `apps/ody-code/src/tui/commands/request-code-review.ts` | 新建 | `/request-code-review` 处理函数 [C:INFERRED] |
| `apps/ody-code/src/tui/commands/receive-code-review.ts` | 新建 | `/receive-code-review` 处理函数 [C:INFERRED] |
| `apps/ody-code/src/tui/commands/dispatch.ts` | 213-325 | 在 `handleBuiltInSlashCommand` switch 中新增两个 case [C:INFERRED] |
| `apps/ody-code/src/tui/types.ts` | 定义 AppState 处 | 新增可选 `receiveCodeReview` 状态 [C:INFERRED] |
| `apps/ody-code/src/tui/ody-tui.ts` 或输入发送路径 | 发送普通消息前 | 调用 `maybeRestoreModelAfterReceiveReview` [C:INFERRED] |

## 错误与降级

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| 当前无会话 | `host.showError(NO_ACTIVE_SESSION_MESSAGE)` | 无 | 用户创建或恢复会话 |
| review 专用模型无效 | 继续 fallback 链 | 使用当前会话模型或 defaultModel | 配置有效模型 |
| `setModel` 失败 | `host.showError` | 保持当前模型，仍注入 skill | 模型服务恢复 |
| `activateSkill` 失败 | `host.showError` | 无 | skill 名称/路径正确 |

## 测试断言

1. `/request-code-review --base HEAD~1 --head HEAD` 在 idle 状态下触发审查并将会话消息注入当前会话。
2. `/request-code-review` 在流式输出过程中不可用（`availability: 'idle-only'`）。
3. `/receive-code-review` 切换模型后，`AppState.receiveCodeReview.active === true`。
4. 用户发送下一条普通消息后，模型恢复到 originalModelAlias，`active === false`。
5. `/receive-code-review` 在 plan/design/office-hours 模式下隐藏。
