# Part 5 — TUI `/request-code-review` + `/receive-code-review`

> Depends on: Part 3 Task 6（SDK `KimiHarness.requestCodeReview()` 可用），Part 1 Task 1（`modeModels` 类型可解析），Part 1 Task 2（`resolveCodeReviewModel` 可用）。`/receive-code-review` 额外依赖 `session.setModel()` 和 `session.activateSkill()`（已存在）。

## 文件列表

| 动作 | 文件 | 说明 |
|---|---|---|
| Create | `apps/ody-code/src/tui/commands/request-code-review.ts` | `/request-code-review` 处理函数 |
| Create | `apps/ody-code/src/tui/commands/receive-code-review.ts` | `/receive-code-review` 处理函数 |
| Modify | `apps/ody-code/src/tui/commands/registry.ts:24-298` | 新增两个命令定义 + `BuiltinSlashCommandName` |
| Modify | `apps/ody-code/src/tui/commands/dispatch.ts:213-325` | switch 增加两个 case |
| Modify | `apps/ody-code/src/tui/types.ts:15-43` | `AppState` 增加 `receiveCodeReview` 可选字段 |
| Modify | `apps/ody-code/src/tui/ody-tui.ts:149-181` | 初始化 `receiveCodeReview` |
| Modify | `apps/ody-code/src/tui/ody-tui.ts:683-706` | `sendNormalUserInput` 前恢复模型 |
| Create | `apps/ody-code/test/tui/commands/request-code-review.test.ts` | TUI handler 测试 |
| Create | `apps/ody-code/test/tui/commands/receive-code-review.test.ts` | TUI handler 测试 |

---

## Task 8: `/request-code-review` slash 命令

**Depends on:** Part 3 Task 6

**Files:**
- Create: `apps/ody-code/src/tui/commands/request-code-review.ts`
- Modify: `apps/ody-code/src/tui/commands/registry.ts`
- Modify: `apps/ody-code/src/tui/commands/dispatch.ts`
- Create: `apps/ody-code/test/tui/commands/request-code-review.test.ts`

### 步骤

- [ ] **Write failing tests** — 创建 `test/tui/commands/request-code-review.test.ts`：

```ts
import { describe, expect, it, vi } from 'vitest';
import { handleRequestCodeReviewCommand } from '../../../src/tui/commands/request-code-review';
import type { SlashCommandHost } from '../../../src/tui/commands/dispatch';

function createMockHost(overrides: Partial<SlashCommandHost> = {}): SlashCommandHost {
  return {
    state: {
      appState: {
        model: 'default-model',
        sessionMode: 'normal',
        streamingPhase: 'idle',
      },
    },
    session: {
      id: 's1',
      setModel: vi.fn(),
      activateSkill: vi.fn(),
    },
    harness: {
      getConfig: vi.fn().mockResolvedValue({
        modeModels: { codeReview: 'review-model' },
        defaultModel: 'fallback',
      }),
      requestCodeReview: vi.fn().mockResolvedValue({
        ok: true,
        reviewerAlias: 'review-model',
        findings: [
          { severity: 'important', title: 'test finding', detail: 'detail' },
        ],
        summary: 'one strength',
      }),
    },
    showStatus: vi.fn(),
    showError: vi.fn(),
    sendNormalUserInput: vi.fn(),
    requireSession: vi.fn(function (this: SlashCommandHost) { return this.session; }),
    cancelInFlight: undefined,
    deferUserMessages: false,
    ...overrides,
  } as unknown as SlashCommandHost;
}

describe('handleRequestCodeReviewCommand', () => {
  it('shows error when no active session', async () => {
    const host = createMockHost({ session: undefined });
    (host.requireSession as ReturnType<typeof vi.fn>).mockImplementation(() => undefined);
    await handleRequestCodeReviewCommand(host, '');
    expect(host.showError).toHaveBeenCalled();
  });

  it('calls harness.requestCodeReview and sends result to chat', async () => {
    const host = createMockHost();
    await handleRequestCodeReviewCommand(host, '--base HEAD~1 --head HEAD');
    expect(host.harness.requestCodeReview).toHaveBeenCalledOnce();
    expect(host.sendNormalUserInput).toHaveBeenCalledWith(
      expect.stringContaining('Code review complete'),
    );
  });

  it('shows error when report is not ok', async () => {
    const host = createMockHost({
      harness: {
        ...createMockHost().harness,
        requestCodeReview: vi.fn().mockResolvedValue({
          ok: false,
          reviewerAlias: 'x',
          findings: [],
          note: 'Diff too large',
        }),
      },
    });
    await handleRequestCodeReviewCommand(host, '');
    expect(host.showError).toHaveBeenCalledWith('Diff too large');
  });
});
```

- [ ] **Verify FAILS** — 文件不存在，或导入失败。

- [ ] **Write implementation:**

**`src/tui/commands/request-code-review.ts`：**

```ts
import { renderCodeReviewReportToMarkdown } from '@odysseythink/ody-code-sdk';

import { NO_ACTIVE_SESSION_MESSAGE } from '#/constant/ody-tui';
import { resolveCodeReviewModel } from '@odysseythink/agent-core';
import type { SlashCommandHost } from './dispatch';

interface SlashArgs {
  readonly base?: string;
  readonly head?: string;
  readonly pr?: string;
  readonly model?: string;
  readonly description?: string;
  readonly requirements?: string;
  readonly deep?: boolean;
}

function parseArgs(args: string): SlashArgs {
  const tokens = args.trim().split(/\s+/).filter((t) => t.length > 0);
  const result: SlashArgs & Record<string, unknown> = {};
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;
    if (token === '--base' || token === '--head' || token === '--pr' || token === '--model' ||
        token === '--description' || token === '--requirements') {
      result[camelFromFlag(token)] = tokens[i + 1];
      i += 1;
    } else if (token === '--deep') {
      result['deep'] = true;
    } else {
      // positional argument: treat as base (first positional is base)
      if (result['base'] === undefined) {
        result['base'] = token;
      } else if (result['head'] === undefined) {
        result['head'] = token;
      }
    }
  }
  return result as unknown as SlashArgs;
}

function camelFromFlag(flag: string): string {
  return flag.replace(/^--/, '').replace(/-([a-z])/g, (_, ch: string) => ch.toUpperCase());
}

function buildDiffSource(parsed: SlashArgs) {
  if (parsed.pr !== undefined) {
    return { kind: 'pr' as const, prUrlOrNumber: parsed.pr };
  }
  if (parsed.base !== undefined || parsed.head !== undefined) {
    return {
      kind: 'commits' as const,
      base: parsed.base ?? 'HEAD~1',
      head: parsed.head ?? 'HEAD',
    };
  }
  return { kind: 'working-tree' as const };
}

export async function handleRequestCodeReviewCommand(
  host: SlashCommandHost,
  args: string,
): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const parsed = parseArgs(args);
  const config = await host.harness.getConfig();
  const currentModel = host.state.appState.model;

  const resolvedModel = resolveCodeReviewModel(
    'request',
    config.modeModels,
    config.defaultModel,
    {
      explicit: parsed.model,
      sessionModel: currentModel.length > 0 ? currentModel : undefined,
    },
    (alias: string) => {
      const models = config.models ?? {};
      const providers = config.providers;
      const modelEntry = models[alias];
      if (modelEntry === undefined) return false;
      return providers[modelEntry.provider] !== undefined;
    },
  );

  host.showStatus(`Running code review on ${resolvedModel}…`);

  const source = buildDiffSource(parsed);
  try {
    const report = await host.harness.requestCodeReview({
      source,
      modelAlias: resolvedModel,
      description: parsed.description,
      requirements: parsed.requirements,
      deep: parsed.deep,
    });

    if (!report.ok) {
      host.showError(report.note ?? 'Code review failed.');
      return;
    }

    const markdown = renderCodeReviewReportToMarkdown(report);
    host.sendNormalUserInput(
      `Code review complete (${report.reviewerAlias}). Findings:\n\n${markdown}\n\nPlease act on the findings.`,
    );
  } catch (error) {
    host.showError(`Code review failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}
```

> **注意**：`NO_ACTIVE_SESSION_MESSAGE` 常量。需确认其在 `apps/ody-code/src/constant/ody-tui.ts` 中。可用 Grep 查找或直接在文件中定义/复用。上述假设 import `#/constant/ody-tui`，若不存在则使用 `'No active session. Create or resume a session first.'` 字符串。

**修改 `src/tui/commands/registry.ts`** — 在 `BUILTIN_SLASH_COMMANDS` 数组末尾（`exit` 定义之前）新增：

```ts
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

**修改 `src/tui/commands/dispatch.ts`** — import 中新增：

```ts
import { handleRequestCodeReviewCommand } from './request-code-review';
import { handleReceiveCodeReviewCommand } from './receive-code-review';
```

在 `handleBuiltInSlashCommand` 的 switch 中新增两个 case（在 `default` 之前）：

```ts
case 'request-code-review':
  await handleRequestCodeReviewCommand(host, args);
  return;
case 'receive-code-review':
  await handleReceiveCodeReviewCommand(host, args);
  return;
```

- [ ] **Run tests + typecheck:**

```bash
cd apps/ody-code && pnpm test -- --reporter=verbose test/tui/commands/request-code-review.test.ts
```

然后全量：

```bash
cd apps/ody-code && pnpm test
```

- [ ] **Commit.**

```bash
git add apps/ody-code/src/tui/commands/request-code-review.ts apps/ody-code/src/tui/commands/registry.ts apps/ody-code/src/tui/commands/dispatch.ts apps/ody-code/test/tui/commands/request-code-review.test.ts
git commit -m "feat: add /request-code-review TUI slash command"
```

---

## Task 9: `/receive-code-review` slash 命令 + 模型恢复

**Depends on:** Task 8（共用 registry/dispatch 修改）

**Files:**
- Create: `apps/ody-code/src/tui/commands/receive-code-review.ts`
- Modify: `apps/ody-code/src/tui/types.ts:15-43`
- Modify: `apps/ody-code/src/tui/ody-tui.ts:149-181, 683-706`
- Create: `apps/ody-code/test/tui/commands/receive-code-review.test.ts`

### 步骤

- [ ] **1. 新增 `AppState.receiveCodeReview` 字段** — 修改 **`src/tui/types.ts`**，在 `AppState` 接口中新增：

```ts
/** /receive-code-review 的模型切换状态。当 active 时，下一条普通消息前恢复原模型。 */
receiveCodeReview?: {
  originalModelAlias: string;
  reviewModelAlias: string;
  active: boolean;
};
```

- [ ] **2. 在 `createInitialAppState` 中初始化** — 修改 **`src/tui/ody-tui.ts`**，在返回的 `AppState` 对象末尾新增：

```ts
receiveCodeReview: undefined,
```

- [ ] **执行编译验证以上两步无类型错误：**

```bash
pnpm -r typecheck
```

- [ ] **3. 实现 `/receive-code-review` handler** — 创建 **`src/tui/commands/receive-code-review.ts`**：

```ts
import { resolveCodeReviewModel } from '@odysseythink/agent-core';
import { NO_ACTIVE_SESSION_MESSAGE } from '#/constant/ody-tui';
import type { SlashCommandHost } from './dispatch';

export async function handleReceiveCodeReviewCommand(
  host: SlashCommandHost,
  _args: string,
): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const config = await host.harness.getConfig({ reload: true });
  const currentModel = host.state.appState.model;

  let reviewModelAlias: string;
  try {
    reviewModelAlias = resolveCodeReviewModel(
      'receive',
      config.modeModels,
      config.defaultModel,
      {
        sessionModel: currentModel.length > 0 ? currentModel : undefined,
      },
      (alias: string) => {
        const models = config.models ?? {};
        const providers = config.providers;
        const modelEntry = models[alias];
        if (modelEntry === undefined) return false;
        return providers[modelEntry.provider] !== undefined;
      },
    );
  } catch (error) {
    host.showError(
      `Cannot enter receive-code-review mode: ${error instanceof Error ? error.message : String(error)}`,
    );
    return;
  }

  // Save original state
  host.setAppState({
    receiveCodeReview: {
      originalModelAlias: currentModel,
      reviewModelAlias,
      active: true,
    },
  });

  // Switch model
  try {
    await session.setModel(reviewModelAlias);
  } catch (error) {
    host.showError(
      `Failed to switch model to ${reviewModelAlias}: ${error instanceof Error ? error.message : String(error)}`,
    );
    // Clear state on failure
    host.setAppState({ receiveCodeReview: undefined });
    return;
  }

  // Activate receiving-code-review skill
  try {
    await session.activateSkill('receiving-code-review');
  } catch (error) {
    host.showError(
      `Failed to load receiving-code-review skill: ${error instanceof Error ? error.message : String(error)}`,
    );
    // Model is already switched; keep state so user can still interact
  }

  host.showStatus(
    `Switched to ${reviewModelAlias} and loaded receiving-code-review skill. Paste the review feedback and continue.`,
  );
}

export function maybeRestoreModelAfterReceiveReview(host: SlashCommandHost): void {
  const state = host.state.appState.receiveCodeReview;
  if (state?.active !== true) return;

  const session = host.session;
  if (session !== undefined && state.originalModelAlias.length > 0) {
    void session.setModel(state.originalModelAlias).catch(() => {});
  }

  host.setAppState({
    model: state.originalModelAlias,
    receiveCodeReview: { ...state, active: false },
  });
}
```

- [ ] **4. 在 `sendNormalUserInput` 中插入恢复逻辑** — 修改 **`src/tui/ody-tui.ts`** 的 `sendNormalUserInput` 方法，在 guard 检查之后 `sendMessage` 之前插入：

```ts
// ody-tui.ts sendNormalUserInput method — insert after the existing guard checks
// and before sendMessage calls:

sendNormalUserInput(text: string): void {
    // ... existing model + session guards ...

    // Restore model if /receive-code-review was active
    if (this.state.appState.receiveCodeReview?.active) {
      import('./commands/receive-code-review').then((mod) => {
        mod.maybeRestoreModelAfterReceiveReview(this);
      });
    }

    // ... existing sendMessage calls ...
```

或者使用静态 import 提高可靠性：

在 `ody-tui.ts` 顶部 import 区新增：

```ts
import { maybeRestoreModelAfterReceiveReview } from './commands/receive-code-review';
```

然后在 `sendNormalUserInput` 中：

```ts
// After session guard check (line ~694 of ody-tui.ts):
const session = this.session;
if (session === undefined) {
  this.showError(LLM_NOT_SET_MESSAGE);
  return;
}
// BEGIN INSERT:
if (this.state.appState.receiveCodeReview?.active) {
  maybeRestoreModelAfterReceiveReview(this as unknown as SlashCommandHost);
}
// END INSERT
```

> 注意：`ody-tui.ts` 中 `this` 实现了 `SlashCommandHost`，直接传递给 `maybeRestoreModelAfterReceiveReview`。需要 import `SlashCommandHost` 或使用 cast。

- [ ] **5. 编写 handler 测试** — 创建 **`test/tui/commands/receive-code-review.test.ts`**：

```ts
import { describe, expect, it, vi } from 'vitest';
import {
  handleReceiveCodeReviewCommand,
  maybeRestoreModelAfterReceiveReview,
} from '../../../src/tui/commands/receive-code-review';
import type { SlashCommandHost } from '../../../src/tui/commands/dispatch';

function createMockHost(overrides: Partial<SlashCommandHost> = {}): SlashCommandHost {
  return {
    state: {
      appState: {
        model: 'default-model',
        sessionMode: 'normal',
        streamingPhase: 'idle',
      },
    },
    session: {
      id: 's1',
      setModel: vi.fn(),
      activateSkill: vi.fn(),
    },
    harness: {
      getConfig: vi.fn().mockResolvedValue({
        modeModels: { codeReviewReceive: 'receiver-model' },
        defaultModel: 'fallback',
        models: { 'receiver-model': { provider: 'test-p', model: 'm1', maxContextSize: 8192 } },
        providers: { 'test-p': { type: 'openai', apiKey: 'sk-test' } },
      }),
    },
    showStatus: vi.fn(),
    showError: vi.fn(),
    setAppState: vi.fn(),
    sendNormalUserInput: vi.fn(),
    requireSession: vi.fn(function (this: SlashCommandHost) { return this.session; }),
    cancelInFlight: undefined,
    deferUserMessages: false,
    ...overrides,
  } as unknown as SlashCommandHost;
}

describe('handleReceiveCodeReviewCommand', () => {
  it('switches model and activates skill', async () => {
    const host = createMockHost();
    await handleReceiveCodeReviewCommand(host, '');
    expect(host.setAppState).toHaveBeenCalledWith(
      expect.objectContaining({
        receiveCodeReview: expect.objectContaining({ active: true }),
      }),
    );
    const setAppStateCall = (host.setAppState as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(setAppStateCall.receiveCodeReview.reviewModelAlias).toBe('receiver-model');
    expect(host.session.setModel).toHaveBeenCalledWith('receiver-model');
    expect(host.session.activateSkill).toHaveBeenCalledWith('receiving-code-review');
  });

  it('shows error when no active session', async () => {
    const host = createMockHost({ session: undefined });
    (host.requireSession as ReturnType<typeof vi.fn>).mockImplementation(() => undefined);
    await handleReceiveCodeReviewCommand(host, '');
    expect(host.showError).toHaveBeenCalled();
  });
});

describe('maybeRestoreModelAfterReceiveReview', () => {
  it('restores model when active', () => {
    const host = createMockHost({
      state: {
        appState: {
          model: 'receiver-model',
          sessionMode: 'normal',
          streamingPhase: 'idle',
          receiveCodeReview: {
            originalModelAlias: 'original',
            reviewModelAlias: 'receiver-model',
            active: true,
          },
        },
      },
    });
    maybeRestoreModelAfterReceiveReview(host);
    expect(host.setAppState).toHaveBeenCalledWith(
      expect.objectContaining({
        model: 'original',
        receiveCodeReview: expect.objectContaining({ active: false }),
      }),
    );
  });

  it('no-ops when not active', () => {
    const host = createMockHost();
    maybeRestoreModelAfterReceiveReview(host);
    expect(host.setAppState).not.toHaveBeenCalled();
  });
});
```

- [ ] **Run tests:**

```bash
cd apps/ody-code && pnpm test -- --reporter=verbose test/tui/commands/receive-code-review.test.ts
```

- [ ] **Final whole-tree typecheck + all tests:**

```bash
pnpm -r typecheck && pnpm test
```

- [ ] **Commit.**

```bash
git add apps/ody-code/src/tui/commands/receive-code-review.ts apps/ody-code/src/tui/types.ts apps/ody-code/src/tui/ody-tui.ts apps/ody-code/test/tui/commands/receive-code-review.test.ts
git commit -m "feat: add /receive-code-review TUI slash command with model restore on next message"
```

---

## 本地 Self-Review

- [ ] 1. **Spec-coverage**: 本 Part 覆盖 `/request-code-review`（参数解析、调用 harness、错误处理、结果注入会话）+ `/receive-code-review`（模型切换、skill 激活、模型恢复、状态管理）+ 命令注册/隐藏模式 + dispatch 路由。✅
- [ ] 2. **Placeholder scan**: 无 TODO/TBD。`NO_ACTIVE_SESSION_MESSAGE` 常量路径需确认，否则可用字符串字面量替代。✅
- [ ] 3. **No phantom tasks**: Task 8 产出了可测试的 handler + registry + dispatch 修改；Task 9 产出了 handler + AppState 扩展 + restore 集成 + 测试。均经编译和测试验证。✅
- [ ] 4. **Dependency soundness**: Task 8 依赖 Part 3 `harness.requestCodeReview` 和 `renderCodeReviewReportToMarkdown`；Task 9 依赖 Part 1 `resolveCodeReviewModel` 和现有 `session.setModel`/`activateSkill`。均已在早期 Part 实现。✅
- [ ] 5. **Caller & build soundness**:
  - `AppState` 新增可选字段 `receiveCodeReview`，不影响现有赋值处（`Object.assign` 合并不做删除）。
  - `builtinSlashCommandName` 类型由 `BUILTIN_SLASH_COMMANDS` 数组推导出新成员，`dispatch.ts` 的 switch 新增 case 后 exhaustiveness 满足。
  - `sendNormalUserInput` 新增的 restore 调用在现有 guard 之后、`sendMessage` 之前，不改变原有逻辑路径。
  - 全 workspace `pnpm -r typecheck` 末尾执行。✅
- [ ] 6. **Test-the-risk**: 
  - `/request-code-review` 测试了无 session、正常调用、report 失败三种路径。
  - `/receive-code-review` 测试了模型切换+skill 激活、无 session 报错、active 状态恢复、非 active noop。
  - `maybeRestoreModelAfterReceiveReview` 的 `setModel` 失败时吞错（`.catch(() => {})`）确保不阻塞消息发送。✅
- [ ] 7. **Type consistency**:
  - `AppState.receiveCodeReview` 类型 `{ originalModelAlias: string; reviewModelAlias: string; active: boolean } | undefined` 与 `maybeRestoreModelAfterReceiveReview` 中的访问一致。
  - `handleReceiveCodeReviewCommand` 调用 `resolveCodeReviewModel` 的 `kind: 'receive'` + validate 回调与 Part 1 Task 2 签名对齐。
  - `host.harness.requestCodeReview` 接收 `{ source, modelAlias, description, requirements, deep }` 与 Part 3 KimiHarness 签名一致。✅
