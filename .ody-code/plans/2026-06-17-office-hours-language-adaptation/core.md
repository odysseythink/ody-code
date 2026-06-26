# Part 1: i18n + Agent/Session + SetOfficeHoursLanguage

本 Part 创建核心 i18n 模块，向 `Agent` 添加运行时语言与持久化回调，并通过 `SetOfficeHoursLanguage` 内置工具让 LLM 设置会话语言。

---

### Task 1: i18n 模块（types + translations + t/isSupportedLanguage/normalizeLanguage）

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/i18n/types.ts`
- Create: `packages/agent-core/src/i18n/translations.ts`
- Create: `packages/agent-core/src/i18n/index.ts`
- Test: `packages/agent-core/test/i18n/index.test.ts`
- Test: `packages/agent-core/test/i18n/language.test.ts`
- Modify: `packages/agent-core/src/index.ts` +1 line (导出 `export * from './i18n';`)

- [ ] Write the failing test — `packages/agent-core/test/i18n/index.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { t, isSupportedLanguage, normalizeLanguage } from '#/i18n';

describe('t', () => {
  it('returns Chinese string for zh language', () => {
    expect(t('officeHours.entered', 'zh')).toBe('Office Hours 模式已激活。');
  });

  it('returns English string for en language', () => {
    expect(t('officeHours.entered', 'en')).toBe('Office hours mode is now active.');
  });

  it('falls back to English when lang is undefined', () => {
    expect(t('officeHours.entered', undefined)).toBe('Office hours mode is now active.');
  });

  it('falls back to English for unsupported language (cast)', () => {
    expect(t('officeHours.entered', 'fr' as any)).toBe('Office hours mode is now active.');
  });

  it('falls back to key string when key is missing in both languages', () => {
    expect(t('nonexistent.key' as any, 'zh')).toBe('nonexistent.key');
  });

  it('returns Chinese text with placeholder for learningRecorded', () => {
    expect(t('officeHours.learningRecorded', 'zh')).toContain('{key}');
  });
});
```

- [ ] Write the failing test — `packages/agent-core/test/i18n/language.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { isSupportedLanguage, normalizeLanguage } from '#/i18n';

describe('isSupportedLanguage', () => {
  it('accepts zh', () => expect(isSupportedLanguage('zh')).toBe(true));
  it('accepts en', () => expect(isSupportedLanguage('en')).toBe(true));
  it('rejects fr', () => expect(isSupportedLanguage('fr')).toBe(false));
  it('rejects undefined', () => expect(isSupportedLanguage(undefined)).toBe(false));
  it('rejects "cn"', () => expect(isSupportedLanguage('cn')).toBe(false));
});

describe('normalizeLanguage', () => {
  it('maps ZH-CN to zh', () => expect(normalizeLanguage('ZH-CN')).toBe('zh'));
  it('maps zh-TW to zh', () => expect(normalizeLanguage('zh-TW')).toBe('zh'));
  it('maps fr to en', () => expect(normalizeLanguage('fr')).toBe('en'));
  it('maps empty string to en', () => expect(normalizeLanguage('')).toBe('en'));
});
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/i18n/
```
Expected: `exports` / `module not found` 因为 `#/i18n` 尚不存在。

- [ ] Write the minimal implementation:

`packages/agent-core/src/i18n/types.ts`:

```typescript
export type SupportedLanguage = 'en' | 'zh';
export const SUPPORTED_LANGUAGES: readonly SupportedLanguage[] = ['en', 'zh'];

export type MessageKey =
  | 'officeHours.entered'
  | 'officeHours.alreadyActive'
  | 'officeHours.anotherModeActive'
  | 'officeHours.failedToEnter'
  | 'officeHours.sessionComplete'
  | 'officeHours.designDocSaved'
  | 'officeHours.appWillExit'
  | 'officeHours.profileAppended'
  | 'officeHours.learningRecorded'
  | 'officeHours.noLearnings'
  | 'officeHours.learningsHeader'
  | 'officeHours.learningTypeLabel'
  | 'officeHours.learningInsightLabel'
  | 'officeHours.learningConfidenceLabel'
  | 'officeHours.learningDateLabel'
  | 'officeHours.learningBranchLabel'
  | 'officeHours.modeNotActive'
  | 'officeHours.designFileNotFound'
  | 'officeHours.gbrainConnected'
  | 'officeHours.gbrainTargetSource'
  | 'officeHours.gbrainNoSourcePin'
  | 'officeHours.gbrainReadyForSync'
  | 'officeHours.gbrainSynced'
  | 'officeHours.gbrainFile'
  | 'officeHours.gbrainCliFailed'
  | 'officeHours.agentsMdCreated'
  | 'officeHours.agentsMdUpdated'
  | 'officeHours.agentsMdAlreadyHasRouting'
  | 'officeHours.failedToEnsureRouting'
  | 'officeHours.failedToSyncArtifact'
  | 'officeHours.languageSet'
  | 'tui.footer.officeHours'
  | 'tui.statusPanel.officeHours'
  | 'tui.statusPanel.on'
  | 'tui.statusPanel.off';
```

`packages/agent-core/src/i18n/translations.ts`:

```typescript
import type { MessageKey, SupportedLanguage } from './types';

export type Translations = Record<SupportedLanguage, Record<MessageKey, string>>;

export const translations: Translations = {
  en: {
    'officeHours.entered': 'Office hours mode is now active.',
    'officeHours.alreadyActive': 'Office hours mode is already active. Use ExitOfficeHoursMode when the session is complete.',
    'officeHours.anotherModeActive': 'Another session mode is already active. Exit it first before entering office hours mode.',
    'officeHours.failedToEnter': 'Failed to enter office hours mode: {message}',
    'officeHours.sessionComplete': 'Office hours session complete.',
    'officeHours.designDocSaved': 'Design document saved to: {path}',
    'officeHours.appWillExit': 'The application will now exit.',
    'officeHours.profileAppended': 'Builder profile entry appended successfully. Session count will be updated for next tier computation.',
    'officeHours.learningRecorded': 'Learning "{key}" recorded successfully.',
    'officeHours.noLearnings': 'No past learnings found.',
    'officeHours.learningsHeader': 'Found {count} learning(s):',
    'officeHours.learningTypeLabel': 'Type',
    'officeHours.learningInsightLabel': 'Insight',
    'officeHours.learningConfidenceLabel': 'Confidence',
    'officeHours.learningDateLabel': 'Date',
    'officeHours.learningBranchLabel': 'Branch',
    'officeHours.modeNotActive': 'Office hours mode is not active.',
    'officeHours.designFileNotFound': 'Design file not found at {path}.',
    'officeHours.gbrainConnected': 'gbrain MCP server is connected.',
    'officeHours.gbrainTargetSource': 'Target source: {source}',
    'officeHours.gbrainNoSourcePin': 'No .gbrain-source pin found.',
    'officeHours.gbrainReadyForSync': 'Design artifact at {path} is ready for sync via MCP.',
    'officeHours.gbrainSynced': 'Design artifact synced via gbrain CLI.',
    'officeHours.gbrainFile': 'File: {path}',
    'officeHours.gbrainCliFailed': 'gbrain CLI sync failed: {message}. Ensure the gbrain CLI is installed and configured.',
    'officeHours.agentsMdCreated': 'AGENTS.md created at {path} with ## Skill routing section.',
    'officeHours.agentsMdUpdated': 'Appended ## Skill routing section to AGENTS.md at {path}.',
    'officeHours.agentsMdAlreadyHasRouting': 'AGENTS.md already has a ## Skill routing section — no changes needed.',
    'officeHours.failedToEnsureRouting': 'Failed to ensure AGENTS.md routing: {message}',
    'officeHours.failedToSyncArtifact': 'Failed to sync design artifact: {message}',
    'officeHours.languageSet': 'User language set to {language}.',
    'tui.footer.officeHours': 'Office Hours',
    'tui.statusPanel.officeHours': 'Office Hours',
    'tui.statusPanel.on': 'on',
    'tui.statusPanel.off': 'off',
  },
  zh: {
    'officeHours.entered': 'Office Hours 模式已激活。',
    'officeHours.alreadyActive': 'Office Hours 模式已经处于激活状态。会话结束后请调用 ExitOfficeHoursMode。',
    'officeHours.anotherModeActive': '另一个会话模式已经激活。请先退出该模式再进入 Office Hours。',
    'officeHours.failedToEnter': '进入 Office Hours 模式失败：{message}',
    'officeHours.sessionComplete': 'Office Hours 会话已结束。',
    'officeHours.designDocSaved': '设计文档已保存至：{path}',
    'officeHours.appWillExit': '应用即将退出。',
    'officeHours.profileAppended': 'Builder 档案条目已追加成功。下次层级计算时将更新会话计数。',
    'officeHours.learningRecorded': '学习洞察 "{key}" 已记录成功。',
    'officeHours.noLearnings': '未找到过往学习洞察。',
    'officeHours.learningsHeader': '找到 {count} 条学习洞察：',
    'officeHours.learningTypeLabel': '类型',
    'officeHours.learningInsightLabel': '洞察',
    'officeHours.learningConfidenceLabel': '置信度',
    'officeHours.learningDateLabel': '日期',
    'officeHours.learningBranchLabel': '分支',
    'officeHours.modeNotActive': 'Office Hours 模式未激活。',
    'officeHours.designFileNotFound': '在 {path} 未找到设计文件。',
    'officeHours.gbrainConnected': 'gbrain MCP 服务器已连接。',
    'officeHours.gbrainTargetSource': '目标源：{source}',
    'officeHours.gbrainNoSourcePin': '未找到 .gbrain-source 固定文件。',
    'officeHours.gbrainReadyForSync': '{path} 处的设计制品已准备好通过 MCP 同步。',
    'officeHours.gbrainSynced': '设计制品已通过 gbrain CLI 同步。',
    'officeHours.gbrainFile': '文件：{path}',
    'officeHours.gbrainCliFailed': 'gbrain CLI 同步失败：{message}。请确保 gbrain CLI 已安装并配置。',
    'officeHours.agentsMdCreated': '已在 {path} 创建 AGENTS.md，并添加 ## Skill routing 章节。',
    'officeHours.agentsMdUpdated': '已在 {path} 的 AGENTS.md 中追加 ## Skill routing 章节。',
    'officeHours.agentsMdAlreadyHasRouting': 'AGENTS.md 已包含 ## Skill routing 章节，无需更改。',
    'officeHours.failedToEnsureRouting': '确保 AGENTS.md 路由失败：{message}',
    'officeHours.failedToSyncArtifact': '同步设计制品失败：{message}',
    'officeHours.languageSet': '用户语言已设置为 {language}。',
    'tui.footer.officeHours': '办公时间',
    'tui.statusPanel.officeHours': '办公时间',
    'tui.statusPanel.on': '开启',
    'tui.statusPanel.off': '关闭',
  },
};
```

`packages/agent-core/src/i18n/index.ts`:

```typescript
import type { MessageKey, SupportedLanguage } from './types';
import { translations } from './translations';

export { translations } from './translations';
export type { MessageKey, SupportedLanguage } from './types';

export function t(
  key: MessageKey,
  lang: SupportedLanguage | undefined,
  fallback?: string,
): string {
  if (lang !== undefined && translations[lang] !== undefined && translations[lang][key] !== undefined) {
    return translations[lang][key];
  }
  const enText = translations['en'][key];
  if (enText !== undefined) return enText;
  if (fallback !== undefined) return fallback;
  return key;
}

export function isSupportedLanguage(value: unknown): value is SupportedLanguage {
  return value === 'en' || value === 'zh';
}

export function normalizeLanguage(value: string): SupportedLanguage {
  const normalized = value.toLowerCase().split('-')[0] ?? '';
  if (['zh', 'zh_cn', 'zh_tw', 'zh_hk'].includes(normalized)) return 'zh';
  return 'en';
}
```

Update `packages/agent-core/src/index.ts`: 在末尾追加一行 `export * from './i18n';`。

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/i18n/
```
Expected: 11 tests PASS.

- [ ] Commit: `git add -A && git commit -m "feat: add i18n module with translations and language helpers"`

---

### Task 2: Agent 运行时 userLanguage + AgentOptions 回调 + emitStatusUpdated 更新

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/agent/index.ts` 81-104 (AgentOptions), 106-231 (constructor, field, method), 628-652 (emitStatusUpdated)
- Test: `packages/agent-core/test/agent/user-language.test.ts`

- [ ] Write the failing test — `packages/agent-core/test/agent/user-language.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';
import { Agent } from '#/agent';
import { testKaos } from '#/test/fixtures/test-kaos';

describe('Agent userLanguage', () => {
  it('restores userLanguage from AgentOptions', () => {
    const agent = new Agent({ kaos: testKaos, userLanguage: 'zh' });
    expect(agent.userLanguage).toBe('zh');
  });

  it('defaults userLanguage to undefined', () => {
    const agent = new Agent({ kaos: testKaos });
    expect(agent.userLanguage).toBeUndefined();
  });

  it('setUserLanguage updates runtime and calls callback', () => {
    const spy = vi.fn();
    const agent = new Agent({ kaos: testKaos, setUserLanguage: spy });
    agent.setUserLanguage('zh');
    expect(agent.userLanguage).toBe('zh');
    expect(spy).toHaveBeenCalledWith('zh');
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('setUserLanguage does not throw when callback is undefined', () => {
    const agent = new Agent({ kaos: testKaos });
    expect(() => agent.setUserLanguage('en')).not.toThrow();
    expect(agent.userLanguage).toBe('en');
  });

  it('setUserLanguage emits status updated event', () => {
    const events: Array<{ type: string; userLanguage?: unknown }> = [];
    const agent = new Agent({
      kaos: testKaos,
      rpc: {
        emitEvent: (event: any) => { events.push(event); },
      } as any,
    });
    // emitStatusUpdated guards on hasModel — seed config
    agent.config.update({
      cwd: '/tmp',
      modelAlias: 'test-model',
      systemPrompt: 'test',
      thinkingLevel: 'off',
    });
    agent.setUserLanguage('zh');
    const statusEvent = events.find(e => e.type === 'agent.status.updated');
    expect(statusEvent).toBeDefined();
    expect(statusEvent!.userLanguage).toBe('zh');
  });

  it('getUserLanguage returns undefined when not set', () => {
    const agent = new Agent({ kaos: testKaos });
    expect(agent.rpcMethods.getUserLanguage({})).toBeUndefined();
  });

  it('getUserLanguage returns language after set', () => {
    const agent = new Agent({ kaos: testKaos });
    agent.setUserLanguage('en');
    expect(agent.rpcMethods.getUserLanguage({})).toBe('en');
  });
});
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/agent/user-language.test.ts
```
Expected: TypeScript 编译报错 — `AgentOptions` 没有 `userLanguage` / `Agent` 没有 `setUserLanguage` / `AgentAPI` 没有 `getUserLanguage`。

- [ ] Write the minimal implementation:

`packages/agent-core/src/agent/index.ts` — 修改 `AgentOptions` 接口（~line 81-104），在 `officeHoursStateStore` 行后追加:

```typescript
  /** User language restored from Session metadata on resume. */
  readonly userLanguage?: SupportedLanguage | undefined;
  /** Callback for Agent to persist a detected language change back to Session. */
  readonly setUserLanguage?: ((lang: SupportedLanguage) => void) | undefined;
```

在文件顶部添加 import:
```typescript
import type { SupportedLanguage } from '#/i18n';
```

`Agent` 类中（~line 158 `officeHoursStateStore` 行后）添加字段:
```typescript
  userLanguage?: SupportedLanguage;
```

构造函数（~line 230 `this.officeHoursStateStore = ...` 行后）添加:
```typescript
    this.userLanguage = options.userLanguage;
```

在 `rpcMethods` getter 中（~`getUsage()` 行后，~line 446）添加:
```typescript
      getUserLanguage: () => this.userLanguage,
```

在 `emitStatusUpdated` 方法（~line 641-651）的 emitEvent 调用中增加:
```typescript
      userLanguage: this.userLanguage,
```

在类中新增方法（在 `emitStatusUpdated` 前，~line 628）:
```typescript
  setUserLanguage(lang: SupportedLanguage): void {
    this.userLanguage = lang;
    try {
      this.options.setUserLanguage?.(lang);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.log.warn('failed to persist user language', { error: message });
    }
    this.emitStatusUpdated();
  }
```

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/agent/user-language.test.ts
```
Expected: 6 tests PASS.

- [ ] Commit: `git add -A && git commit -m "feat: add Agent.userLanguage with emit and AgentAPI getUserLanguage"`

---

### Task 3: Session 持久化 — instantiateAgent 传递 userLanguage 与回调

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/src/session/index.ts` 479-510 (instantiateAgent)
- Test: `packages/agent-core/test/session/user-language-persistence.test.ts`

- [ ] Write the failing test — `packages/agent-core/test/session/user-language-persistence.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';
import { join } from 'pathe';
import { Session } from '#/session';
import { testKaos } from '#/test/fixtures/test-kaos';

async function makeTempDir(): Promise<string> {
  const dir = testKaos.pathClass() === 'posix'
    ? `/tmp/ody-test-${Math.random().toString(36).slice(2)}`
    : join(process.cwd(), 'test-tmp', Math.random().toString(36).slice(2));
  await testKaos.mkdir(dir, { parents: true, existOk: true });
  return dir;
}

describe('Session userLanguage persistence', () => {
  it('restores userLanguage from metadata.custom when creating agent', async () => {
    const sessionDir = await makeTempDir();
    const workDir = await makeTempDir();
    const session = new Session({
      id: 'test-lang-persist',
      kaos: testKaos.withCwd(workDir),
      homedir: sessionDir,
      rpc: { emitEvent: vi.fn() } as any,
      skills: { explicitDirs: [join(workDir, 'missing-skills')] },
    });
    // simulate a previously set language
    session.metadata.custom = { userLanguage: 'zh' };
    const main = await session.createMain();
    expect(main.userLanguage).toBe('zh');
  });

  it('writes userLanguage to metadata and persists when setUserLanguage is called', async () => {
    const sessionDir = await makeTempDir();
    const workDir = await makeTempDir();
    const session = new Session({
      id: 'test-lang-persist-2',
      kaos: testKaos.withCwd(workDir),
      homedir: sessionDir,
      rpc: { emitEvent: vi.fn() } as any,
      skills: { explicitDirs: [join(workDir, 'missing-skills')] },
    });
    const main = await session.createMain();
    const writeSpy = vi.spyOn(session, 'writeMetadata');
    main.setUserLanguage('zh');
    expect(session.metadata.custom['userLanguage']).toBe('zh');
    expect(writeSpy).toHaveBeenCalled();
  });

  it('defaults userLanguage to undefined when metadata has no entry', async () => {
    const sessionDir = await makeTempDir();
    const workDir = await makeTempDir();
    const session = new Session({
      id: 'test-lang-persist-3',
      kaos: testKaos.withCwd(workDir),
      homedir: sessionDir,
      rpc: { emitEvent: vi.fn() } as any,
      skills: { explicitDirs: [join(workDir, 'missing-skills')] },
    });
    const main = await session.createMain();
    expect(main.userLanguage).toBeUndefined();
  });
});
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/session/user-language-persistence.test.ts
```
Expected: `Agent.userLanguage` 始终为 `undefined`，无法从 Session metadata 恢复。

- [ ] Write the minimal implementation:

`packages/agent-core/src/session/index.ts` — 在 `instantiateAgent()` 方法（~line 488-509）的 `new Agent({...})` 调用中添加:

```typescript
      userLanguage: this.metadata.custom?.['userLanguage'],
      setUserLanguage: (lang) => {
        this.metadata.custom ??= {};
        this.metadata.custom['userLanguage'] = lang;
        void this.writeMetadata().catch((error: unknown) => {
          this.log.warn('failed to persist user language metadata', error);
        });
      },
```

在文件顶部添加 import（`import type { SupportedLanguage } from '#/i18n';` 不需要，因为 Session 不直接使用该类型，回调参数类型由 AgentOptions 约束）。

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/session/user-language-persistence.test.ts
```
Expected: 3 tests PASS。

- [ ] Commit: `git add -A && git commit -m "feat: persist userLanguage via Session metadata and AgentOptions callback"`

---

### Task 4: SetOfficeHoursLanguage 工具创建与注册

**Depends on:** Task 3

**Files:**
- Create: `packages/agent-core/src/tools/builtin/office-hours/set-language.md`
- Create: `packages/agent-core/src/tools/builtin/office-hours/set-language.ts`
- Modify: `packages/agent-core/src/tools/builtin/index.ts` +1 line
- Modify: `packages/agent-core/src/agent/tool/index.ts` +2 lines (~421)
- Test: `packages/agent-core/test/tools/builtin/office-hours/set-language.test.ts`

- [ ] Write the failing test — `packages/agent-core/test/tools/builtin/office-hours/set-language.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';
import type { Agent } from '../../../../src/agent';
import { SetOfficeHoursLanguageTool } from '../../../../src/tools/builtin/office-hours/set-language';

function mockAgent(overrides: Partial<{ isActive: boolean; kind: string; userLanguage: string | undefined }> = {}) {
  return {
    sessionMode: {
      isActive: overrides.isActive ?? false,
      kind: overrides.kind ?? 'normal',
    },
    userLanguage: overrides.userLanguage,
    setUserLanguage: vi.fn(),
  } as unknown as Agent;
}

describe('SetOfficeHoursLanguageTool', () => {
  it('sets userLanguage when office-hours is active and code is valid', async () => {
    const agent = mockAgent({ isActive: true, kind: 'office-hours' });
    const tool = new SetOfficeHoursLanguageTool(agent);
    const exec = tool.resolveExecution({ language: 'zh' });
    expect(exec.description).toBe('Setting office hours user language');
    expect(exec.approvalRule).toBe('SetOfficeHoursLanguage');
    const result = await exec.execute();
    expect(agent.setUserLanguage).toHaveBeenCalledWith('zh');
    expect(result.output).toBe('用户语言已设置为 zh。');
  });

  it('rejects with modeNotActive when not in office-hours', async () => {
    const agent = mockAgent({ isActive: false });
    const tool = new SetOfficeHoursLanguageTool(agent);
    const result = await tool.resolveExecution({ language: 'en' }).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Office hours mode is not active.');
  });

  it('rejects unsupported language code', async () => {
    const agent = mockAgent({ isActive: true, kind: 'office-hours' });
    const tool = new SetOfficeHoursLanguageTool(agent);
    const result = await tool.resolveExecution({ language: 'fr' as any }).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Unsupported language: fr');
  });
});
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/set-language.test.ts
```
Expected: 文件不存在。

- [ ] Write the minimal implementation:

`packages/agent-core/src/tools/builtin/office-hours/set-language.md`:

```
Call once at the start of office-hours to record the language the user is writing in. This localizes tool outputs and TUI labels.
```

`packages/agent-core/src/tools/builtin/office-hours/set-language.ts`:

```typescript
import type { Agent } from '#/agent';
import { t, isSupportedLanguage, type SupportedLanguage } from '#/i18n';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './set-language.md';

export const SetOfficeHoursLanguageInputSchema = z.object({
  language: z.string().refine(isSupportedLanguage, {
    message: 'Language must be "en" or "zh"',
  }),
}).strict();
export type SetOfficeHoursLanguageInput = z.infer<typeof SetOfficeHoursLanguageInputSchema>;

export class SetOfficeHoursLanguageTool implements BuiltinTool<SetOfficeHoursLanguageInput> {
  readonly name = 'SetOfficeHoursLanguage' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SetOfficeHoursLanguageInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SetOfficeHoursLanguageInput): ToolExecution {
    return {
      description: 'Setting office hours user language',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
          };
        }

        if (!isSupportedLanguage(args.language)) {
          return {
            isError: true,
            output: `Unsupported language: ${args.language}`,
          };
        }

        this.agent.setUserLanguage(args.language as SupportedLanguage);
        return {
          output: t('officeHours.languageSet', args.language as SupportedLanguage)
            .replace('{language}', args.language),
        };
      },
    };
  }
}
```

`packages/agent-core/src/tools/builtin/index.ts` — 追加 `export * from './office-hours/set-language';`（在第 35 行 `sync-artifact` 之后）。

`packages/agent-core/src/agent/tool/index.ts` — 在 office-hours 工具注册区（~line 421-427）添加:

```typescript
        new b.SetOfficeHoursLanguageTool(this.agent),
```

放在现有 office-hours 工具之前（line 421 `EnterOfficeHoursModeTool` 上方一行）。

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/agent-core test test/tools/builtin/office-hours/set-language.test.ts
```
Expected: 3 tests PASS。

- [ ] Commit: `git add -A && git commit -m "feat: add SetOfficeHoursLanguage tool for in-session language detection"`

---

## Part 1 Local Self-Review

- [ ] 2. Placeholder scan：所有步骤包含完整代码/测试/命令，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：4 个任务均产生新增文件或修改，无虚拟占位。
- [ ] 4. Dependency soundness：Task 2→1, Task 3→2, Task 4→3，无后序依赖。
- [ ] 5. Caller & build soundness：Task 2 新增 `AgentOptions` 字段是可选的，不影响现有调用方；Task 3 仅修改 `instantiateAgent` 内部逻辑；Task 4 新增工具注册。无现有接口破坏。
- [ ] 6. Test-the-risk：所有状态变更（`setUserLanguage`、metadata 写入、工具调用）均有行为断言；语言归一化边界覆盖 `ZH-CN`、`zh-TW`、`fr`、`''`。
- [ ] 7. Type consistency：`SupportedLanguage` 类型在 import 路径 `#/i18n` 下统一定义，其他文件引用一致。
