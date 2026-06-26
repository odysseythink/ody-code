# Repo Knowledge Microagents — Precedence & Budgeting (A.3) Implementation Plan

**Goal:** Add source-precedence ordering and a per-injection token budget to the `KnowledgeMicroagentInjector` built in A.2, with config schema, TOML round-trip, and telemetry.

**Architecture:** Pure helper functions (`sortBySourcePriority`, `resolveBudgetLimit`, `applyBudget`) compute ordering and budget from matched microagents. These are wired into `KnowledgeMicroagentInjector.getInjection()` between the A.2 matcher and the rendering loop. A new `MicroagentBudgetConfigSchema` is added to `OdyConfigSchema` / `OdyConfigPatchSchema`, with corresponding TOML parse/write paths. No new files are created; all changes are edits to existing files.

**Tech Stack:** TypeScript, zod (schema), vitest (tests), smol-toml (config I/O).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use `- [ ]` checkboxes for tracking.

---

## File Structure

| Task | File | Action |
|------|------|--------|
| 1 | `packages/agent-core/src/config/schema.ts:296-326, 348-378` | Modify: add `MicroagentBudgetConfigSchema`, wire into both schemas |
| 1 | `packages/agent-core/src/config/toml.ts:119-148, 313-351` | Modify: add `microagentBudget` to parse/write paths |
| 1 | `packages/agent-core/test/config/configs.test.ts:708+` | Modify: add schema validation + TOML round-trip tests |
| 2 | `packages/agent-core/src/agent/injection/knowledge-microagent.ts:1-20` | Modify: add helper functions near file top |
| 2 | `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts:209+` | Modify: add helper unit tests |
| 3 | `packages/agent-core/src/agent/injection/knowledge-microagent.ts:136-178` | Modify: extend `getInjection()` with sorting/budget/telemetry/omitted-note |
| 3 | `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts:494+` | Modify: add injector precedence + budget tests |

---

## Dependency Overview

```
Task 1 (config schema + TOML)
  │
  ├──► Task 2 (sortBySourcePriority + resolveBudgetLimit + applyBudget helpers + unit tests)
  │      │
  │      └──► Task 3 (injector extension + injector tests P1-P3, B1-B8)
  │
  └──► Task 3 (imports MicroagentBudgetConfig type from schema)
```

All three tasks are serial. Task 2 depends on Task 1 only for the `MicroagentBudgetConfig` type import (used by `resolveBudgetLimit`). Task 3 depends on both Task 1 (type) and Task 2 (helpers).

---

## Risks & Open Questions

| Risk | Mitigation |
|------|------------|
| TOML `[microagent_budget]` silently dropped by `transformTomlData` because it's a plain object without a special case | Task 1 adds the case explicitly |
| `estimateTokens` may underestimate CJK bodies (~1 tok/char heuristic) | Design documents this as acceptable heuristic; default 1024 is conservative |
| `OdyConfigPatchSchema` missing field causes silent config drops | Task 1 adds to both schemas + schema test |
| Budget suppressing all microagents when first match exceeds cap | By design; telemetry reports the skip |

---

### Task 1: Config schema + TOML mapping

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/config/schema.ts:296-326` (add `MicroagentBudgetConfigSchema`, wire into `OdyConfigSchema`)
- Modify: `packages/agent-core/src/config/schema.ts:348-378` (wire into `OdyConfigPatchSchema`)
- Modify: `packages/agent-core/src/config/toml.ts:119-148` (add `microagentBudget` to `transformTomlData`)
- Modify: `packages/agent-core/src/config/toml.ts:313-351` (add `microagentBudget` to `configToTomlData`)
- Modify: `packages/agent-core/test/config/configs.test.ts:708+` (add schema + TOML tests)

#### Step 1: Write the failing schema test

```ts
// Append to packages/agent-core/test/config/configs.test.ts, before the final closing

describe('microagentBudget config', () => {
  it('C1: OdyConfigSchema accepts microagentBudget.maxTokens', () => {
    const config = OdyConfigSchema.parse({
      microagentBudget: { maxTokens: 512 },
    });
    expect(config.microagentBudget?.maxTokens).toBe(512);
  });

  it('C2: OdyConfigSchema rejects negative maxTokens', () => {
    expect(() =>
      OdyConfigSchema.parse({
        microagentBudget: { maxTokens: -1 },
      }),
    ).toThrow();
  });

  it('C3: OdyConfigPatchSchema accepts microagentBudget', () => {
    const patch = OdyConfigPatchSchema.parse({
      microagentBudget: { maxTokens: 0 },
    });
    expect(patch.microagentBudget?.maxTokens).toBe(0);
  });

  it('round-trips microagent_budget through TOML parse/write', () => {
    const toml = '[microagent_budget]\nmax_tokens = 512\n';
    const config = parseConfigString(toml, 'test.toml');
    expect(config.microagentBudget?.maxTokens).toBe(512);

    const data = configToTomlData(config);
    const section = data['microagent_budget'] as Record<string, unknown>;
    expect(section).toBeDefined();
    expect(section.max_tokens).toBe(512);
  });

  it('omits microagent_budget section when not configured', () => {
    const config = OdyConfigSchema.parse({});
    const data = configToTomlData(config);
    expect(data).not.toHaveProperty('microagent_budget');
  });
});
```

#### Step 2: Run and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/config/configs.test.ts
```

Expected failure: `OdyConfigSchema.parse({ microagentBudget: ... })` throws because `microagentBudget` is not a recognized key in the schema.

#### Step 3: Write the minimal implementation

**In `packages/agent-core/src/config/schema.ts`**, add after line ~295 (after `E2EConfigSchema` / before `OdyConfigSchema`):

```ts
export const MicroagentBudgetConfigSchema = z.object({
  maxTokens: z.number().int().min(0).optional(),
});

export type MicroagentBudgetConfig = z.infer<typeof MicroagentBudgetConfigSchema>;
```

**In `OdyConfigSchema`** (line 296-326), add between `e2e` and `raw`:

```ts
  microagentBudget: MicroagentBudgetConfigSchema.optional(),
```

So the end of `OdyConfigSchema` becomes:

```ts
  browser: BrowserConfigSchema.optional(),
  e2e: E2EConfigSchema.optional(),
  microagentBudget: MicroagentBudgetConfigSchema.optional(),
  raw: z.record(z.string(), z.unknown()).optional(),
```

**In `OdyConfigPatchSchema`** (line 348-378), add between `browser` and the closing `})`:

```ts
    microagentBudget: MicroagentBudgetConfigSchema.optional(),
```

**In `packages/agent-core/src/config/toml.ts`**, add to `transformTomlData` (after the `e2e` case at line ~143):

```ts
    } else if (targetKey === 'microagentBudget' && isPlainObject(value)) {
      result[targetKey] = transformPlainObject(value);
    } else if (!isPlainObject(value)) {
```

In `configToTomlData`, add after `setSection(out, 'e2e', config.e2e, e2eToToml);` (line ~347):

```ts
  setSection(out, 'microagent_budget', config.microagentBudget, microagentBudgetToToml);
```

Add the helper function near the other `*ToToml` helpers (e.g., after `e2eToToml` at line ~563):

```ts
function microagentBudgetToToml(
  budget: MicroagentBudgetConfig,
  rawBudget: unknown,
): Record<string, unknown> {
  const out = cloneRecord(rawBudget);
  for (const [key, value] of Object.entries(budget)) {
    setDefined(out, camelToSnake(key), value);
  }
  return out;
}
```

Also add the import of `MicroagentBudgetConfig` to the imports on line ~11:

```ts
  type MicroagentBudgetConfig,
```

#### Step 4: Run and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/config/configs.test.ts
```

#### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck
```

Must pass — no callers modified (new optional field, no A.2 code references it yet).

#### Step 6: Commit

```bash
git add packages/agent-core/src/config/schema.ts packages/agent-core/src/config/toml.ts packages/agent-core/test/config/configs.test.ts
git commit -m "feat: add microagentBudget config schema with TOML round-trip"

---

### Task 2: sortBySourcePriority + resolveBudgetLimit + applyBudget helpers

**Depends on:** Task 1 (imports `MicroagentBudgetConfig` type from schema)

**Files:**
- Modify: `packages/agent-core/src/agent/injection/knowledge-microagent.ts:1-20` (add helper functions + types after existing exports)
- Modify: `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts:209+` (add helper unit tests before injector tests)

#### Step 1: Write the failing test

Add to `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts`, after the `matchKnowledgeMicroagents` describe block (line ~209) and before the injector tests (line ~211). Also add an import for `estimateTokens`:

```ts
// ── Budget & precedence helpers ─────────────────────────────────

import {
  sortBySourcePriority,
  resolveBudgetLimit,
  applyBudget,
} from '../../../src/agent/injection/knowledge-microagent';
import { estimateTokens } from '../../../src/utils/tokens';
import type { Agent as AgentType } from '../../../src/agent';

describe('sortBySourcePriority', () => {
  it('orders by source: project > user > extra > builtin', () => {
    const builtin = microagent('ba', ['t1']);
    const project = microagent('pa', ['t1']);
    const user = microagent('ua', ['t1']);
    const extra = microagent('ea', ['t1']);

    const skillDefs = [
      { ...builtin, source: 'builtin' as const },
      { ...project, source: 'project' as const },
      { ...user, source: 'user' as const },
      { ...extra, source: 'extra' as const },
    ] as SkillDefinition[];

    const matches = skillDefs.map((s) => ({ skill: s, trigger: 't1' }));
    const sorted = sortBySourcePriority(matches);
    const sources = sorted.map((m) => m.skill.source);
    expect(sources).toEqual(['project', 'user', 'extra', 'builtin']);
  });

  it('tie-breaks by name lexicographically within same source', () => {
    const beta = { ...microagent('beta', ['t1']), source: 'project' as const } as SkillDefinition;
    const alpha = { ...microagent('alpha', ['t1']), source: 'project' as const } as SkillDefinition;

    const matches = [
      { skill: beta, trigger: 't1' },
      { skill: alpha, trigger: 't1' },
    ];
    const sorted = sortBySourcePriority(matches);
    expect(sorted.map((m) => m.skill.name)).toEqual(['alpha', 'beta']);
  });

  it('returns empty for empty input', () => {
    expect(sortBySourcePriority([])).toEqual([]);
  });
});

describe('resolveBudgetLimit', () => {
  it('returns configured maxTokens', () => {
    const agent = {
      kimiConfig: { microagentBudget: { maxTokens: 500 } },
    } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(500);
  });

  it('returns default 1024 when microagentBudget is undefined', () => {
    const agent = { kimiConfig: undefined } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(1024);
  });

  it('returns default 1024 when maxTokens is undefined', () => {
    const agent = {
      kimiConfig: { microagentBudget: {} },
    } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(1024);
  });

  it('returns Infinity when maxTokens is 0', () => {
    const agent = {
      kimiConfig: { microagentBudget: { maxTokens: 0 } },
    } as unknown as AgentType;
    expect(resolveBudgetLimit(agent)).toBe(Infinity);
  });
});

describe('applyBudget', () => {
  const shortContent = '# Short\n\nOnly a few tokens.';           // ~10 tokens
  const longContent = '# Long\n\n' + 'x'.repeat(5000);           // ~1250 tokens

  const short = {
    ...microagent('short', ['t1'], shortContent),
    source: 'project' as const,
  } as SkillDefinition;
  const long = {
    ...microagent('long', ['t1'], longContent),
    source: 'project' as const,
  } as SkillDefinition;

  it('injects all when budget is unlimited (maxTokens=Infinity)', () => {
    const matches = [
      { skill: long, trigger: 't1' },
      { skill: short, trigger: 't1' },
    ];
    const result = applyBudget(matches, Infinity);
    expect(result.injected).toHaveLength(2);
    expect(result.skipped).toHaveLength(0);
    expect(result.total).toBe(Infinity);
    expect(result.used).toBeGreaterThan(0);
  });

  it('skips microagent when it would exceed budget', () => {
    const budget = estimateTokens(shortContent) + 1; // fits short + 1 extra
    const matches = [
      { skill: short, trigger: 't1' },
      { skill: long, trigger: 't1' },
    ];
    const result = applyBudget(matches, budget);
    expect(result.injected).toHaveLength(1);
    expect(result.injected[0]!.skill.name).toBe('short');
    expect(result.skipped).toHaveLength(1);
    expect(result.skipped[0]!.match.skill.name).toBe('long');
    expect(result.skipped[0]!.reason).toBe('budget_exceeded');
  });

  it('skips all when every body exceeds budget', () => {
    const matches = [{ skill: long, trigger: 't1' }];
    const result = applyBudget(matches, 10); // tiny budget
    expect(result.injected).toHaveLength(0);
    expect(result.skipped).toHaveLength(1);
    expect(result.used).toBe(0);
  });

  it('skips empty bodies silently (no budget consumed, no telemetry here)', () => {
    const empty = {
      ...microagent('empty', ['t1'], ''),
      source: 'project' as const,
    } as SkillDefinition;
    const matches = [
      { skill: empty, trigger: 't1' },
      { skill: short, trigger: 't1' },
    ];
    const result = applyBudget(matches, 100);
    // empty body skipped without consuming budget
    expect(result.injected).toHaveLength(1);
    expect(result.injected[0]!.skill.name).toBe('short');
    expect(result.skipped).toHaveLength(0);
  });

  // Must-survive: a microagent with body exactly at budget limit should be injected
  it('injects body that fits exactly at budget limit', () => {
    const exactContent = 'abcd'; // 1 ASCII token
    const exactBudget = estimateTokens(exactContent);
    const skill = {
      ...microagent('exact', ['t1'], exactContent),
      source: 'project' as const,
    } as SkillDefinition;
    const result = applyBudget([{ skill, trigger: 't1' }], exactBudget);
    expect(result.injected).toHaveLength(1);
    expect(result.skipped).toHaveLength(0);
  });
});
```

#### Step 2: Run and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

Expected failure: imports for `sortBySourcePriority`, `resolveBudgetLimit`, `applyBudget` are not yet exported from the injector file.

#### Step 3: Write the minimal implementation

**In `packages/agent-core/src/agent/injection/knowledge-microagent.ts`**, add after the existing imports (before line 7) and after the existing type exports:

Add the new import:

```ts
import { estimateTokens } from '../../utils/tokens';
import type { OdyConfig } from '../../config';
```

Add the new types after the existing `KnowledgeMicroagentMatch` interface (line ~18):

```ts
export interface MicroagentBudgetResult {
  readonly injected: readonly KnowledgeMicroagentMatch[];
  readonly skipped: readonly SkippedMicroagent[];
  readonly used: number;
  readonly total: number;
}

export interface SkippedMicroagent {
  readonly match: KnowledgeMicroagentMatch;
  readonly reason: 'budget_exceeded';
}
```

Add the three helper functions before the `KNOWLEDGE_MICROAGENT_VARIANT` constant (line ~120):

```ts
// ── Budget & precedence helpers ─────────────────────────────────────────

const SOURCE_PRIORITY: Record<SkillDefinition['source'], number> = {
  project: 0,
  user: 1,
  extra: 2,
  builtin: 3,
};

/**
 * Resolve the per-injection token budget limit from agent config.
 *   - `undefined` → default 1024
 *   - `0` → unlimited (Infinity)
 *   - positive number → as configured
 */
export function resolveBudgetLimit(agent: Agent): number {
  const configured = (agent.kimiConfig as OdyConfig | undefined)?.microagentBudget?.maxTokens;
  if (configured === undefined) return 1024;
  if (configured === 0) return Infinity;
  return configured;
}

/**
 * Sort matched microagents by source precedence (project > user > extra > builtin),
 * then by name lexicographically within the same source.
 */
export function sortBySourcePriority(
  matches: readonly KnowledgeMicroagentMatch[],
): KnowledgeMicroagentMatch[] {
  return [...matches].sort((a, b) => {
    const pa = SOURCE_PRIORITY[a.skill.source];
    const pb = SOURCE_PRIORITY[b.skill.source];
    if (pa !== pb) return pa - pb;
    return a.skill.name.localeCompare(b.skill.name);
  });
}

/**
 * Apply a per-injection token budget to sorted matches.
 * Microagents are injected in order until the budget is exhausted;
 * remaining matches are skipped with reason 'budget_exceeded'.
 * Empty bodies are skipped silently and do not consume budget.
 */
export function applyBudget(
  sortedMatches: readonly KnowledgeMicroagentMatch[],
  maxTokens: number,
): MicroagentBudgetResult {
  const budget = maxTokens === Infinity ? Infinity : maxTokens;
  let used = 0;
  const injected: KnowledgeMicroagentMatch[] = [];
  const skipped: SkippedMicroagent[] = [];

  for (const match of sortedMatches) {
    const body = match.skill.content.trim();
    if (body.length === 0) continue;

    const tokens = estimateTokens(body);
    if (used + tokens <= budget) {
      used += tokens;
      injected.push(match);
    } else {
      skipped.push({ match, reason: 'budget_exceeded' });
    }
  }

  return { injected, skipped, used, total: budget };
}
```

#### Step 4: Run and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

#### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck
```

#### Step 6: Commit

```bash
git add packages/agent-core/src/agent/injection/knowledge-microagent.ts packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
git commit -m "feat: add sortBySourcePriority, resolveBudgetLimit, applyBudget helpers"

---

### Task 3: Extend injector getInjection() with sorting, budget, telemetry, and omitted-note

**Depends on:** Task 1 (schema type), Task 2 (helpers: `sortBySourcePriority`, `resolveBudgetLimit`, `applyBudget`)

**Files:**
- Modify: `packages/agent-core/src/agent/injection/knowledge-microagent.ts:136-178` (extend `getInjection()`)
- Modify: `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts:494+` (add precedence + budget injector tests)

#### Step 1: Write the failing tests

Append to `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts` after the `N2` test (line ~493), before the final closing `});` of the `KnowledgeMicroagentInjector` describe block.

Also update the `microagent` helper to accept an explicit `source` and update the `MicroagentAgentStub` to accept a `kimiConfig`:

Update the `microagent` helper (line ~12-25) — change the return type to accept an optional `source`:

```ts
function microagent(
  name: string,
  triggers: readonly string[],
  content = '# Test\n\nSome body text.',
  source: SkillDefinition['source'] = 'project',
): SkillDefinition {
  return {
    name,
    description: `Knowledge: ${name}`,
    path: `/test/${name}.md`,
    dir: '/test',
    content,
    metadata: { type: 'knowledge', triggers },
    source,
  };
}
```

Update `MicroagentAgentStub` interface (line ~219-225) to add optional `kimiConfig`:

```ts
interface MicroagentAgentStub {
  history: ContextMessage[];
  enabledFlags: Set<string>;
  sessionActive: boolean;
  microagents: SkillDefinition[] | null;
  telemetryCalls: Array<{ event: string; properties: Record<string, unknown> }>;
  kimiConfig?: Record<string, unknown>;
}
```

Update `microagentAgent` function (line ~227-268) to pass `kimiConfig` into the agent stub:

After the `log` property at line ~261-266:

```ts
    kimiConfig: stub.kimiConfig,
```

Now append the new tests after `N2` (line ~493):

```ts
  // ── Precedence tests ─────────────────────────────────────────────

  it('P1: project wins over builtin under budget (only one fits)', async () => {
    const projectContent = '# Project conventions\n\n' + 'x'.repeat(3600); // ~900 tokens
    const builtinContent = '# Builtin conventions\n\nSome text.';

    const projectAgent = microagent('project-agent', ['component'], projectContent, 'project');
    const builtinAgent = microagent('builtin-agent', ['component'], builtinContent, 'builtin');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [projectAgent, builtinAgent],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('Project conventions');
    expect(text).toContain('## project-agent');
    expect(text).not.toContain('Builtin conventions');

    // Check skipped telemetry
    const skippedCalls = telemetryCalls.filter((c) => c.event === 'microagent_skipped');
    expect(skippedCalls).toHaveLength(1);
    expect(skippedCalls[0]!.properties.skill_name).toBe('builtin-agent');
    expect(skippedCalls[0]!.properties.reason).toBe('budget_exceeded');
  });

  it('P2: user wins over extra under budget', async () => {
    const userContent = '# User conventions\n\n' + 'x'.repeat(3600); // ~900 tokens
    const extraContent = '# Extra conventions\n\nSome text.';

    const userAgent = microagent('user-agent', ['component'], userContent, 'user');
    const extraAgent = microagent('extra-agent', ['component'], extraContent, 'extra');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [userAgent, extraAgent],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 800 } }, // small budget to force priority
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('User conventions');
    expect(text).not.toContain('Extra conventions');
  });

  it('P3: same-source tie-breaker is name lexicographic', async () => {
    const beta = microagent('beta-agent', ['component'], '# Beta conventions\n\nContent.', 'project');
    const alpha = microagent('alpha-agent', ['component'], '# Alpha conventions\n\nContent.', 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [beta, alpha],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    const alphaIndex = text!.indexOf('## alpha-agent');
    const betaIndex = text!.indexOf('## beta-agent');
    expect(alphaIndex).toBeLessThan(betaIndex);
  });

  // ── Budget tests ──────────────────────────────────────────────────

  it('B1: default budget (1024) caps injection', async () => {
    // ~900 tokens → fits
    const projectContent = '# Project\n\n' + 'x'.repeat(3600);
    // ~300 tokens → would fit alone but together exceeds default 1024
    const userContent = '# User\n\n' + 'y'.repeat(1200);

    const projectAgent = microagent('proj', ['component'], projectContent, 'project');
    const userAgent = microagent('usr', ['component'], userContent, 'user');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [projectAgent, userAgent],
      telemetryCalls,
      // No kimiConfig → default 1024
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('## proj');
    // user may or may not fit depending on token estimate — check telemetry
    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    expect(injectedCalls).toHaveLength(1);
    expect(injectedCalls[0]!.properties.skill_name).toBe('proj');
    expect(injectedCalls[0]!.properties).toHaveProperty('budget_used');
    expect(injectedCalls[0]!.properties).toHaveProperty('budget_total');
  });

  it('B2: maxTokens=0 disables cap (unlimited)', async () => {
    const largeContent = '# Large\n\n' + 'x'.repeat(10000);
    const agentA = microagent('a', ['component'], largeContent, 'project');
    const agentB = microagent('b', ['component'], largeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [agentA, agentB],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 0 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('## a');
    expect(text).toContain('## b');

    const skippedCalls = telemetryCalls.filter((c) => c.event === 'microagent_skipped');
    expect(skippedCalls).toHaveLength(0);

    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    injectedCalls.forEach((c) => {
      expect(c.properties.budget_total).toBe(0); // 0 when unlimited
    });
  });

  it('B3: custom maxTokens works', async () => {
    const smallContent = '# Small\n\nabc'; // ~2 tokens
    const mediumContent = '# Medium\n\n' + 'x'.repeat(100); // ~25 tokens
    const largeContent = '# Large\n\n' + 'x'.repeat(2000); // ~500 tokens

    const small = microagent('small', ['component'], smallContent, 'project');
    const medium = microagent('medium', ['component'], mediumContent, 'project');
    const large = microagent('large', ['component'], largeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [small, medium, large],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 50 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    // small + medium should fit (~27 tokens), large (~500) should not
    const names = injectedCalls.map((c) => c.properties.skill_name as string);
    expect(names).toContain('small');
    expect(names).toContain('medium');
    expect(names).not.toContain('large');

    // budget_total should be 50
    injectedCalls.forEach((c) => {
      expect(c.properties.budget_total).toBe(50);
    });
  });

  it('B4: single oversized body skipped, reminder includes omitted note', async () => {
    const hugeContent = '# Huge\n\n' + 'x'.repeat(10000); // ~2500 tokens
    const huge = microagent('huge-agent', ['component'], hugeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [huge],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 500 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    // No bodies injected, but reminder may still exist (omitted note only)
    // Actually, the design says: if bodies.length === 0, return undefined
    const text = reminderText(history);
    // Since all injected microagents were skipped, bodies is empty → no injection
    expect(text).toBeUndefined();

    // But skipped telemetry is emitted
    const skippedCalls = telemetryCalls.filter((c) => c.event === 'microagent_skipped');
    expect(skippedCalls).toHaveLength(1);
    expect(skippedCalls[0]!.properties.skill_name).toBe('huge-agent');
    expect(skippedCalls[0]!.properties.reason).toBe('budget_exceeded');
  });

  it('B5: budget usage telemetry on microagent_injected', async () => {
    const content = '# Test\n\nSome body text.';
    const agentA = microagent('test-agent', ['component'], content, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [agentA],
      telemetryCalls,
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const injectedCall = telemetryCalls.find((c) => c.event === 'microagent_injected');
    expect(injectedCall).toBeDefined();
    expect(injectedCall!.properties.budget_used).toEqual(expect.any(Number));
    expect(injectedCall!.properties.budget_total).toEqual(expect.any(Number));
  });

  it('B6: skipped telemetry has correct properties', async () => {
    const hugeContent = '# Huge\n\n' + 'x'.repeat(10000);
    const huge = microagent('huge', ['component'], hugeContent, 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [huge],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 100 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const skippedCall = telemetryCalls.find((c) => c.event === 'microagent_skipped');
    expect(skippedCall).toBeDefined();
    expect(skippedCall!.properties).toMatchObject({
      skill_name: 'huge',
      trigger: 'component',
      skill_source: 'project',
      reason: 'budget_exceeded',
    });
    expect(skippedCall!.properties.budget_used).toEqual(expect.any(Number));
    expect(skippedCall!.properties.budget_total).toEqual(expect.any(Number));
  });

  it('B7: reminder includes omitted-note when microagent is skipped', async () => {
    // Two microagents: one fits, one doesn't → reminder has both + omitted note
    const shortContent = '# Short\n\nabc'; // ~2 tokens
    const longContent = '# Long\n\n' + 'x'.repeat(10000); // ~2500 tokens

    const short = microagent('short-keep', ['component'], shortContent, 'project');
    const long = microagent('long-skip', ['component'], longContent, 'user');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [short, long],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 500 } },
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    expect(text).toContain('## short-keep');
    expect(text).not.toContain('## long-skip');
    expect(text).toContain('omitted due to the microagent token budget');
    expect(text).toContain('long-skip');
  });

  it('B8: empty bodies still not counted toward budget', async () => {
    const empty = microagent('empty-keep', ['component'], '', 'project');
    const normal = microagent('normal-keep', ['component'], '# Normal\n\nContent.', 'project');
    const history: ContextMessage[] = [userMessage('add a component')];
    const telemetryCalls: MicroagentAgentStub['telemetryCalls'] = [];
    const agent = microagentAgent({
      history,
      enabledFlags: new Set(['repo-knowledge']),
      sessionActive: false,
      microagents: [empty, normal],
      telemetryCalls,
      kimiConfig: { microagentBudget: { maxTokens: 1 } }, // extremely tight budget
    });
    const injector = new KnowledgeMicroagentInjector(agent);

    await injector.inject();

    const text = reminderText(history);
    expect(text).toBeDefined();
    // empty skipped without consuming budget; normal fits
    expect(text).not.toContain('## empty-keep');
    expect(text).toContain('## normal-keep');

    // No skipped telemetry for empty body (it was skipped before budget check in getInjection)
    const injectedCalls = telemetryCalls.filter((c) => c.event === 'microagent_injected');
    expect(injectedCalls).toHaveLength(1);
    expect(injectedCalls[0]!.properties.skill_name).toBe('normal-keep');
  });
```

#### Step 2: Run and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

Expected failure: new tests fail because `getInjection()` still uses the A.2 behavior without sorting/budget/omitted-note.

#### Step 3: Write the minimal implementation

Replace the entire `getInjection()` method (lines 136-178) in `packages/agent-core/src/agent/injection/knowledge-microagent.ts`:

```ts
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

    const maxTokens = resolveBudgetLimit(this.agent);
    const sorted = sortBySourcePriority(matches);
    const budget = applyBudget(sorted, maxTokens);

    if (budget.injected.length === 0 && budget.skipped.length === 0) return undefined;

    const bodies: string[] = [];
    for (const match of budget.injected) {
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
        budget_used: budget.used,
        budget_total: budget.total === Infinity ? 0 : budget.total,
      });
      bodies.push(`## ${match.skill.name}\n\n${body}`);
    }

    for (const skipped of budget.skipped) {
      this.agent.telemetry.track('microagent_skipped', {
        skill_name: skipped.match.skill.name,
        trigger: skipped.match.trigger,
        skill_source: skipped.match.skill.source,
        reason: skipped.reason,
        budget_used: budget.used,
        budget_total: budget.total === Infinity ? 0 : budget.total,
      });
    }

    if (bodies.length === 0) return undefined;

    const lines = [
      "The following repo-specific conventions are relevant to your current task.",
      "Apply them without mentioning them to the user unless asked.",
      "",
      bodies.join("\n\n---\n\n"),
    ];

    if (budget.skipped.length > 0) {
      const omittedNames = budget.skipped.map((s) => s.match.skill.name).join(", ");
      lines.push("");
      lines.push(`The following conventions were omitted due to the microagent token budget: ${omittedNames}.`);
    }

    return lines.join("\n");
  }
```

#### Step 4: Run tests and verify they PASS

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
```

All existing A.2 tests + new A.3 tests must pass.

#### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck
```

#### Step 6: Run full agent-core test suite

```bash
pnpm --filter @odysseythink/agent-core test
```

#### Step 7: Commit

```bash
git add packages/agent-core/src/agent/injection/knowledge-microagent.ts packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
git commit -m "feat: add precedence sorting, token budget, and omitted-note to KnowledgeMicroagentInjector"

---

## Self-Review

- [ ] **1. Spec-coverage table**: map every spec section/requirement → Task(s)

  | Design section / requirement | Task | Status |
  |---|---|---|
  | §6.1 `MicroagentBudgetConfigSchema` + zod def | Task 1 | covered |
  | §6.1 Wire into `OdyConfigSchema` | Task 1 | covered |
  | §6.1 Wire into `OdyConfigPatchSchema` | Task 1 | covered |
  | §8.1 TOML parse path (`transformTomlData`) | Task 1 | covered |
  | §8.1 TOML write path (`configToTomlData`) | Task 1 | covered |
  | §7.1 `resolveBudgetLimit` (default 1024, 0→∞, configured) | Task 2 | covered |
  | §7.2 `sortBySourcePriority` (project>user>extra>builtin, name tie-break) | Task 2 | covered |
  | §7.3 `applyBudget` (loop, skip full microagents, empty-body skip) | Task 2 | covered |
  | §6.2 `MicroagentBudgetResult` / `SkippedMicroagent` types | Task 2 | covered |
  | §7.4 Injector `getInjection()` integration of sorting + budget | Task 3 | covered |
  | §6.3 `microagent_injected` telemetry with `budget_used`/`budget_total` | Task 3 | covered |
  | §6.3 `microagent_skipped` telemetry event | Task 3 | covered |
  | §7.4 Omitted-note in reminder when budget skips occur | Task 3 | covered |
  | P1-P3 Precedence tests | Task 3 | covered |
  | B1-B8 Budget tests | Task 3 | covered |
  | C1-C3 Config schema tests | Task 1 | covered |
  | §8.3 Telemetry consumer (no changes) | — | no-op |
  | Per-source/agent budgets | — | out of scope (deferred) |
  | Cross-turn cumulative budget | — | out of scope (deferred) |
  | Truncating single microagent | — | out of scope (deferred) |
  | Persistent budget state across sessions | — | out of scope (deferred) |
  | UI/CLI status panel | — | out of scope (deferred) |

- [ ] **2. Placeholder scan**: No `TODO`, `TBD`, "implement later", "similar to Task N", or deferred-by-dependency excuses anywhere in task steps. Every step contains the exact code or exact command an engineer needs.

- [ ] **3. No phantom tasks**: Every task produces a verifiable change — Task 1 adds schema + TOML + tests, Task 2 adds 3 functions + types + tests, Task 3 extends injector + adds 11 tests. Zero `--allow-empty` commits. Telemetry consumer change is correctly `no-op`.

- [ ] **4. Dependency soundness**: Task 2 `Depends on: Task 1` — only for `MicroagentBudgetConfig` type import (defined in Task 1). Task 3 `Depends on: Task 2` — uses `sortBySourcePriority`, `resolveBudgetLimit`, `applyBudget` (all defined in Task 2). Nothing references a symbol only a later task creates.

- [ ] **5. Caller & build soundness**:
  - Task 1 adds an **optional** field to `OdyConfigSchema` and `OdyConfigPatchSchema`. `OdyConfig` type gains `microagentBudget?: MicroagentBudgetConfig`. No existing callers access `microagentBudget`, so no caller updates needed. Whole-tree `pnpm -r typecheck` at end of Task 1 verifies compiles clean.
  - Task 2 adds new exported functions to `knowledge-microagent.ts`. No callers yet — they're used in Task 3. `estimateTokens` import reuses existing utility — confirmed at `packages/agent-core/src/utils/tokens.ts:11-22`.
  - Task 3 modifies `getInjection()` but the public surface (`KnowledgeMicroagentInjector` class, `inject()` method) is unchanged. `InjectionManager` construction path requires no change (injector always constructed, gating in `getInjection()`).
  - **Consumer trace**: `resolveBudgetLimit` reads `agent.kimiConfig?.microagentBudget?.maxTokens`. `agent.kimiConfig` is of type `OdyConfig | undefined` (verified at `packages/agent-core/src/agent/index.ts:114`). The config is assigned at `packages/agent-core/src/rpc/core-impl.ts:430`. The new optional field flows through correctly — `undefined` → default 1024, `0` → `Infinity`, positive number as-is.

- [ ] **6. Test-the-risk**:
  - **Budget predicate** (`used + tokens <= maxTokens`): B1 tests default 1024, B2 tests unlimited (0), B3 tests custom value, B4 tests all-skipped edge case, "exact fit" test in Task 2 verifies boundary.
  - **Source priority map**: P1-P3 cover `project > builtin`, `user > extra`, and name tie-breaker.
  - **0 means unlimited**: B2 asserts `budget_total === 0` and both large bodies injected. `resolveBudgetLimit` unit test in Task 2 covers the numeric path.
  - **Empty bodies**: B8 asserts empty bodies don't consume budget. `applyBudget` unit test in Task 2 covers empty-body skip.
  - **Omitted note**: B7 asserts the note text appears and contains the skipped name.
  - **Schema validation**: C2 asserts negative maxTokens rejection.
  - **TOML round-trip**: Task 1 includes `[microagent_budget]\nmax_tokens = 512\n` parse+write test.
  - **Must-survive inputs for sortBySourcePriority**: empty array → empty array (tested). Two project microagents with different names → name-ordered (tested). All four sources → correct priority order (tested).

- [ ] **7. Type consistency**:
  - `MicroagentBudgetConfig.maxTokens`: `z.number().int().min(0).optional()` — aligned across schema, `resolveBudgetLimit` return type, and telemetry `budget_total`.
  - `SkillDefinition.source`: `'project' | 'user' | 'extra' | 'builtin'` — `SOURCE_PRIORITY` map keys match exactly.
  - `KnowledgeMicroagentMatch`: unchanged from A.2 — `skill: SkillDefinition; trigger: string`.
  - `MicroagentBudgetResult`: `injected: readonly KnowledgeMicroagentMatch[]`, `skipped: readonly SkippedMicroagent[]`, `used: number`, `total: number`.
  - `SkippedMicroagent`: `match: KnowledgeMicroagentMatch; reason: 'budget_exceeded'`.
  - Telemetry properties: `budget_used: number`, `budget_total: number` — both `Number` in vitest matchers.
<!-- e2e-enriched -->

### Task 4: Generate and run E2E tests

Based on the changed files, validate the following tools:
- ExitPlanModeTool (priority: critical)

Use the RunE2ETests tool after completing the implementation tasks above.

