# Part 2: prompt 注入 + 7 个 Office-Hours 工具本地化

本 Part 在所有 office-hours contract 变体顶部注入 Language 指令（Task 5），然后依次本地化 7 个工具的用户可见输出字符串（Task 6–8）。

---

### Task 5: office-hours-contract prompt 注入 Language 指令

**Depends on:** Part 1 完成（i18n 可用，但本任务不依赖 `t()`，只插入静态文本）

**Files:**
- Modify: `packages/agent-core/src/agent/injection/office-hours-contract.ts` 27-40 (entry), 44-157 (full), 161-169 (sparse), 173-179 (reentry)
- Modify: `packages/agent-core/test/agent/injection/office-hours-contract.test.ts` +4 tests

- [ ] Write the failing test — 追加到 `packages/agent-core/test/agent/injection/office-hours-contract.test.ts`：

在 `describe('office-hours-contract', () => {` 内部追加：

```typescript
  const LANGUAGE_PREFIX = '**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.';

  it('entry reminder starts with Language instruction', () => {
    const msg = officeHoursEntryReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });

  it('full reminder starts with Language instruction', () => {
    const msg = officeHoursFullReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });

  it('sparse reminder starts with Language instruction', () => {
    const msg = officeHoursSparseReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });

  it('reentry reminder starts with Language instruction', () => {
    const msg = officeHoursReentryReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });
```

在文件顶部追加辅助函数:

```typescript
function escapeRegex(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/agent/injection/office-hours-contract.test.ts
```
Expected: 4 new tests FAIL — 没有 `**Language:**` 行。

- [ ] Write the minimal implementation:

`packages/agent-core/src/agent/injection/office-hours-contract.ts` — 在文件顶部（import 之后）添加常量:

```typescript
const LANG_INSTRUCTION = '**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.';
```

修改 `officeHoursEntryReminder`—— 在 `return [` 后第一行插入（~line 30）:

```typescript
    LANG_INSTRUCTION,
    '',
```

修改 `officeHoursFullReminder`—— 在 `return [` 后第一行插入（~line 47）:

```typescript
    LANG_INSTRUCTION,
    '',
```

修改 `officeHoursSparseReminder`—— 在 `return [` 后第一行插入（~line 163）:

```typescript
    LANG_INSTRUCTION,
    '',
```

修改 `officeHoursReentryReminder`—— 在 `return [` 后第一行插入（~line 175）:

```typescript
    LANG_INSTRUCTION,
    '',
```

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/agent/injection/office-hours-contract.test.ts
```
Expected: 现有测试 + 4 个新测试全部 PASS。

- [ ] Commit: `git add -A && git commit -m "feat: inject Language instruction into all office-hours prompt variants"`

---

### Task 6: Enter/Exit Office-Hours 工具本地化

**Depends on:** Task 1（使用 `t()`）

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/office-hours/enter-office-hours.ts` 25-47
- Modify: `packages/agent-core/src/tools/builtin/office-hours/exit-office-hours.ts` 24-39
- Test: `packages/agent-core/test/tools/builtin/office-hours/enter-exit.test.ts`

- [ ] Write the failing test — `packages/agent-core/test/tools/builtin/office-hours/enter-exit.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import type { Agent } from '../../../../src/agent';
import { EnterOfficeHoursModeTool } from '../../../../src/tools/builtin/office-hours/enter-office-hours';
import { ExitOfficeHoursModeTool } from '../../../../src/tools/builtin/office-hours/exit-office-hours';

function mockAgent(overrides: Partial<{ isActive: boolean; kind: string; userLanguage: string | undefined; path: string | null }> = {}) {
  return {
    sessionMode: {
      isActive: overrides.isActive ?? false,
      kind: overrides.kind ?? 'normal',
      exit: () => {},
      enter: async () => {},
      sessionModeFilePath: overrides.path ?? null,
    },
    userLanguage: overrides.userLanguage,
    kaos: { stat: async () => {} },
  } as unknown as Agent;
}

describe('EnterOfficeHoursModeTool localized output', () => {
  it('returns Chinese error when already active in zh', async () => {
    const agent = mockAgent({ isActive: true, kind: 'office-hours', userLanguage: 'zh' });
    const tool = new EnterOfficeHoursModeTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Office Hours 模式已经处于激活状态。会话结束后请调用 ExitOfficeHoursMode。');
  });

  it('returns Chinese error when another mode active in zh', async () => {
    const agent = mockAgent({ isActive: true, kind: 'plan', userLanguage: 'zh' });
    const tool = new EnterOfficeHoursModeTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('另一个会话模式已经激活。请先退出该模式再进入 Office Hours。');
  });

  it('returns English error when already active in en', async () => {
    const agent = mockAgent({ isActive: true, kind: 'office-hours', userLanguage: 'en' });
    const tool = new EnterOfficeHoursModeTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toContain('already active');
  });

  it('returns English error when language is undefined (fallback)', async () => {
    const agent = mockAgent({ isActive: true, kind: 'office-hours', userLanguage: undefined });
    const tool = new EnterOfficeHoursModeTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toContain('already active');
  });
});

describe('ExitOfficeHoursModeTool localized output', () => {
  it('returns Chinese error when not in office-hours (zh)', async () => {
    const agent = mockAgent({ isActive: false, userLanguage: 'zh' });
    const tool = new ExitOfficeHoursModeTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Office Hours 模式未激活。');
  });

  it('returns Chinese success with path (zh)', async () => {
    const agent = mockAgent({ isActive: true, kind: 'office-hours', userLanguage: 'zh', path: '/tmp/design.md' });
    const tool = new ExitOfficeHoursModeTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.output).toContain('Office Hours 会话已结束。');
    expect(result.output).toContain('设计文档已保存至：/tmp/design.md');
    expect(result.output).toContain('应用即将退出。');
  });
});
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/enter-exit.test.ts
```
Expected: 断言失败 — 当前输出仍是硬编码英文。

- [ ] Write the minimal implementation:

`packages/agent-core/src/tools/builtin/office-hours/enter-office-hours.ts` — 修改 `execute`（line 23-49）:

```typescript
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (this.agent.sessionMode.isActive) {
          if (this.agent.sessionMode.kind === 'office-hours') {
            return {
              isError: true,
              output: t('officeHours.alreadyActive', lang),
            };
          }
          return {
            isError: true,
            output: t('officeHours.anotherModeActive', lang),
          };
        }

        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'office-hours');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter office hours mode.';
          return {
            isError: true,
            output: t('officeHours.failedToEnter', lang).replace('{message}', message),
          };
        }

        return {
          output: officeHoursEntryReminder(this.agent.sessionMode.sessionModeFilePath),
        };
      },
```

添加 import（顶部）:
```typescript
import { t } from '#/i18n';
```

`packages/agent-core/src/tools/builtin/office-hours/exit-office-hours.ts` — 修改 `execute`（line 23-42）:

```typescript
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: t('officeHours.modeNotActive', lang),
          };
        }

        const path = this.agent.sessionMode.sessionModeFilePath;
        this.agent.sessionMode.exit();

        const parts = [
          t('officeHours.sessionComplete', lang),
        ];
        if (path) {
          parts.push(t('officeHours.designDocSaved', lang).replace('{path}', path));
        }
        parts.push(t('officeHours.appWillExit', lang));

        return {
          output: parts.join('\n'),
        };
      },
```

添加 import（顶部）:
```typescript
import { t } from '#/i18n';
```

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/enter-exit.test.ts
```
Expected: 6 tests PASS。

- [ ] Commit: `git add -A && git commit -m "feat: localize EnterOfficeHours and ExitOfficeHours tool outputs"`

---

### Task 7: AppendBuilderProfile / AppendLearning / SearchLearnings 本地化

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/office-hours/append-profile.ts` 34-41
- Modify: `packages/agent-core/src/tools/builtin/office-hours/append-learning.ts` 31-35, 50-52
- Modify: `packages/agent-core/src/tools/builtin/office-hours/search-learnings.ts` 27-30, 40-52
- Test: `packages/agent-core/test/tools/builtin/office-hours/state-tools.test.ts`

- [ ] Write the failing test — `packages/agent-core/test/tools/builtin/office-hours/state-tools.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import type { Agent } from '../../../../src/agent';
import { AppendBuilderProfileTool } from '../../../../src/tools/builtin/office-hours/append-profile';
import { AppendLearningTool } from '../../../../src/tools/builtin/office-hours/append-learning';
import { SearchLearningsTool } from '../../../../src/tools/builtin/office-hours/search-learnings';

function mockAgent(userLanguage?: string) {
  return {
    sessionMode: { isActive: true, kind: 'office-hours' },
    userLanguage,
    officeHoursStateStore: {
      appendProfile: async () => {},
      appendLearning: async () => {},
      searchLearnings: async (args: any) => [] as any[],
    },
    config: { cwd: '/tmp' },
  } as unknown as Agent;
}

describe('AppendBuilderProfileTool localized', () => {
  it('returns Chinese success message (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new AppendBuilderProfileTool(agent);
    const result = await tool.resolveExecution({
      mode: 'startup', projectSlug: 'test',
      signalCount: 5, signals: [], resourcesShown: [], topics: [],
    } as any).execute();
    expect(result.output).toBe('Builder 档案条目已追加成功。下次层级计算时将更新会话计数。');
  });

  it('returns English success message (en)', async () => {
    const agent = mockAgent('en');
    const tool = new AppendBuilderProfileTool(agent);
    const result = await tool.resolveExecution({
      mode: 'startup', projectSlug: 'test',
      signalCount: 5, signals: [], resourcesShown: [], topics: [],
    } as any).execute();
    expect(result.output).toBe('Builder profile entry appended successfully. Session count will be updated for next tier computation.');
  });
});

describe('AppendLearningTool localized', () => {
  it('returns Chinese message with key (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new AppendLearningTool(agent);
    const result = await tool.resolveExecution({
      type: 'eureka', key: 'insight-1', insight: 'test',
      confidence: 1.0,
    }).execute();
    expect(result.output).toBe('学习洞察 "insight-1" 已记录成功。');
  });
});

describe('SearchLearningsTool localized', () => {
  it('returns Chinese no learnings message (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new SearchLearningsTool(agent);
    const result = await tool.resolveExecution({ limit: 10 }).execute();
    expect(result.output).toBe('未找到过往学习洞察。');
  });

  it('returns Chinese header with count', async () => {
    const agent = {
      sessionMode: { isActive: true, kind: 'office-hours' },
      userLanguage: 'zh',
      officeHoursStateStore: {
        searchLearnings: async () => [{
          ts: '2026-01-01', type: 'eureka', key: 'x', insight: 'y',
          confidence: 0.5, source: 'observed' as const,
        }],
      },
      config: { cwd: '/tmp' },
    } as unknown as Agent;
    const tool = new SearchLearningsTool(agent);
    const result = await tool.resolveExecution({ limit: 10 }).execute();
    expect(result.output).toContain('找到 1 条学习洞察：');
    expect(result.output).toContain('类型');
    expect(result.output).toContain('洞察');
    expect(result.output).toContain('置信度');
    expect(result.output).toContain('日期');
  });
});
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/state-tools.test.ts
```
Expected: 断言失败 — 输出是英文原文。

- [ ] Write the minimal implementation:

`packages/agent-core/src/tools/builtin/office-hours/append-profile.ts` — 添加 import `import { t } from '#/i18n';`，修改 `execute` 中的输出（~line 55-57）:

```typescript
          return {
            output: t('officeHours.profileAppended', this.agent.userLanguage),
          };
```

错误输出中 modeNotActive（line 36-37）也本地化:
```typescript
          return {
            isError: true,
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
          };
```

`packages/agent-core/src/tools/builtin/office-hours/append-learning.ts` — 添加 import `import { t } from '#/i18n';`，修改三处输出:

modeNotActive（line 33-34）:
```typescript
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
```

成功消息（line 50-52）:
```typescript
            output: t('officeHours.learningRecorded', this.agent.userLanguage)
              .replace('{key}', args.key),
```

`packages/agent-core/src/tools/builtin/office-hours/search-learnings.ts` — 添加 import `import { t } from '#/i18n';`，修改两处:

modeNotActive（line 29-30）:
```typescript
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
```

no learnings（line 42-43）:
```typescript
            output: t('officeHours.noLearnings', this.agent.userLanguage),
```

带结果的输出（line 46-52）—— 将格式化代码改为:

```typescript
          const lang = this.agent.userLanguage;
          const formatted = learnings.map((l, i) =>
            `[${i + 1}] ${t('officeHours.learningTypeLabel', lang)}: ${l.type.toUpperCase()}: ${l.key}\n    ${t('officeHours.learningInsightLabel', lang)}: ${l.insight}\n    ${t('officeHours.learningConfidenceLabel', lang)}: ${l.confidence}\n    ${t('officeHours.learningDateLabel', lang)}: ${l.ts}${l.branch ? `\n    ${t('officeHours.learningBranchLabel', lang)}: ${l.branch}` : ''}`
          ).join('\n\n');

          return {
            output: t('officeHours.learningsHeader', lang)
              .replace('{count}', String(learnings.length)) + '\n\n' + formatted,
          };
```

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/state-tools.test.ts
```
Expected: 5 tests PASS。

- [ ] Commit: `git add -A && git commit -m "feat: localize AppendBuilderProfile, AppendLearning, SearchLearnings tool outputs"`

---

### Task 8: SyncOfficeHoursArtifact / EnsureClaudeMdRouting 本地化

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/office-hours/sync-artifact.ts` 27-102
- Modify: `packages/agent-core/src/tools/builtin/office-hours/ensure-routing.ts` 29-68
- Test: `packages/agent-core/test/tools/builtin/office-hours/artifact-tools.test.ts`

- [ ] Write the failing test — `packages/agent-core/test/tools/builtin/office-hours/artifact-tools.test.ts`：

```typescript
import { describe, expect, it, vi } from 'vitest';
import type { Agent } from '../../../../src/agent';
import { EnsureClaudeMdRoutingTool } from '../../../../src/tools/builtin/office-hours/ensure-routing';
import { SyncOfficeHoursArtifactTool } from '../../../../src/tools/builtin/office-hours/sync-artifact';

function mockAgent(userLanguage?: string, mcp?: { name: string; status: string }[]) {
  const agent = {
    sessionMode: { isActive: true, kind: 'office-hours' },
    userLanguage,
    config: { cwd: '/tmp' },
    kaos: {
      readText: vi.fn(async () => { throw new Error('not found'); }),
      writeText: vi.fn(async () => {}),
      stat: vi.fn(async () => {}),
    },
    mcp: mcp ? {
      list: () => mcp,
    } : undefined,
  } as unknown as Agent;
  return agent;
}

describe('EnsureClaudeMdRoutingTool localized', () => {
  it('returns Chinese created message (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new EnsureClaudeMdRoutingTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.output).toContain('创建');
  });

  it('returns Chinese already-has message (zh)', async () => {
    const agent = {
      ...mockAgent('zh'),
      kaos: {
        readText: vi.fn(async () => '## Skill routing\nexisting'),
        writeText: vi.fn(async () => {}),
        stat: vi.fn(async () => {}),
      },
    } as unknown as Agent;
    const tool = new EnsureClaudeMdRoutingTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.output).toContain('已包含');
  });
});

describe('SyncOfficeHoursArtifactTool localized', () => {
  it('returns Chinese design-file-not-found (zh)', async () => {
    const agent = {
      ...mockAgent('zh'),
      kaos: {
        readText: vi.fn(async () => { throw new Error('no file'); }),
        stat: vi.fn(async () => { throw new Error('no file'); }),
      },
    } as unknown as Agent;
    const tool = new SyncOfficeHoursArtifactTool(agent);
    const result = await tool.resolveExecution({ designFilePath: '/tmp/missing.md' }).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toContain('在');
    expect(result.output).toContain('未找到设计文件');
  });

  it('returns Chinese MCP connected message (zh)', async () => {
    const agent = mockAgent('zh', [{ name: 'gbrain-server', status: 'connected', transport: 'stdio', toolCount: 1 }]);
    const tool = new SyncOfficeHoursArtifactTool(agent);
    const result = await tool.resolveExecution({ designFilePath: '/tmp/test.md' }).execute();
    expect(result.output).toContain('gbrain MCP');
    expect(result.output).toContain('连接');
  });
});
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/artifact-tools.test.ts
```
Expected: 断言失败 — 输出是英文。

- [ ] Write the minimal implementation:

`packages/agent-core/src/tools/builtin/office-hours/sync-artifact.ts` — 添加 import `import { t } from '#/i18n';`。修改所有用户可见输出:

modeNotActive（line 30-32）:
```typescript
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
```

design file not found（line 52）:
```typescript
            return { isError: true, output: t('officeHours.designFileNotFound', this.agent.userLanguage).replace('{path}', args.designFilePath) };
```

MCP connected 分支（line 69-75）:
```typescript
            const lang = this.agent.userLanguage;
            return {
              output: [
                t('officeHours.gbrainConnected', lang),
                gbrainSource
                  ? t('officeHours.gbrainTargetSource', lang).replace('{source}', gbrainSource)
                  : t('officeHours.gbrainNoSourcePin', lang),
                t('officeHours.gbrainReadyForSync', lang).replace('{path}', args.designFilePath),
              ].filter(Boolean).join('\n'),
            };
```

CLI synced 分支（line 86-90）:
```typescript
            const lang = this.agent.userLanguage;
            return {
              output: [
                t('officeHours.gbrainSynced', lang),
                gbrainSource
                  ? t('officeHours.gbrainTargetSource', lang).replace('{source}', gbrainSource)
                  : '',
                t('officeHours.gbrainFile', lang).replace('{path}', args.designFilePath),
              ].filter(Boolean).join('\n'),
            };
```

CLI failed 分支（line 93-97）:
```typescript
            const message = cliError instanceof Error ? cliError.message : String(cliError);
            const lang = this.agent.userLanguage;
            return {
              isError: true,
              output: t('officeHours.gbrainCliFailed', lang).replace('{message}', message),
            };
```

通用失败（line 100-101）:
```typescript
          const message = error instanceof Error ? error.message : 'Failed to sync design artifact.';
          const lang = this.agent.userLanguage;
          return {
            isError: true,
            output: t('officeHours.failedToSyncArtifact', lang).replace('{message}', message),
          };
```

`packages/agent-core/src/tools/builtin/office-hours/ensure-routing.ts` — 添加 import `import { t } from '#/i18n';`。修改:

modeNotActive（line 35-37）:
```typescript
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
```

created（line 54）:
```typescript
            return { output: t('officeHours.agentsMdCreated', this.agent.userLanguage).replace('{path}', claudeMdPath) };
```

already has routing（line 59）:
```typescript
            return { output: t('officeHours.agentsMdAlreadyHasRouting', this.agent.userLanguage) };
```

updated（line 63-64）:
```typescript
          return { output: t('officeHours.agentsMdUpdated', this.agent.userLanguage).replace('{path}', claudeMdPath) };
```

failed（line 67）:
```typescript
          const message = error instanceof Error ? error.message : 'Failed to ensure AGENTS.md routing.';
          return { isError: true, output: t('officeHours.failedToEnsureRouting', this.agent.userLanguage).replace('{message}', message) };
```

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/artifact-tools.test.ts
```
Expected: 4 tests PASS。

- [ ] Commit: `git add -A && git commit -m "feat: localize SyncOfficeHoursArtifact and EnsureClaudeMdRouting tool outputs"`

---

## Part 2 Local Self-Review

- [ ] 2. Placeholder scan：所有步骤包含完整代码/测试/命令，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：4 个任务均产生文件修改与新增测试。
- [ ] 4. Dependency soundness：Task 5 仅修改 contract 文件（无 i18n 依赖），Task 6-8 依赖 Task 1（`t()`），均在前序 Part 完成后可用。
- [ ] 5. Caller & build soundness：未修改任何共享签名，仅修改工具内部输出字符串构造方式。
- [ ] 6. Test-the-risk：每个工具的中文/英文/回退场景均有行为断言；`SearchLearnings` 验证中文标签（类型/洞察/置信度/日期）。
- [ ] 7. Type consistency：仅导入 `t` 与 `Agent` 类型，无新类型定义。
