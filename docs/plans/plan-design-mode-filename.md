# Topic-Based Filename for Plan/Design Mode — Implementation Plan

**Goal:** Generate plan and design mode filenames from an LLM-extracted topic slug plus UTC timestamp, while keeping `planId` as an independent random slug for records, replay, and permission guards.

**Architecture:** A new `TopicGenerator` class lives in `agent/plan/` and is invoked from the tool layer (`EnterDesignModeTool` / `EnterPlanModeTool`). It extracts a kebab-case topic from the most recent user message via a lightweight LLM call, then applies a cleanup pipeline (slugify, sensitive-word filter, truncate). The tools compose `fileStem = "<topic>-YYYYMMDD-HHMMSS"` and pass it to `PlanMode.enter()`. `PlanMode` stores `_fileStem` separately from `_planId`, persists it in the wire record, and uses it for the on-disk filename. All existing callers of `PlanMode.enter` remain valid because `fileStem` is an optional trailing parameter.

**Tech Stack:** TypeScript, Vitest, pnpm workspaces (`packages/agent-core`, `apps/ody-code`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Responsibility | Create | Modify |
|---|---|---|
| Topic extraction + cleanup | `packages/agent-core/src/agent/plan/topic-generator.ts` | — |
| Topic extraction tests | `packages/agent-core/test/agent/plan/topic-generator.test.ts` | — |
| PlanMode filename stem | — | `packages/agent-core/src/agent/plan/index.ts` |
| Wire record persistence | — | `packages/agent-core/src/agent/records/types.ts`<br>`packages/agent-core/src/agent/records/index.ts` |
| Design-mode entry tool | — | `packages/agent-core/src/tools/builtin/planning/enter-design-mode.ts`<br>`packages/agent-core/src/tools/builtin/planning/enter-design-mode.md` |
| Design-mode entry tests | `packages/agent-core/test/tools/enter-design-mode.test.ts` | — |
| Plan-mode entry tool | — | `packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts`<br>`packages/agent-core/src/tools/builtin/planning/enter-plan-mode.md` |
| Plan-mode entry tests | — | `packages/agent-core/test/tools/enter-plan-mode.test.ts`<br>`packages/agent-core/test/tools/planning/enter-plan-mode-telemetry.test.ts` |
| Core plan-mode tests | — | `packages/agent-core/test/agent/plan.test.ts` |

---

## Dependency Overview

```
Task 1 ──→ Task 2 ──→ Task 3 ──→ Task 5
            │           │
            └──────────→ Task 4 ──→┘
```

- **Task 1** (TopicGenerator) has no dependencies.
- **Task 2** (PlanMode core) has no dependencies; `fileStem` is optional so existing callers compile unchanged.
- **Task 3** (EnterDesignModeTool) depends on Task 1 + Task 2.
- **Task 4** (EnterPlanModeTool) depends on Task 1 + Task 2; can be done in parallel with Task 3.
- **Task 5** (Final verification) depends on Task 3 + Task 4.

---

### Task 1: Create TopicGenerator class with cleanup utilities and tests

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/agent/plan/topic-generator.ts`
- Create: `packages/agent-core/test/agent/plan/topic-generator.test.ts`

**Steps:**

- [ ] Write the failing test by creating the test file:

```ts
// packages/agent-core/test/agent/plan/topic-generator.test.ts
import { describe, expect, it, vi } from 'vitest';
import {
  TopicGenerator,
  buildTopicPrompt,
  cleanupTopic,
  formatUtcTimestamp,
} from '../../../src/agent/plan/topic-generator';
import type { Agent } from '../../../src/agent';

function makeAgent(overrides: {
  generate?: () => Promise<{
    message: { content: Array<{ type: string; text: string }> };
  }>;
  history?: Array<{
    role: string;
    content: Array<{ type: string; text: string }>;
    origin?: { kind: string; variant?: string };
  }>;
} = {}): Agent {
  return {
    context: {
      history: overrides.history ?? [],
    },
    config: {
      get provider() {
        return { name: 'mock', modelName: 'mock-model' };
      },
    },
    generate:
      overrides.generate ??
      vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'user-dashboard' }] },
      }),
    telemetry: { track: vi.fn() },
  } as unknown as Agent;
}

describe('cleanupTopic', () => {
  it('returns kebab-case slug for valid input', () => {
    expect(cleanupTopic('User Dashboard')).toBe('user-dashboard');
    expect(cleanupTopic('Auth Refactor')).toBe('auth-refactor');
  });

  it('filters built-in sensitive words', () => {
    expect(cleanupTopic('my-password-manager')).toBeNull();
    expect(cleanupTopic('api-key-handler')).toBeNull();
    expect(cleanupTopic('secret-token-store')).toBeNull();
    expect(cleanupTopic('credential-auth-flow')).toBeNull();
  });

  it('truncates to max length and strips trailing hyphens', () => {
    const long = 'a'.repeat(100);
    expect(cleanupTopic(long)).toHaveLength(50);
    expect(cleanupTopic(long)).toBe('a'.repeat(50));
  });

  it('returns null for too-short input', () => {
    expect(cleanupTopic('a')).toBeNull();
    expect(cleanupTopic('')).toBeNull();
  });

  it('handles non-ASCII input', () => {
    expect(cleanupTopic('用户仪表盘')).toBeNull();
    expect(cleanupTopic('hello 世界 world')).toBe('hello-world');
  });

  it('collapses multiple hyphens and trims edges', () => {
    expect(cleanupTopic('hello---world')).toBe('hello-world');
    expect(cleanupTopic('-hello-')).toBe('hello');
  });
});

describe('formatUtcTimestamp', () => {
  it('formats a UTC date as YYYYMMDD-HHMMSS', () => {
    const date = new Date('2025-06-04T14:30:52.000Z');
    expect(formatUtcTimestamp(date)).toBe('20250604-143052');
  });
});

describe('buildTopicPrompt', () => {
  it('includes the user message and kebab-case instruction', () => {
    const prompt = buildTopicPrompt('Build a dashboard');
    expect(prompt).toContain('Build a dashboard');
    expect(prompt).toContain('kebab-case');
    expect(prompt).toContain('Ignore API keys');
  });
});

describe('TopicGenerator', () => {
  it('generates topic from the last real user message', async () => {
    const agent = makeAgent({
      history: [
        { role: 'assistant', content: [{ type: 'text', text: 'Hello' }] },
        {
          role: 'user',
          content: [{ type: 'text', text: 'Build a user dashboard' }],
          origin: { kind: 'user' },
        },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('user-dashboard');
    expect(agent.generate).toHaveBeenCalledTimes(1);
  });

  it('ignores injection and system messages', async () => {
    const agent = makeAgent({
      history: [
        {
          role: 'user',
          content: [{ type: 'text', text: 'injected reminder' }],
          origin: { kind: 'injection', variant: 'plan_mode' },
        },
        {
          role: 'user',
          content: [{ type: 'text', text: 'real request' }],
          origin: { kind: 'user' },
        },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('real-request');
  });

  it('returns null when no user message exists', async () => {
    const agent = makeAgent({ history: [] });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'no_user_message',
    });
  });

  it('returns null when user message is empty', async () => {
    const agent = makeAgent({
      history: [
        { role: 'user', content: [{ type: 'text', text: '' }], origin: { kind: 'user' } },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'empty_user_message',
    });
  });

  it('returns null and tracks on LLM error', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockRejectedValue(new Error('Timeout')),
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'Error',
    });
  });

  it('returns null when LLM returns empty text', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: '   ' }] },
      }),
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'empty_result',
    });
  });

  it('returns null when cleaned topic contains sensitive words', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'password-reset' }] },
      }),
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'sensitive_content_or_invalid',
    });
  });

  it('respects custom maxLength', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'very-long-topic-name-here' }] },
      }),
    });
    const generator = new TopicGenerator(agent, { maxLength: 10 });
    const topic = await generator.generate();
    expect(topic).toBe('very-long');
  });
});
```

- [ ] Run the test and verify it FAILS:

```bash
pnpm vitest run packages/agent-core/test/agent/plan/topic-generator.test.ts
```

Expected failure: `Error: Cannot find module '../../../src/agent/plan/topic-generator'` (or similar import resolution error because the source file does not exist yet).

- [ ] Write the minimal implementation:

```ts
// packages/agent-core/src/agent/plan/topic-generator.ts
import type { Agent } from '..';

export const DEFAULT_SENSITIVE_WORDS = [
  'key',
  'token',
  'password',
  'secret',
  'credential',
  'auth',
] as const;

export interface TopicGeneratorOptions {
  readonly maxLength?: number;
  readonly sensitiveWords?: readonly string[];
}

export function buildTopicPrompt(userMessageText: string): string {
  return `You are a concise topic extractor. Based on the user's message below, generate a short English topic phrase (2-5 words) in kebab-case (lowercase, hyphen-separated).

Rules:
- Ignore API keys, passwords, tokens, secrets, credentials, or any sensitive information.
- Focus on the functional topic or feature being discussed.
- If the message is ambiguous, return "general".
- Output ONLY the kebab-case topic, nothing else.

User message: """${userMessageText}"""`;
}

export function cleanupTopic(
  raw: string,
  maxLength = 50,
  sensitiveWords?: readonly string[],
): string | null {
  const words = sensitiveWords ?? DEFAULT_SENSITIVE_WORDS;

  let topic = raw.trim().toLowerCase();
  topic = topic.replace(/[^a-z0-9]+/g, '-');
  topic = topic.replace(/^-+|-+$/g, '');
  topic = topic.replace(/-+/g, '-');

  if (words.some((w) => topic.includes(w))) {
    return null;
  }

  if (topic.length > maxLength) {
    topic = topic.slice(0, maxLength);
    topic = topic.replace(/-+$/, '');
  }

  if (topic.length < 2) {
    return null;
  }

  return topic;
}

export function formatUtcTimestamp(date: Date): string {
  const iso = date.toISOString();
  return (
    iso.slice(0, 4) +
    iso.slice(5, 7) +
    iso.slice(8, 10) +
    '-' +
    iso.slice(11, 13) +
    iso.slice(14, 16) +
    iso.slice(17, 19)
  );
}

export class TopicGenerator {
  constructor(
    private readonly agent: Agent,
    private readonly options: TopicGeneratorOptions = {},
  ) {}

  async generate(): Promise<string | null> {
    const history = this.agent.context.history;
    const lastUserMessage = history.findLast(
      (msg) => msg.role === 'user' && msg.origin?.kind === 'user',
    );

    if (lastUserMessage === undefined) {
      this.agent.telemetry.track('topic_generation_failed', { reason: 'no_user_message' });
      return null;
    }

    const userMessageText = lastUserMessage.content
      .filter((part) => part.type === 'text')
      .map((part) => part.text)
      .join('')
      .trim();

    if (userMessageText.length === 0) {
      this.agent.telemetry.track('topic_generation_failed', { reason: 'empty_user_message' });
      return null;
    }

    let rawTopic: string;
    try {
      const provider = this.agent.config.provider;
      const result = await this.agent.generate(
        provider,
        buildTopicPrompt(userMessageText),
        [],
        [{ role: 'user', content: [{ type: 'text', text: userMessageText }], toolCalls: [] }],
        {},
        { signal: AbortSignal.timeout(3000) },
      );
      rawTopic = result.message.content
        .filter((part) => part.type === 'text')
        .map((part) => part.text)
        .join('')
        .trim();
    } catch (error) {
      const reason = error instanceof Error ? error.name : 'unknown_error';
      this.agent.telemetry.track('topic_generation_failed', { reason });
      return null;
    }

    if (rawTopic.length === 0) {
      this.agent.telemetry.track('topic_generation_failed', { reason: 'empty_result' });
      return null;
    }

    const topic = cleanupTopic(rawTopic, this.options.maxLength, this.options.sensitiveWords);
    if (topic === null) {
      this.agent.telemetry.track('topic_generation_failed', {
        reason: 'sensitive_content_or_invalid',
      });
      return null;
    }

    return topic;
  }
}
```

- [ ] Run the test and verify it PASSES:

```bash
pnpm vitest run packages/agent-core/test/agent/plan/topic-generator.test.ts
```

Expected: all 15 tests pass.

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/plan/topic-generator.ts packages/agent-core/test/agent/plan/topic-generator.test.ts
git commit -m "feat(agent-core): add TopicGenerator for LLM-based topic extraction"
```

---

### Task 2: Update PlanMode core — fileStem, wire record, restoreEnter, and tests

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/plan/index.ts` (lines 23–189)
- Modify: `packages/agent-core/src/agent/records/types.ts` (lines 41–44)
- Modify: `packages/agent-core/src/agent/records/index.ts` (lines 62–63)
- Modify: `packages/agent-core/test/agent/plan.test.ts` (add new test after line 67)

**Steps:**

- [ ] Write the failing test additions first. Open `packages/agent-core/test/agent/plan.test.ts` and add this test inside the `describe('manual plan entry', () => {` block, right after the `derives the no-homedir plan path from cwd on enter and restore` test (after line 67):

```ts
  it('uses fileStem for plan path when provided and restores it from wire records', async () => {
    const ctx = testAgent({
      kaos: createPlanKaos(),
    });
    await ctx.agent.planMode.enter('plan-id', false, true, 'plan', 'custom-stem');

    expect(ctx.agent.planMode.planFilePath).toBe('/workspace/plan/custom-stem.md');
    expect(ctx.agent.planMode.fileStem).toBe('custom-stem');

    const enterRecord = ctx.allEvents.find(
      (event) => event.type === '[wire]' && event.event === 'plan_mode.enter',
    );
    expect(enterRecord?.args).toMatchObject({
      id: 'plan-id',
      kind: 'plan',
      fileStem: 'custom-stem',
    });

    const resumed = testAgent({ kaos: createFakeKaos() });
    resumed.dispatch({
      type: 'plan_mode.enter',
      id: 'plan-id',
      fileStem: 'custom-stem',
    });

    expect(resumed.agent.planMode.planFilePath).toBe('/workspace/plan/custom-stem.md');
    expect(resumed.agent.planMode.fileStem).toBe('custom-stem');
  });
```

- [ ] Run the new test and verify it FAILS:

```bash
pnpm vitest run packages/agent-core/test/agent/plan.test.ts -t "uses fileStem"
```

Expected failure: `TypeError: Cannot read properties of undefined (reading 'planFilePath')` or similar because `fileStem` parameter and `_fileStem` field do not exist yet.

- [ ] Write the minimal implementation changes.

**A. `packages/agent-core/src/agent/plan/index.ts`** — apply these edits:

Add `_fileStem` field after `_kind` (line 27):
```ts
  protected _fileStem: string | null = null;
```

Add `fileStem` getter after `kind` getter (after line 126):
```ts
  get fileStem(): string | null {
    return this._fileStem;
  }
```

Update `enter` signature and body (lines 35–73):
```ts
  async enter(
    id = this.createPlanId(),
    createFile = false,
    emitStatus = true,
    kind: PlanKind = 'plan',
    fileStem?: string,
  ): Promise<void> {
    if (this._isActive) {
      throw new Error('Already in plan mode');
    }

    this._isActive = true;
    this._planId = id;
    this._kind = kind;
    this._planFilePath = null;
    this._fileStem = fileStem ?? id;

    let enterRecorded = false;
    try {
      const planFilePath = this.planFilePathFor(this._fileStem);
      this._planFilePath = planFilePath;
      await this.ensurePlanDirectory(planFilePath);
      this.agent.records.logRecord({
        type: 'plan_mode.enter',
        id,
        kind,
        ...(this._fileStem !== id ? { fileStem: this._fileStem } : {}),
      });
      enterRecorded = true;
      if (createFile) {
        await this.writeEmptyPlanFile(planFilePath);
      }
    } catch (error) {
      if (enterRecorded) {
        this.cancel(id);
      } else {
        this._isActive = false;
        this._planId = null;
        this._planFilePath = null;
        this._fileStem = null;
        this._kind = 'plan';
      }
      throw error;
    }

    if (emitStatus) this.agent.emitStatusUpdated();
  }
```

Update `restoreEnter` (lines 75–86):
```ts
  restoreEnter({
    id,
    kind = 'plan',
    fileStem,
  }: {
    readonly id: string;
    readonly kind?: PlanKind;
    readonly fileStem?: string;
  }): void {
    this.agent.replayBuilder.push({
      type: 'plan_updated',
      enabled: true,
      kind,
    });

    this._isActive = true;
    this._planId = id;
    this._kind = kind;
    this._fileStem = fileStem ?? id;
    this._planFilePath = this.planFilePathFor(this._fileStem);
  }
```

Update `cancel` (lines 88–99) to clear `_fileStem`:
```ts
  cancel(id?: string): void {
    this.agent.records.logRecord({ type: 'plan_mode.cancel', id });
    this.agent.replayBuilder.push({
      type: 'plan_updated',
      enabled: false,
      kind: this._kind,
    });
    this._isActive = false;
    this._planId = null;
    this._planFilePath = null;
    this._fileStem = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }
```

Update `exit` (lines 107–119) to clear `_fileStem`:
```ts
  exit(id?: string): void {
    this.agent.records.logRecord({ type: 'plan_mode.exit', id });
    this.agent.replayBuilder.push({
      type: 'plan_updated',
      enabled: false,
      kind: this._kind,
    });
    this._isActive = false;
    this._planId = null;
    this._planFilePath = null;
    this._fileStem = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }
```

Update `planFilePathFor` parameter name (lines 181–189):
```ts
  private planFilePathFor(stem: string): string {
    const cwdSubdir = this._kind === 'design' ? 'design' : 'plan';
    const homeSubdir = this._kind === 'design' ? 'designs' : 'plans';
    const plansDir =
      this.agent.homedir === undefined
        ? join(this.agent.config.cwd, cwdSubdir)
        : join(this.agent.homedir, homeSubdir);
    return join(plansDir, `${stem}.md`);
  }
```

**B. `packages/agent-core/src/agent/records/types.ts`** — add optional `fileStem` to the `plan_mode.enter` record (lines 41–44):
```ts
  'plan_mode.enter': {
    id: string;
    kind?: PlanKind;
    fileStem?: string;
  };
```

**C. `packages/agent-core/src/agent/records/index.ts`** — pass `fileStem` through to `restoreEnter` (line 62–63):
```ts
    case 'plan_mode.enter':
      agent.planMode.restoreEnter({
        id: input.id,
        kind: input.kind ?? 'plan',
        fileStem: input.fileStem,
      });
      return;
```

- [ ] Find all callers to confirm the signature change is safe:

```bash
grep -rn "\.planMode\.enter(" packages/
grep -rn "restoreEnter(" packages/
```

All existing callers pass 0–4 positional arguments, so the new optional 5th parameter compiles without changes. The tools (`enter-design-mode.ts`, `enter-plan-mode.ts`) will be updated in Tasks 3 and 4 to pass `fileStem`.

- [ ] Run the targeted test and verify it PASSES:

```bash
pnpm vitest run packages/agent-core/test/agent/plan.test.ts -t "uses fileStem"
```

- [ ] Run the whole-tree typecheck to confirm no stale callers:

```bash
pnpm -r typecheck
```

Expected: zero errors across all packages.

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/plan/index.ts packages/agent-core/src/agent/records/types.ts packages/agent-core/src/agent/records/index.ts packages/agent-core/test/agent/plan.test.ts
git commit -m "feat(agent-core): add fileStem to PlanMode for topic-based filenames"
```

---

### Task 3: Update EnterDesignModeTool — topic param, auto-generation, description, and tests

**Depends on:** Task 1, Task 2

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/planning/enter-design-mode.ts` (full file)
- Modify: `packages/agent-core/src/tools/builtin/planning/enter-design-mode.md` (append parameter note)
- Create: `packages/agent-core/test/tools/enter-design-mode.test.ts`

**Steps:**

- [ ] Update `packages/agent-core/src/tools/builtin/planning/enter-design-mode.ts` to the following complete content:

```ts
/**
 * EnterDesignModeTool — design-mode entry tool.
 *
 * Design mode is the brainstorming / spec-exploration sibling of plan mode.
 * It reuses the same read-only-with-one-writable-file machinery as plan mode
 * (see {@link PlanMode}) but enters with `kind: 'design'`, which routes the
 * design document to the `designs/` directory and swaps the plan-mode prompt
 * for the brainstorming workflow. Entering design mode does not require
 * approval in any permission mode.
 */

import type { Agent } from '#/agent';
import { z } from 'zod';

import { designModeEntryMessage } from '../../../agent/injection/design-mode-contract';
import {
  cleanupTopic,
  formatUtcTimestamp,
  TopicGenerator,
} from '../../../agent/plan/topic-generator';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-design-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export const EnterDesignModeInputSchema = z
  .object({
    topic: z.string().max(100).optional(),
  })
  .strict();
export type EnterDesignModeInput = z.infer<typeof EnterDesignModeInputSchema>;

export class EnterDesignModeTool implements BuiltinTool<EnterDesignModeInput> {
  readonly name = 'EnterDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterDesignModeInput): ToolExecution {
    return {
      description: 'Requesting to enter design mode',
      approvalRule: this.name,
      execute: async () => {
        // Guard: already in plan/design mode
        if (this.agent.planMode.isActive) {
          const active = this.agent.planMode.kind === 'design' ? 'Design' : 'Plan';
          return {
            isError: true,
            output: `${active} mode is already active. Use ExitDesignMode when the design is ready, or exit first.`,
          };
        }

        let topic: string | null = null;
        if (_args.topic !== undefined) {
          topic = cleanupTopic(_args.topic);
        } else {
          const generator = new TopicGenerator(this.agent);
          topic = await generator.generate();
        }
        if (topic === null) {
          topic = 'design';
        }
        const fileStem = `${topic}-${formatUtcTimestamp(new Date())}`;

        try {
          await this.agent.planMode.enter(undefined, undefined, undefined, 'design', fileStem);
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter design mode.';
          return { isError: true, output: `Failed to enter design mode: ${message}` };
        }

        this.agent.telemetry.track('design_enter_resolved', { outcome: 'auto_approved' });
        return {
          output: designModeEntryMessage(
            this.agent.planMode.planFilePath,
            this.agent.rpc?.openExternal !== undefined,
          ),
        };
      },
    };
  }
}
```

- [ ] Append to `packages/agent-core/src/tools/builtin/planning/enter-design-mode.md`:

```md

Optional parameter:
- `topic` — A short topic phrase (2–5 words) to include in the design filename. If omitted, the topic is inferred automatically from the conversation.
```

- [ ] Write the test file `packages/agent-core/test/tools/enter-design-mode.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import type { PermissionMode } from '../../src/agent/permission';
import {
  EnterDesignModeInputSchema,
  EnterDesignModeTool,
} from '../../src/tools/builtin/planning/enter-design-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

function makeAgent(
  input: {
    readonly active?: boolean;
    readonly kind?: 'plan' | 'design';
    readonly mode?: PermissionMode;
    readonly planFilePath?: string | null;
    readonly enter?: () => Promise<void>;
    readonly generate?: () => Promise<{
      message: { content: Array<{ type: string; text: string }> };
    }>;
    readonly history?: Array<{
      role: string;
      content: Array<{ type: string; text: string }>;
      origin?: { kind: string };
    }>;
  } = {},
): { agent: Agent; requestApproval: ReturnType<typeof vi.fn>; enterSpy: ReturnType<typeof vi.fn> } {
  let active = input.active ?? false;
  const requestApproval = vi.fn(async () => ({ decision: 'approved' }));
  const enterSpy = vi.fn(async () => {
    active = true;
    if (input.enter) await input.enter();
  });
  const agent = {
    planMode: {
      get isActive() {
        return active;
      },
      get kind() {
        return input.kind ?? 'design';
      },
      get planFilePath() {
        return input.planFilePath ?? null;
      },
      enter: enterSpy,
    },
    permission: { mode: input.mode ?? 'manual' },
    rpc: { requestApproval },
    telemetry: { track: vi.fn() },
    context: {
      history: input.history ?? [],
    },
    config: {
      get provider() {
        return { name: 'mock', modelName: 'mock-model' };
      },
    },
    generate:
      input.generate ??
      vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'user-dashboard' }] },
      }),
  } as unknown as Agent;
  return { agent, requestApproval, enterSpy };
}

describe('EnterDesignModeTool', () => {
  it('has name, description, and parameters from the current schema', () => {
    const { agent } = makeAgent();
    const tool = new EnterDesignModeTool(agent);

    expect(tool.name).toBe('EnterDesignMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(EnterDesignModeInputSchema.safeParse({}).success).toBe(true);
    expect(EnterDesignModeInputSchema.safeParse({ topic: 'Auth Refactor' }).success).toBe(true);
    expect(tool.parameters).toMatchObject({
      type: 'object',
      properties: {
        topic: { type: 'string' },
      },
    });
  });

  it('returns an error when design mode is already active', async () => {
    const { agent } = makeAgent({ active: true, kind: 'design' });
    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('Design mode is already active');
  });

  it('returns an error when plan mode is already active', async () => {
    const { agent } = makeAgent({ active: true, kind: 'plan' });
    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_2',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('Plan mode is already active');
  });

  it.each(['manual', 'auto', 'yolo'] satisfies PermissionMode[])(
    'enters in %s mode without approval and auto-generates topic filename',
    async (mode) => {
      const { agent, requestApproval, enterSpy } = makeAgent({ mode });

      const result = await executeTool(new EnterDesignModeTool(agent), {
        turnId: '0',
        toolCallId: `tc_${mode}`,
        args: {},
        signal,
      });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Design mode is now active');
      expect(requestApproval).not.toHaveBeenCalled();
      expect(enterSpy).toHaveBeenCalledWith(
        undefined,
        undefined,
        undefined,
        'design',
        expect.stringMatching(/^user-dashboard-\d{8}-\d{6}$/),
      );
    },
  );

  it('uses user-provided topic when given', async () => {
    const { agent, enterSpy } = makeAgent({ mode: 'yolo' });

    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_topic',
      args: { topic: 'Auth Refactor' },
      signal,
    });

    expect(result.isError).toBeFalsy();
    expect(enterSpy).toHaveBeenCalledWith(
      undefined,
      undefined,
      undefined,
      'design',
      expect.stringMatching(/^auth-refactor-\d{8}-\d{6}$/),
    );
  });

  it('falls back to design-timestamp when topic generation fails', async () => {
    const { agent, enterSpy } = makeAgent({
      mode: 'yolo',
      generate: vi.fn().mockRejectedValue(new Error('Timeout')),
    });

    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_fallback',
      args: {},
      signal,
    });

    expect(result.isError).toBeFalsy();
    expect(enterSpy).toHaveBeenCalledWith(
      undefined,
      undefined,
      undefined,
      'design',
      expect.stringMatching(/^design-\d{8}-\d{6}$/),
    );
  });

  it('returns an error when entering design mode fails', async () => {
    const { agent } = makeAgent({
      mode: 'yolo',
      enter: vi.fn().mockRejectedValue(new Error('state error')),
    });

    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_error',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('state error');
  });
});
```

- [ ] Run the new test and verify it PASSES:

```bash
pnpm vitest run packages/agent-core/test/tools/enter-design-mode.test.ts
```

Expected: all 8 tests pass.

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/planning/enter-design-mode.ts packages/agent-core/src/tools/builtin/planning/enter-design-mode.md packages/agent-core/test/tools/enter-design-mode.test.ts
git commit -m "feat(agent-core): add optional topic param to EnterDesignModeTool"
```

---

### Task 4: Update EnterPlanModeTool — topic param, auto-generation, description, and tests

**Depends on:** Task 1, Task 2

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts` (full file)
- Modify: `packages/agent-core/src/tools/builtin/planning/enter-plan-mode.md` (append parameter note)
- Modify: `packages/agent-core/test/tools/enter-plan-mode.test.ts` (full rewrite)
- Modify: `packages/agent-core/test/tools/planning/enter-plan-mode-telemetry.test.ts` (add missing stubs)

**Steps:**

- [ ] Update `packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts` to the following complete content:

```ts
/**
 * EnterPlanModeTool — plan-mode entry tool.
 *
 * The LLM calls this tool to enter plan mode directly. Entering plan mode
 * does not require approval in any permission mode.
 */

import type { Agent } from '#/agent';
import { z } from 'zod';

import { planModeEntryMessage } from '../../../agent/injection/plan-mode-contract';
import {
  cleanupTopic,
  formatUtcTimestamp,
  TopicGenerator,
} from '../../../agent/plan/topic-generator';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-plan-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export const EnterPlanModeInputSchema = z
  .object({
    topic: z.string().max(100).optional(),
  })
  .strict();
export type EnterPlanModeInput = z.infer<typeof EnterPlanModeInputSchema>;

export class EnterPlanModeTool implements BuiltinTool<EnterPlanModeInput> {
  readonly name = 'EnterPlanMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterPlanModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterPlanModeInput): ToolExecution {
    return {
      description: 'Requesting to enter plan mode',
      approvalRule: this.name,
      execute: async () => {
        // Guard: already in plan mode
        if (this.agent.planMode.isActive) {
          return {
            isError: true,
            output: 'Plan mode is already active. Use ExitPlanMode when the plan is ready.',
          };
        }

        let topic: string | null = null;
        if (_args.topic !== undefined) {
          topic = cleanupTopic(_args.topic);
        } else {
          const generator = new TopicGenerator(this.agent);
          topic = await generator.generate();
        }
        if (topic === null) {
          topic = 'plan';
        }
        const fileStem = `${topic}-${formatUtcTimestamp(new Date())}`;

        try {
          await this.agent.planMode.enter(undefined, undefined, undefined, 'plan', fileStem);
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter plan mode.';
          return { isError: true, output: `Failed to enter plan mode: ${message}` };
        }

        this.agent.telemetry.track('plan_enter_resolved', { outcome: 'auto_approved' });
        return { output: planModeEntryMessage(this.agent.planMode.planFilePath) };
      },
    };
  }
}
```

- [ ] Append to `packages/agent-core/src/tools/builtin/planning/enter-plan-mode.md`:

```md

Optional parameter:
- `topic` — A short topic phrase (2–5 words) to include in the plan filename. If omitted, the topic is inferred automatically from the conversation.
```

- [ ] Rewrite `packages/agent-core/test/tools/enter-plan-mode.test.ts` to the following complete content:

```ts
/**
 * EnterPlanModeTool tests against the current Agent-backed tool surface.
 */

import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import type { PermissionMode } from '../../src/agent/permission';
import {
  EnterPlanModeInputSchema,
  EnterPlanModeTool,
} from '../../src/tools/builtin/planning/enter-plan-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

function makeAgent(
  input: {
    readonly active?: boolean;
    readonly mode?: PermissionMode;
    readonly planFilePath?: string | null;
    readonly enter?: () => Promise<void>;
    readonly generate?: () => Promise<{
      message: { content: Array<{ type: string; text: string }> };
    }>;
    readonly history?: Array<{
      role: string;
      content: Array<{ type: string; text: string }>;
      origin?: { kind: string };
    }>;
  } = {},
): { agent: Agent; requestApproval: ReturnType<typeof vi.fn>; enterSpy: ReturnType<typeof vi.fn> } {
  let active = input.active ?? false;
  const requestApproval = vi.fn(async () => {
    return { decision: 'approved' };
  });
  const enterSpy = vi.fn(async () => {
    active = true;
    if (input.enter) await input.enter();
  });
  const agent = {
    planMode: {
      get isActive() {
        return active;
      },
      get planFilePath() {
        return input.planFilePath ?? null;
      },
      enter: enterSpy,
    },
    permission: { mode: input.mode ?? 'manual' },
    rpc: { requestApproval },
    telemetry: { track: vi.fn() },
    context: {
      history: input.history ?? [],
    },
    config: {
      get provider() {
        return { name: 'mock', modelName: 'mock-model' };
      },
    },
    generate:
      input.generate ??
      vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'user-dashboard' }] },
      }),
  } as unknown as Agent;
  return { agent, requestApproval, enterSpy };
}

describe('EnterPlanModeTool', () => {
  it('has name, description, and parameters from the current schema', () => {
    const { agent } = makeAgent();
    const tool = new EnterPlanModeTool(agent);

    expect(tool.name).toBe('EnterPlanMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(tool.description).toContain('Use it when ANY of these conditions apply');
    expect(tool.description).toContain('New Feature Implementation');
    expect(tool.description).toContain('When NOT to use');
    expect(tool.description).toContain('subagent_type="explore"');
    expect(EnterPlanModeInputSchema.safeParse({}).success).toBe(true);
    expect(EnterPlanModeInputSchema.safeParse({ topic: 'Auth Refactor' }).success).toBe(true);
    expect(tool.parameters).toMatchObject({
      type: 'object',
      properties: {
        topic: { type: 'string' },
      },
    });
    expect((tool.parameters['properties'] as Record<string, unknown>)['reason']).toBeUndefined();
  });

  it('returns an error when plan mode is already active', async () => {
    const { agent } = makeAgent({ active: true });
    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('already active');
  });

  it.each(['manual', 'auto', 'yolo'] satisfies PermissionMode[])(
    'enters in %s mode without an approval request and auto-generates topic filename',
    async (mode) => {
      const { agent, requestApproval, enterSpy } = makeAgent({ mode });

      const result = await executeTool(new EnterPlanModeTool(agent), {
        turnId: '0',
        toolCallId: `tc_${mode}`,
        args: {},
        signal,
      });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Plan mode is now active');
      expect(requestApproval).not.toHaveBeenCalled();
      expect(enterSpy).toHaveBeenCalledWith(
        undefined,
        undefined,
        undefined,
        'plan',
        expect.stringMatching(/^user-dashboard-\d{8}-\d{6}$/),
      );
    },
  );

  it('uses user-provided topic when given', async () => {
    const { agent, enterSpy } = makeAgent({ mode: 'yolo' });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_topic',
      args: { topic: 'Auth Refactor' },
      signal,
    });

    expect(result.isError).toBeFalsy();
    expect(enterSpy).toHaveBeenCalledWith(
      undefined,
      undefined,
      undefined,
      'plan',
      expect.stringMatching(/^auth-refactor-\d{8}-\d{6}$/),
    );
  });

  it('falls back to plan-timestamp when topic generation fails', async () => {
    const { agent, enterSpy } = makeAgent({
      mode: 'yolo',
      generate: vi.fn().mockRejectedValue(new Error('Timeout')),
    });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_fallback',
      args: {},
      signal,
    });

    expect(result.isError).toBeFalsy();
    expect(enterSpy).toHaveBeenCalledWith(
      undefined,
      undefined,
      undefined,
      'plan',
      expect.stringMatching(/^plan-\d{8}-\d{6}$/),
    );
  });

  it('uses inline guidance when no plan file path is available', async () => {
    const { agent } = makeAgent({ mode: 'yolo', planFilePath: null });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_inline',
      args: {},
      signal,
    });

    expect(result.output).toContain('No plan file path is available in this host yet');
    expect(result.output).not.toContain('`plan` parameter');
    expect(result.output).not.toContain('Plan file:');
  });

  it('uses plan-file guidance when the host provides a plan file path', async () => {
    const { agent } = makeAgent({ mode: 'yolo', planFilePath: '/tmp/kimi/plans/example.md' });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_file',
      args: {},
      signal,
    });

    expect(result.output).toContain('Plan file: /tmp/kimi/plans/example.md');
    expect(result.output).toContain('Write the plan — incrementally');
    expect(result.output).toContain('Depends on:');
  });

  it('returns an error when entering plan mode fails', async () => {
    const { agent } = makeAgent({
      mode: 'yolo',
      enter: vi.fn().mockRejectedValue(new Error('state error')),
    });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_error',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('state error');
  });

  it('resolveExecution description returns a stable phrase', () => {
    const { agent } = makeAgent();
    const execution = new EnterPlanModeTool(agent).resolveExecution({});
    expect(execution.isError).toBeFalsy();
    if (execution.isError === true) throw new Error('expected runnable execution');
    expect(execution.description).toContain('plan mode');
  });
});
```

- [ ] Update `packages/agent-core/test/tools/planning/enter-plan-mode-telemetry.test.ts` — replace the `makeAgent` stub with one that includes the fields TopicGenerator needs:

```ts
import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../src/agent';
import type { PermissionMode } from '../../../src/agent/permission';
import { EnterPlanModeTool } from '../../../src/tools/builtin/planning/enter-plan-mode';
import { executeTool } from '../fixtures/execute-tool';

function makeAgent(mode: PermissionMode): {
  readonly agent: Agent;
  readonly requestApproval: ReturnType<typeof vi.fn>;
  readonly telemetryTrack: ReturnType<typeof vi.fn>;
} {
  let active = false;
  const requestApproval = vi.fn(async () => ({ decision: 'approved' }));
  const telemetryTrack = vi.fn();
  const agent = {
    planMode: {
      get isActive() {
        return active;
      },
      get planFilePath() {
        return '/tmp/kimi-plan.md';
      },
      enter: vi.fn(async () => {
        active = true;
      }),
    },
    permission: { mode },
    rpc: { requestApproval },
    telemetry: { track: telemetryTrack },
    context: { history: [] },
    config: {
      get provider() {
        return { name: 'mock', modelName: 'mock-model' };
      },
    },
    generate: vi.fn().mockResolvedValue({
      message: { content: [{ type: 'text', text: '' }] },
    }),
  } as unknown as Agent;
  return { agent, requestApproval, telemetryTrack };
}

describe('EnterPlanMode telemetry', () => {
  it.each(['manual', 'auto', 'yolo'] satisfies PermissionMode[])(
    'tracks direct entry as auto_approved in %s mode',
    async (mode) => {
      const { agent, requestApproval, telemetryTrack } = makeAgent(mode);

      const result = await executeTool(new EnterPlanModeTool(agent), {
        turnId: '0',
        toolCallId: `call_${mode}`,
        args: {},
        signal: new AbortController().signal,
      });

      expect(result.isError).toBeFalsy();
      expect(requestApproval).not.toHaveBeenCalled();
      expect(telemetryTrack).toHaveBeenCalledWith('plan_enter_resolved', {
        outcome: 'auto_approved',
      });
    },
  );
});
```

- [ ] Run the plan-mode tests and verify they PASSES:

```bash
pnpm vitest run packages/agent-core/test/tools/enter-plan-mode.test.ts packages/agent-core/test/tools/planning/enter-plan-mode-telemetry.test.ts
```

Expected: all tests pass (11 in enter-plan-mode.test.ts + 3 in telemetry test).

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts packages/agent-core/src/tools/builtin/planning/enter-plan-mode.md packages/agent-core/test/tools/enter-plan-mode.test.ts packages/agent-core/test/tools/planning/enter-plan-mode-telemetry.test.ts
git commit -m "feat(agent-core): add optional topic param to EnterPlanModeTool"
```

---

### Task 5: Final verification — update harness test, run full test suite and typecheck

**Depends on:** Task 3, Task 4

**Files:**
- Modify: `packages/agent-core/test/agent/plan.test.ts` (lines 69–94, the EnterPlanMode harness test)

**Steps:**

- [ ] Update the harness test `enters plan mode through the EnterPlanMode tool and reminds the next step` in `packages/agent-core/test/agent/plan.test.ts` to account for the extra LLM call consumed by TopicGenerator during tool execution:

Replace lines 69–94 with:

```ts
  it('enters plan mode through the EnterPlanMode tool and reminds the next step', async () => {
    const enterPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_enter_plan',
      name: 'EnterPlanMode',
      arguments: '{}',
    };
    const ctx = testAgent({
      kaos: createPlanKaos({
        writeText: vi.fn(async (_path: string, content: string) => content.length),
      }),
    });
    ctx.configure({ tools: ['EnterPlanMode'] });
    await ctx.rpc.setPermission({ mode: 'yolo' });

    // TopicGenerator consumes one generate call during tool execution
    ctx.mockNextResponse({ type: 'text', text: 'user-dashboard' });
    ctx.mockNextResponse({ type: 'text', text: 'I will enter plan mode.' }, enterPlanModeCall);
    ctx.mockNextResponse({ type: 'text', text: 'Plan mode is active now.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Plan first' }] });

    await ctx.untilTurnEnd();
    await delay(10);
    expect(ctx.agent.planMode.isActive).toBe(true);
    expect(ctx.llmCalls).toHaveLength(3);
    expect(toolResultText(ctx.llmCalls[2]!.history)).toContain('Plan mode is now active');
    await ctx.expectResumeMatches();
  });
```

- [ ] Run the full agent-core test suite:

```bash
pnpm test --filter agent-core
```

Expected: all tests pass. If any snapshot test fails due to token count drift, run with `--update` once and review the diff to ensure only token counts changed:

```bash
pnpm vitest run --update packages/agent-core
```

- [ ] Run the agent-core typecheck:

```bash
pnpm tsc --noEmit --filter agent-core
```

Expected: zero type errors.

- [ ] Run the ody-code app tests:

```bash
pnpm test --filter ody-code
```

Expected: all tests pass. The ody-code Footer rendering only reads `planFilePath` as an opaque string; the filename format change does not affect it.

- [ ] Commit:

```bash
git add packages/agent-core/test/agent/plan.test.ts
git commit -m "test(agent-core): update harness test for TopicGenerator LLM call"
```

---

## Self-Review

- [ ] 1. Spec-coverage table:

| Spec Requirement | Task(s) | Status |
|---|---|---|
| EnterDesignModeTool / EnterPlanModeTool add optional `topic` param | Task 3, Task 4 | covered |
| LLM auto-extracts topic from recent user message | Task 1, Task 3, Task 4 | covered |
| Filename format `<topic>-YYYYMMDD-HHMMSS.md` | Task 1, Task 3, Task 4 | covered |
| planId stays independent random hero slug | Task 2 | covered |
| LLM failure fallback to `design-` / `plan-` prefix + timestamp | Task 3, Task 4 | covered |
| Prompt security instruction + code-level sensitive word filter | Task 1 | covered |
| Telemetry `topic_generation_failed` with reason | Task 1 | covered |
| Wire record persists `fileStem` for resume compatibility | Task 2 | covered |
| `isWritablePlanPath` continues to use `planId` | Task 2 (no change) | covered |

- [ ] 2. Placeholder scan: no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders.
- [ ] 3. No phantom tasks: every task produces a verifiable change; zero `--allow-empty`.
- [ ] 4. Dependency soundness: Task 1 has no deps; Task 2 has no deps; Tasks 3 and 4 depend on Tasks 1 and 2; Task 5 depends on Tasks 3 and 4. No later-task symbols referenced earlier.
- [ ] 5. Caller & build soundness: Task 2 changes `PlanMode.enter` and `restoreEnter` shared signatures. All callers were found via `grep` and compile without changes because `fileStem` is optional. Task 2 ends with a whole-tree typecheck. The same signature is not changed again in later tasks.
- [ ] 6. Test-the-risk: Task 1 tests stateless cleanup pipeline; Task 2 tests `fileStem` mutation and wire-record round-trip; Tasks 3 and 4 test tool execution mutates `planMode.enter` arguments with correct `fileStem`.
- [ ] 7. Type consistency: `fileStem?: string` in `PlanMode.enter`, `restoreEnter`, and the wire record type all align. `TopicGenerator.generate()` returns `Promise<string | null>` as used by the tools.

---

## Risks & Mitigations

| # | Risk | Mitigation |
|---|---|---|
| 1 | `agent.generate` called during tool execution consumes a mocked response in harness tests | Add an extra `mockNextResponse` before existing mocks in affected harness tests; document the exact change in Task 2 |
| 2 | Old wire records lack `fileStem`; restore must fall back gracefully | `restoreEnter` uses `fileStem ?? id`; wire type keeps `fileStem` optional |
| 3 | `isWritablePlanPath` must continue to use `planId`, not `fileStem` | No code change to `isWritablePlanPath`; verified by existing tests |
| 4 | Extra LLM call adds latency to plan/design entry | Lightweight prompt (< 200 tokens) + 3 s timeout; failure telemetry emitted |

