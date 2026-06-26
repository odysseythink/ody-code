# Design Mode 切回时自动提示继续未完成的设计 — Implementation Plan

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use `- [ ]` checkboxes for tracking.

**Goal:** 当用户通过 `/design` 重新进入 design mode 时，自动扫描 checkpoint 中记录的已批准 split index，提示用户选择要继续的 pending part，并将选中的 index 锁定为当前设计文件；首次提醒中追加“继续写该 part”的指令。

**Architecture:** 在 `Agent.enterPlan` 的 design 分支中插入内部 `ResumeDesignMode` 对象，负责扫描 `SessionMode.designSessions` 的 `approvedPath`、解析 Parts manifest、通过 `requestQuestion` 完成两次选择；选择结果写入 `SessionMode` 的临时 `targetPendingPart` 状态，并作为 `initialFilePath` 传入 `SessionMode.enter` 锁定文件路径；`DesignModeInjector` 在首次 re-entry reminder 中读取该状态并追加 resume directive，随后消费掉该状态。

**Tech Stack:** TypeScript, Vitest, pnpm workspace monorepo（改动集中在 `packages/agent-core`）

---

## File Structure

| 文件 | 职责 |
|---|---|
| `packages/agent-core/src/agent/injection/design-mode-contract.ts` | 新增 `designResumePartDirective` 提示文本构建函数 |
| `packages/agent-core/src/agent/injection/parts-manifest.ts` | 导出 `scanManifestRows`，供 `ResumeDesignMode` 获取所有 pending rows |
| `packages/agent-core/src/tools/builtin/planning/resume-design-mode.ts` | 新建内部 `ResumeDesignMode` 类（不注册为 LLM tool） |
| `packages/agent-core/src/agent/session-mode/index.ts` | 新增 `_targetPendingPart` 状态、`getTargetPendingPart`/`setTargetPendingPart`、`enter(initialFilePath?)` 签名扩展 |
| `packages/agent-core/src/agent/index.ts` | `enterPlan` design 分支插入扫描、选择、telemetry 逻辑 |
| `packages/agent-core/src/agent/injection/design-mode.ts` | `getInjection` 读取 `targetPendingPart` 并追加 resume directive |
| `packages/agent-core/test/tools/resume-design-mode.test.ts` | 新增 `ResumeDesignMode` 单元测试 |
| `packages/agent-core/test/agent/session-mode.test.ts` | 新增 `targetPendingPart` 与 `initialFilePath` 行为测试 |
| `packages/agent-core/test/agent/plan.test.ts` | 新增 `enterPlan({ kind: 'design' })` 扫描/选择/降级测试 |
| `packages/agent-core/test/agent/injection/design-mode.test.ts` | 扩展 injector 测试，验证 resume directive 注入与消费 |

---

## Dependency Overview

```
Task 1 (resume directive contract)
         │
         ▼
Task 5 (injector uses directive)

Task 2 (ResumeDesignMode class + parts-manifest export)
         │
         ▼
Task 4 (Agent.enterPlan uses ResumeDesignMode)

Task 3 (SessionMode target + initialFilePath)
         │
         ├──► Task 4 (enter passes initialFilePath)
         └──► Task 5 (injector reads target)

Task 4 ──► Task 6 (whole-tree verification)
Task 5 ──► Task 6 (whole-tree verification)
```

- **Task 1 / Task 2 / Task 3 可并行**：三者分别位于 contract、tools/session-mode，互不依赖。
- **Task 4 依赖 Task 2 与 Task 3**：`Agent.enterPlan` 需要 `ResumeDesignMode` 类和 `SessionMode.enter(initialFilePath)`。
- **Task 5 依赖 Task 1 与 Task 3**：`DesignModeInjector` 需要 directive 函数和 `SessionMode` 的 target API。
- **Task 6 依赖 Task 4 与 Task 5**：最终跑全量 typecheck 与指定测试。

---

## Task 1: 新增 `designResumePartDirective` 提示文本

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/injection/design-mode-contract.ts:189-193` 附近
- Test: `packages/agent-core/test/agent/injection/design-mode.test.ts`

- [ ] **Step 1: 写失败测试**

在 `packages/agent-core/test/agent/injection/design-mode.test.ts` 的 `describe('DesignModeInjector content', ...)` 同级新增：

```ts
import { designResumePartDirective } from '../../../src/agent/injection/design-mode-contract';

describe('designResumePartDirective', () => {
  it('points the model at the selected pending part inside the index subdirectory', () => {
    const directive = designResumePartDirective('api.md', '2026-06-12-my-design');
    expect(directive).toContain('Continue split design');
    expect(directive).toContain('api.md');
    expect(directive).toContain('2026-06-12-my-design.md');
    expect(directive).toContain('2026-06-12-my-design/api.md');
    expect(directive).toContain('Do NOT rewrite already-done parts');
  });
});
```

- [ ] **Step 2: 运行并确认失败**

```bash
cd packages/agent-core
pnpm test -- test/agent/injection/design-mode.test.ts
```

预期失败：`designResumePartDirective is not exported` 或类似错误。

- [ ] **Step 3: 最小实现**

在 `packages/agent-core/src/agent/injection/design-mode-contract.ts` 中 `designSplitFinalReviewDirective` 之后新增：

```ts
/** Directive appended on design-mode re-entry when the user explicitly selected a pending part to resume. */
export function designResumePartDirective(partName: string, indexStem: string): string {
  const target = `${indexStem}/${partName}`;
  return `## Continue split design — resume pending part
The user selected to continue designing \`${partName}\`. Read the index \`${indexStem}.md\` and write ONLY the part file \`${target}\` this turn. Do NOT rewrite already-done parts.`;
}
```

- [ ] **Step 4: 运行并确认通过**

```bash
cd packages/agent-core
pnpm test -- test/agent/injection/design-mode.test.ts
```

预期：所有测试通过。

- [ ] **Step 5: 提交**

```bash
git add packages/agent-core/src/agent/injection/design-mode-contract.ts packages/agent-core/test/agent/injection/design-mode.test.ts
git commit -m "feat(agent-core): add designResumePartDirective for resuming pending split-design part"
```

---

## Task 2: 实现内部 `ResumeDesignMode` 类

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/injection/parts-manifest.ts`
- Create: `packages/agent-core/src/tools/builtin/planning/resume-design-mode.ts`
- Test: `packages/agent-core/test/tools/resume-design-mode.test.ts`

- [ ] **Step 1: 导出 `scanManifestRows`**

`ResumeDesignMode` 需要拿到所有 pending rows，而 `parsePartsManifest` 只返回第一个 pending。修改 `packages/agent-core/src/agent/injection/parts-manifest.ts`：

将 private 的 `ManifestRow` 接口与 `scanManifestRows` 改为导出：

```ts
export interface ManifestRow {
  readonly file: string;
  readonly scope: string;
  readonly status: 'pending' | 'done';
}

export function scanManifestRows(content: string): ManifestRow[] {
  // 保持现有实现不变
}
```

- [ ] **Step 2: 写失败测试**

新建 `packages/agent-core/test/tools/resume-design-mode.test.ts`：

```ts
import { describe, expect, it, vi } from 'vitest';

import { ResumeDesignMode } from '../../src/tools/builtin/planning/resume-design-mode';
import type { Agent } from '../../src/agent';
import type { QuestionRequest, QuestionResult } from '../../src/rpc';

function makeAgent(overrides: {
  designSessions?: { approvedPath?: string }[];
  files?: Record<string, string>;
} = {}): Agent {
  const sessions = overrides.designSessions ?? [];
  const files = overrides.files ?? {};
  return {
    sessionMode: {
      designSessions: sessions.map((s) => ({
        designSessionID: 'id',
        startedAtMsg: 0,
        exitedAtMsg: 1,
        approvedPath: s.approvedPath,
      })),
    },
    kaos: {
      readText: vi.fn(async (path: string) => {
        if (files[path] !== undefined) return files[path];
        throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
      }),
    },
  } as unknown as Agent;
}

function requestQuestion(results: QuestionResult[]): (req: QuestionRequest) => Promise<QuestionResult> {
  let i = 0;
  return async () => {
    const result = results[i];
    i++;
    if (result === undefined) throw new Error('unexpected requestQuestion call');
    return result;
  };
}

describe('ResumeDesignMode.scanPendingDesigns', () => {
  it('returns indexes that have pending parts and skips done-only indexes', async () => {
    const indexA = '/workspace/.ody-code/designs/2026-06-12-a.md';
    const indexB = '/workspace/.ody-code/designs/2026-06-12-b.md';
    const agent = makeAgent({
      designSessions: [{ approvedPath: indexA }, { approvedPath: indexB }],
      files: {
        [indexA]: `## Parts
| # | File | Scope | Status |
|---|------|-------|--------|
| 1 | phase1.md | core | done |
| 2 | phase2.md | api | pending |`,
        [indexB]: `## Parts
| # | File | Scope | Status |
|---|------|-------|--------|
| 1 | phase1.md | core | done |`,
      },
    });
    const resume = new ResumeDesignMode({ agent, requestQuestion: async () => null });
    const pending = await resume.scanPendingDesigns();
    expect(pending).toHaveLength(1);
    expect(pending[0].indexPath).toBe(indexA);
    expect(pending[0].pendingParts).toEqual([{ file: 'phase2.md', scope: 'api' }]);
  });

  it('deduplicates repeated approved paths', async () => {
    const indexA = '/workspace/.ody-code/designs/2026-06-12-a.md';
    const agent = makeAgent({
      designSessions: [{ approvedPath: indexA }, { approvedPath: indexA }],
      files: {
        [indexA]: `## Parts
| 1 | phase1.md | core | pending |`,
      },
    });
    const resume = new ResumeDesignMode({ agent, requestQuestion: async () => null });
    const pending = await resume.scanPendingDesigns();
    expect(pending).toHaveLength(1);
  });

  it('skips missing files and indexes without a manifest', async () => {
    const agent = makeAgent({
      designSessions: [{ approvedPath: '/missing.md' }, { approvedPath: '/plain.md' }],
      files: { '/plain.md': '# No manifest here' },
    });
    const resume = new ResumeDesignMode({ agent, requestQuestion: async () => null });
    expect(await resume.scanPendingDesigns()).toEqual([]);
  });
});

describe('ResumeDesignMode.promptForPendingPart', () => {
  it('skips questions when only one index and one part exist', async () => {
    const agent = makeAgent();
    const resume = new ResumeDesignMode({ agent, requestQuestion: async () => null });
    const selected = await resume.promptForPendingPart([
      { indexPath: '/a.md', pendingParts: [{ file: 'p1.md', scope: 's1' }] },
    ]);
    expect(selected).toEqual({ indexPath: '/a.md', partName: 'p1.md' });
  });

  it('asks for index then part and returns the selection', async () => {
    const agent = makeAgent();
    const resume = new ResumeDesignMode({
      agent,
      requestQuestion: requestQuestion([
        { answers: { '0': '1' } },
        { answers: { '0': '0' } },
      ]),
    });
    const selected = await resume.promptForPendingPart([
      { indexPath: '/a.md', pendingParts: [{ file: 'p1.md', scope: 's1' }] },
      { indexPath: '/b.md', pendingParts: [{ file: 'p2.md', scope: 's2' }, { file: 'p3.md', scope: 's3' }] },
    ]);
    expect(selected).toEqual({ indexPath: '/b.md', partName: 'p2.md' });
  });

  it('returns null when the user dismisses', async () => {
    const agent = makeAgent();
    const resume = new ResumeDesignMode({
      agent,
      requestQuestion: requestQuestion([null]),
    });
    const selected = await resume.promptForPendingPart([
      { indexPath: '/a.md', pendingParts: [{ file: 'p1.md', scope: 's1' }, { file: 'p2.md', scope: 's2' }] },
    ]);
    expect(selected).toBeNull();
  });
});
```

- [ ] **Step 3: 运行并确认失败**

```bash
cd packages/agent-core
pnpm test -- test/tools/resume-design-mode.test.ts
```

预期失败：模块不存在 / `scanManifestRows` 未导出。

- [ ] **Step 4: 最小实现**

新建 `packages/agent-core/src/tools/builtin/planning/resume-design-mode.ts`：

```ts
import { basename } from 'pathe';

import type { Agent } from '../../../agent';
import type { QuestionRequest, QuestionResult } from '../../../rpc';
import { scanManifestRows } from '../../../agent/injection/parts-manifest';

export interface PendingPart {
  readonly file: string;
  readonly scope: string;
}

export interface PendingDesignIndex {
  readonly indexPath: string;
  readonly pendingParts: readonly PendingPart[];
}

export interface ResumeDesignModeContext {
  readonly agent: Agent;
  readonly requestQuestion: (request: QuestionRequest) => Promise<QuestionResult>;
}

export class ResumeDesignMode {
  constructor(private readonly ctx: ResumeDesignModeContext) {}

  async scanPendingDesigns(): Promise<PendingDesignIndex[]> {
    const sessions = this.ctx.agent.sessionMode.designSessions;
    const seen = new Set<string>();
    const results: PendingDesignIndex[] = [];

    for (const session of sessions) {
      const path = session.approvedPath;
      if (path === undefined || path.length === 0) continue;
      if (seen.has(path)) continue;
      seen.add(path);

      let content: string;
      try {
        content = await this.ctx.agent.kaos.readText(path);
      } catch {
        continue;
      }

      const pendingParts = this.pendingPartsFrom(content);
      if (pendingParts.length === 0) continue;

      results.push({ indexPath: path, pendingParts });
    }

    return results;
  }

  async promptForPendingPart(
    pendingIndexes: readonly PendingDesignIndex[],
  ): Promise<{ indexPath: string; partName: string } | null> {
    const selectedIndex = await this.selectIndex(pendingIndexes);
    if (selectedIndex === null) return null;

    const selectedPart = await this.selectPart(selectedIndex);
    if (selectedPart === null) return null;

    return { indexPath: selectedIndex.indexPath, partName: selectedPart.file };
  }

  private async selectIndex(
    pendingIndexes: readonly PendingDesignIndex[],
  ): Promise<PendingDesignIndex | null> {
    if (pendingIndexes.length === 1) return pendingIndexes[0];

    const answer = await this.ctx.requestQuestion({
      questions: [{
        question: 'Which design do you want to continue?',
        options: pendingIndexes.map((i) => ({
          label: basename(i.indexPath),
          description: `${i.pendingParts.length} pending part(s)`,
        })),
      }],
    });

    const normalized = normalizeAnswer(answer);
    if (normalized === null) return null;

    const selected = pendingIndexes[Number(normalized)];
    return selected ?? null;
  }

  private async selectPart(
    index: PendingDesignIndex,
  ): Promise<PendingPart | null> {
    if (index.pendingParts.length === 1) return index.pendingParts[0];

    const answer = await this.ctx.requestQuestion({
      questions: [{
        question: `Which part of ${basename(index.indexPath)} do you want to design?`,
        options: index.pendingParts.map((p) => ({
          label: p.file,
          description: p.scope,
        })),
      }],
    });

    const normalized = normalizeAnswer(answer);
    if (normalized === null) return null;

    const selected = index.pendingParts[Number(normalized)];
    return selected ?? null;
  }

  private pendingPartsFrom(content: string): PendingPart[] {
    return scanManifestRows(content)
      .filter((row) => row.status === 'pending')
      .map((row) => ({ file: basename(row.file), scope: row.scope }));
  }
}

function normalizeAnswer(result: QuestionResult): string | null {
  if (result === null) return null;
  const answers = typeof result === 'object' && 'answers' in result ? result.answers : result;
  if (answers === null || typeof answers !== 'object' || Array.isArray(answers)) return null;
  const keys = Object.keys(answers);
  if (keys.length === 0) return null;
  const value = answers[keys[0]];
  return typeof value === 'string' ? value : null;
}
```

- [ ] **Step 5: 运行并确认通过**

```bash
cd packages/agent-core
pnpm test -- test/tools/resume-design-mode.test.ts
```

预期：全部通过。

- [ ] **Step 6: 提交**

```bash
git add packages/agent-core/src/agent/injection/parts-manifest.ts packages/agent-core/src/tools/builtin/planning/resume-design-mode.ts packages/agent-core/test/tools/resume-design-mode.test.ts
git commit -m "feat(agent-core): add ResumeDesignMode internal helper to scan and prompt for pending split-design parts"
```

---

## Task 3: 扩展 `SessionMode` 支持 target pending part 与初始文件路径

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:38-43`, `:57-127`, `:161-183`, `:198-223`
- Modify: `packages/agent-core/test/agent/injection/design-mode.test.ts:30-45`（stub 补方法）
- Test: `packages/agent-core/test/agent/session-mode.test.ts`

- [ ] **Step 1: 写失败测试**

在 `packages/agent-core/test/agent/session-mode.test.ts` 的 `describe('SessionMode', ...)` 内新增：

```ts
  describe('targetPendingPart', () => {
    it('stores and returns the target pending part', () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.setTargetPendingPart('/workspace/.ody-code/designs/idx.md', 'part.md');
      expect(sm.getTargetPendingPart()).toEqual({
        indexPath: '/workspace/.ody-code/designs/idx.md',
        partName: 'part.md',
      });
    });

    it('rejects empty indexPath or partName', () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      expect(() => sm.setTargetPendingPart('', 'part.md')).toThrow();
      expect(() => sm.setTargetPendingPart('/idx.md', '')).toThrow();
    });

    it('clears target on enter, cancel, and exit', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.setTargetPendingPart('/idx.md', 'part.md');
      await sm.enter('id', false, false, 'design');
      expect(sm.getTargetPendingPart()).toBeNull();

      sm.setTargetPendingPart('/idx.md', 'part.md');
      sm.cancel('id');
      expect(sm.getTargetPendingPart()).toBeNull();

      await sm.enter('id2', false, false, 'design');
      sm.setTargetPendingPart('/idx.md', 'part.md');
      sm.exit('id2');
      expect(sm.getTargetPendingPart()).toBeNull();
    });

    it('consumes target on read', () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.setTargetPendingPart('/idx.md', 'part.md');
      expect(sm.consumeTargetPendingPart()).toEqual({ indexPath: '/idx.md', partName: 'part.md' });
      expect(sm.getTargetPendingPart()).toBeNull();
    });
  });

  describe('enter with initialFilePath', () => {
    it('locks the file path immediately when initialFilePath is provided', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id', false, false, 'design', '/workspace/.ody-code/designs/existing-index.md');
      expect(sm.sessionModeFilePath).toBe('/workspace/.ody-code/designs/existing-index.md');
    });

    it('keeps lazy path resolution when initialFilePath is omitted', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id', false, false, 'design');
      expect(sm.sessionModeFilePath).toBeNull();
    });
  });
```

- [ ] **Step 2: 运行并确认失败**

```bash
cd packages/agent-core
pnpm test -- test/agent/session-mode.test.ts
```

预期失败：`setTargetPendingPart` / `getTargetPendingPart` / `enter(initialFilePath?)` 不存在。

- [ ] **Step 3: 最小实现**

修改 `packages/agent-core/src/agent/session-mode/index.ts`：

1. 在 `private _designSessions` 之后新增状态：

```ts
  private _targetPendingPart: { indexPath: string; partName: string } | null = null;
```

2. 扩展 `enter` 签名并在进入时锁定路径：

```ts
  async enter(
    id = this.createSessionModeId(),
    _createFile = false,
    emitStatus = true,
    kind: SessionModeKind = 'plan',
    initialFilePath?: string,
  ): Promise<void> {
    if (this._isActive) {
      if (this._kind === kind) {
        return;
      }
      this.exit();
    }

    this._targetPendingPart = null;
    this._isActive = true;
    this._sessionModeId = id;
    this._kind = kind;
    this._sessionModeFilePath =
      initialFilePath !== undefined && initialFilePath.length > 0 ? initialFilePath : null;

    if (kind === 'design') {
      this.startDesignSession(id);
    }
    // ... 后续逻辑保持不变
```

3. 在 `cancel()` 中于 `this._kind === 'design'` 判断之前加入：

```ts
    this._targetPendingPart = null;
```

4. 在 `exit()` 中于 `this._kind === 'design'` 判断之前加入：

```ts
    this._targetPendingPart = null;
```

5. 在类末尾（`findUniqueStem` 之前）新增 getter/setter：

```ts
  getTargetPendingPart(): { indexPath: string; partName: string } | null {
    return this._targetPendingPart;
  }

  setTargetPendingPart(indexPath: string, partName: string): void {
    if (indexPath.length === 0 || partName.length === 0) {
      throw new Error('targetPendingPart indexPath and partName must be non-empty');
    }
    this._targetPendingPart = { indexPath, partName };
  }

  consumeTargetPendingPart(): { indexPath: string; partName: string } | null {
    const target = this._targetPendingPart;
    this._targetPendingPart = null;
    return target;
  }
```

- [ ] **Step 4: 更新 `DesignModeInjector` 测试 stub**

`packages/agent-core/test/agent/injection/design-mode.test.ts` 中的 `sessionMode` stub 必须暴露新方法，否则后续 Task 5 编译失败。将 `sessionMode` stub 中的对应部分替换为：

```ts
      getTargetPendingPart: () => null,
      consumeTargetPendingPart: () => null,
```

（`setTargetPendingPart` 只在 `Agent.enterPlan` 中调用，injector 测试 stub 不调用，但为类型安全可一并加上 `setTargetPendingPart: () => {}`。）

- [ ] **Step 5: 运行 SessionMode 测试并确认通过**

```bash
cd packages/agent-core
pnpm test -- test/agent/session-mode.test.ts test/agent/injection/design-mode.test.ts
```

预期：全部通过。

- [ ] **Step 6: 搜索所有 `SessionMode.enter` 调用并确认无需改动**

```bash
cd /Users/ranwei/workspace/ody-code
rg -n "\.enter\(" packages/agent-core/src packages/agent-core/test --type ts
```

预期：调用点均使用位置参数，新增的 `initialFilePath` 是可选的，因此无需修改；`Agent.enterPlan` 将在 Task 4 中传入该参数。

- [ ] **Step 7: 全项目 typecheck**

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r typecheck
```

预期：无编译错误。

- [ ] **Step 8: 提交**

```bash
git add packages/agent-core/src/agent/session-mode/index.ts packages/agent-core/test/agent/session-mode.test.ts packages/agent-core/test/agent/injection/design-mode.test.ts
git commit -m "feat(agent-core): add targetPendingPart state and initialFilePath to SessionMode"
```

---

## Task 4: 在 `Agent.enterPlan` design 分支集成扫描与选择

**Depends on:** Task 2, Task 3

**Files:**
- Modify: `packages/agent-core/src/agent/index.ts:448-473`
- Test: `packages/agent-core/test/agent/plan.test.ts`

- [ ] **Step 1: 写失败测试**

在 `packages/agent-core/test/agent/plan.test.ts` 新增 describe：

```ts
import { ResumeDesignMode } from '../../src/tools/builtin/planning/resume-design-mode';

// 在文件合适位置新增
describe('enterPlan design mode resume', () => {
  it('scans pending designs, prompts, sets target, and enters with initialFilePath', async () => {
    const ctx = testAgent({
      kaos: createFakeKaos({
        mkdir: vi.fn().mockResolvedValue(undefined),
        readText: vi.fn(async (path: string) => {
          if (path === '/workspace/.ody-code/designs/idx.md') {
            return `## Parts\n| 1 | part1.md | core | pending |`;
          }
          throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
        }),
      }),
    });

    // Seed a design session with approvedPath.
    (ctx.agent.sessionMode as unknown as { _designSessions: unknown[] })._designSessions = [{
      designSessionID: 'prev',
      startedAtMsg: 0,
      exitedAtMsg: 1,
      approvedPath: '/workspace/.ody-code/designs/idx.md',
    }];

    const requestQuestion = vi.fn(async () => ({ answers: { '0': '0' } }));
    ctx.agent.rpc = { requestQuestion };

    const setTargetSpy = vi.spyOn(ctx.agent.sessionMode, 'setTargetPendingPart');
    const enterSpy = vi.spyOn(ctx.agent.sessionMode, 'enter');

    await ctx.rpc.enterPlan({ kind: 'design' });
    await delay(10);

    expect(requestQuestion).toHaveBeenCalledTimes(1);
    expect(setTargetSpy).toHaveBeenCalledWith('/workspace/.ody-code/designs/idx.md', 'part1.md');
    expect(enterSpy).toHaveBeenCalledWith(
      expect.any(String),
      false,
      true,
      'design',
      '/workspace/.ody-code/designs/idx.md',
    );
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBe('/workspace/.ody-code/designs/idx.md');
  });

  it('falls back to blank design when no pending designs exist', async () => {
    const ctx = testAgent({ kaos: createFakeKaos() });
    const requestQuestion = vi.fn(async () => ({ answers: { '0': '0' } }));
    ctx.agent.rpc = { requestQuestion };

    const enterSpy = vi.spyOn(ctx.agent.sessionMode, 'enter');

    await ctx.rpc.enterPlan({ kind: 'design' });
    await delay(10);

    expect(requestQuestion).not.toHaveBeenCalled();
    expect(enterSpy).toHaveBeenCalledWith(expect.any(String), false, true, 'design', undefined);
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();
  });

  it('falls back to blank design when requestQuestion is unavailable', async () => {
    const ctx = testAgent({
      kaos: createFakeKaos({
        readText: vi.fn(async () => `## Parts\n| 1 | part1.md | core | pending |`),
      }),
    });
    (ctx.agent.sessionMode as unknown as { _designSessions: unknown[] })._designSessions = [{
      designSessionID: 'prev',
      startedAtMsg: 0,
      exitedAtMsg: 1,
      approvedPath: '/workspace/.ody-code/designs/idx.md',
    }];

    const enterSpy = vi.spyOn(ctx.agent.sessionMode, 'enter');

    await ctx.rpc.enterPlan({ kind: 'design' });
    await delay(10);

    expect(enterSpy).toHaveBeenCalledWith(expect.any(String), false, true, 'design', undefined);
  });
});
```

- [ ] **Step 2: 运行并确认失败**

```bash
cd packages/agent-core
pnpm test -- test/agent/plan.test.ts
```

预期失败：`ResumeDesignMode` 未导入 / `enterPlan` 未按 design 分支处理。

- [ ] **Step 3: 最小实现**

在 `packages/agent-core/src/agent/index.ts` 顶部导入：

```ts
import { ResumeDesignMode } from '../tools/builtin/planning/resume-design-mode';
```

替换 `enterPlan: async (payload) => { ... }`（当前 `packages/agent-core/src/agent/index.ts:448-473`）：

```ts
      enterPlan: async (payload) => {
        if (payload.kind === 'design') {
          const resume = new ResumeDesignMode({
            agent: this,
            requestQuestion: (req) => this.rpc!.requestQuestion!(req),
          });
          const pending = await resume.scanPendingDesigns();
          const selected =
            pending.length > 0 && this.rpc?.requestQuestion !== undefined
              ? await resume.promptForPendingPart(pending)
              : null;
          if (selected !== null) {
            this.sessionMode.setTargetPendingPart(selected.indexPath, selected.partName);
          }
          await this.sessionMode.enter(
            this.sessionMode.createSessionModeId(),
            false,
            true,
            'design',
            selected?.indexPath,
          );
          this.telemetry.track('design_enter_resolved', {
            outcome: 'auto_approved',
            resumed: selected !== null,
            resumedPart: selected?.partName,
          });
          return;
        }

        if (
          this.sessionMode.isActive &&
          this.sessionMode.kind !== (payload.kind ?? 'plan')
        ) {
          // No finalize needed — path is resolved lazily on first write
        }
        let sourceAbs: string | undefined;
        if (payload.sourceFilePath !== undefined) {
          sourceAbs = isAbsolute(payload.sourceFilePath)
            ? payload.sourceFilePath
            : join(this.config.cwd, payload.sourceFilePath);
          await this.sessionMode.validatePlanSource(sourceAbs);
        }
        await this.sessionMode.enter(
          undefined,
          undefined,
          undefined,
          payload.kind ?? 'plan',
        );
        if (sourceAbs !== undefined) {
          await this.sessionMode.setWritingPlanSource(sourceAbs);
        }
      },
```

- [ ] **Step 4: 运行并确认通过**

```bash
cd packages/agent-core
pnpm test -- test/agent/plan.test.ts
```

预期：新增测试与原有 plan 测试均通过。

- [ ] **Step 5: 提交**

```bash
git add packages/agent-core/src/agent/index.ts packages/agent-core/test/agent/plan.test.ts
git commit -m "feat(agent-core): resume pending split-design part on enterPlan design mode"
```

---

## Task 5: `DesignModeInjector` 追加 resume directive

**Depends on:** Task 1, Task 3

**Files:**
- Modify: `packages/agent-core/src/agent/injection/design-mode.ts:49-57`
- Test: `packages/agent-core/test/agent/injection/design-mode.test.ts`

- [ ] **Step 1: 写失败测试**

在 `packages/agent-core/test/agent/injection/design-mode.test.ts` 新增：

```ts
  it('appends resume directive when targetPendingPart is set and consumes it', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    let target: { indexPath: string; partName: string } | null = {
      indexPath: '/tmp/design.md',
      partName: 'api.md',
    };
    agent.sessionMode = {
      ...agent.sessionMode,
      getTargetPendingPart: () => target,
      setTargetPendingPart: (_p: string, _n: string) => { target = { indexPath: _p, partName: _n }; },
    };

    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('Continue split design');
    expect(text).toContain('api.md');
    expect(text).toContain('/tmp/design.md');
    // Should be consumed after first injection.
    expect(agent.sessionMode.getTargetPendingPart()).toBeNull();
  });
```

注意：需要让 `designAgent` stub 的 `sessionMode` 包含 `getTargetPendingPart`（已在 Task 3 中更新）。

- [ ] **Step 2: 运行并确认失败**

```bash
cd packages/agent-core
pnpm test -- test/agent/injection/design-mode.test.ts
```

预期失败：reminder 文本不包含 `Continue split design`。

- [ ] **Step 3: 最小实现**

修改 `packages/agent-core/src/agent/injection/design-mode.ts`：

1. 在 imports 中加入 `designResumePartDirective`：

```ts
import {
  designModeFullReminder,
  designModeReentryReminder,
  designModeSparseReminder,
  designSplitContinuationDirective,
  designSplitFinalReviewDirective,
  designResumePartDirective,
} from './design-mode-contract';
```

2. 在 `getInjection` 的 `if (!this.wasActive)` 块中，处理 reentry 路径时读取并消费 target：

```ts
    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      const target = this.agent.sessionMode.getTargetPendingPart();
      if (target !== null) {
        this.agent.sessionMode.setTargetPendingPart('', ''); // consume
        const directive = designResumePartDirective(
          target.partName,
          indexStemFor(target.indexPath),
        );
        return appendSkillsReminder(
          designModeReentryReminder(sessionModeFilePath, mockupAvailable, directive),
          skillsReminder,
        );
      }
      if (content.trim().length > 0) {
        const directive = splitDirectiveFor(content, sessionModeFilePath);
        return appendSkillsReminder(designModeReentryReminder(sessionModeFilePath, mockupAvailable, directive), skillsReminder);
      }
    }
```

注意：`setTargetPendingPart('', '')` 会抛错，因为我们在 Task 3 中加了非空校验。应该用另一种方式消费。更好的方式：在 `SessionMode` 中新增 `consumeTargetPendingPart()` 方法，或者在 injector 中直接清空内部状态（但不能访问私有字段）。所以应该加一个 `consumeTargetPendingPart()` 方法到 SessionMode，返回 target 并清空。

调整 Task 3：新增 `consumeTargetPendingPart()`：

```ts
  consumeTargetPendingPart(): { indexPath: string; partName: string } | null {
    const target = this._targetPendingPart;
    this._targetPendingPart = null;
    return target;
  }
```

然后 Task 5 的 injector 使用该方法。

更新 Task 3 的测试：

```ts
    it('consumes target on read', () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.setTargetPendingPart('/idx.md', 'part.md');
      expect(sm.consumeTargetPendingPart()).toEqual({ indexPath: '/idx.md', partName: 'part.md' });
      expect(sm.getTargetPendingPart()).toBeNull();
    });
```

更新 Task 5 的实现：

```ts
      const target = this.agent.sessionMode.consumeTargetPendingPart();
      if (target !== null) {
        const directive = designResumePartDirective(
          target.partName,
          indexStemFor(target.indexPath),
        );
        ...
      }
```

更新 Task 5 的测试 stub：

```ts
      consumeTargetPendingPart: () => {
        const t = target;
        target = null;
        return t;
      },
```

（我需要在 Task 3 的实现中加入 `consumeTargetPendingPart`。）

- [ ] **Step 4: 运行并确认通过**

```bash
cd packages/agent-core
pnpm test -- test/agent/injection/design-mode.test.ts
```

预期：全部通过。

- [ ] **Step 5: 提交**

```bash
git add packages/agent-core/src/agent/injection/design-mode.ts packages/agent-core/test/agent/injection/design-mode.test.ts
git commit -m "feat(agent-core): inject resume directive on design mode re-entry"
```

---

## Task 6: 全项目验证与回归测试

**Depends on:** Task 4, Task 5

**Files:** 无新增

- [ ] **Step 1: 运行设计指定的 must-pass 测试命令**

```bash
cd packages/agent-core
pnpm test -- test/agent/plan.test.ts test/agent/injection/design-mode.test.ts test/tools/resume-design-mode.test.ts test/agent/session-mode.test.ts
```

预期：全部通过。

- [ ] **Step 2: 全项目 typecheck**

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r typecheck
```

预期：无编译错误。

- [ ] **Step 3: 运行 agent-core 全量测试（可选但推荐）**

```bash
cd packages/agent-core
pnpm test
```

预期：全部通过。

- [ ] **Step 4: 提交**

```bash
git commit --allow-empty -m "test(agent-core): verify design-mode resume pending parts integration"
```

注意：此任务不产生源码改动，仅验证；按照规则“zero `--allow-empty`”，如果前面已提交且本任务无文件改动，可跳过提交，将验证作为 Task 5 的收尾步骤。若坚持独立 commit，需附带 `--allow-empty` 并记录这是验证节点。推荐：不单独 commit，将 Step 1-3 写入计划作为最终验收清单。

---

| # | 风险 | 缓解 |
|---|---|---|
| 1 | `ResumeDesignMode` 放在 `tools/builtin/planning/` 下但不是真正 tool，可能被误注册 | 计划明确不注册到 `ToolManager`；代码中不实现 `BuiltinTool` 接口 |
| 2 | `SessionMode.enter` 新增可选参数后，所有调用点（含测试）必须保持编译通过 | Task 3 以 whole-tree typecheck 结束，并显式搜索所有 `enter(` 调用 |
| 3 | `targetPendingPart` 状态在异常退出时残留 | `enter()`、`cancel()`、`exit()` 均清空；Task 3 的测试断言该行为 |
| 4 | `requestQuestion` 返回格式与 AskUserQuestionTool 不一致 | `ResumeDesignMode` 使用与 `normalizeQuestionResult` 等价的本地归一化逻辑；测试中覆盖 `QuestionResponse`/`QuestionAnswers`/dismissed 三种形态 |
