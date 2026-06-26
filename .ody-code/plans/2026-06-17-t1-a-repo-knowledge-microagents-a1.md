# T1-A.1 — Repo Knowledge Microagents Parser Implementation Plan

**Goal:** Extend the existing `SkillRegistry` to parse and load `type: knowledge` microagents from `.ody-code/microagents/` — keeping them out of the invocable skill listing while making them discoverable via a dedicated registry helper.

**Architecture:** Microagents reuse the existing `SkillDefinition` / `SkillRegistry` infrastructure (discovery, parsing, indexing). A new `triggers` field is added to `SkillMetadata`, validated only for `type: knowledge` via a `parseTriggers` helper. The scanner picks up `.ody-code/microagents/` as a second project brand dir, loaded after `.ody-code/skills/` so skills win on name collisions. A `listKnowledgeMicroagents()` registry method provides a filtered view for future A.2 consumers.

**Tech Stack:** TypeScript, Vitest (tests), existing `packages/agent-core` build toolchain.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Task | File | Action |
|------|------|--------|
| 1 | `packages/agent-core/src/skill/types.ts:11,71-75` | Modify: add `triggers` field, `isKnowledgeSkillType`, update `isSupportedSkillType` |
| 1 | `packages/agent-core/src/skill/parser.ts:35-37,138,244+` | Modify: update error message, integrate trigger validation, add `parseTriggers` |
| 1 | `packages/agent-core/test/skill/microagent-parser.test.ts` | Create: type-helper tests + P1–P5 parser tests |
| 2 | `packages/agent-core/src/skill/scanner.ts:10` | Modify: add `.ody-code/microagents` to `PROJECT_BRAND_DIRS` |
| 2 | `packages/agent-core/test/skill/microagent-parser.test.ts` | Append: D1–D3 discovery tests |
| 3 | `packages/agent-core/src/skill/registry.ts:4,134+` | Modify: import `isKnowledgeSkillType`, add `listKnowledgeMicroagents()` |
| 3 | `packages/agent-core/test/skill/microagent-parser.test.ts` | Append: R1–R3 registry tests + A1 activation guard test |

## Dependency Overview

```
Task 1 (types + parser) ──┬──► Task 2 (scanner + discovery)
                          └──► Task 3 (registry + activation)
```

Tasks 2 and 3 are independent of each other and can run in parallel after Task 1.

## Risks & Open Questions

| # | Risk | Mitigation |
|---|------|------------|
| R1 | `pushExistingRoot` skips non-existent dirs silently; scanner tests create temp dirs without `.ody-code/microagents/` so the extra brand dir is never added | Verified: no existing scanner tests need modification. Confirmed by tracing `pushBrandGroup` → `pushExistingRoot` → `isDir` check. |
| R2 | `walkSkillDir` top-level flat `.md` scan (scanner.ts:180) also applies to `.ody-code/microagents/` root — `SKILL.md` in a microagent subdirectory would be treated as a directory skill | Design says top-level `.md` only for microagents. Test D2 creates only a flat `.md`; subdirectory behavior is harmless but not tested. |
| R3 | `UnsupportedSkillTypeError` message (parser.ts:36) lists "prompt, inline, flow" — after adding knowledge, the message is technically incomplete for truly unsupported types | Knowledge passes `isSupportedSkillType` so it's never rejected here. The message list is accurate for types that CAN be rejected. No change needed. |

---

### Task 1: Types extension, parseTriggers, and parser integration

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/skill/types.ts:11,71-75` — add `triggers`, `isKnowledgeSkillType`, update `isSupportedSkillType`
- Modify: `packages/agent-core/src/skill/parser.ts:135-138,244+` — integrate trigger validation, add `parseTriggers`
- Create: `packages/agent-core/test/skill/microagent-parser.test.ts` — type-helper tests + P1–P5 parser tests

- [ ] **Step 1: Write the failing test file**

Create `packages/agent-core/test/skill/microagent-parser.test.ts`:

```ts
import { describe, expect, it } from 'vitest';

import { parseSkillText } from '../../src/skill/parser';
import { isKnowledgeSkillType, isSupportedSkillType } from '../../src/skill/types';

// ---- Type helpers ----

describe('isKnowledgeSkillType', () => {
  it('returns true for "knowledge"', () => {
    expect(isKnowledgeSkillType('knowledge')).toBe(true);
  });

  it('returns false for undefined', () => {
    expect(isKnowledgeSkillType(undefined)).toBe(false);
  });

  it('returns false for "prompt"', () => {
    expect(isKnowledgeSkillType('prompt')).toBe(false);
  });

  it('returns false for "flow"', () => {
    expect(isKnowledgeSkillType('flow')).toBe(false);
  });
});

describe('isSupportedSkillType', () => {
  it('accepts knowledge alongside existing types', () => {
    expect(isSupportedSkillType('knowledge')).toBe(true);
    expect(isSupportedSkillType('prompt')).toBe(true);
    expect(isSupportedSkillType('flow')).toBe(true);
    expect(isSupportedSkillType(undefined)).toBe(true); // undefined → inline
  });

  it('rejects unknown types', () => {
    expect(isSupportedSkillType('garbage')).toBe(false);
  });
});

// ---- Parser: knowledge microagent validation ----

function skillText(lines: string[]): string {
  return lines.join('\n');
}

function parse(text: string): ReturnType<typeof parseSkillText> {
  return parseSkillText({
    text,
    skillMdPath: '/tmp/test.md',
    skillDirName: 'test',
    source: 'project',
  });
}

describe('parseSkillText with type: knowledge', () => {
  // P1: valid knowledge microagent
  it('parses a valid knowledge microagent with normalized triggers', () => {
    const skill = parse(skillText([
      '---',
      'type: knowledge',
      'triggers:',
      '  -  Page ',
      '  - PAGE',
      '  - component',
      '---',
      '这是知识内容。',
    ]));

    expect(skill.metadata.type).toBe('knowledge');
    expect(skill.metadata.triggers).toEqual(['component', 'page']);
    expect(skill.content).toBe('这是知识内容。');
  });

  // P2: triggers with mixed case, whitespace, and duplicates → normalized
  it('normalizes triggers: lowercased, trimmed, deduplicated, sorted', () => {
    const skill = parse(skillText([
      '---',
      'type: knowledge',
      'triggers:',
      '  -  Page ',
      '  - PAGE',
      '  - component',
      '---',
      'Body',
    ]));

    expect(skill.metadata.triggers).toEqual(['component', 'page']);
  });

  // P3: missing triggers rejected
  it('rejects knowledge microagent without triggers', () => {
    expect(() =>
      parse(skillText(['---', 'type: knowledge', '---', 'Body'])),
    ).toThrow(/triggers/);
  });

  // P4: empty trigger string rejected
  it('rejects knowledge microagent with an empty trigger string', () => {
    expect(() =>
      parse(skillText([
        '---',
        'type: knowledge',
        'triggers:',
        '  - ""',
        '---',
        'Body',
      ])),
    ).toThrow(/triggers/);
  });

  // P5: non-array triggers rejected
  it('rejects knowledge microagent with non-array triggers', () => {
    expect(() =>
      parse(skillText([
        '---',
        'type: knowledge',
        'triggers: not-an-array',
        '---',
        'Body',
      ])),
    ).toThrow(/triggers/);
  });
});
```

- [ ] **Step 2: Run tests, verify they FAIL**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/microagent-parser.test.ts
```

Expected failures:
- `isKnowledgeSkillType is not defined` (not exported from types.ts)
- `parseSkillText` with `type: knowledge` throws `UnsupportedSkillTypeError` because `isSupportedSkillType` rejects knowledge
- All P1–P5 tests fail

- [ ] **Step 3: Write the minimal implementation**

**3a. `packages/agent-core/src/skill/types.ts`** — add `triggers` field after line 11 (`arguments`):

```ts
  readonly triggers?: readonly string[] | undefined;
```

Full context (lines 3–14 after edit):
```ts
export interface SkillMetadata {
  readonly name?: string | undefined;
  readonly description?: string | undefined;
  readonly type?: string | undefined;
  readonly whenToUse?: string | undefined;
  readonly disableModelInvocation?: boolean | undefined;
  readonly hiddenInModes?: readonly string[] | undefined;
  readonly safe?: boolean | undefined;
  readonly arguments?: readonly unknown[] | string | undefined;
  readonly triggers?: readonly string[] | undefined;
  readonly [key: string]: unknown;
}
```

**3b. `packages/agent-core/src/skill/types.ts`** — add `isKnowledgeSkillType` after line 71 (`isUserActivatableSkillType`):

```ts
export function isKnowledgeSkillType(type: string | undefined): boolean {
  return type === 'knowledge';
}
```

**3c. `packages/agent-core/src/skill/types.ts`** — update `isSupportedSkillType` at lines 73–75:

Old:
```ts
export function isSupportedSkillType(type: string | undefined): boolean {
  return isUserActivatableSkillType(type);
}
```

New:
```ts
export function isSupportedSkillType(type: string | undefined): boolean {
  return isUserActivatableSkillType(type) || isKnowledgeSkillType(type);
}
```

**3d. `packages/agent-core/src/skill/parser.ts`** — add `parseTriggers` function. Insert after `normalizeMetadata` (after line 244, before `descriptionFromBody`):

```ts
function parseTriggers(raw: unknown): readonly string[] {
  if (!Array.isArray(raw) || raw.length === 0) {
    throw new SkillParseError(
      "microagent 'triggers' must be a non-empty array of strings",
    );
  }
  const seen = new Set<string>();
  const result: string[] = [];
  for (const item of raw) {
    if (typeof item !== 'string' || item.trim() === '') {
      throw new SkillParseError('each trigger must be a non-empty string');
    }
    const normalized = item.trim().toLowerCase();
    if (!seen.has(normalized)) {
      seen.add(normalized);
      result.push(normalized);
    }
  }
  return result.toSorted();
}
```

**3e. `packages/agent-core/src/skill/parser.ts`** — integrate trigger validation in `parseSkillText`. After line 138 (after the `UnsupportedSkillTypeError` throw), add:

```ts
  if (metadata.type === 'knowledge') {
    metadata.triggers = parseTriggers(metadata.triggers);
  }
```

The relevant section of `parseSkillText` (lines 135–148) becomes:

```ts
  const metadata = normalizeMetadata(frontmatter);
  if (!isSupportedSkillType(metadata.type)) {
    throw new UnsupportedSkillTypeError(metadata.type ?? String(frontmatter['type']));
  }

  if (metadata.type === 'knowledge') {
    metadata.triggers = parseTriggers(metadata.triggers);
  }

  const name = nonEmptyString(metadata.name);
  const description = nonEmptyString(metadata.description);
```

- [ ] **Step 4: Run tests, verify they PASS**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/microagent-parser.test.ts
```

Expected: all 10 tests pass (3 type-helper + 4 `isSupportedSkillType` + 5 parser = should be 12; recount: 4 `isKnowledgeSkillType` + 4 `isSupportedSkillType` + 5 parser = 13 tests pass).

- [ ] **Step 5: Run whole-tree typecheck to confirm no caller breakage**

```bash
pnpm -r typecheck
```

Expected: clean build. The `SkillMetadata` interface change adds an optional field — no existing callers write to `triggers`, so no compile errors. `isSupportedSkillType` now accepts `knowledge` — only callers are `parseSkillText` (which now handles it) and no other code gates on the supported-type set in a way that would break.

- [ ] **Step 6: Commit**

```bash
git add packages/agent-core/src/skill/types.ts packages/agent-core/src/skill/parser.ts packages/agent-core/test/skill/microagent-parser.test.ts
git commit -m "feat: add knowledge microagent type support, parseTriggers, and trigger validation"

---

### Task 2: Scanner — add `.ody-code/microagents` to project brand dirs

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/skill/scanner.ts:10` — update `PROJECT_BRAND_DIRS`
- Modify: `packages/agent-core/test/skill/microagent-parser.test.ts` — append D1–D3 discovery tests

- [ ] **Step 1: Write the failing discovery tests**

Append to `packages/agent-core/test/skill/microagent-parser.test.ts`:

```ts
// ---- Discovery tests ----

import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'pathe';
import { afterEach } from 'vitest';

import { discoverSkills, resolveSkillRoots } from '../../src/skill';

const microagentTempDirs: string[] = [];

afterEach(async () => {
  for (const dir of microagentTempDirs.splice(0)) {
    await rm(dir, { recursive: true, force: true });
  }
});

async function makeMicroagentWorkspace(): Promise<{
  homeDir: string; repoDir: string; workDir: string;
}> {
  const tmp = await mkdtemp(path.join(tmpdir(), 'kimi-microagent-'));
  microagentTempDirs.push(tmp);
  const homeDir = path.join(tmp, 'home');
  const repoDir = path.join(tmp, 'repo');
  const workDir = path.join(repoDir, 'packages', 'app');
  await mkdir(path.join(repoDir, '.git'), { recursive: true });
  await mkdir(workDir, { recursive: true });
  return { homeDir, repoDir, workDir };
}

describe('microagent discovery', () => {
  // D1: .ody-code/microagents/ root discovered
  it('discovers .ody-code/microagents as a project skill root', async () => {
    const { homeDir, repoDir, workDir } = await makeMicroagentWorkspace();
    const microagentsDir = path.join(repoDir, '.ody-code', 'microagents');
    await mkdir(microagentsDir, { recursive: true });

    const roots = await resolveSkillRoots({
      paths: { userHomeDir: homeDir, workDir },
    });

    const microagentRoot = roots.find(
      (r) => r.path.endsWith('.ody-code/microagents') && r.source === 'project',
    );
    expect(microagentRoot).toBeDefined();
  });

  // D2: microagents loaded via discoverSkills
  it('loads flat .md microagents from .ody-code/microagents', async () => {
    const { homeDir, repoDir, workDir } = await makeMicroagentWorkspace();
    const microagentsDir = path.join(repoDir, '.ody-code', 'microagents');
    await mkdir(microagentsDir, { recursive: true });
    await writeFile(
      path.join(microagentsDir, 'reuse-conventions.md'),
      [
        '---',
        'type: knowledge',
        'triggers:',
        '  - reuse',
        '  - conventions',
        '---',
        'Prefer existing utilities.',
      ].join('\n'),
    );

    const roots = await resolveSkillRoots({
      paths: { userHomeDir: homeDir, workDir },
    });
    const skills = await discoverSkills({ roots });

    const reuse = skills.find((s) => s.name === 'reuse-conventions');
    expect(reuse).toBeDefined();
    expect(reuse?.metadata.type).toBe('knowledge');
    expect(reuse?.metadata.triggers).toEqual(['conventions', 'reuse']);
    expect(reuse?.content).toBe('Prefer existing utilities.');
  });

  // D3: invalid microagent skipped with warning
  it('skips invalid microagent and calls onWarning', async () => {
    const { homeDir, repoDir, workDir } = await makeMicroagentWorkspace();
    const microagentsDir = path.join(repoDir, '.ody-code', 'microagents');
    await mkdir(microagentsDir, { recursive: true });
    await writeFile(
      path.join(microagentsDir, 'bad-triggers.md'),
      [
        '---',
        'type: knowledge',
        'triggers: not-an-array',
        '---',
        'Body',
      ].join('\n'),
    );

    const roots = await resolveSkillRoots({
      paths: { userHomeDir: homeDir, workDir },
    });
    const warnings: string[] = [];
    const skills = await discoverSkills({
      roots,
      onWarning: (msg) => warnings.push(msg),
    });

    expect(skills.find((s) => s.name === 'bad-triggers')).toBeUndefined();
    expect(warnings.length).toBeGreaterThanOrEqual(1);
    expect(warnings.some((w) => w.includes('bad-triggers'))).toBe(true);
  });
});
```

- [ ] **Step 2: Run tests, verify they FAIL**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/microagent-parser.test.ts
```

Expected: D1 fails — `resolveSkillRoots` does not return a root for `.ody-code/microagents` (the directory is not in `PROJECT_BRAND_DIRS`). D2 and D3 also fail because no microagent root is discovered.

- [ ] **Step 3: Update `PROJECT_BRAND_DIRS`**

In `packages/agent-core/src/skill/scanner.ts`, line 10, change:

```ts
const PROJECT_BRAND_DIRS = ['.ody-code/skills'] as const;
```

To:

```ts
const PROJECT_BRAND_DIRS = ['.ody-code/skills', '.ody-code/microagents'] as const;
```

- [ ] **Step 4: Run tests, verify they PASS**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/microagent-parser.test.ts
```

Expected: all tests (Task 1 + Task 2) pass — 13 Task 1 tests + 3 discovery tests = 16 tests pass.

- [ ] **Step 5: Run existing scanner tests to confirm no regression**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/scanner.test.ts
```

Expected: all existing scanner tests pass. The extra brand dir `.ody-code/microagents` does not exist in test temp dirs, so `pushExistingRoot` silently skips it.

- [ ] **Step 6: Run whole-tree typecheck**

```bash
pnpm -r typecheck
```

Expected: clean build.

- [ ] **Step 7: Commit**

```bash
git add packages/agent-core/src/skill/scanner.ts packages/agent-core/test/skill/microagent-parser.test.ts
git commit -m "feat: add .ody-code/microagents as a project skill root for knowledge microagents"

---

### Task 3: Registry — add `listKnowledgeMicroagents()` + activation guard verification

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/skill/registry.ts:4,134+` — import `isKnowledgeSkillType`, add `listKnowledgeMicroagents()`
- Modify: `packages/agent-core/test/skill/microagent-parser.test.ts` — append R1–R3 registry tests + A1 activation guard test

- [ ] **Step 1: Write the failing registry + activation guard tests**

Append to `packages/agent-core/test/skill/microagent-parser.test.ts`:

```ts
// ---- Registry tests ----

import { SkillRegistry } from '../../src/skill/registry';

describe('SkillRegistry with knowledge microagents', () => {
  // R1: invocable skills exclude knowledge
  it('excludes knowledge microagents from listInvocableSkills', () => {
    const registry = new SkillRegistry();
    registry.register(makeKnowledgeSkill('reuse'));
    registry.register(makePromptSkill('greet'));

    const invocable = registry.listInvocableSkills();
    expect(invocable.map((s) => s.name)).toEqual(['greet']);
    expect(invocable.map((s) => s.name)).not.toContain('reuse');
  });

  // R2: listKnowledgeMicroagents returns only knowledge skills
  it('listKnowledgeMicroagents returns only knowledge-typed skills', () => {
    const registry = new SkillRegistry();
    registry.register(makeKnowledgeSkill('reuse'));
    registry.register(makeKnowledgeSkill('conventions'));
    registry.register(makePromptSkill('greet'));

    const microagents = registry.listKnowledgeMicroagents();
    expect(microagents.map((s) => s.name).toSorted()).toEqual([
      'conventions',
      'reuse',
    ]);
  });

  // R3: skill wins over same-name microagent
  it('skill wins over same-name microagent via first-wins registration', () => {
    const registry = new SkillRegistry();
    // Register skill first (simulating load order: skills before microagents)
    registry.register(makePromptSkill('collision'));
    registry.register(makeKnowledgeSkill('collision'));

    const skill = registry.getSkill('collision');
    expect(skill?.metadata.type).toBe('prompt');
  });
});

// ---- Activation guard test ----

import { OdyError } from '#/errors';
import { SkillManager } from '../../src/agent/skill';
import type { Agent } from '../../src/agent';

describe('SkillManager activation guard', () => {
  // A1: knowledge microagent cannot be activated
  it('throws SKILL_TYPE_UNSUPPORTED when activating a knowledge microagent', () => {
    const registry = new SkillRegistry();
    registry.register(makeKnowledgeSkill('reuse'));

    // Minimal Agent stub — SkillManager.activate only uses registry.getSkill
    // and checks isUserActivatableSkillType; it does not access other Agent fields
    // for this error path.
    const agent = { emitEvent: () => {}, telemetry: { track: () => {} } } as unknown as Agent;
    const manager = new SkillManager(agent, registry);

    expect(() =>
      manager.activate({ name: 'reuse' } as any),
    ).toThrow(OdyError);

    try {
      manager.activate({ name: 'reuse' } as any);
    } catch (error) {
      expect(error).toBeInstanceOf(OdyError);
      expect((error as OdyError).code).toBe('SKILL_TYPE_UNSUPPORTED');
    }
  });

  it('can still activate a regular prompt skill', () => {
    const registry = new SkillRegistry();
    registry.register(makePromptSkill('greet'));

    // The activate method will call agent.turn.prompt() after registry.renderSkillPrompt.
    // We don't need a full Agent — we just verify the guard does NOT throw for prompt.
    const agent = {
      emitEvent: () => {},
      telemetry: { track: () => {} },
      turn: { prompt: () => {} },
    } as unknown as Agent;
    const manager = new SkillManager(agent, registry);

    expect(() =>
      manager.activate({ name: 'greet' } as any),
    ).not.toThrow();
  });
});

// ---- Test helpers for registry/activation tests ----

import type { SkillDefinition } from '../../src/skill/types';

function makeKnowledgeSkill(name: string): SkillDefinition {
  return {
    name,
    description: `Knowledge about ${name}`,
    path: `/tmp/${name}.md`,
    dir: '/tmp',
    content: `Content for ${name}`,
    metadata: { type: 'knowledge', triggers: ['test'] },
    source: 'project',
  };
}

function makePromptSkill(name: string): SkillDefinition {
  return {
    name,
    description: `Skill: ${name}`,
    path: `/tmp/${name}.md`,
    dir: '/tmp',
    content: `Content for ${name}`,
    metadata: { type: 'prompt' },
    source: 'project',
  };
}
```

- [ ] **Step 2: Run tests, verify they FAIL**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/microagent-parser.test.ts
```

Expected: R1, R2, R3 fail because `listKnowledgeMicroagents()` does not exist on `SkillRegistry`. A1 passes already (the existing `isUserActivatableSkillType` guard blocks knowledge) — but we verify it explicitly.

Note: if `listKnowledgeMicroagents` does not exist, the test file itself may fail to compile. Confirm that the import/usage causes a TypeScript/Vitest error before proceeding.

- [ ] **Step 3: Add `listKnowledgeMicroagents()` to `SkillRegistry`**

**3a. Import `isKnowledgeSkillType`** at `packages/agent-core/src/skill/registry.ts`, line 4:

Old:
```ts
import { isInlineSkillType, normalizeSkillName } from './types';
```

New:
```ts
import { isInlineSkillType, isKnowledgeSkillType, normalizeSkillName } from './types';
```

**3b. Add method after `listInvocableSkills`** (after line 134, `listInvocableSkills` closing brace):

```ts
  listKnowledgeMicroagents(): readonly SkillDefinition[] {
    return this.listSkills().filter((skill) => isKnowledgeSkillType(skill.metadata.type));
  }
```

- [ ] **Step 4: Run tests, verify they PASS**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/microagent-parser.test.ts
```

Expected: all tests pass — 13 (Task 1) + 3 (Task 2) + 3 (registry) + 2 (activation guard) = 21 tests pass.

- [ ] **Step 5: Run all skill tests to confirm no regression**

```bash
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/skill/
```

Expected: all skill tests pass — including `parser.test.ts`, `scanner.test.ts`, `registry.test.ts`, `builtin-skills.test.ts`, `simplicity-first.test.ts`, `parser-frontmatter.test.ts`.

- [ ] **Step 6: Run whole-tree typecheck**

```bash
pnpm -r typecheck
```

Expected: clean build.

- [ ] **Step 7: Commit**

```bash
git add packages/agent-core/src/skill/registry.ts packages/agent-core/test/skill/microagent-parser.test.ts
git commit -m "feat: add listKnowledgeMicroagents() to SkillRegistry with activation guard tests"

---

## Self-Review

- [ ] **1. Spec-coverage table**

| Design Spec Section | Requirement | Task(s) | Status |
|---|---|---|---|
| Scope In — `type: knowledge` as supported skill/metadata type | Extend `SkillMetadata`, add `isKnowledgeSkillType`, update `isSupportedSkillType` | Task 1 | covered |
| Scope In — `triggers` array parsing + validation | `parseTriggers` function: array check, non-empty, trim/lowercase/dedup/sort | Task 1 | covered |
| Scope In — `.ody-code/microagents/` as project-local skill root | Update `PROJECT_BRAND_DIRS` in scanner.ts | Task 2 | covered |
| Scope In — load into `SkillRegistry`, excluded from invocable listing | Existing `listInvocableSkills` filter excludes non-inline types; `listKnowledgeMicroagents()` added | Task 3 | covered |
| Scope In — project skills take precedence over same-name microagents | First-wins in `discoverSkills` via root order (skills before microagents in `PROJECT_BRAND_DIRS` array); R3 test | Task 2, Task 3 | covered |
| Scope In — unit tests (parsing, registry filtering, discovery order, invalid triggers) | P1–P5, D1–D3, R1–R3, A1 | Task 1, 2, 3 | covered |
| Scope Out — trigger matching | Deferred to A.2 | — | no-op |
| Scope Out — context injection | Deferred to A.2 | — | no-op |
| Scope Out — token caps / precedence rules | Deferred to A.3 | — | no-op |
| Scope Out — `/microagent` authoring | Deferred to A.4 | — | no-op |
| Scope Out — container sandbox, risk scoring | Deferred to T1-B/T1-D | — | no-op |
| Architecture — `resolveSkillRoots` adds `.ody-code/microagents` | `pushBrandGroup` iterates `PROJECT_BRAND_DIRS` in order | Task 2 | covered |
| Architecture — `discoverSkills` walks both roots | `walkSkillDir` invoked for each root; top-level `.md` parsed as knowledge | Task 2 | covered |
| Architecture — `parseSkillText` validates triggers for knowledge | Integration after `isSupportedSkillType` check | Task 1 | covered |
| Architecture — `listInvocableSkills` filters via `isInlineSkillType` | No code change; existing filter already excludes knowledge | Task 3 (A1 test) | covered |
| Architecture — `listKnowledgeMicroagents` filters via `isKnowledgeSkillType` | New method on `SkillRegistry` | Task 3 | covered |
| Error Handling — invalid triggers → `SkillParseError` caught by `parseAndRegister` | `parseTriggers` throws `SkillParseError`; `parseAndRegister` catch handles it | Task 1, D3 test | covered |
| Error Handling — name collision: skill wins | First-wins insertion order in `discoverSkills` byName map | Task 3 (R3 test) | covered |
| Activation guard — `SkillManager.activate` blocks knowledge | Existing `isUserActivatableSkillType` check unchanged | Task 3 (A1 test) | covered |

- [ ] **2. Placeholder scan**

Scanning all plan content for: `TODO`, `TBD`, "implement later", "add appropriate error handling", "write tests for the above" without code, "similar to Task N" (without the code), references to undefined types/functions, author deliberation.

Result: **CLEAN**. Every test has concrete code; every implementation step has exact code; no deferred-by-dependency excuses; no dead-code placeholders.

- [ ] **3. No phantom tasks**

- Task 1: produces `types.ts` changes, `parser.ts` changes, new test file — verifiable
- Task 2: produces `scanner.ts` change, new tests — verifiable
- Task 3: produces `registry.ts` change, new tests — verifiable

Zero `--allow-empty` commits. No task claims "already done in Task N".

- [ ] **4. Dependency soundness**

| Task | Depends on | Uses symbols from |
|------|-----------|-------------------|
| 1 | none | — |
| 2 | Task 1 | `parseTriggers` (indirectly via parser), `isKnowledgeSkillType` (indirectly), `SkillMetadata.triggers` |
| 3 | Task 1 | `isKnowledgeSkillType` (imported in registry.ts), `SkillMetadata.triggers` (in tests) |

Tasks 2 and 3 are independent of each other and can run in parallel after Task 1. No task references a symbol only a later task creates.

- [ ] **5. Caller & build soundness**

Shared-signature changes and their callers:

| Change | File | Callers checked |
|--------|------|-----------------|
| `SkillMetadata.triggers` (new optional field) | `types.ts:12` | No existing callers write to `triggers` — optional field is backward-compatible. Verified via `grep -rn "triggers" packages/` — only `triggers` in plan are new code. |
| `isSupportedSkillType` now returns true for `'knowledge'` | `types.ts:73-75` | Called in `parser.ts:136` (`parseSkillText`), handled by Task 1. Called in `scanner.ts:339-377` (`parseAndRegister` — catches `UnsupportedSkillTypeError`), knowledge now passes this gate so it won't be skipped. No other callers. Verified via `grep -rn "isSupportedSkillType" packages/`. |
| `isKnowledgeSkillType` (new export) | `types.ts:73-75` | Used in `registry.ts:4` (Task 3 import) and in tests. Automatically exported via barrel `index.ts:5` (`export * from './types'`). |
| `PROJECT_BRAND_DIRS` extended | `scanner.ts:10` | Consumed by `pushBrandGroup` at scanner.ts:78. Existing tests don't create `.ody-code/microagents` so `pushExistingRoot` skips it — no regression. |

Each shared-signature task ends with `pnpm -r typecheck` (whole-tree), not a single-package build.

End-to-end trace: a knowledge microagent file at `.ody-code/microagents/reuse.md` → `resolveSkillRoots` adds the root → `discoverSkills` walks it → `walkSkillDir` scans top-level `.md` → `parseSkillFromFile` → `parseSkillText` → `parseTriggers` normalizes triggers → `SkillRegistry.loadRoots` indexes by name → `listKnowledgeMicroagents()` returns it, `listInvocableSkills()` excludes it. All consumers consistent.

- [ ] **6. Test-the-risk**

Risk-tested behaviors:

| Risk | Test | Assertion |
|------|------|-----------|
| Knowledge parsed as regular skill → invocable | R1 | `listInvocableSkills()` does not contain knowledge skill |
| Missing triggers → silent skip | P3, D3 | `parseSkillText` throws; `discoverSkills` calls `onWarning` |
| Duplicate/whitespace triggers → wrong matcher input | P1, P2 | Output deep-equals `['component', 'page']` sorted |
| Non-array triggers → crash | P5 | Throws `SkillParseError` |
| Name collision → skill shadowed | R3 | Skill registered first wins |
| Activation of knowledge → unexpected injection | A1 | `SKILL_TYPE_UNSUPPORTED` thrown |

Must-survive input check for `parseTriggers` filter: input `[' Page ', 'PAGE', 'component']` → output `['component', 'page']`. The trim+lowercase+dedup+sort pipeline correctly preserves `component` and `page` (no substring collisions). Confirmed: no trigger value can be silently dropped because every non-empty string passes — the only rejection condition is empty/whitespace-only strings.

- [ ] **7. Type consistency**

| Type/symbol | Defined in | Used in |
|-------------|-----------|---------|
| `SkillMetadata.triggers` | Task 1 (types.ts:12) | Task 1 tests (P1–P5), Task 2 tests (D2), Task 3 tests (helpers) |
| `isKnowledgeSkillType` | Task 1 (types.ts) | Task 3 (registry.ts:4), Task 1 tests, Task 3 tests |
| `isSupportedSkillType` (updated) | Task 1 (types.ts) | Task 1 (parser.ts:136) |
| `parseTriggers` | Task 1 (parser.ts) | Task 1 (parser.ts, integration) |
| `listKnowledgeMicroagents` | Task 3 (registry.ts) | Task 3 tests (R1, R2) |
| `SkillRegistry` | Existing (registry.ts) | Task 3 tests (R1–R3, A1) |

All types, signatures, and property names match between definition and usage. No type defined in one task is used with a different name or shape in another task.

---
<!-- e2e-enriched -->

### Task 4: Generate and run E2E tests

Based on the changed files, validate the following tools:
- ExitPlanModeTool (priority: critical)

Use the RunE2ETests tool after completing the implementation tasks above.

