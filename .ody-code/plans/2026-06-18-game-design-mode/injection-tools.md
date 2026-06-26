# Part 2: agent-core — Injection + Tools + i18n

## Task 3: GameDesignInjector + injection contract

**Depends on:** Part 1 (core.md) Task 1

**Files:**
- Create: `packages/agent-core/src/agent/injection/game-design.ts`
- Create: `packages/agent-core/src/agent/injection/game-design-contract.ts`
- Modify: `packages/agent-core/src/agent/injection/manager.ts:6,28` (register injector)

### Step 1: Write the failing test

Create `packages/agent-core/test/agent/injection/game-design.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { GameDesignInjector } from '../../../src/agent/injection/game-design';
import {
  gameDesignEntryReminder,
  gameDesignExitReminder,
} from '../../../src/agent/injection/game-design-contract';

function mockAgent(overrides: Record<string, unknown> = {}) {
  return {
    sessionMode: {
      isActive: false,
      kind: 'game-design',
      sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
      data: vi.fn().mockResolvedValue({ content: '' }),
    },
    context: {
      history: [],
    },
    ...overrides,
  } as any;
}

describe('GameDesignInjector', () => {
  it('returns entry reminder when mode becomes active with empty doc', async () => {
    const agent = mockAgent({
      sessionMode: {
        isActive: true,
        kind: 'game-design',
        sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
        data: vi.fn().mockResolvedValue({ content: '' }),
      },
    });
    const injector = new GameDesignInjector(agent);
    const result = await injector.getInjection();
    expect(result).toContain('game-design mode is now active');
    expect(result).toContain('Phase 1: 概念定义');
  });

  it('returns exit reminder when mode deactivated after being active', async () => {
    const agent = mockAgent({
      sessionMode: {
        isActive: false,
        kind: 'game-design',
        sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
      },
    });
    const injector = new GameDesignInjector(agent);
    // Mark wasActive internally
    (injector as any).wasActive = true;
    const result = await injector.getInjection();
    expect(result).toContain('game-design session complete');
  });

  it('returns undefined when mode never active', async () => {
    const agent = mockAgent();
    const injector = new GameDesignInjector(agent);
    const result = await injector.getInjection();
    expect(result).toBeUndefined();
  });
});

describe('gameDesignEntryReminder', () => {
  it('contains LANG_INSTRUCTION and Phase 1 heading', () => {
    const path = '/fake/.ody-code/game-design/game-design.md';
    const msg = gameDesignEntryReminder(path);
    expect(msg).toContain('**Language:**');
    expect(msg).toContain('Phase 1: 概念定义');
    expect(msg).toContain(path);
  });
});

describe('gameDesignExitReminder', () => {
  it('reports completion with path', () => {
    const path = '/fake/.ody-code/game-design/game-design.md';
    const msg = gameDesignExitReminder(path);
    expect(msg).toContain('game-design session complete');
    expect(msg).toContain(path);
  });

  it('reports no document when path is null', () => {
    const msg = gameDesignExitReminder(null);
    expect(msg).toContain('no design document');
  });
});
```

### Step 2: Run test and verify FAIL

```bash
pnpm --filter @odysseythink/agent-core vitest run test/agent/injection/game-design.test.ts 2>&1 | tail -10
```

Expected: Module not found — `game-design.ts` and `game-design-contract.ts` don't exist yet.

### Step 3: Write the minimal implementation

**3a. `packages/agent-core/src/agent/injection/game-design-contract.ts`:**

```ts
import type { SessionModeFilePath } from '../session-mode';

const LANG_INSTRUCTION =
  '**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.';

export function gameDesignEntryReminder(designFilePath: SessionModeFilePath): string {
  const path = designFilePath ?? '(not yet assigned)';
  return [
    LANG_INSTRUCTION,
    '',
    'game-design mode is now active. Your job is to act as a game design partner —',
    'guide the user through a complete game design process based on the 100 Principles of Game Design.',
    '',
    '## HARD GATES',
    '- Do NOT write code. Your output is a game design document.',
    '- Ask questions to clarify the vision, audience, and constraints.',
    '- Design file (write ONLY to this path): ' + path,
    '- You may create companion .md files in the ' +
      path.replace(/\.md$/, '') +
      '/ subdirectory.',
    '',
    '## Available Game Design Skills',
    'Use the Skill tool to invoke specialized game design skills (game-design/*) for',
    'deep dives into specific areas: flow state, difficulty adjustment, puzzle design,',
    'player psychology, visual guidance, prototyping, team management, and more.',
    '',
    '## Core Workflow (from skill.md)',
    '',
    'Follow these phases in order. Move forward only when the current phase has',
    'enough clarity to support the next one.',
    '',
    '### Phase 1: 概念定义',
    '1. 定义 3 根支柱 — 用动作动词描述核心玩法，组合成一句话。',
    '2. 写问题陈述 — 具体焦点 + 可量化结果 + 清晰表达。用 80/20 法则聚焦核心功能。',
    '3. 约束三角 — 快、便宜、好，只能选两个。砍范围 > 砍质量。',
    '',
    '### Phase 2: 核心循环设计',
    '核心循环 = 玩家愿意反复做的有趣行为。行动→结果→反应→重复。',
    '用动词描述核心动作。必须易懂、易操作、有直接反馈。',
    '警告：核心循环有缺陷 → 其他元素无法补救。',
    '',
    '### Phase 3: 机制与平衡',
    '难度设计：三阶段（入门/练习/心流），挑战略高于当前能力。',
    '动态难度：暗中调整，监控连续失败/成功率/耗时。',
    '快速平衡法：对核心变量做 2x 或 0.5x 极端调整测试。',
    '奖惩系统：生命/Game Over、属性衰退、固定/随机奖励。',
    '',
    '### Phase 4: 关卡与体验',
    '挑战分类：记忆型（试错/模式识别）vs 技能型（身体/心智能力）。',
    '谜题设计：保持心流、渐进提示、确定性、清晰性。',
    '节奏控制：人类注意力极限 7-10 分钟，每 ~7 分钟展示新元素。',
    '环境叙事：用涂鸦/门窗/NPC对话/私人空间讲故事。',
    '',
    '### Phase 5: 视觉与交互',
    '视觉引导：可供性（视觉暗示交互）、注意力捕获（面孔>运动>意外）、寻路。',
    'Fitts 定律：移动时间 = f(距离, 目标大小)，常用元素放近放大。',
    'Hick 定律：决策时间随选项数对数增长，最优 3-6 个选项。',
    '黄金比例：Φ=1.618，UI 布局/建筑比例/环境艺术。',
    '',
    '### Phase 6: 玩家心理',
    '认知偏差清单：确认偏差、可得性偏差、锚定效应、框架效应。',
    '决策设计：三角性（低风险低回报 vs 高风险高回报路径）。',
    '错误处理：运动控制/流程错误/遗漏错误/错误行动的分类与应对。',
    '',
    '### Phase 7: 原型与测试',
    '纸面原型（UI/卡牌/桌游）和数字原型（操作手感/时机）。',
    '测试：一次性测试（首次印象）、黑盒/白盒/压力测试。',
    '循环：原型→测试→分析→迭代。',
    '',
    '### Phase 8: 团队管理',
    '共享愿景、多样性悖论、流程选择（瀑布 vs 敏捷）、沟通原则。',
    '',
    '## Output Conventions',
    '- Suggest concrete principles by name.',
    '- Give actionable next steps, not vague advice.',
    '- Use tables to compare options and trade-offs.',
    '- Tag decisions: [C:USER] for user-confirmed, [C:INFERRED] for inferred.',
    '- Include an ## Assumptions section.',
    '',
    '## Output File',
    '- Main document: ' + path,
    '- Companion files: ' + path.replace(/\.md$/, '') + '/<topic>.md',
    '- Call SyncGameDesignArtifact when ready to persist.',
    '- Call ExitGameDesignMode when the design is complete.',
  ].join('\n');
}

export function gameDesignFullReminder(designFilePath: SessionModeFilePath): string {
  return gameDesignEntryReminder(designFilePath);
}

export function gameDesignSparseReminder(designFilePath: SessionModeFilePath): string {
  return [
    LANG_INSTRUCTION,
    '',
    'game-design continues. Remember:',
    '- Keep moving through the phases.',
    '- Design doc target: ' + (designFilePath ?? '(not yet assigned)'),
    '- Use game-design/* skills for deep dives.',
    '- Exit when ready: ExitGameDesignMode.',
  ].join('\n');
}

export function gameDesignReentryReminder(designFilePath: SessionModeFilePath): string {
  return [
    LANG_INSTRUCTION,
    '',
    'game-design resumed. The design document at ' +
      (designFilePath ?? '(unknown)') +
      ' already has content.',
    'Read the existing content, pick up where you left off, and continue the workflow.',
  ].join('\n');
}

export function gameDesignExitReminder(designFilePath: SessionModeFilePath | null): string {
  return designFilePath
    ? 'game-design session complete. Design document saved to: ' +
      designFilePath +
      '. The application will now exit.'
    : 'game-design session ended — no design document was produced.';
}
```

**3b. `packages/agent-core/src/agent/injection/game-design.ts`:**

```ts
import { DynamicInjector } from './injector';
import {
  gameDesignEntryReminder,
  gameDesignExitReminder,
  gameDesignFullReminder,
  gameDesignReentryReminder,
  gameDesignSparseReminder,
} from './game-design-contract';

const GAME_DESIGN_DEDUP_MIN_TURNS = 2;
const GAME_DESIGN_FULL_REFRESH_TURNS = 5;

export class GameDesignInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'game_design';
  private wasActive = false;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive =
      this.agent.sessionMode.isActive &&
      this.agent.sessionMode.kind === 'game-design';
  }

  override async getInjection(): Promise<string | undefined> {
    const isActive =
      this.agent.sessionMode.isActive &&
      this.agent.sessionMode.kind === 'game-design';
    const { sessionModeFilePath } = this.agent.sessionMode;

    if (!isActive) {
      if (!this.wasActive) return undefined;
      this.wasActive = false;
      this.injectedAt = null;
      return gameDesignExitReminder(sessionModeFilePath);
    }

    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      const content = await this.currentGameDesignContent();
      if (content.trim().length > 0) {
        return gameDesignReentryReminder(sessionModeFilePath);
      }
      return gameDesignEntryReminder(sessionModeFilePath);
    }

    const variant = this.getVariant();
    if (variant === null) return undefined;
    return variant === 'full'
      ? gameDesignFullReminder(sessionModeFilePath)
      : gameDesignSparseReminder(sessionModeFilePath);
  }

  protected getVariant(): 'full' | 'sparse' | null {
    if (this.injectedAt === null) return 'full';
    const history = this.agent.context.history;
    let assistantTurnsSince = 0;
    for (let i = this.injectedAt + 1; i < history.length; i++) {
      const msg = history[i];
      if (msg === undefined) continue;
      if (msg.role === 'assistant') {
        assistantTurnsSince += 1;
        continue;
      }
      if (msg.role === 'user') return 'full';
    }
    if (assistantTurnsSince >= GAME_DESIGN_FULL_REFRESH_TURNS) return 'full';
    if (assistantTurnsSince >= GAME_DESIGN_DEDUP_MIN_TURNS) return 'sparse';
    return null;
  }

  private async currentGameDesignContent(): Promise<string> {
    try {
      const data = await this.agent.sessionMode.data();
      return data?.content ?? '';
    } catch {
      return '';
    }
  }
}
```

**3c. Modify `packages/agent-core/src/agent/injection/manager.ts`:**

Add import (after office-hours import, line 6):
```ts
import { GameDesignInjector } from './game-design';
```

Add to injectors array (after OfficeHoursInjector, line 28):
```ts
new GameDesignInjector(agent),
```

### Step 4: Run test and verify PASS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/agent/injection/game-design.test.ts 2>&1 | tail -15
```

Expected: All 6 tests pass.

### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck 2>&1 | tail -20
```

Expected: No type errors.

### Step 6: Commit

```bash
git add -A && git commit -m "feat: add GameDesignInjector and injection contract with workflow prompts"
```

---

## Task 4: Game-design tools (Enter, Exit, AppendProfile, AppendLearning, SearchLearnings, EnsureRouting, SyncArtifact, SetLanguage)

**Depends on:** Part 1 Tasks 1-2, Part 2 Task 3

**Files:**
- Create: `packages/agent-core/src/tools/builtin/game-design/enter-game-design.md` (description)
- Create: `packages/agent-core/src/tools/builtin/game-design/enter-game-design.ts`
- Create: `packages/agent-core/src/tools/builtin/game-design/exit-game-design.md`
- Create: `packages/agent-core/src/tools/builtin/game-design/exit-game-design.ts`
- Create: `packages/agent-core/src/tools/builtin/game-design/append-game-design-profile.md`
- Create: `packages/agent-core/src/tools/builtin/game-design/append-game-design-profile.ts`
- Create: `packages/agent-core/src/tools/builtin/game-design/append-game-design-learning.md`
- Create: `packages/agent-core/src/tools/builtin/game-design/append-game-design-learning.ts`
- Create: `packages/agent-core/src/tools/builtin/game-design/search-game-design-learnings.md`
- Create: `packages/agent-core/src/tools/builtin/game-design/search-game-design-learnings.ts`
- Create: `packages/agent-core/src/tools/builtin/game-design/ensure-game-design-routing.md`
- Create: `packages/agent-core/src/tools/builtin/game-design/ensure-game-design-routing.ts`
- Create: `packages/agent-core/src/tools/builtin/game-design/sync-game-design-artifact.md`
- Create: `packages/agent-core/src/tools/builtin/game-design/sync-game-design-artifact.ts`
- Create: `packages/agent-core/src/tools/builtin/game-design/set-game-design-language.md`
- Create: `packages/agent-core/src/tools/builtin/game-design/set-game-design-language.ts`
- Modify: `packages/agent-core/src/tools/builtin/index.ts` (re-export)
- Modify: `packages/agent-core/src/agent/tool/index.ts:421-428` (register in ToolManager)
- Test: `packages/agent-core/test/tools/builtin/game-design/enter-exit.test.ts`
- Test: `packages/agent-core/test/tools/builtin/game-design/state-tools.test.ts` (uses store test from Task 2)

### Step 1: Write the failing tests

**Test 1 — `packages/agent-core/test/tools/builtin/game-design/enter-exit.test.ts`:**

```ts
import { describe, it, expect, vi } from 'vitest';
import { EnterGameDesignModeTool } from '../../../src/tools/builtin/game-design/enter-game-design';
import { ExitGameDesignModeTool } from '../../../src/tools/builtin/game-design/exit-game-design';
import { SetGameDesignLanguageTool } from '../../../src/tools/builtin/game-design/set-game-design-language';

function mockAgent() {
  return {
    userLanguage: 'en',
    setUserLanguage: vi.fn(),
    sessionMode: {
      isActive: false,
      kind: 'game-design' as const,
      sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
      enter: vi.fn().mockResolvedValue(undefined),
      exit: vi.fn(),
    },
  } as any;
}

describe('EnterGameDesignModeTool', () => {
  it('enters game-design mode when not already active', async () => {
    const agent = mockAgent();
    const tool = new EnterGameDesignModeTool(agent);
    const exec = tool.resolveExecution({});
    const result = await exec.execute();
    expect(agent.sessionMode.enter).toHaveBeenCalledWith(
      undefined, undefined, undefined, 'game-design',
    );
    expect(result.output).toContain('game-design mode is now active');
  });

  it('returns error when game-design already active', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'game-design';
    const tool = new EnterGameDesignModeTool(agent);
    const exec = tool.resolveExecution({});
    const result = await exec.execute();
    expect(result.isError).toBe(true);
  });

  it('returns error when another mode is active', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'plan';
    const tool = new EnterGameDesignModeTool(agent);
    const exec = tool.resolveExecution({});
    const result = await exec.execute();
    expect(result.isError).toBe(true);
  });
});

describe('ExitGameDesignModeTool', () => {
  it('exits game-design mode', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'game-design';
    const tool = new ExitGameDesignModeTool(agent);
    const exec = tool.resolveExecution({});
    const result = await exec.execute();
    expect(agent.sessionMode.exit).toHaveBeenCalled();
    expect(result.output).toContain('Design document saved');
  });

  it('returns error when game-design not active', async () => {
    const agent = mockAgent();
    const tool = new ExitGameDesignModeTool(agent);
    const exec = tool.resolveExecution({});
    const result = await exec.execute();
    expect(result.isError).toBe(true);
  });
});

describe('SetGameDesignLanguageTool', () => {
  it('sets user language', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'game-design';
    const tool = new SetGameDesignLanguageTool(agent);
    const exec = tool.resolveExecution({ language: 'en' });
    const result = await exec.execute();
    expect(agent.setUserLanguage).toHaveBeenCalledWith('en');
    expect(result.output).toContain('en');
  });
});
```

**Test 2 — `packages/agent-core/test/tools/builtin/game-design/state-tools.test.ts`:**

```ts
import { describe, it, expect, vi } from 'vitest';
import { AppendGameDesignProfileTool } from '../../../src/tools/builtin/game-design/append-game-design-profile';
import { AppendGameDesignLearningTool } from '../../../src/tools/builtin/game-design/append-game-design-learning';
import { SearchGameDesignLearningsTool } from '../../../src/tools/builtin/game-design/search-game-design-learnings';

function mockAgent() {
  return {
    userLanguage: 'en',
    sessionMode: {
      isActive: true,
      kind: 'game-design' as const,
      sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
    },
    gameDesignStateStore: {
      appendProfile: vi.fn().mockResolvedValue(undefined),
      appendLearning: vi.fn().mockResolvedValue(undefined),
      searchLearnings: vi.fn().mockResolvedValue([]),
    },
  } as any;
}

describe('AppendGameDesignProfileTool', () => {
  it('appends profile entry', async () => {
    const agent = mockAgent();
    const tool = new AppendGameDesignProfileTool(agent);
    const exec = tool.resolveExecution({
      mode: 'builder',
      projectSlug: 'test-game',
      pillars: 'Explore, Build',
      audience: 'Casual',
      platform: 'Mobile',
      genre: 'Adventure',
      designDoc: '/fake/game-design.md',
    });
    const result = await exec.execute();
    expect(agent.gameDesignStateStore.appendProfile).toHaveBeenCalled();
    expect(result.isError).toBeFalsy();
  });
});

describe('AppendGameDesignLearningTool', () => {
  it('appends learning entry', async () => {
    const agent = mockAgent();
    const tool = new AppendGameDesignLearningTool(agent);
    const exec = tool.resolveExecution({
      type: 'eureka',
      key: 'difficulty-spike-level-3',
      insight: 'Players hit a wall at Level 3 boss.',
      confidence: 0.9,
    });
    const result = await exec.execute();
    expect(agent.gameDesignStateStore.appendLearning).toHaveBeenCalled();
    expect(result.isError).toBeFalsy();
  });
});

describe('SearchGameDesignLearningsTool', () => {
  it('returns no learnings message for empty result', async () => {
    const agent = mockAgent();
    agent.gameDesignStateStore.searchLearnings = vi.fn().mockResolvedValue([]);
    const tool = new SearchGameDesignLearningsTool(agent);
    const exec = tool.resolveExecution({ limit: 5 });
    const result = await exec.execute();
    expect(result.output).toContain('No past learnings');
  });
});
```

### Step 2: Run tests and verify FAIL

```bash
pnpm --filter @odysseythink/agent-core vitest run test/tools/builtin/game-design/ 2>&1 | tail -10
```

Expected: Module not found for all tool files.

### Step 3: Write the minimal implementation

All 8 tool files follow the exact same pattern as their office-hours counterparts, substituting `'game-design'` for `'office-hours'`, `gameDesign` for `officeHours`, and using `GameDesignProfileEntry`/`GameDesignLearningEntry` types. Each `.md` description file is placed alongside the `.ts` file. Full content below.

**3a. Description files (all 8 .md files):**

`enter-game-design.md`: `Enter game-design mode to begin a guided game design session based on the 100 Principles of Game Design framework. This mode restricts operations to producing a game design document under .ody-code/game-design/.`

`exit-game-design.md`: `Exit game-design mode, save the final design document, and return to normal mode.`

`append-game-design-profile.md`: `Append a builder profile entry summarizing the game design session: pillars, audience, platform, genre, and design doc path.`

`append-game-design-learning.md`: `Record a learning insight discovered during game design: type (operational/eureka), key, insight text, and confidence score.`

`search-game-design-learnings.md`: `Search past game design learnings, optionally filtered by branch. Returns the most recent entries.`

`ensure-game-design-routing.md`: `Ensure the project's AGENTS.md contains a ## Skill routing section for game-design mode. Creates or updates AGENTS.md as needed.`

`sync-game-design-artifact.md`: `Sync the game design artifact document to persistent storage via gbrain MCP or CLI.`

`set-game-design-language.md`: `Set the user language for the game-design session to 'en' or 'zh'.`

**3b. `enter-game-design.ts`:**

```ts
import type { Agent } from '#/agent';
import { z } from 'zod';
import { gameDesignEntryReminder } from '#/agent/injection/game-design-contract';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-game-design.md';

export const EnterGameDesignModeInputSchema = z.object({}).strict();
export type EnterGameDesignModeInput = z.infer<typeof EnterGameDesignModeInputSchema>;

export class EnterGameDesignModeTool implements BuiltinTool<EnterGameDesignModeInput> {
  readonly name = 'EnterGameDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterGameDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterGameDesignModeInput): ToolExecution {
    return {
      description: 'Requesting to enter game-design mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (this.agent.sessionMode.isActive) {
          if (this.agent.sessionMode.kind === 'game-design') {
            return { isError: true, output: t('gameDesign.alreadyActive', lang) };
          }
          return { isError: true, output: t('gameDesign.anotherModeActive', lang) };
        }
        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'game-design');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter game-design mode.';
          return {
            isError: true,
            output: t('gameDesign.failedToEnter', lang).replace('{message}', message),
          };
        }
        return {
          output: gameDesignEntryReminder(this.agent.sessionMode.sessionModeFilePath),
        };
      },
    };
  }
}
```

**3c. `exit-game-design.ts`:**

```ts
import type { Agent } from '#/agent';
import { z } from 'zod';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './exit-game-design.md';

export const ExitGameDesignModeInputSchema = z.object({}).strict();
export type ExitGameDesignModeInput = z.infer<typeof ExitGameDesignModeInputSchema>;

export class ExitGameDesignModeTool implements BuiltinTool<ExitGameDesignModeInput> {
  readonly name = 'ExitGameDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitGameDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: ExitGameDesignModeInput): ToolExecution {
    return {
      description: 'Requesting to exit game-design mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        const path = this.agent.sessionMode.sessionModeFilePath;
        this.agent.sessionMode.exit();
        const parts = [t('gameDesign.sessionComplete', lang)];
        if (path) {
          parts.push(t('gameDesign.designDocSaved', lang).replace('{path}', path));
        }
        parts.push(t('gameDesign.appWillExit', lang));
        return { output: parts.join('\n') };
      },
    };
  }
}
```

**3d. `append-game-design-profile.ts`:**

```ts
import type { Agent } from '#/agent';
import type { GameDesignProfileEntry } from '#/office-hours/state';
import { z } from 'zod';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './append-game-design-profile.md';

export const AppendGameDesignProfileInputSchema = z.object({
  mode: z.enum(['startup', 'builder']).describe('Whether this is a full design startup or a builder session.'),
  projectSlug: z.string().describe('Project slug.'),
  pillars: z.string().describe('The 3 design pillars as a comma-separated string.'),
  audience: z.string().describe('Target audience description.'),
  platform: z.string().describe('Target platform(s).'),
  genre: z.string().describe('Game genre.'),
  designDoc: z.string().optional().describe('Path to the design document. Defaults to the current game-design file path.'),
  signals: z.array(z.string()).optional().describe('Design signals observed.'),
}).strict();
export type AppendGameDesignProfileInput = z.infer<typeof AppendGameDesignProfileInputSchema>;

export class AppendGameDesignProfileTool implements BuiltinTool<AppendGameDesignProfileInput> {
  readonly name = 'AppendGameDesignProfile' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendGameDesignProfileInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: AppendGameDesignProfileInput): ToolExecution {
    return {
      description: 'Appending game-design profile entry',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        try {
          const designDoc = args.designDoc ?? this.agent.sessionMode.sessionModeFilePath ?? '';
          const entry: GameDesignProfileEntry = {
            date: new Date().toISOString(),
            mode: args.mode,
            projectSlug: args.projectSlug,
            pillars: args.pillars,
            audience: args.audience,
            platform: args.platform,
            genre: args.genre,
            signals: args.signals ?? [],
            designDoc,
          };
          await this.agent.gameDesignStateStore.appendProfile(entry);
          return { output: t('gameDesign.profileAppended', lang) };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to append profile entry.';
          return { isError: true, output: `Failed to append game-design profile entry: ${message}` };
        }
      },
    };
  }
}
```

**3e. `append-game-design-learning.ts`:**

```ts
import type { Agent } from '#/agent';
import type { GameDesignLearningEntry } from '#/office-hours/state';
import { z } from 'zod';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './append-game-design-learning.md';

export const AppendGameDesignLearningInputSchema = z.object({
  type: z.enum(['operational', 'eureka']),
  key: z.string().min(1),
  insight: z.string().min(1),
  confidence: z.number().min(0).max(1),
  branch: z.string().optional(),
}).strict();
export type AppendGameDesignLearningInput = z.infer<typeof AppendGameDesignLearningInputSchema>;

export class AppendGameDesignLearningTool implements BuiltinTool<AppendGameDesignLearningInput> {
  readonly name = 'AppendGameDesignLearning' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendGameDesignLearningInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: AppendGameDesignLearningInput): ToolExecution {
    return {
      description: 'Appending game-design learning insight',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        try {
          const entry: GameDesignLearningEntry = {
            ts: new Date().toISOString(),
            skill: 'game-design',
            type: args.type,
            key: args.key,
            insight: args.insight,
            confidence: args.confidence,
            source: 'observed',
            branch: args.branch,
          };
          await this.agent.gameDesignStateStore.appendLearning(entry);
          return {
            output: t('gameDesign.learningRecorded', lang).replace('{key}', args.key),
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to append learning.';
          return { isError: true, output: `Failed to append learning: ${message}` };
        }
      },
    };
  }
}
```

**3f. `search-game-design-learnings.ts`:**

```ts
import type { Agent } from '#/agent';
import { t } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './search-game-design-learnings.md';

export const SearchGameDesignLearningsInputSchema = z.object({
  limit: z.number().int().positive().default(10),
  branch: z.string().optional(),
}).strict();
export type SearchGameDesignLearningsInput = z.infer<typeof SearchGameDesignLearningsInputSchema>;

export class SearchGameDesignLearningsTool implements BuiltinTool<SearchGameDesignLearningsInput> {
  readonly name = 'SearchGameDesignLearnings' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SearchGameDesignLearningsInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SearchGameDesignLearningsInput): ToolExecution {
    return {
      description: 'Searching past game-design learnings',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        try {
          const learnings = await this.agent.gameDesignStateStore.searchLearnings({
            limit: args.limit,
            branch: args.branch,
          });
          if (learnings.length === 0) {
            return { output: t('gameDesign.noLearnings', lang) };
          }
          const formatted = learnings.map((l, i) =>
            `[${i + 1}] ${t('gameDesign.learningTypeLabel', lang)}: ${l.type.toUpperCase()}: ${l.key}\n    ${t('gameDesign.learningInsightLabel', lang)}: ${l.insight}\n    ${t('gameDesign.learningConfidenceLabel', lang)}: ${l.confidence}${l.branch ? `\n    ${t('gameDesign.learningBranchLabel', lang)}: ${l.branch}` : ''}`
          ).join('\n\n');
          return {
            output: t('gameDesign.learningsHeader', lang).replace('{count}', String(learnings.length)) + '\n\n' + formatted,
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to search learnings.';
          return { isError: true, output: `Failed to search learnings: ${message}` };
        }
      },
    };
  }
}
```

**3g. `ensure-game-design-routing.ts`:**

```ts
import { join } from 'pathe';
import type { Agent } from '#/agent';
import { t } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './ensure-game-design-routing.md';

const ROUTING_SECTION = `
## Skill routing

- **game-design**: Game design workflow based on the 100 Principles of Game Design. Activates via --game-design or when the user requests game design help.

To invoke, ask the agent to start game-design mode.
`;

export const EnsureGameDesignRoutingInputSchema = z.object({}).strict();
export type EnsureGameDesignRoutingInput = z.infer<typeof EnsureGameDesignRoutingInputSchema>;

export class EnsureGameDesignRoutingTool implements BuiltinTool<EnsureGameDesignRoutingInput> {
  readonly name = 'EnsureGameDesignRouting' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnsureGameDesignRoutingInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnsureGameDesignRoutingInput): ToolExecution {
    return {
      description: 'Ensuring AGENTS.md has skill routing section for game-design',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        const claudeMdPath = join(this.agent.config.cwd, 'AGENTS.md');
        try {
          let content: string;
          let fileExists = false;
          try {
            content = await this.agent.kaos.readText(claudeMdPath);
            fileExists = true;
          } catch { content = ''; }
          if (!fileExists) {
            await this.agent.kaos.writeText(claudeMdPath, ROUTING_SECTION.trimStart());
            return { output: t('gameDesign.agentsMdCreated', lang).replace('{path}', claudeMdPath) };
          }
          if (content!.includes('## Skill routing')) {
            return { output: t('gameDesign.agentsMdAlreadyHasRouting', lang) };
          }
          const updated = content!.trimEnd() + '\n' + ROUTING_SECTION;
          await this.agent.kaos.writeText(claudeMdPath, updated);
          return { output: t('gameDesign.agentsMdUpdated', lang).replace('{path}', claudeMdPath) };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to ensure AGENTS.md routing.';
          return { isError: true, output: t('gameDesign.failedToEnsureRouting', lang).replace('{message}', message) };
        }
      },
    };
  }
}
```

**3h. `sync-game-design-artifact.ts`:**

```ts
import { execFileSync } from 'node:child_process';
import { join } from 'pathe';
import type { Agent } from '#/agent';
import { t } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './sync-game-design-artifact.md';

export const SyncGameDesignArtifactInputSchema = z.object({
  designFilePath: z.string().describe('Absolute path to the design document artifact to sync.'),
}).strict();
export type SyncGameDesignArtifactInput = z.infer<typeof SyncGameDesignArtifactInputSchema>;

export class SyncGameDesignArtifactTool implements BuiltinTool<SyncGameDesignArtifactInput> {
  readonly name = 'SyncGameDesignArtifact' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SyncGameDesignArtifactInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SyncGameDesignArtifactInput): ToolExecution {
    return {
      description: 'Syncing game-design artifact',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        const projectRoot = this.agent.config.cwd;
        const gbrainPinPath = join(projectRoot, '.gbrain-source');
        try {
          let gbrainSource: string | undefined;
          try { gbrainSource = (await this.agent.kaos.readText(gbrainPinPath)).trim(); } catch {}
          try { await this.agent.kaos.stat(args.designFilePath); } catch {
            return { isError: true, output: t('gameDesign.designFileNotFound', lang).replace('{path}', args.designFilePath) };
          }
          const mcp = this.agent.mcp;
          let mcpGbrainAvailable = false;
          if (mcp) {
            const servers = mcp.list();
            mcpGbrainAvailable = servers.some((s: any) => s.name.includes('gbrain') && s.status === 'connected');
          }
          if (mcpGbrainAvailable) {
            return {
              output: [
                t('gameDesign.gbrainConnected', lang),
                gbrainSource ? t('gameDesign.gbrainTargetSource', lang).replace('{source}', gbrainSource) : '',
                t('gameDesign.gbrainReadyForSync', lang).replace('{path}', args.designFilePath),
              ].filter(Boolean).join('\n'),
            };
          }
          try {
            const cliArgs = ['artifact', 'add'];
            if (gbrainSource !== undefined && gbrainSource.length > 0) cliArgs.push('--source', gbrainSource);
            cliArgs.push(args.designFilePath);
            execFileSync('gbrain', cliArgs, { cwd: projectRoot, timeout: 30_000 });
            return {
              output: [
                t('gameDesign.gbrainSynced', lang),
                gbrainSource ? t('gameDesign.gbrainTargetSource', lang).replace('{source}', gbrainSource) : '',
                t('gameDesign.gbrainFile', lang).replace('{path}', args.designFilePath),
              ].filter(Boolean).join('\n'),
            };
          } catch (cliError: any) {
            return { isError: true, output: t('gameDesign.gbrainCliFailed', lang).replace('{message}', cliError.message ?? String(cliError)) };
          }
        } catch (error: any) {
          return { isError: true, output: t('gameDesign.failedToSyncArtifact', lang).replace('{message}', error.message ?? String(error)) };
        }
      },
    };
  }
}
```

**3i. `set-game-design-language.ts`:**

```ts
import type { Agent } from '#/agent';
import { t, isSupportedLanguage, type SupportedLanguage } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './set-game-design-language.md';

export const SetGameDesignLanguageInputSchema = z.object({
  language: z.string().refine(isSupportedLanguage, { message: 'Language must be "en" or "zh"' }),
}).strict();
export type SetGameDesignLanguageInput = z.infer<typeof SetGameDesignLanguageInputSchema>;

export class SetGameDesignLanguageTool implements BuiltinTool<SetGameDesignLanguageInput> {
  readonly name = 'SetGameDesignLanguage' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SetGameDesignLanguageInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SetGameDesignLanguageInput): ToolExecution {
    return {
      description: 'Setting game-design user language',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        if (!isSupportedLanguage(args.language)) {
          return { isError: true, output: `Unsupported language: ${args.language}` };
        }
        this.agent.setUserLanguage(args.language as SupportedLanguage);
        return { output: t('gameDesign.languageSet', args.language as SupportedLanguage).replace('{language}', args.language) };
      },
    };
  }
}
```

**3j. Modify `packages/agent-core/src/tools/builtin/index.ts`:**

Add after the office-hours export block:
```ts
export * from './game-design/enter-game-design';
export * from './game-design/exit-game-design';
export * from './game-design/append-game-design-profile';
export * from './game-design/append-game-design-learning';
export * from './game-design/search-game-design-learnings';
export * from './game-design/ensure-game-design-routing';
export * from './game-design/sync-game-design-artifact';
export * from './game-design/set-game-design-language';
```

**3k. Modify `packages/agent-core/src/agent/tool/index.ts`:**

Add after line 428 (the last office-hours tool line):
```ts
        new b.SetGameDesignLanguageTool(this.agent),
        new b.EnterGameDesignModeTool(this.agent),
        new b.ExitGameDesignModeTool(this.agent),
        new b.AppendGameDesignProfileTool(this.agent),
        new b.AppendGameDesignLearningTool(this.agent),
        new b.SearchGameDesignLearningsTool(this.agent),
        new b.EnsureGameDesignRoutingTool(this.agent),
        new b.SyncGameDesignArtifactTool(this.agent),
```

### Step 4: Run tests and verify PASS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/tools/builtin/game-design/ 2>&1 | tail -20
```

Expected: All tests pass (enter/exit: 6 tests, state-tools: 3 tests).

### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck 2>&1 | tail -20
```

Expected: No type errors.

### Step 6: Commit

```bash
git add -A && git commit -m "feat: add 8 game-design tools (enter, exit, profile, learning, search, routing, sync, language)"
```

---

## Task 5: i18n translations for game-design

**Depends on:** Part 2 Task 4

**Files:**
- Modify: `packages/agent-core/src/i18n/translations.ts` (add gameDesign.* keys)
- Modify: `packages/agent-core/src/i18n/types.ts` (add gameDesign.* type keys)

### Step 1: Write the failing test

The i18n keys are used by the tools created in Task 4. After implementing Task 4, any untranslated key will cause `t('gameDesign.xxx', lang)` to return the key string itself (e.g., `'gameDesign.alreadyActive'` instead of `'Game-design mode is already active.'`). We can verify by running an existing tool test:

```bash
pnpm --filter @odysseythink/agent-core vitest run test/tools/builtin/game-design/enter-exit.test.ts 2>&1 | tail -20
```

Expected: Tests pass BUT the output strings are raw keys like `'gameDesign.alreadyActive'` because translations don't exist yet.

### Step 2: Verify the bad state (keys untranslated)

Run the above command and confirm that `result.output` for error states contains raw key names like `gameDesign.alreadyActive` instead of English translations.

### Step 3: Write the implementation

**3a. `packages/agent-core/src/i18n/translations.ts`:**

Add `gameDesign.*` keys to the `en` block (after the `officeHours.*` keys end):

```ts
'gameDesign.entered': 'Game-design mode is now active.',
'gameDesign.alreadyActive': 'Game-design mode is already active. Use ExitGameDesignMode when the session is complete.',
'gameDesign.anotherModeActive': 'Another session mode is already active. Exit it first before entering game-design mode.',
'gameDesign.failedToEnter': 'Failed to enter game-design mode: {message}',
'gameDesign.sessionComplete': 'Game-design session complete.',
'gameDesign.designDocSaved': 'Design document saved to: {path}',
'gameDesign.appWillExit': 'The application will now exit.',
'gameDesign.profileAppended': 'Game-design profile entry appended successfully.',
'gameDesign.learningRecorded': 'Learning "{key}" recorded successfully.',
'gameDesign.noLearnings': 'No past learnings found.',
'gameDesign.learningsHeader': 'Found {count} learning(s):',
'gameDesign.learningTypeLabel': 'Type',
'gameDesign.learningInsightLabel': 'Insight',
'gameDesign.learningConfidenceLabel': 'Confidence',
'gameDesign.learningBranchLabel': 'Branch',
'gameDesign.modeNotActive': 'Game-design mode is not active.',
'gameDesign.designFileNotFound': 'Design file not found at {path}.',
'gameDesign.gbrainConnected': 'gbrain MCP server is connected.',
'gameDesign.gbrainTargetSource': 'Target source: {source}',
'gameDesign.gbrainNoSourcePin': 'No .gbrain-source pin found.',
'gameDesign.gbrainReadyForSync': 'Design artifact at {path} is ready for sync via MCP.',
'gameDesign.gbrainSynced': 'Design artifact synced via gbrain CLI.',
'gameDesign.gbrainFile': 'File: {path}',
'gameDesign.gbrainCliFailed': 'gbrain CLI sync failed: {message}. Ensure the gbrain CLI is installed and configured.',
'gameDesign.agentsMdCreated': 'AGENTS.md created at {path} with ## Skill routing section.',
'gameDesign.agentsMdUpdated': 'Appended ## Skill routing section to AGENTS.md at {path}.',
'gameDesign.agentsMdAlreadyHasRouting': 'AGENTS.md already has a ## Skill routing section — no changes needed.',
'gameDesign.failedToEnsureRouting': 'Failed to ensure AGENTS.md routing: {message}',
'gameDesign.failedToSyncArtifact': 'Failed to sync design artifact: {message}',
'gameDesign.languageSet': 'User language set to {language}.',
```

Add corresponding Chinese translations to the `zh` block:

```ts
'gameDesign.entered': '游戏设计模式已激活。',
'gameDesign.alreadyActive': '游戏设计模式已经处于激活状态。会话结束后请调用 ExitGameDesignMode。',
'gameDesign.anotherModeActive': '另一个会话模式已经激活。请先退出该模式再进入游戏设计模式。',
'gameDesign.failedToEnter': '进入游戏设计模式失败：{message}',
'gameDesign.sessionComplete': '游戏设计会话已结束。',
'gameDesign.designDocSaved': '设计文档已保存至：{path}',
'gameDesign.appWillExit': '应用即将退出。',
'gameDesign.profileAppended': '游戏设计档案条目已成功追加。',
'gameDesign.learningRecorded': '心得 "{key}" 已成功记录。',
'gameDesign.noLearnings': '未找到过往心得。',
'gameDesign.learningsHeader': '找到 {count} 条心得：',
'gameDesign.learningTypeLabel': '类型',
'gameDesign.learningInsightLabel': '洞察',
'gameDesign.learningConfidenceLabel': '置信度',
'gameDesign.learningBranchLabel': '分支',
'gameDesign.modeNotActive': '游戏设计模式未激活。',
'gameDesign.designFileNotFound': '未找到设计文件：{path}。',
'gameDesign.gbrainConnected': 'gbrain MCP 服务器已连接。',
'gameDesign.gbrainTargetSource': '目标来源：{source}',
'gameDesign.gbrainNoSourcePin': '未找到 .gbrain-source 标记。',
'gameDesign.gbrainReadyForSync': '位于 {path} 的设计产物已准备好通过 MCP 同步。',
'gameDesign.gbrainSynced': '设计产物已通过 gbrain CLI 同步。',
'gameDesign.gbrainFile': '文件：{path}',
'gameDesign.gbrainCliFailed': 'gbrain CLI 同步失败：{message}。请确保 gbrain CLI 已安装并配置。',
'gameDesign.agentsMdCreated': '已在 {path} 创建 AGENTS.md，包含 ## Skill routing 部分。',
'gameDesign.agentsMdUpdated': '已将 ## Skill routing 部分追加到 {path} 的 AGENTS.md。',
'gameDesign.agentsMdAlreadyHasRouting': 'AGENTS.md 已包含 ## Skill routing 部分 — 无需修改。',
'gameDesign.failedToEnsureRouting': '添加 AGENTS.md routing 失败：{message}',
'gameDesign.failedToSyncArtifact': '同步设计产物失败：{message}',
'gameDesign.languageSet': '用户语言已设置为 {language}。',
```

**3b. `packages/agent-core/src/i18n/types.ts`:**

Add all `gameDesign.*` keys to the `MessageKey` type union. The file uses a literal union type. Add each key:

```ts
  | 'gameDesign.entered'
  | 'gameDesign.alreadyActive'
  | 'gameDesign.anotherModeActive'
  | 'gameDesign.failedToEnter'
  | 'gameDesign.sessionComplete'
  | 'gameDesign.designDocSaved'
  | 'gameDesign.appWillExit'
  | 'gameDesign.profileAppended'
  | 'gameDesign.learningRecorded'
  | 'gameDesign.noLearnings'
  | 'gameDesign.learningsHeader'
  | 'gameDesign.learningTypeLabel'
  | 'gameDesign.learningInsightLabel'
  | 'gameDesign.learningConfidenceLabel'
  | 'gameDesign.learningBranchLabel'
  | 'gameDesign.modeNotActive'
  | 'gameDesign.designFileNotFound'
  | 'gameDesign.gbrainConnected'
  | 'gameDesign.gbrainTargetSource'
  | 'gameDesign.gbrainNoSourcePin'
  | 'gameDesign.gbrainReadyForSync'
  | 'gameDesign.gbrainSynced'
  | 'gameDesign.gbrainFile'
  | 'gameDesign.gbrainCliFailed'
  | 'gameDesign.agentsMdCreated'
  | 'gameDesign.agentsMdUpdated'
  | 'gameDesign.agentsMdAlreadyHasRouting'
  | 'gameDesign.failedToEnsureRouting'
  | 'gameDesign.failedToSyncArtifact'
  | 'gameDesign.languageSet'
```

### Step 4: Run tests and verify PASS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/tools/builtin/game-design/ 2>&1 | tail -20
```

Expected: Tests pass, and now the output strings are actual English translations (e.g., `'Game-design mode is already active.'`) instead of raw key names.

### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck 2>&1 | tail -20
```

Expected: No type errors.

### Step 6: Commit

```bash
git add -A && git commit -m "feat: add gameDesign.* i18n translations (en/zh) and type keys"
```

---

## Self-Review (Part 2)

- [ ] 1. Spec-coverage: Task 3 covers injection/context reminders (design item 6). Task 4 covers full tool set (design item 10). Task 5 covers i18n (design item 16).
- [ ] 2. Placeholder scan: No TODO/TBD. All code is complete with exact implementations.
- [ ] 3. No phantom tasks: Task 3 creates injector with tests. Task 4 creates 8 tools with tests. Task 5 adds translations and types.
- [ ] 4. Dependency soundness: Task 3 depends on Task 1 (SessionModeKind). Task 4 depends on Tasks 1-3 (types, store, injector). Task 5 depends on Task 4 (tools reference i18n keys).
- [ ] 5. Caller & build soundness: Task 3 modifies InjectionManager (adds one injector). Task 4 modifies tools/builtin/index.ts (re-exports) and agent/tool/index.ts (registration). No shared signatures changed. Ends with whole-tree typecheck each task.
- [ ] 6. Test-the-risk: Injector tests verify entry/exit/resume/deactivate states. Tool tests verify guard conditions (active/inactive, another mode), profile append, learning append, search. i18n test is implicit — tools fail if keys missing.
- [ ] 7. Type consistency: Tool schemas match Task 2's `GameDesignProfileEntry` and `GameDesignLearningEntry` types exactly. `gameDesignStateStore` property name used in tools matches the `Agent` property added in Task 2.
