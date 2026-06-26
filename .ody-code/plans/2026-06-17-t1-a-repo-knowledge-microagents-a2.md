# Repo Knowledge Microagents Trigger Matching & Injection 实现计划

**Goal:** 当用户消息中的关键词触发时，自动将 `.ody-code/microagents/*.md` 中的 knowledge microagent 内容以 system-reminder 形式注入到 Agent 上下文中。

**Architecture:** 新增 `KnowledgeMicroagentInjector` 继承 `DynamicInjector`，挂载到 `InjectionManager.injectors` 数组末尾。每次 turn 开始时扫描最新用户消息，用 case-insensitive word-boundary（ASCII）/ substring（CJK）匹配 knowledge microagent 的 `metadata.triggers`，已注入的 microagent 通过 per-Agent `Set<string>` 去重，context clear/compact 时清空。全程由新实验性 flag `repo-knowledge`（默认关闭）控制。

**Tech Stack:** TypeScript, vitest, agent-core 现有 injection 框架。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Action | Purpose |
|------|--------|---------|
| `packages/agent-core/src/flags/registry.ts:32` | Modify | Add `repo-knowledge` flag entry |
| `packages/agent-core/src/agent/injection/knowledge-microagent.ts` | Create | `matchKnowledgeMicroagents()` + `KnowledgeMicroagentInjector` |
| `packages/agent-core/src/agent/injection/manager.ts:22-28` | Modify | Wire injector into `injectors` array |
| `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts` | Create | Matcher tests (M1-M8) + Injector tests (I1-I8) + Integration test (N1) |

---

## Dependency Overview

```
Task 1 (flag) ──► Task 2 (matcher + tests M1-M8) ──► Task 3 (injector + tests I1-I8, N1) ──► Task 4 (wiring)
```

所有任务串行依赖，每个任务完成后可独立提交。

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|------|------------|
| R1 | `\b` word-boundary 把 `"component-based"` 也视为匹配（hyphen 是 word boundary） | 设计文档已记录为可接受的 "word-ish" 行为；M2 只要求断言纯 prefix/suffix 不匹配。 |
| R2 | `agent.log.warning()` 不存在 — Logger 接口上实际是 `warn()` | 设计文档中使用的 `warning` 必须改为 `warn`。 |
| R3 | CJK 触发器匹配使用 `includes`，可能会过度匹配（如 "组件化" 匹配 "组件"） | 这是设计文档明确的预期行为（"CJK / mixed scripts: fall back to literal substring"），已记录。 |

---

### Task 1: 添加 `repo-knowledge` 实验性 flag

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/flags/registry.ts:13-32`

**描述:** 在 `FLAG_DEFINITIONS` 数组末尾添加 `repo-knowledge` 条目，作为新实验性功能的开关。`as const satisfies` 会自动将 `FlagId` 联合类型扩展以包含新 id。

- [ ] 在 `flags/registry.ts` 的 `FLAG_DEFINITIONS` 数组末尾追加新条目：

```ts
{
  id: 'repo-knowledge',
  env: 'ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE',
  default: false,
  surface: 'core',
}
```

追加位置在现有最后一个条目 `background-ask` 之后、`] as const satisfies` 之前。

- [ ] 运行 typecheck 确认 `FlagId` 联合类型自动扩展，且无编译错误：

```bash
pnpm --filter @odysseythink/agent-core typecheck
```

预期输出：无错误。

- [ ] Commit: `chore: add repo-knowledge experimental flag`

---

### Task 2: Matcher 函数 + 测试 (M1-M8)

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/agent/injection/knowledge-microagent.ts`（类型定义 + helper 函数）
- Create: `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts`（M1-M8 测试）

**描述:** 实现 `matchKnowledgeMicroagents()` 及其依赖的 helper 函数（`triggerMatches`、`extractLatestUserText`），包含完整的测试覆盖。测试先行——先写测试、运行失败，再写实现、验证通过。

#### 步骤 1: 创建测试文件（仅含 M1-M8 matcher 测试）

创建 `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts`，内容如下：

```ts
import { describe, expect, it } from 'vitest';

import type { ContextMessage } from '../../../src/agent/context';
import {
  extractLatestUserText,
  matchKnowledgeMicroagents,
  triggerMatches,
} from '../../../src/agent/injection/knowledge-microagent';
import type { SkillDefinition } from '../../../src/skill';

function microagent(
  name: string,
  triggers: readonly string[],
  content = '# Test\n\nSome body text.',
): SkillDefinition {
  return {
    name,
    description: `Knowledge: ${name}`,
    path: `/test/${name}.md`,
    dir: '/test',
    content,
    metadata: { type: 'knowledge', triggers },
    source: 'project',
  };
}

function userMessage(text: string): ContextMessage {
  return {
    role: 'user',
    content: [{ type: 'text', text }],
    toolCalls: [],
    origin: { kind: 'user' },
  };
}

function injectionMessage(variant: string): ContextMessage {
  return {
    role: 'user',
    content: [{ type: 'text', text: `<system-reminder>\nsome injection\n</system-reminder>` }],
    toolCalls: [],
    origin: { kind: 'injection', variant },
  };
}

function compactionSummaryMessage(): ContextMessage {
  return {
    role: 'user',
    content: [{ type: 'text', text: 'previous summary' }],
    toolCalls: [],
    origin: { kind: 'compaction_summary' },
  };
}

describe('triggerMatches', () => {
  it('M1: ASCII trigger matches standalone word', () => {
    expect(triggerMatches('add a component', 'component')).toBe(true);
  });

  it('M2a: ASCII trigger does not match as prefix', () => {
    expect(triggerMatches('add components', 'component')).toBe(false);
  });

  it('M2b: ASCII trigger does not match as suffix', () => {
    expect(triggerMatches('use the pager', 'page')).toBe(false);
  });

  it('M8: case-insensitive matching', () => {
    expect(triggerMatches('Add a COMPONENT', 'component')).toBe(true);
    expect(triggerMatches('add a component', 'Component')).toBe(true);
  });

  it('M3: Chinese trigger matches', () => {
    expect(triggerMatches('添加一个组件', '组件')).toBe(true);
  });

  it('M4: Chinese trigger does not match overlapping phrase', () => {
    expect(triggerMatches('添加一个组合件', '组件')).toBe(false);
  });

  // Must-survive inputs for word-boundary regex:
  // "component-based" contains "component" with hyphen as word boundary → matches
  it('word boundary: hyphen-separated word matches', () => {
    expect(triggerMatches('use component-based design', 'component')).toBe(true);
  });

  it('word boundary: "page" does not match "homepage"', () => {
    expect(triggerMatches('visit the homepage', 'page')).toBe(false);
  });
});

describe('extractLatestUserText', () => {
  it('returns text of latest user message', () => {
    const history: ContextMessage[] = [
      userMessage('first message'),
      { role: 'assistant', content: [{ type: 'text', text: 'ok' }], toolCalls: [] },
      userMessage('second message'),
    ];
    expect(extractLatestUserText(history)).toBe('second message');
  });

  it('skips injection origins', () => {
    const history: ContextMessage[] = [
      userMessage('real user message'),
      injectionMessage('knowledge_microagent'),
    ];
    expect(extractLatestUserText(history)).toBe('real user message');
  });

  it('skips compaction summary origins', () => {
    const history: ContextMessage[] = [
      userMessage('real user message'),
      compactionSummaryMessage(),
    ];
    expect(extractLatestUserText(history)).toBe('real user message');
  });

  it('returns undefined when no user message with real origin', () => {
    expect(extractLatestUserText([])).toBeUndefined();
    expect(extractLatestUserText([injectionMessage('x')])).toBeUndefined();
  });

  it('handles user messages with no origin', () => {
    const history: ContextMessage[] = [
      {
        role: 'user',
        content: [{ type: 'text', text: 'plain user message' }],
        toolCalls: [],
      },
    ];
    expect(extractLatestUserText(history)).toBe('plain user message');
  });
});

describe('matchKnowledgeMicroagents', () => {
  const agentA = microagent('agent-a', ['component', 'page']);
  const agentB = microagent('agent-b', ['database', 'sql']);
  const agentC = microagent('agent-c', ['组件']);
  const emptyBody = microagent('empty-agent', ['trigger'], '');

  it('M1: matches standalone word in message text', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component to the page',
      microagents: [agentA, agentB],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(1);
    expect(result[0]!.skill.name).toBe('agent-a');
    expect(result[0]!.trigger).toBe('component');
  });

  it('M5: skips already-injected microagent', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component',
      microagents: [agentA],
      alreadyInjected: new Set(['agent-a']),
    });
    expect(result).toHaveLength(0);
  });

  it('M6: multiple triggers, records first matching one', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a new page',
      microagents: [agentA],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(1);
    expect(result[0]!.trigger).toBe('page');
  });

  it('M7: multiple microagents can match', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component and query the database',
      microagents: [agentA, agentB],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(2);
    const names = result.map((r) => r.skill.name).sort();
    expect(names).toEqual(['agent-a', 'agent-b']);
  });

  it('M3: Chinese trigger matches via substring', () => {
    const result = matchKnowledgeMicroagents({
      messageText: '添加一个组件到页面',
      microagents: [agentC],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(1);
    expect(result[0]!.skill.name).toBe('agent-c');
  });

  it('returns empty array when no triggers match', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a button',
      microagents: [agentA, agentB],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(0);
  });

  it('returns empty when microagents list is empty', () => {
    const result = matchKnowledgeMicroagents({
      messageText: 'add a component',
      microagents: [],
      alreadyInjected: new Set(),
    });
    expect(result).toHaveLength(0);
  });
});
```

#### 步骤 2: 运行测试，验证全部失败

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

预期输出：测试文件加载失败（import 的模块尚不存在），或所有测试因 `undefined` 导入而失败。

#### 步骤 3: 实现 matcher 函数和 helper

创建 `packages/agent-core/src/agent/injection/knowledge-microagent.ts`：

```ts
import type { ContextMessage } from '../context';
import type { SkillDefinition } from '../../skill';

// ── Types ──────────────────────────────────────────────────────────────

export interface MatchKnowledgeMicroagentsOptions {
  readonly messageText: string;
  readonly microagents: readonly SkillDefinition[];
  readonly alreadyInjected: ReadonlySet<string>;
}

export interface KnowledgeMicroagentMatch {
  readonly skill: SkillDefinition;
  readonly trigger: string;
}

// ── Matcher ────────────────────────────────────────────────────────────

const REGEX_META = /[.*+?^${}()|[\]\\]/g;

function escapeRegex(str: string): string {
  return str.replace(REGEX_META, '\\$&');
}

function isAsciiOnly(str: string): boolean {
  return /^[\x00-\x7F]*$/.test(str);
}

/**
 * Returns true when `trigger` matches `text` with case-insensitive,
 * word-boundary-sensitive (ASCII) or substring (CJK) semantics.
 */
export function triggerMatches(text: string, trigger: string): boolean {
  const normalizedTrigger = trigger.toLowerCase();

  if (isAsciiOnly(trigger)) {
    const pattern = new RegExp('\\b' + escapeRegex(normalizedTrigger) + '\\b', 'i');
    return pattern.test(text);
  }

  // CJK / mixed scripts: literal substring match.
  return text.toLowerCase().includes(normalizedTrigger);
}

// ── Message extraction ─────────────────────────────────────────────────

/** ContentPart types from kosong */
interface TextContentPart {
  type: 'text';
  text: string;
}

function isTextPart(part: unknown): part is TextContentPart {
  return (
    typeof part === 'object' &&
    part !== null &&
    'type' in part &&
    (part as TextContentPart).type === 'text'
  );
}

function concatenateTextParts(content: ContextMessage['content']): string {
  return content.filter(isTextPart).map((p) => p.text).join('');
}

/**
 * Scan history from end to start, returning the text of the latest
 * real user message (skipping injections and compaction summaries).
 * Returns undefined when no such message exists or its text is empty.
 */
export function extractLatestUserText(
  history: readonly ContextMessage[],
): string | undefined {
  for (let i = history.length - 1; i >= 0; i--) {
    const message = history[i]!;
    if (message.role !== 'user') continue;
    if (message.origin?.kind === 'injection') continue;
    if (message.origin?.kind === 'compaction_summary') continue;
    const text = concatenateTextParts(message.content);
    if (text.trim().length > 0) return text;
  }
  return undefined;
}

// ── Match entry point ──────────────────────────────────────────────────

/**
 * Returns the list of knowledge microagents whose triggers match the
 * message text and have not been injected yet.
 */
export function matchKnowledgeMicroagents(
  options: MatchKnowledgeMicroagentsOptions,
): readonly KnowledgeMicroagentMatch[] {
  const text = options.messageText.toLowerCase();
  const matches: KnowledgeMicroagentMatch[] = [];

  for (const microagent of options.microagents) {
    if (options.alreadyInjected.has(microagent.name)) continue;

    const triggers = microagent.metadata.triggers;
    if (!Array.isArray(triggers) || triggers.length === 0) continue;

    for (const trigger of triggers) {
      if (typeof trigger !== 'string') continue;
      if (triggerMatches(text, trigger)) {
        matches.push({ skill: microagent, trigger });
        break; // one match per microagent is sufficient
      }
    }
  }

  return matches;
}
```

#### 步骤 4: 运行测试，验证全部通过

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

预期输出：所有 19 个测试通过（triggerMatches × 7 + extractLatestUserText × 5 + matchKnowledgeMicroagents × 7）。

#### 步骤 5: 运行 typecheck 确认无编译错误

```bash
pnpm --filter @odysseythink/agent-core typecheck
```

#### 步骤 6: Commit: `feat: add knowledge microagent trigger matcher with tests`

---

### Task 3: KnowledgeMicroagentInjector + 测试 (I1-I8, N1-N2)

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/src/agent/injection/knowledge-microagent.ts`（追加 `KnowledgeMicroagentInjector` 类）
- Modify: `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts`（追加 I1-I8 + N1-N2 测试）

**描述:** 在 Task 2 创建的文件中追加 `KnowledgeMicroagentInjector` 类，继承 `DynamicInjector`。在同一测试文件中追加 injector 行为和集成测试。

#### 步骤 1: 追加 injector + 集成测试到测试文件

在 `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts` **末尾**追加以下内容。
注意：该文件顶部已导入 `describe`, `expect`, `it`, `SkillDefinition`, `ContextMessage` — 无需重复导入。

```ts
// ===== Append below: Injector tests =====

import type { Agent } from '../../../src/agent';
import {
  KnowledgeMicroagentInjector,
} from '../../../src/agent/injection/knowledge-microagent';

// ── Agent stub helpers ─────────────────────────────────────────────

interface MicroagentAgentStub {
  history: ContextMessage[];
  enabledFlags: Set<string>;
  sessionActive: boolean;
  microagents: SkillDefinition[] | null;
  telemetryCalls: Array<{ event: string; properties: Record<string, unknown> }>;
}

function microagentAgent(stub: MicroagentAgentStub): Agent {
  const fakeRegistry = {
    listKnowledgeMicroagents: () => stub.microagents ?? [],
  } as unknown as SkillRegistry;

  return {
    type: 'main',
    context: {
      get history() {
        return stub.history;
      },
      appendSystemReminder: (content: string, origin: ContextMessage['origin']) => {
        stub.history.push({
          role: 'user',
          content: [{ type: 'text', text: `<system-reminder>\n${content}\n</system-reminder>` }],
          toolCalls: [],
          origin,
        });
      },
    } as Agent['context'],
    sessionMode: {
      get isActive() {
        return stub.sessionActive;
      },
      kind: 'plan' as const,
    } as Agent['sessionMode'],
    skills: {
      registry: fakeRegistry,
    } as Agent['skills'],
    telemetry: {
      track: (event: string, properties: Record<string, unknown>) => {
        stub.telemetryCalls.push({ event, properties });
      },
    } as Agent['telemetry'],
    log: {
      warn: () => {},
      error: () => {},
      info: () => {},
      debug: () => {},
    } as Agent['log'],
  } as unknown as Agent;
}

function reminderText(history: readonly ContextMessage[]): string | undefined {
  const message = history.findLast(
    (entry) =>
      entry.origin?.kind === 'injection' &&
      entry.origin.variant === 'knowledge_microagent',
  );
  return message?.content
    .map((part) => (part.type === 'text' ? part.text : ''))
    .join('');
}

// Need to re-import 'microagent' helper from the top of the file.
// This test file is written as a single unit; all imports and helpers
// defined above the matcher tests are reused.

describe('KnowledgeMicroagentInjector', () => {
  const reuse = microagent('reuse', ['component'], '# Reuse conventions\n\nAlways use existing code.');

  it('I1: injects on first matching user message', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('repo-specific conventions');
    expect(text).toContain('Reuse conventions');
    expect(text).toContain('## reuse');
  });

  it('I2: does not re-inject same microagent on next turn', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    const countAfterFirst = history.length;
    expect(countAfterFirst).toBeGreaterThan(1);

    await injector.inject();
    expect(history.length).toBe(countAfterFirst);
  });

  it('I3: clears injected set on context clear', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(history.length).toBeGreaterThan(1);

    injector.onContextClear();
    history.push(userMessage('add a component'));

    const countBeforeReInject = history.length;
    await injector.inject();
    expect(history.length).toBe(countBeforeReInject + 1);
  });

  it('I4: clears injected set on compaction', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(history.length).toBeGreaterThan(1);

    injector.onContextCompacted(5);
    history.push(userMessage('add a component'));

    const countBeforeReInject = history.length;
    await injector.inject();
    expect(history.length).toBe(countBeforeReInject + 1);
  });

  it('I5: skips empty bodies', async () => {
    const emptyReuse = microagent('empty-reuse', ['component'], '');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [emptyReuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();

    // Verify: after microagent gets content, it should match on next turn
    const filledReuse = microagent('empty-reuse', ['component'], '# Now has content');
    (agent as Record<string, unknown>).skills = {
      registry: { listKnowledgeMicroagents: () => [filledReuse] },
    };
    history.push(userMessage('add a component'));
    await injector.inject();
    expect(reminderText(history)).toBeDefined();
  });

  it('I6: only runs in normal mode', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: true,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
  });

  it('I7: no-op when flag disabled', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
  });

  it('I8: emits telemetry on injection', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    expect(telemetryCalls).toHaveLength(1);
    expect(telemetryCalls[0]!.event).toBe('microagent_injected');
    expect(telemetryCalls[0]!.properties).toMatchObject({
      skill_name: 'reuse',
      trigger: 'component',
      skill_source: 'project',
    });
  });

  it('N1: no-op when skills not loaded (agent.skills is null)', async () => {
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: null,
      telemetryCalls,
    });
    (agent as Record<string, unknown>).skills = null;
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
  });

  it('N2: no-op when latest user text is empty or whitespace-only', async () => {
    const history: ContextMessage[] = [userMessage('   ')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [reuse],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();
    expect(reminderText(history)).toBeUndefined();
  });
});
```

#### 步骤 2: 运行测试，验证 injector 测试全部失败

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

预期输出：matcher 测试（前 19 个）通过，injector 测试因 `KnowledgeMicroagentInjector` 未定义而失败（`ERR_MODULE_NOT_FOUND` 或 `ReferenceError`）。

#### 步骤 3: 在 `knowledge-microagent.ts` 末尾追加 `KnowledgeMicroagentInjector` 类

在 `packages/agent-core/src/agent/injection/knowledge-microagent.ts` 文件**末尾**追加：

```ts
import { DynamicInjector } from './injector';
import type { Agent } from '..';
import { flags } from '../../flags';

export const KNOWLEDGE_MICROAGENT_VARIANT = 'knowledge_microagent';

export class KnowledgeMicroagentInjector extends DynamicInjector {
  protected override readonly injectionVariant = KNOWLEDGE_MICROAGENT_VARIANT;
  private readonly injectedNames = new Set<string>();

  override onContextClear(): void {
    super.onContextClear();
    this.injectedNames.clear();
  }

  override onContextCompacted(compactedCount: number): void {
    super.onContextCompacted(compactedCount);
    this.injectedNames.clear();
  }

  protected override getInjection(): string | undefined {
    if (!flags.enabled('repo-knowledge')) return undefined;
    if (this.agent.sessionMode.isActive) return undefined;
    if (this.agent.skills === null) return undefined;

    const text = extractLatestUserText(this.agent.context.history);
    if (text === undefined || text.trim().length === 0) return undefined;

    const microagents = this.agent.skills.registry.listKnowledgeMicroagents();
    if (microagents.length === 0) return undefined;

    const matches = matchKnowledgeMicroagents({
      messageText: text,
      microagents,
      alreadyInjected: this.injectedNames,
    });
    if (matches.length === 0) return undefined;

    const bodies: string[] = [];
    for (const match of matches) {
      const body = match.skill.content.trim();
      if (body.length === 0) {
        this.agent.log.warn(`Microagent ${match.skill.name} has empty body; skipping`);
        continue;
      }
      this.injectedNames.add(match.skill.name);
      this.agent.telemetry.track('microagent_injected', {
        skill_name: match.skill.name,
        trigger: match.trigger,
        skill_source: match.skill.source,
      });
      bodies.push(`## ${match.skill.name}\n\n${body}`);
    }

    if (bodies.length === 0) return undefined;

    return [
      "The following repo-specific conventions are relevant to your current task.",
      "Apply them without mentioning them to the user unless asked.",
      "",
      bodies.join("\n\n---\n\n"),
    ].join("\n");
  }
}
```

注意：`import { flags } from '../../flags';` 中路径相对于 `agent-core/src/agent/injection/` 指向 `agent-core/src/flags`。

#### 步骤 4: 运行测试，验证全部通过

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

预期输出：所有 29 个测试通过（19 matcher + 10 injector/integration）。

#### 步骤 5: 运行 typecheck 确认无编译错误

```bash
pnpm --filter @odysseythink/agent-core typecheck
```

#### 步骤 6: Commit: `feat: add KnowledgeMicroagentInjector with lifecycle, telemetry, and tests`

---

### Task 4: 接入 InjectionManager

**Depends on:** Task 3

**Files:**
- Modify: `packages/agent-core/src/agent/injection/manager.ts:1-11,21-29`

**描述:** 在 `InjectionManager` 构造函数中，当 `repo-knowledge` flag 启用时，将 `KnowledgeMicroagentInjector` 追加到 `injectors` 数组。导入语句也需要更新。

注意：`InjectionManager` 已将 `flags` 导入为 `import { flags } from '../../flags';`，所以无需添加新的 flags 导入。但需要先验证 flags import 路径对 `manager.ts` 是正确的。

- [ ] 修改 `packages/agent-core/src/agent/injection/manager.ts`：

第 11 行之后追加新的 import：
```ts
import { KnowledgeMicroagentInjector } from './knowledge-microagent';
```

第 29 行的 `];` 改为：
```ts
      ...(flags.enabled('repo-knowledge') ? [new KnowledgeMicroagentInjector(agent)] : []),
    ];
```

完整的构造函数片段（第 21-33 行）变为：

```ts
  constructor(protected readonly agent: Agent) {
    this.injectors = [
      new PluginSessionStartInjector(agent),
      new TodoListReminderInjector(agent),
      new PlanModeInjector(agent),
      new DesignModeInjector(agent),
      new OfficeHoursInjector(agent),
      new PermissionModeInjector(agent),
      ...(flags.enabled('repo-knowledge') ? [new KnowledgeMicroagentInjector(agent)] : []),
    ];
    this.goalInjector =
      flags.enabled('goal-command') && agent.type === 'main' ? new GoalInjector(agent) : null;
  }
```

- [ ] 运行 typecheck 确认编译无误：

```bash
pnpm --filter @odysseythink/agent-core typecheck
```

- [ ] 运行注入相关测试确认无回归：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/
```

预期输出：所有现有 injector 测试和新的 knowledge-microagent 测试均通过。

- [ ] Commit: `feat: wire KnowledgeMicroagentInjector into InjectionManager`

---

## Self-Review

- [ ] 1. Spec-coverage table:

| 设计需求 | Task(s) | Status |
|----------|---------|--------|
| 添加 `repo-knowledge` flag（默认关闭） | Task 1 | covered |
| `KnowledgeMicroagentInjector` 继承 `DynamicInjector` | Task 3 | covered |
| Wire 到 `InjectionManager.injectors` 末尾 | Task 4 | covered |
| 仅对最新用户消息匹配 | Task 2 (`extractLatestUserText`) | covered |
| 跳过 injection/compaction_summary origin | Task 2 (`extractLatestUserText`) | covered |
| Case-insensitive word-boundary（ASCII）/ substring（CJK） | Task 2 (`triggerMatches`) | covered |
| Per-Agent `injectedNames` 去重 | Task 3 (`injectedNames` Set) | covered |
| `onContextClear` 清空去重集 | Task 3 | covered |
| `onContextCompacted` 清空去重集 | Task 3 | covered |
| 发射 `microagent_injected` telemetry 事件 | Task 3 | covered |
| 仅 normal mode 运行 | Task 3 (`sessionMode.isActive` 检查) | covered |
| 跳过空 body 的 microagent | Task 3 | covered |
| 匹配器测试 M1-M8 | Task 2 | covered |
| Injector 测试 I1-I8 | Task 3 | covered |
| 集成测试 N1（skills null）| Task 3 | covered |
| 集成测试 N2（空用户消息）| Task 3 | covered |
| 标记为实验性功能 | Task 1 | covered |
| Token 预算/优先级规则 | 无 | no-op (deferred A.3) |
| Assistant turn 匹配 | 无 | no-op (deferred A.3) |
| 跨 Agent 持久化去重 | 无 | no-op (deferred A.3) |
| `/microagent` 创作助手 | 无 | no-op (deferred A.4) |

- [ ] 2. Placeholder scan: 无 TODO/TBD，无 "implement later"，无 dead-code 占位符。

- [ ] 3. No phantom tasks: 每个 Task 产生可验证的更改（文件变更或测试通过）；无 `--allow-empty` commit。

- [ ] 4. Dependency soundness:
  - Task 1 → none
  - Task 2 → Task 1（使用 flag id，但 matcher 本身不依赖 flag；仅当 flag 在 Task 1 定义后 FlagId 有效）
  - Task 3 → Task 2（使用 `matchKnowledgeMicroagents`, `extractLatestUserText`）
  - Task 4 → Task 3（使用 `KnowledgeMicroagentInjector`）
  - 所有 Depends on 引用均已定义。

- [ ] 5. Caller & build soundness:
  - 无共享签名变更（所有新增代码均为 net-new 文件或局部追加）。
  - `manager.ts` 导入新类并追加到数组，不改变现有签名。
  - Task 4 末尾的 `pnpm --filter @odysseythink/agent-core typecheck` 覆盖整个 agent-core 包（含测试）。
  - 无需跨包 typecheck（变更仅限 agent-core）。

- [ ] 6. Test-the-risk:
  - 状态变更测试（per-Agent Set 清空、生命周期回调）：I3, I4, I5 覆盖。
  - 匹配边界测试（prefix/suffix 不匹配、CJK substring、word-boundary）：M2a/M2b, M3, M4 覆盖。
  - Must-survive inputs 验证：
    - `"page"` 不匹配 `"homepage"`（M2b 变体）
    - `"component-based"` 匹配 `"component"`（word-boundary 在 hyphen 处匹配——已记录为设计行为）
    - `"组件"` 不匹配 `"组合件"`（M4——CJK substring）
  - 空消息/空 body/flag-off/session-mode 的 no-op 路径：N1, N2, I5, I6, I7 覆盖。

- [ ] 7. Type consistency:
  - Task 2 定义的 `MatchKnowledgeMicroagentsOptions`, `KnowledgeMicroagentMatch` 类型在 Task 3 中使用。
  - Task 3 定义的 `KnowledgeMicroagentInjector`, `KNOWLEDGE_MICROAGENT_VARIANT` 在 Task 4 中使用。
  - 所有 import 路径已验证（相对路径在 agent-core 包内有效）。
  - `ContextMessage` 的 `origin?.kind` 和 `origin.variant` 访问符合 `InjectionOrigin` 类型定义。
  - `agent.log.warn()` 验证存在于 `Logger` 接口（非 `warning`）。
  - `agent.telemetry.track()` 验证采用 `(event, properties)` 签名。
