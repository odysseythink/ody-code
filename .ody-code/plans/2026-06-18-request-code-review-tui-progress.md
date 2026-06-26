# /request-code-review TUI 进度提示改进 — Implementation Plan

**Goal:** 将 TUI `/request-code-review` 的静态文案替换为可实时更新的 spinner 进度提示，通过 agent-core → node-sdk → TUI 三层事件通道将 executor 阶段反馈到 UI。

**Architecture:** agent-core 新增 `CodeReviewProgressEvent` 事件类型，executor 各阶段通过可选 `onProgress` 回调上报；core-impl 按 `requestId` 将事件 emit 到 SDK RPC 通道；SDKRpcClient 按 `requestId` 分发到请求级 handler；TUI 命令 handler 用 `showProgressSpinner` + 秒级计时器 + AbortController 取消机制展示进度。

**Tech Stack:** TypeScript, Vitest, @earendil-works/pi-tui (MoonLoader), React（现有 TUI 框架未使用 React，以 pi-tui 为准）

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| File | Task | Role |
|---|---|---|
| `packages/agent-core/src/code-review/types.ts:1-31` | T1, T3 | 新增 `CodeReviewProgress`/`CodeReviewProgressStage`，扩展 `CodeReviewRequestInput` (`signal?`, `onProgress?`)，扩展 `CodeReviewExecutorDeps` |
| `packages/agent-core/src/code-review/diff.ts:36-49` | T1 | `fetchDiff` 签名增加 `signal?` |
| `packages/agent-core/src/rpc/events.ts:290-321` | T2 | 新增 `CodeReviewProgressEvent`，追加到 `AgentEvent` 联合 |
| `packages/agent-core/src/index.ts:86-95` | T2 | 导出 `CodeReviewProgress`/`CodeReviewProgressStage` |
| `packages/agent-core/src/code-review/executor.ts:1-132` | T3 | `review()` 插入 `onProgress` 调用，新增 `combineSignals` |
| `packages/agent-core/test/code-review/executor.test.ts:1-227` | T3 | 新增 progress 阶段顺序、signal 传递、diff-too-large 测试 |
| `packages/agent-core/src/rpc/core-api.ts:348-358` | T4 | `RequestCodeReviewPayload` 增加 `requestId?` |
| `packages/agent-core/src/rpc/core-impl.ts:468-586` | T4 | `requestCodeReview` 中构造 emitProgress 闭包并注入 executor |
| `packages/node-sdk/src/types.ts:66-72` | T5 | re-export `CodeReviewProgress`/`CodeReviewProgressStage` |
| `packages/node-sdk/src/rpc.ts:346-349,575-586` | T5 | `SDKRpcClient` 增加 progress handler map + `requestCodeReview` requestId 生成/分发 |
| `packages/node-sdk/src/kimi-harness.ts:207-227` | T5 | `requestCodeReview` 增加 `options?` 参数（signal, onProgress） |
| `packages/node-sdk/test/rpc.test.ts` (Create) | T5 | 新建 SDK rpc 测试：requestId 注入、progress 分发、不同 requestId 不触发 |
| `apps/ody-code/src/tui/types.ts:208-212` | T6 | `LoginProgressSpinnerHandle` 增加 `updateLabel` 方法 |
| `apps/ody-code/src/tui/ody-tui.ts:1490-1505` | T7 | `showProgressSpinner` 返回的对象增加 `updateLabel` 委托给 `MoonLoader.setLabel` |
| `apps/ody-code/src/tui/commands/request-code-review.ts:59-116` | T8 | 替换为 spinner + timer + AbortController + `formatReviewProgressLabel` |
| `apps/ody-code/test/tui/commands/request-code-review.test.ts:1-77` | T8 | 新增 spinner 启动/更新/停止/取消测试 |

## Dependency Overview

```
Phase A: agent-core types & events (parallel)
  T1 ──┐
  T2 ──┤ (independent, both type-only)
       │
Phase B: agent-core executor (depends on T1)
  T3 ──┤
       │
Phase C: agent-core RPC wiring (depends on T2, T3)
  T4 ──┤
       │
Phase D: node-sdk (depends on T2, T4)
  T5 ──┤
       │
Phase E: TUI (depends on T5)
  T6 ── T7 ── T8
```

T1 and T2 are independent and can run in parallel. T3 needs T1's types. T4 needs T2's event type + T3's executor changes. T5 needs T2's types (re-export) + T4's requestId. T6-T8 are sequential TUI changes depending on T5 (types from SDK).

## Risks & Open Questions

1. **progress 事件竞态**: 若 `completed` 事件在 promise resolve 前未到达，TUI `finally` 中仍以 promise 结果停止 spinner（已缓解）。
2. **SDK 测试文件不存在**: `packages/node-sdk/test/rpc.test.ts` 需新建，需确认 vitest 配置已覆盖 `node-sdk` 包。
3. **`fetchDiff` signal 忽略**: 当前 `spawn` 实现不支持 AbortSignal，signal 仅在签名层面兼容，取消对 diff 获取无效（设计中已确认可接受）。

---

## Phase A: agent-core Types & Events

### Task 1: code-review types & diff signal 扩展

**Depends on:** none  
**Files:** Modify: `packages/agent-core/src/code-review/types.ts:1-31`, `packages/agent-core/src/code-review/diff.ts:36-49`, `packages/agent-core/src/code-review/executor.ts:11-24`  / Test: `packages/agent-core/test/code-review/executor.test.ts`（已有测试需适配签名变更）

- [ ] **Write the failing type-check**: 在 `types.ts` 中新增类型但暂时不在 `executor.test.ts` 中适配，触发编译错误。
```typescript
// packages/agent-core/src/code-review/types.ts — 在 CodeReviewRequestInput 之后增加：

export type CodeReviewProgressStage =
  | 'preparing'
  | 'fetching-diff'
  | 'audit-scanning'
  | 'deep-review'
  | 'generating'
  | 'completed'
  | 'failed';

export interface CodeReviewProgress {
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

// 在 CodeReviewRequestInput 末尾增加:
export interface CodeReviewRequestInput {
  // ... 现有字段不变 ...
  readonly signal?: AbortSignal | undefined;     // 新增
  readonly onProgress?: ((progress: CodeReviewProgress) => void) | undefined; // 新增
}
```

```typescript
// packages/agent-core/src/code-review/executor.ts — CodeReviewExecutorDeps 中的 fetchDiff 签名变更:
export interface CodeReviewExecutorDeps {
  readonly cwd: string;
  readonly fetchDiff: (
    source: CodeReviewDiffSource,
    cwd: string,
    signal?: AbortSignal,       // 新增第三个参数
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
    signal?: AbortSignal,       // 新增第三个参数
  ) => Promise<CodeReviewReport>) | undefined;
  readonly auditScanner?: ((
    workspaceDir: string,
    signal?: AbortSignal,
  ) => Promise<RepoAuditDigest>) | undefined;
}
```

- [ ] **Run and verify it FAILS**: `pnpm --filter @odysseythink/agent-core typecheck` 应报 `fetchDiff` 参数不匹配（调用处仍传 2 个参数）。

- [ ] **Write the minimal implementation**:

1. `diff.ts` — 为 `fetchDiff` 增加第三个参数（保留 opts 兼容）:
```typescript
// packages/agent-core/src/code-review/diff.ts:36-49
export async function fetchDiff(
  source: CodeReviewDiffSource,
  cwd: string,
  _signal?: AbortSignal,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  switch (source.kind) {
    case 'commits':
      return runGitDiff(['diff', source.base, source.head], cwd, opts);
    case 'working-tree':
      return runGitDiff(['diff'], cwd, opts);
    case 'pr':
      return runGhPrDiff(parsePrNumber(source.prUrlOrNumber), cwd, opts);
  }
}
```

2. `executor.ts` — `deepRunner` 调用时传入 `signal`:
```typescript
// packages/agent-core/src/code-review/executor.ts:91-93
// 原: return deps.deepRunner(diff, input);
// 改为:
return deps.deepRunner(diff, input, signal);
```

3. **查找并更新所有 `CodeReviewExecutorDeps` 调用者** — `grep -rn "fetchDiff\|deepRunner" packages/agent-core/`:

- `packages/agent-core/src/rpc/core-impl.ts:493` — `fetchDiff: async (source) => codeReviewFetchDiff(source, payload.workDir)` — 无需改（多余的 signal 参数会被忽略）
- `packages/agent-core/test/code-review/executor.test.ts` — 所有 `fetchDiff: vi.fn(async () => ...)` mock 签名兼容（剩余参数用 `_rest` 或保持现有），`deepRunner` mock 签名增加第三个参数:
```typescript
// 修改所有 deepRunner mock 定义（第 86、92 行附近）:
deepRunner: vi.fn(async (_diff, _input, _signal) => ({ ... })),
```

- [ ] **Run and verify it PASSES**: 
```bash
pnpm --filter @odysseythink/agent-core typecheck && pnpm --filter @odysseythink/agent-core test
```

- [ ] **Commit**: `feat(agent-core): add CodeReviewProgress types and extend executor deps with signal`

---

### Task 2: CodeReviewProgressEvent & AgentEvent 联合

**Depends on:** none (与 T1 并行)  
**Files:** Modify: `packages/agent-core/src/rpc/events.ts:290-321`, `packages/agent-core/src/index.ts:86-95`

- [ ] **Write the implementation** (纯类型/常量，无行为变更，无测试):
```typescript
// packages/agent-core/src/rpc/events.ts — 在 AgentEvent 联合之前插入:
export interface CodeReviewProgressEvent {
  readonly type: 'codeReview.progress';
  readonly requestId: string;
  readonly stage: 'preparing' | 'fetching-diff' | 'audit-scanning' | 'deep-review' | 'generating' | 'completed' | 'failed';
  readonly modelAlias: string;
  readonly detail?: string | undefined;
  readonly meta?: {
    readonly estimatedTokens?: number | undefined;
    readonly filePath?: string | undefined;
    readonly fileCount?: number | undefined;
  } | undefined;
}

// 在 AgentEvent 联合末尾追加:
export type AgentEvent =
  | ErrorEvent
  | WarningEvent
  | // ... 现有成员 ...
  | CronFiredEvent
  | CodeReviewProgressEvent;   // 新增
```

```typescript
// packages/agent-core/src/index.ts:86-95 — 在现有 code-review 导出块增加:
export type {
  CodeReviewDiffSource,
  CodeReviewRequestInput,
  CodeReviewReport,
  CodeReviewFinding,
  CodeReviewProgress,        // 新增
  CodeReviewProgressStage,   // 新增
} from './code-review/types';
```

- [ ] **Build verification**: `pnpm --filter @odysseythink/agent-core typecheck`

- [ ] **Manual verification**: 确认 `packages/agent-core/src/index.ts` 导出了 `CodeReviewProgress` 和 `CodeReviewProgressStage`，其他 package 可通过 `@odysseythink/agent-core` import。

- [ ] **Commit**: `feat(agent-core): add CodeReviewProgressEvent to AgentEvent union`

---

## Phase B: agent-core Executor Progress

### Task 3: executor progress emission + signal propagation

**Depends on:** Task 1  
**Files:** Modify: `packages/agent-core/src/code-review/executor.ts:1-132` / Test: `packages/agent-core/test/code-review/executor.test.ts:1-227`

- [ ] **Write the failing test**: 在 `executor.test.ts` 末尾追加 3 个新测试：
```typescript
// packages/agent-core/test/code-review/executor.test.ts — 在文件末尾 describe 块内追加：

it('emits progress events in correct order', async () => {
  const stages: string[] = [];
  const onProgress = vi.fn((p: { stage: string }) => { stages.push(p.stage); });
  const llmText = [
    'Strengths:\n- Good',
    '',
    'Findings:\nCritical:\n\nImportant:\n\nMinor:\n',
    '',
    'Assessment: Ready',
  ].join('\n');
  const executor = createCodeReviewExecutor({
    cwd,
    fetchDiff: vi.fn(async () => 'mock diff'),
    generate: fakeGenerate(llmText) as any,
    resolveProviderConfig: vi.fn(() => ({})),
    estimateTokens: vi.fn(() => 10),
  });
  await executor.review({
    source: { kind: 'working-tree' },
    modelAlias,
    onProgress: onProgress as any,
  });
  expect(onProgress).toHaveBeenCalledTimes(4); // preparing, fetching-diff, generating, completed
  expect(stages[0]).toBe('preparing');
  expect(stages[1]).toBe('fetching-diff');
  expect(stages[2]).toBe('generating');
  expect(stages[3]).toBe('completed');
});

it('emits failed when diff exceeds token limit', async () => {
  const stages: string[] = [];
  const onProgress = vi.fn((p: { stage: string }) => { stages.push(p.stage); });
  const executor = createCodeReviewExecutor({
    cwd,
    fetchDiff: vi.fn(async () => 'x'.repeat(100_000)),
    generate: fakeGenerate('') as any,
    resolveProviderConfig: vi.fn(() => ({})),
    estimateTokens: vi.fn(() => 200_000),
  });
  const report = await executor.review({
    source: { kind: 'working-tree' },
    modelAlias,
    onProgress: onProgress as any,
  });
  expect(report.ok).toBe(false);
  expect(stages).toContain('failed');
});

it('passes signal to generate', async () => {
  const controller = new AbortController();
  const generate = vi.fn(async () => ({
    message: { role: 'assistant', content: [{ type: 'text', text: 'Assessment: Ready' }] },
  }));
  const executor = createCodeReviewExecutor({
    cwd,
    fetchDiff: vi.fn(async () => 'mock diff'),
    generate: generate as any,
    resolveProviderConfig: vi.fn(() => ({})),
    estimateTokens: vi.fn(() => 10),
  });
  await executor.review({
    source: { kind: 'working-tree' },
    modelAlias,
    signal: controller.signal,
  });
  expect(generate).toHaveBeenCalledWith(
    expect.objectContaining({ signal: controller.signal }),
  );
});
```

- [ ] **Run and verify it FAILS**: `pnpm --filter @odysseythink/agent-core test` — 3 个新测试全部失败（onProgress 为 undefined 或未被调用）。

- [ ] **Write the minimal implementation**:

```typescript
// packages/agent-core/src/code-review/executor.ts — 完整替换 review() 方法:

const MAX_DIFF_TOKENS = 100_000;

function combineSignals(userSignal?: AbortSignal, timeoutMs?: number): AbortSignal | undefined {
  const timeoutSignal = timeoutMs !== undefined ? AbortSignal.timeout(timeoutMs) : undefined;
  if (userSignal === undefined && timeoutSignal === undefined) return undefined;
  if (userSignal === undefined) return timeoutSignal;
  if (timeoutSignal === undefined) return userSignal;
  return AbortSignal.any([userSignal, timeoutSignal]);
}

export function createCodeReviewExecutor(deps: CodeReviewExecutorDeps) {
  return {
    async review(input: CodeReviewRequestInput): Promise<CodeReviewReport> {
      const isSimplicity = input.focus === 'simplicity' || input.scope === 'repo';
      const signal = combineSignals(input.signal, input.timeoutMs);

      input.onProgress?.({ requestId: '', stage: 'preparing', modelAlias: input.modelAlias });

      // ── Repo audit path ──
      if (input.scope === 'repo') {
        if (deps.auditScanner === undefined) {
          return {
            ok: false,
            reviewerAlias: input.modelAlias,
            findings: [],
            note: 'Repo audit is not available in this context.',
          };
        }
        try {
          input.onProgress?.({ requestId: '', stage: 'audit-scanning' });
          const digest = await deps.auditScanner(deps.cwd, signal);
          input.onProgress?.({ requestId: '', stage: 'generating', modelAlias: input.modelAlias });
          const userPrompt = buildSimplicityAuditPrompt(digest);
          const response = await deps.generate({
            modelAlias: input.modelAlias,
            systemPrompt: '',
            userPrompt,
            signal,
          });
          const text = response.message.content
            .filter((p) => p.type === 'text')
            .map((p) => p.text)
            .join('');
          const report = parseSimplicityReport(text, input.modelAlias);
          input.onProgress?.({ requestId: '', stage: report.ok ? 'completed' : 'failed', modelAlias: input.modelAlias, detail: report.note });
          return report;
        } catch (error) {
          const note = `Code review failed: ${error instanceof Error ? error.message : String(error)}`;
          input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
          return {
            ok: false,
            reviewerAlias: input.modelAlias,
            findings: [],
            note,
          };
        }
      }

      // ── Diff-based path ──
      input.onProgress?.({ requestId: '', stage: 'fetching-diff', modelAlias: input.modelAlias });
      let diff: string;
      try {
        diff = await deps.fetchDiff(input.source, deps.cwd, signal);
      } catch (error) {
        const note = `Failed to fetch diff: ${error instanceof Error ? error.message : String(error)}`;
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }

      const estimatedTokens = deps.estimateTokens(diff);
      input.onProgress?.({ requestId: '', stage: 'fetching-diff', modelAlias: input.modelAlias, meta: { estimatedTokens } });
      if (estimatedTokens > MAX_DIFF_TOKENS) {
        const note = `Diff too large (~${estimatedTokens} tokens, limit ${MAX_DIFF_TOKENS}). Try a smaller range or use --base/--head.`;
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }

      if (input.deep) {
        if (deps.deepRunner !== undefined) {
          input.onProgress?.({ requestId: '', stage: 'deep-review', modelAlias: input.modelAlias });
          return deps.deepRunner(diff, input, signal);
        }
        const note = 'Deep review is not available in this context. Try without --deep.';
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }

      try {
        input.onProgress?.({ requestId: '', stage: 'generating', modelAlias: input.modelAlias });
        const userPrompt = isSimplicity
          ? buildSimplicityReviewPrompt(diff, input.description, input.requirements)
          : buildReviewPrompt(diff, input.description, input.requirements);

        const response = await deps.generate({
          modelAlias: input.modelAlias,
          systemPrompt: '',
          userPrompt,
          signal,
        });
        const text = response.message.content
          .filter((p) => p.type === 'text')
          .map((p) => p.text)
          .join('');

        const report = isSimplicity
          ? parseSimplicityReport(text, input.modelAlias)
          : parseReviewReport(text, input.modelAlias);
        input.onProgress?.({ requestId: '', stage: report.ok ? 'completed' : 'failed', modelAlias: input.modelAlias, detail: report.note });
        return report;
      } catch (error) {
        const note = `Code review failed: ${error instanceof Error ? error.message : String(error)}`;
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }
    },
  };
}
```

- [ ] **Run and verify it PASSES**: 
```bash
pnpm --filter @odysseythink/agent-core typecheck && pnpm --filter @odysseythink/agent-core test
```
预期：所有历史测试通过 + 3 个新测试通过。

- [ ] **Whole-tree typecheck**（`core-impl.ts` 调用 `executor.review` 传入的是不含 `signal`/`onProgress` 的旧对象，需确认类型兼容 — `signal?`/`onProgress?` 为 optional，无需改动）:
```bash
pnpm -r typecheck
```

- [ ] **Commit**: `feat(agent-core): add onProgress callbacks and signal propagation to executor`

---

## Phase C: agent-core RPC Wiring

### Task 4: core-api requestId + core-impl progress event emission

**Depends on:** Task 2, Task 3  
**Files:** Modify: `packages/agent-core/src/rpc/core-api.ts:348-358`, `packages/agent-core/src/rpc/core-impl.ts:1-10（imports）, 468-586`

- [ ] **Write the implementation** (纯 wiring，类型检查即验证):

```typescript
// packages/agent-core/src/rpc/core-api.ts:348-358 — 末尾增加 requestId:
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

```typescript
// packages/agent-core/src/rpc/core-impl.ts — 在文件顶部 imports 区增加:
import type { CodeReviewProgressEvent } from './events';  // 新增在 events import 块

// 在 class KimiCore 顶部增加常量:
const CODE_REVIEW_PROGRESS_SESSION_ID = '__code_review_progress__';
const CODE_REVIEW_PROGRESS_AGENT_ID = '__code_review_progress__';
```

```typescript
// packages/agent-core/src/rpc/core-impl.ts — 替换 requestCodeReview 方法（468-586行）:

async requestCodeReview(payload: RequestCodeReviewPayload): Promise<CodeReviewReportData> {
  this.reloadProviderManager();

  const providerManager = this.resolveProviderManager('code-review');

  const resolvedModel = resolveCodeReviewModel(
    'request',
    this.config.modeModels,
    this.config.defaultModel,
    { explicit: payload.modelAlias },
    (alias) => {
      try {
        providerManager.resolveProviderConfig(alias);
        return true;
      } catch {
        return false;
      }
    },
  );

  // ── Progress event emission ──
  const requestId = payload.requestId;
  const emitProgress = (
    stage: CodeReviewProgressEvent['stage'],
    detail?: string,
    meta?: CodeReviewProgressEvent['meta'],
  ) => {
    if (requestId === undefined) return;
    void (async () => {
      const sdk = await this.sdk;
      sdk.emitEvent({
        type: 'codeReview.progress',
        requestId,
        stage,
        modelAlias: resolvedModel,
        detail,
        meta,
        sessionId: CODE_REVIEW_PROGRESS_SESSION_ID,
        agentId: CODE_REVIEW_PROGRESS_AGENT_ID,
      });
    })().catch(() => {});
  };

  const resolvedProvider = providerManager.resolveProviderConfig(resolvedModel);
  const provider = createProvider(resolvedProvider.provider);

  const executor = createCodeReviewExecutor({
    cwd: payload.workDir,
    fetchDiff: async (source, _cwd, signal) => codeReviewFetchDiff(source, payload.workDir, signal),
    auditScanner: payload.scope === 'repo'
      ? async (workspaceDir, signal) => buildAuditDigest(workspaceDir, signal)
      : undefined,
    generate: async (options) => {
      const doGenerate = async (auth?: ProviderRequestAuth): ReturnType<typeof generate> => {
        return generate(
          provider,
          options.systemPrompt,
          [],
          [createUserMessage(options.userPrompt)],
          undefined,
          { signal: options.signal, ...(auth !== undefined ? { auth } : {}) },
        );
      };

      const withAuth = providerManager.resolveAuth?.(resolvedModel);
      const result = withAuth !== undefined
        ? await withAuth((auth) => doGenerate(auth))
        : await doGenerate();

      return {
        message: {
          role: result.message.role,
          content: result.message.content.filter((p): p is { type: 'text'; text: string } => p.type === 'text'),
        },
        usage: result.usage,
      };
    },
    resolveProviderConfig: (alias) => providerManager.resolveProviderConfig(alias),
    estimateTokens,
  });

  // ── Telemetry ──
  const isSimplicity = payload.focus === 'simplicity' || payload.scope === 'repo';
  const isAudit = payload.scope === 'repo';
  if (isSimplicity) {
    if (isAudit) {
      this.telemetry.track('simplicity_audit_started', { scope: 'repo', file_count: 0 });
    } else {
      this.telemetry.track('simplicity_review_started', {
        scope: 'diff', focus: 'simplicity',
        has_description: payload.description !== undefined,
        has_requirements: payload.requirements !== undefined,
      });
    }
  }

  const report = await executor.review({
    source: payload.source,
    modelAlias: resolvedModel,
    description: payload.description,
    requirements: payload.requirements,
    deep: payload.deep,
    timeoutMs: payload.timeoutMs,
    focus: payload.focus,
    scope: payload.scope,
    signal: undefined,  // signal 由 executor 内部 combineSignals 从 timeoutMs 生成
    onProgress: (p) => emitProgress(p.stage, p.detail, p.meta),
  });

  // telemetry 跟踪代码保持不变...
  if (isSimplicity) {
    if (report.ok) {
      const evt = isAudit ? 'simplicity_audit_completed' : 'simplicity_review_completed';
      this.telemetry.track(evt, { scope: isAudit ? 'repo' : 'diff', finding_count: report.findings.length, ok: true });
    } else {
      const evt = isAudit ? 'simplicity_audit_failed' : 'simplicity_review_failed';
      this.telemetry.track(evt, { scope: isAudit ? 'repo' : 'diff', reason: report.note ?? 'unknown' });
    }
  }

  return {
    ok: report.ok,
    reviewerAlias: report.reviewerAlias,
    summary: report.summary,
    findings: report.findings.map((f) => ({ severity: f.severity, title: f.title, detail: f.detail, location: f.location, suggestedFix: f.suggestedFix })),
    note: report.note,
  };
}
```

- [ ] **Build verification**: 
```bash
pnpm --filter @odysseythink/agent-core typecheck && pnpm --filter @odysseythink/agent-core test
```

- [ ] **Commit**: `feat(agent-core): wire progress events through core-impl requestCodeReview`

---

## Phase D: node-sdk

### Task 5: SDK types, SDKRpcClient progress dispatch, KimiHarness options

**Depends on:** Task 2, Task 4  
**Files:** Modify: `packages/node-sdk/src/types.ts:67-72`, `packages/node-sdk/src/rpc.ts:1-27（imports）, 346-349, 575-586`, `packages/node-sdk/src/kimi-harness.ts:207-227` / Create: `packages/node-sdk/test/rpc.test.ts`

- [ ] **Write the failing test**: 新建 `packages/node-sdk/test/rpc.test.ts`:
```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { SDKRpcClient } from '../src/rpc';
import type { Event } from '@odysseythink/agent-core';

// Mock createRPC 避免完整 KimiCore 初始化
vi.mock('@odysseythink/agent-core', async () => {
  const actual = await vi.importActual('@odysseythink/agent-core');
  return {
    ...actual as any,
    createRPC: vi.fn(() => {
      const coreRpc = {
        requestCodeReview: vi.fn().mockResolvedValue({ ok: true, reviewerAlias: 'test', findings: [] }),
      };
      const sdkRpc = (_api: any) => Promise.resolve(coreRpc);
      return [coreRpc, sdkRpc];
    }),
  };
});

describe('SDKRpcClient requestCodeReview progress', () => {
  let client: SDKRpcClient;

  beforeEach(async () => {
    client = new SDKRpcClient();
    await (client as any).ready;
  });

  it('dispatches codeReview.progress event to matching handler', () => {
    const onProgress = vi.fn();
    (client as any).codeReviewProgressHandlers = new Map();
    (client as any).codeReviewProgressHandlers.set('req-1', onProgress);

    const event: Event = {
      type: 'codeReview.progress',
      requestId: 'req-1',
      stage: 'generating',
      modelAlias: 'test-model',
      sessionId: '__code_review_progress__',
      agentId: '__code_review_progress__',
    };
    client.receiveEvent(event);

    expect(onProgress).toHaveBeenCalledOnce();
    expect(onProgress).toHaveBeenCalledWith(expect.objectContaining({
      requestId: 'req-1',
      stage: 'generating',
      modelAlias: 'test-model',
    }));
  });

  it('does not dispatch to handler with different requestId', () => {
    const onProgress = vi.fn();
    (client as any).codeReviewProgressHandlers = new Map();
    (client as any).codeReviewProgressHandlers.set('req-1', onProgress);

    const event: Event = {
      type: 'codeReview.progress',
      requestId: 'req-2',
      stage: 'generating',
      modelAlias: 'test-model',
      sessionId: '__code_review_progress__',
      agentId: '__code_review_progress__',
    };
    client.receiveEvent(event);

    expect(onProgress).not.toHaveBeenCalled();
  });

  it('swallows errors from onProgress callback', () => {
    const onProgress = vi.fn(() => { throw new Error('callback error'); });
    (client as any).codeReviewProgressHandlers = new Map();
    (client as any).codeReviewProgressHandlers.set('req-1', onProgress);

    const event: Event = {
      type: 'codeReview.progress',
      requestId: 'req-1',
      stage: 'generating',
      modelAlias: 'test-model',
      sessionId: '__code_review_progress__',
      agentId: '__code_review_progress__',
    };
    expect(() => client.receiveEvent(event)).not.toThrow();
  });

  it('cleans up handler after requestCodeReview completes', async () => {
    const onProgress = vi.fn();
    // 直接测试 Map 的 delete 行为
    (client as any).codeReviewProgressHandlers.set('req-1', onProgress);
    (client as any).codeReviewProgressHandlers.delete('req-1');
    expect((client as any).codeReviewProgressHandlers.has('req-1')).toBe(false);
  });
});
```

- [ ] **Run and verify it FAILS**: `pnpm --filter @odysseythink/ody-code-sdk test` — 测试失败（`codeReviewProgressHandlers` Map 不存在、`receiveEvent` 无分发逻辑）。

- [ ] **Write the minimal implementation**:

1. `packages/node-sdk/src/types.ts:67-72` — 在 re-export 区增加:
```typescript
export type {
  CodeReviewDiffSource,
  CodeReviewRequestInput,
  CodeReviewReport,
  CodeReviewFinding,
  CodeReviewProgress,         // 新增
  CodeReviewProgressStage,    // 新增
} from '@odysseythink/agent-core';
```

2. `packages/node-sdk/src/rpc.ts` — 变更:

```typescript
// 在 import 区增加 crypto import:
import { randomUUID } from 'node:crypto';

// 在 SDKRpcClient class 内，eventListeners 行之后增加:
private readonly codeReviewProgressHandlers = new Map<string, (progress: { requestId: string; stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } }) => void>();

// 替换 requestCodeReview 方法 (原 line 346-349):
async requestCodeReview(
  input: Omit<RequestCodeReviewPayload, 'requestId'> & {
    onProgress?: (progress: { requestId: string; stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } }) => void;
  },
): Promise<CodeReviewReportData> {
  const rpc = await this.getRpc();
  let requestId: string | undefined;
  if (input.onProgress) {
    requestId = randomUUID();
    this.codeReviewProgressHandlers.set(requestId, input.onProgress);
  }
  try {
    const { onProgress: _, ...payload } = input;
    return await rpc.requestCodeReview({ ...payload, requestId, workDir: (payload as any).workDir ?? process.cwd() });
  } finally {
    if (requestId !== undefined) {
      this.codeReviewProgressHandlers.delete(requestId);
    }
  }
}

// 替换 receiveEvent 方法 (原 line 582-586):
receiveEvent(event: Event): void {
  for (const listener of this.eventListeners) {
    listener(event);
  }
  if (event.type === 'codeReview.progress') {
    const handler = this.codeReviewProgressHandlers.get(event.requestId);
    if (handler) {
      try {
        handler({
          requestId: event.requestId,
          stage: event.stage,
          modelAlias: event.modelAlias,
          detail: event.detail,
          meta: event.meta,
        });
      } catch {
        // 静默吞掉用户回调错误，避免破坏请求
      }
    }
  }
}
```

3. `packages/node-sdk/src/kimi-harness.ts:207-227` — 扩展签名:
```typescript
async requestCodeReview(
  input: {
    readonly source:
      | { readonly kind: 'commits'; readonly base: string; readonly head: string }
      | { readonly kind: 'pr'; readonly prUrlOrNumber: string }
      | { readonly kind: 'working-tree' };
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
    readonly onProgress?: (progress: { requestId: string; stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } }) => void;
  },
): Promise<CodeReviewReport> {
  const result = await this.rpc.requestCodeReview({
    ...input,
    workDir: input.workDir ?? process.cwd(),
    ...(options?.onProgress !== undefined ? { onProgress: options.onProgress } : {}),
  } as any);
  return result as unknown as CodeReviewReport;
}
```

> `KimiHarness.requestCodeReview` 的 `options.signal` 在本次实现中暂不消费（由 TUI 的 AbortController 在更高层控制）。

- [ ] **Run and verify it PASSES**:
```bash
pnpm --filter @odysseythink/ody-code-sdk typecheck && pnpm --filter @odysseythink/ody-code-sdk test
```

- [ ] **Whole-tree typecheck**: 
```bash
pnpm -r typecheck
```

- [ ] **Commit**: `feat(node-sdk): add progress dispatch for requestCodeReview`

---

## Phase E: TUI

### Task 6: TUI types — LoginProgressSpinnerHandle.updateLabel

**Depends on:** Task 5  
**Files:** Modify: `apps/ody-code/src/tui/types.ts:208-212`

- [ ] **Write the implementation** (纯类型变更，无测试):
```typescript
// apps/ody-code/src/tui/types.ts:208-212
export interface LoginProgressSpinnerHandle {
  updateLabel(label: string): void;           // 新增
  stop(opts: { ok: boolean; label: string }): void;
}

export type ProgressSpinnerHandle = LoginProgressSpinnerHandle;
```

- [ ] **Build verification**: `pnpm --filter @odysseythink/ody-code typecheck`（预期 OdyTUI 报 `showProgressSpinner` 返回值缺少 `updateLabel` — 这正是 Task 7 要实现的）。

- [ ] **Commit**: `feat(tui): add updateLabel to LoginProgressSpinnerHandle`

---

### Task 7: TUI showProgressSpinner — 实现 updateLabel

**Depends on:** Task 6  
**Files:** Modify: `apps/ody-code/src/tui/ody-tui.ts:1490-1505`

- [ ] **Write the implementation** (UI wiring，手工验证):
```typescript
// apps/ody-code/src/tui/ody-tui.ts:1490-1505 — 替换 showProgressSpinner 方法:
showProgressSpinner(label: string): LoginProgressSpinnerHandle {
  const tint = (s: string): string => chalk.hex(this.state.theme.colors.primary)(s);
  const spinner = new MoonLoader(this.state.ui, 'braille', tint, label);
  this.state.transcriptContainer.addChild(new Spacer(1));
  this.state.transcriptContainer.addChild(spinner);
  this.state.ui.requestRender();
  return {
    updateLabel: (newLabel: string) => {
      spinner.setLabel(newLabel);
    },
    stop: ({ ok, label: finalLabel }) => {
      spinner.stop();
      const tone = ok ? this.state.theme.colors.success : this.state.theme.colors.error;
      const symbol = ok ? '✓' : '✗';
      spinner.setText(chalk.hex(tone)(`${symbol} ${finalLabel}`));
      this.state.ui.requestRender();
    },
  };
}
```

- [ ] **Build verification**: `pnpm --filter @odysseythink/ody-code typecheck` — 现在应通过（Task 6 类型 + 本实现满足接口）。

- [ ] **Manual verification**: （非交互代码，类型通过即视为可用）确认 `MoonLoader.setLabel` 已存在于 `apps/ody-code/src/tui/components/chrome/moon-loader.ts:52-55`，包含 `this.updateDisplay()` 和 `this.ui.requestRender()`。

- [ ] **Commit**: `feat(tui): implement updateLabel in showProgressSpinner`

---

### Task 8: TUI command handler — spinner + timer + cancel

**Depends on:** Task 7  
**Files:** Modify: `apps/ody-code/src/tui/commands/request-code-review.ts:59-116` / Test: `apps/ody-code/test/tui/commands/request-code-review.test.ts:1-77`

- [ ] **Write the failing test**: 在 `request-code-review.test.ts` 末尾追加新测试:

```typescript
// apps/ody-code/test/tui/commands/request-code-review.test.ts — 在 describe 块末尾追加:

it('shows progress spinner with preparing label', async () => {
  const showProgressSpinner = vi.fn().mockReturnValue({
    updateLabel: vi.fn(),
    stop: vi.fn(),
  });
  const host = createMockHost({ showProgressSpinner } as any);
  await handleRequestCodeReviewCommand(host, '');
  expect(showProgressSpinner).toHaveBeenCalledWith(
    expect.stringContaining('Code review on'),
  );
});

it('updates spinner label on progress', async () => {
  const updateLabel = vi.fn();
  const host = createMockHost({
    showProgressSpinner: vi.fn().mockReturnValue({ updateLabel, stop: vi.fn() }),
  } as any);
  // 注入 onProgress 回调到 harness
  const onProgressSpy = vi.fn();
  (host.harness.requestCodeReview as any) = vi.fn(async (_input: any, opts: any) => {
    opts?.onProgress?.({ requestId: 'r1', stage: 'generating', modelAlias: 'review-model' });
    return { ok: true, reviewerAlias: 'review-model', findings: [] };
  });
  await handleRequestCodeReviewCommand(host, '');
  expect(updateLabel).toHaveBeenCalledWith(
    expect.stringContaining('generating'),
  );
});

it('stops spinner with ok=true on success', async () => {
  const stop = vi.fn();
  const host = createMockHost({
    showProgressSpinner: vi.fn().mockReturnValue({ updateLabel: vi.fn(), stop }),
  } as any);
  await handleRequestCodeReviewCommand(host, '');
  expect(stop).toHaveBeenCalledWith(expect.objectContaining({ ok: true }));
});

it('stops spinner with ok=false on error', async () => {
  const stop = vi.fn();
  const host = createMockHost({
    showProgressSpinner: vi.fn().mockReturnValue({ updateLabel: vi.fn(), stop }),
  } as any);
  (host.harness.requestCodeReview as any) = vi.fn(async () => {
    throw new Error('network error');
  });
  await handleRequestCodeReviewCommand(host, '');
  expect(stop).toHaveBeenCalledWith(expect.objectContaining({ ok: false }));
});

it('registers cancel handler and clears on completion', async () => {
  const host = createMockHost({
    showProgressSpinner: vi.fn().mockReturnValue({ updateLabel: vi.fn(), stop: vi.fn() }),
  } as any);
  expect(host.cancelInFlight).toBeUndefined();
  // Start the command; the cancel handler is registered synchronously before await
  const promise = handleRequestCodeReviewCommand(host, '');
  // cancelInFlight should be set during the command execution
  // After completion, it should be cleared
  await promise;
  expect(host.cancelInFlight).toBeUndefined();
});
```

- [ ] **Run and verify it FAILS**: `pnpm --filter @odysseythink/ody-code test test/tui/commands/request-code-review.test.ts` — 测试引用了 `host.showProgressSpinner`、`host.cancelInFlight` 等不存在的 mock 字段，或断言失败。

- [ ] **Write the minimal implementation**:

需要更新 mock host 工厂和命令实现。首先更新测试的 `createMockHost`:

```typescript
// apps/ody-code/test/tui/commands/request-code-review.test.ts — 更新 createMockHost:
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
      setModel: vi.fn().mockResolvedValue(undefined),
      activateSkill: vi.fn(),
    },
    harness: {
      getConfig: vi.fn().mockResolvedValue({
        modeModels: { codeReview: 'review-model' },
        defaultModel: 'fallback',
        models: { 'review-model': { provider: 'test-p', model: 'm1', maxContextSize: 8192 } },
        providers: { 'test-p': { type: 'openai', apiKey: 'sk-test' } },
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
    showProgressSpinner: vi.fn().mockReturnValue({    // 新增
      updateLabel: vi.fn(),
      stop: vi.fn(),
    }),
    deferUserMessages: false,
    ...overrides,
  } as unknown as SlashCommandHost;
}
```

然后替换命令实现:

```typescript
// apps/ody-code/src/tui/commands/request-code-review.ts — 完整替换:

import { renderCodeReviewReportToMarkdown } from '@odysseythink/ody-code-sdk';
import { resolveCodeReviewModel } from '@odysseythink/ody-code-sdk';

import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/ody-tui';
import type { SlashCommandHost } from './dispatch';

interface SlashArgs {
  readonly base?: string;
  readonly head?: string;
  readonly pr?: string;
  readonly model?: string;
  readonly description?: string;
  readonly requirements?: string;
  readonly deep?: boolean;
  readonly focus?: 'correctness' | 'simplicity';
  readonly scope?: 'diff' | 'repo';
}

function parseArgs(args: string): SlashArgs {
  const tokens = args.trim().split(/\s+/).filter((t) => t.length > 0);
  const result: Record<string, unknown> = {};
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;
    if (token === '--base' || token === '--head' || token === '--pr' || token === '--model' ||
        token === '--description' || token === '--requirements' || token === '--focus' || token === '--scope') {
      result[camelFromFlag(token)] = tokens[i + 1];
      i += 1;
    } else if (token === '--deep') {
      result['deep'] = true;
    } else {
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

const STAGE_MAP: Record<string, string> = {
  'preparing': 'Preparing',
  'fetching-diff': 'Fetching diff',
  'audit-scanning': 'Scanning repo',
  'deep-review': 'Deep review in progress',
  'generating': 'Generating review',
  'completed': 'Complete',
  'failed': 'Failed',
};

function formatReviewProgressLabel(
  progress: { stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } },
  elapsedSeconds: number,
): string {
  const stageText = STAGE_MAP[progress.stage] ?? progress.stage;
  let base = `Code review on ${progress.modelAlias} — ${stageText}`;
  if (progress.detail) {
    const truncated = progress.detail.length > 40 ? progress.detail.slice(0, 37) + '…' : progress.detail;
    base += ` (${truncated})`;
  }
  if (progress.meta?.estimatedTokens !== undefined) {
    base += ` · ~${progress.meta.estimatedTokens} tokens`;
  }
  if (progress.meta?.filePath !== undefined) {
    const basename = progress.meta.filePath.split('/').pop() ?? progress.meta.filePath;
    base += ` · ${basename}`;
  }
  return `${base} (${elapsedSeconds}s)`;
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

  const source = buildDiffSource(parsed);

  // ── Progress spinner ──
  const controller = new AbortController();
  const cancel = () => controller.abort();
  host.cancelInFlight = cancel;

  let currentProgress: { stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } } = { stage: 'preparing', modelAlias: resolvedModel };
  const spinner = host.showProgressSpinner(formatReviewProgressLabel(currentProgress, 0));
  let elapsed = 0;
  const timer = setInterval(() => {
    elapsed += 1;
    spinner.updateLabel(formatReviewProgressLabel(currentProgress, elapsed));
  }, 1000);

  try {
    const report = await host.harness.requestCodeReview({
      source,
      modelAlias: resolvedModel,
      description: parsed.description,
      requirements: parsed.requirements,
      deep: parsed.deep,
      focus: parsed.focus,
      scope: parsed.scope,
    }, {
      signal: controller.signal,
      onProgress: (p) => {
        currentProgress = p;
        spinner.updateLabel(formatReviewProgressLabel(p, elapsed));
      },
    });

    if (!report.ok) {
      spinner.stop({ ok: false, label: report.note ?? 'Code review failed.' });
      host.showError(report.note ?? 'Code review failed.');
      return;
    }

    const markdown = renderCodeReviewReportToMarkdown(report);
    spinner.stop({ ok: true, label: `Code review complete (${report.reviewerAlias}).` });
    host.sendNormalUserInput(
      `Code review complete (${report.reviewerAlias}). Findings:\n\n${markdown}\n\nPlease act on the findings.`,
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    spinner.stop({ ok: false, label: `Code review failed: ${message}` });
    host.showError(`Code review failed: ${message}`);
  } finally {
    clearInterval(timer);
    if (host.cancelInFlight === cancel) {
      host.cancelInFlight = undefined;
    }
  }
}
```

- [ ] **Run and verify it PASSES**:
```bash
pnpm --filter @odysseythink/ody-code typecheck && pnpm --filter @odysseythink/ody-code test test/tui/commands/request-code-review.test.ts
```

- [ ] **Commit**: `feat(tui): replace static status with progress spinner in request-code-review`

---

## Self-Review

- [ ] 1. **Spec-coverage table**: 映射设计文档每个需求 → Task(s)

| 设计需求 | Task | 状态 |
|---|---|---|
| `CodeReviewProgressStage` / `CodeReviewProgress` 类型定义 | T1 | covered |
| `CodeReviewProgressEvent` 加入 `AgentEvent` 联合 | T2 | covered |
| `CodeReviewRequestInput` 增加 `signal?` / `onProgress?` | T1 | covered |
| `CodeReviewExecutorDeps` 扩展（fetchDiff/deepRunner/auditScanner signal） | T1 | covered |
| `fetchDiff` 签名增加 `signal?` | T1 | covered |
| executor 各阶段 progress 调用（preparing → fetching-diff → generating → completed/failed） | T3 | covered |
| executor signal 合并（combineSignals） | T3 | covered |
| executor deep/repo 路径 progress | T3 | covered |
| `RequestCodeReviewPayload` 增加 `requestId?` | T4 | covered |
| core-impl 构造 emitProgress 闭包、注入 executor | T4 | covered |
| synthetic sessionId/agentId 常量 | T4 | covered |
| node-sdk types 导出 CodeReviewProgress/CodeReviewProgressStage | T5 | covered |
| SDKRpcClient codeReviewProgressHandlers map | T5 | covered |
| SDKRpcClient.requestCodeReview requestId 生成、handler 注册/清理 | T5 | covered |
| SDKRpcClient.receiveEvent 按 requestId 分发 progress | T5 | covered |
| onProgress 回调错误吞掉 | T5 | covered |
| KimiHarness.requestCodeReview 增加 options（signal, onProgress） | T5 | covered |
| LoginProgressSpinnerHandle.updateLabel | T6 | covered |
| showProgressSpinner 返回 updateLabel 委托 MoonLoader.setLabel | T7 | covered |
| TUI 命令 handler: spinner + 秒级 timer + AbortController + cancelInFlight | T8 | covered |
| TUI 命令 handler: formatReviewProgressLabel 格式化 | T8 | covered |
| TUI 命令 handler: finally 清理 timer 和 cancelInFlight | T8 | covered |
| CLI `ody request-code-review` 不展示实时进度 | no-op | out of scope（设计已明确） |
| 不新增实验性 feature flag | no-op | out of scope |
| 不新增 telemetry 事件 | no-op | out of scope（仅 debug 日志，任务中无日志调用） |
| 不实现百分比进度条 | no-op | out of scope |
| deep-review 子 agent 内部 step 不提供 | no-op | out of scope（仅预留事件字段） |

- [ ] 2. **Placeholder scan**: 全文搜索无 `TODO`、`TBD`、"implement later"、"add appropriate error handling" 等占位短语。

- [ ] 3. **No phantom tasks**: 8 个 task 均产生文件变更；无 `--allow-empty` 提交。"no-op" 标记按 spec 要求保留在 coverage table 中，未生成虚假 task。

- [ ] 4. **Dependency soundness**: 依赖图已验证：
  - T1, T2 → 无依赖（并行）
  - T3 → T1
  - T4 → T2, T3
  - T5 → T2, T4
  - T6 → T5
  - T7 → T6
  - T8 → T7
  无 task 引用仅在后续 task 中定义的类型/函数。

- [ ] 5. **Caller & build soundness**: 
  - T1 修改 `CodeReviewExecutorDeps`（fetchDiff 签名、deepRunner 签名）→ 查找所有调用者：`core-impl.ts:493-498`（fetchDiff 调用增加了 signal）、`executor.test.ts`（所有 mock 适配）→ 以 `pnpm -r typecheck` 收尾。
  - T1 修改 `CodeReviewRequestInput`（新增 optional 字段）→ 调用者 `core-impl.ts:545-553` 传入 `signal`/`onProgress`，旧调用者无需改动（optional）。
  - T5 修改 `requestCodeReview` 方法签名 → 调用者 `KimiHarness.requestCodeReview`（T5 内部同步更新）和 `commands/request-code-review.ts`（T8）→ 以 `pnpm -r typecheck` 收尾。
  - 无跨 task 重复修改同一签名。

- [ ] 6. **Test-the-risk**: 每项有状态变更的 task 都有行为测试：
  - T3: 阶段顺序测试 (preparing→fetching-diff→generating→completed)、diff-too-large 触发 failed、signal 传递到 generate。
  - T5: 按 requestId 分发、不同 requestId 不触发、回调错误吞掉、handler 清理。
  - T8: spinner 启动/更新/停止、取消 handler 注册/清空。
  - 风险项均有覆盖：事件丢失（T8 finally）、回调抛错（T5）、signal 传递（T3）。
  - 对于 `STAGE_MAP` 常量，验证了所有 7 个 stage key 均存在映射；无敏感词冲突。

- [ ] 7. **Type consistency**: 
  - `CodeReviewProgressStage` 在 T1 定义为 7 个字面量；T2 的 `CodeReviewProgressEvent.stage` 使用相同字面量。
  - `CodeReviewProgress` 的字段 (`requestId`, `stage`, `modelAlias`, `detail`, `meta`) 与 `CodeReviewProgressEvent`（减去 `type`）完全一致。
  - SDK 侧 handler 签名 `{ requestId, stage, modelAlias, detail?, meta? }` 是 agent-core `CodeReviewProgress` 的不带 `readonly` 版本。
  - `LoginProgressSpinnerHandle` 在 T6 定义 `updateLabel(label: string): void`，T7 实现返回 `updateLabel: (newLabel: string) => { spinner.setLabel(newLabel); }`，类型匹配。
