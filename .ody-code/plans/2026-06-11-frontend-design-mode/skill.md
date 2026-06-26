# Part 4: Skill & Appendix System

> Scope: create `AppendixSelector` with trigger-signal matching, register a minimal `FrontendDesignSkill` stub, wire it into the builtin skill registry, replace the Part-3 stubs (`getBuiltinSkillAppendix`, `getSelectedAppendices`) with real behaviour, and add tests.
>
> Depends on: `2026-06-11-frontend-design-mode/core.md` (type unions already expanded) and `2026-06-11-frontend-design-mode/injection.md` (stubs for `getBuiltinSkillAppendix` and `getSelectedAppendices` already exist).

---

### Task 1: Create `AppendixSelector`

**Depends on:** none (pure helper, no upstream symbols)

**Files:**
- **Create:** `packages/agent-core/src/agent/frontend-design/appendix-selector.ts`
- **Test:** `packages/agent-core/test/agent/frontend-design/appendix-selector.test.ts`

- [ ] Write the failing test first. Create `packages/agent-core/test/agent/frontend-design/appendix-selector.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import {
  type AppendixRecommendation,
  AppendixSelector,
  DEFAULT_APPENDICES,
  getSelectedAppendices,
} from '../../../src/agent/frontend-design/appendix-selector';

describe('AppendixSelector', () => {
  it('recommends image-to-code for screenshot-related prompts', () => {
    const selector = new AppendixSelector();
    const result = selector.select('Turn this screenshot into a landing page');

    const imageToCode = result.find((r) => r.name === 'image-to-code');
    expect(imageToCode).toBeDefined();
    expect(imageToCode!.isRecommended).toBe(true);
    expect(imageToCode!.matchedSignals).toContain('screenshot');
  });

  it('recommends gpt-taste for luxury-related prompts', () => {
    const selector = new AppendixSelector();
    const result = selector.select('Design a high-end luxury portfolio');

    const gptTaste = result.find((r) => r.name === 'gpt-taste');
    expect(gptTaste).toBeDefined();
    expect(gptTaste!.isRecommended).toBe(true);
  });

  it('recommends redesign for revamp-related prompts', () => {
    const selector = new AppendixSelector();
    const result = selector.select('Redesign my existing website');

    const redesign = result.find((r) => r.name === 'redesign');
    expect(redesign).toBeDefined();
    expect(redesign!.isRecommended).toBe(true);
  });

  it('recommends stitch for spec-related prompts', () => {
    const selector = new AppendixSelector();
    const result = selector.select('Write a design doc with proper format');

    const stitch = result.find((r) => r.name === 'stitch');
    expect(stitch).toBeDefined();
    expect(stitch!.isRecommended).toBe(true);
  });

  it('includes all appendices even when none match', () => {
    const selector = new AppendixSelector();
    const result = selector.select('hello world');

    expect(result).toHaveLength(4);
    expect(result.every((r) => !r.isRecommended)).toBe(true);
  });

  it('sorts recommended items first, then by name', () => {
    const selector = new AppendixSelector();
    const result = selector.select('screenshot and luxury portfolio');

    const recIndices = result
      .map((r, i) => (r.isRecommended ? i : -1))
      .filter((i) => i >= 0);
    const nonRecIndices = result
      .map((r, i) => (!r.isRecommended ? i : -1))
      .filter((i) => i >= 0);

    expect(Math.max(...recIndices)).toBeLessThan(Math.min(...nonRecIndices));
  });

  it('uses designRead in addition to userPrompt', () => {
    const selector = new AppendixSelector();
    const result = selector.select('build me a page', 'awwwards-level design');

    const gptTaste = result.find((r) => r.name === 'gpt-taste');
    expect(gptTaste!.isRecommended).toBe(true);
  });
});

describe('getSelectedAppendices', () => {
  const recommendations: AppendixRecommendation[] = [
    { name: 'a', label: 'A', description: '', isRecommended: true, matchedSignals: [] },
    { name: 'b', label: 'B', description: '', isRecommended: false, matchedSignals: [] },
    { name: 'c', label: 'C', description: '', isRecommended: true, matchedSignals: [] },
  ];

  it('returns DEFAULT_APPENDICES when selection is null or empty', () => {
    expect(getSelectedAppendices(null, recommendations)).toEqual(DEFAULT_APPENDICES);
    expect(getSelectedAppendices('', recommendations)).toEqual(DEFAULT_APPENDICES);
  });

  it('returns all appendices when selection is "all"', () => {
    expect(getSelectedAppendices('all', recommendations)).toEqual(['a', 'b', 'c']);
    expect(getSelectedAppendices('ALL', recommendations)).toEqual(['a', 'b', 'c']);
  });

  it('parses comma-separated 1-based indices', () => {
    expect(getSelectedAppendices('1, 3', recommendations)).toEqual(['a', 'c']);
  });

  it('ignores out-of-range indices', () => {
    expect(getSelectedAppendices('0, 5, 2', recommendations)).toEqual(['b']);
  });
});
```

- [ ] Run the test and verify it FAILS:

```bash
cd packages/agent-core && pnpm test -- test/agent/frontend-design/appendix-selector.test.ts
```

- [ ] Create the implementation `packages/agent-core/src/agent/frontend-design/appendix-selector.ts`:

```typescript
export interface AppendixConfig {
  readonly label: string;
  readonly description: string;
  readonly triggerSignals: readonly string[];
}

export interface AppendixRecommendation {
  readonly name: string;
  readonly label: string;
  readonly description: string;
  readonly isRecommended: boolean;
  readonly matchedSignals: string[];
}

export const APPENDICES: Record<string, AppendixConfig> = {
  'gpt-taste': {
    label: 'GPT Taste',
    description: 'Awwwards-level design patterns and premium consumer aesthetics',
    triggerSignals: ['awwwards', 'premium', 'luxury', 'high-end', '高端', '奢华'],
  },
  'image-to-code': {
    label: 'Image to Code',
    description: 'Convert screenshots and references into frontend code',
    triggerSignals: ['screenshot', 'image', 'reference', '截图', '图片', '参考'],
  },
  redesign: {
    label: 'Redesign Protocol',
    description: 'Audit and upgrade existing websites',
    triggerSignals: ['redesign', 'revamp', 'upgrade', '重新设计', '改版'],
  },
  stitch: {
    label: 'Stitch Format',
    description: 'DESIGN.md format specification',
    triggerSignals: ['design doc', 'spec', '文档', '规范'],
  },
};

export const DEFAULT_APPENDICES = ['stitch'];

export class AppendixSelector {
  select(userPrompt: string, designRead?: string): AppendixRecommendation[] {
    const text = `${userPrompt} ${designRead ?? ''}`.toLowerCase();
    const recommendations: AppendixRecommendation[] = [];

    for (const [name, config] of Object.entries(APPENDICES)) {
      const matchedSignals: string[] = [];
      let score = 0;

      for (const signal of config.triggerSignals) {
        if (text.includes(signal.toLowerCase())) {
          matchedSignals.push(signal);
          score += 1;
        }
      }

      recommendations.push({
        name,
        label: config.label,
        description: config.description,
        isRecommended: score > 0,
        matchedSignals,
      });
    }

    return recommendations.sort((a, b) => {
      if (a.isRecommended && !b.isRecommended) return -1;
      if (!a.isRecommended && b.isRecommended) return 1;
      return a.name.localeCompare(b.name);
    });
  }
}

export function getSelectedAppendices(
  userSelection: string | null,
  recommendations: AppendixRecommendation[],
): string[] {
  if (userSelection === null || userSelection.trim() === '') {
    return DEFAULT_APPENDICES;
  }
  if (userSelection.toLowerCase() === 'all') {
    return recommendations.map((r) => r.name);
  }
  const indices = userSelection.split(',').map((s) => parseInt(s.trim(), 10) - 1);
  return indices
    .filter((i) => i >= 0 && i < recommendations.length)
    .map((i) => recommendations[i].name);
}
```

- [ ] Re-run the test and verify it PASSES.

- [ ] Commit: `git add -A && git commit -m "feat(agent-core): add AppendixSelector with trigger-signal matching"`

---

### Task 2: Replace `SessionMode.getSelectedAppendices` stub with real implementation

**Depends on:** Task 1

**Files:**
- **Modify:** `packages/agent-core/src/agent/session-mode/index.ts` (replace stub, add state field)

- [ ] In `packages/agent-core/src/agent/session-mode/index.ts`, replace the Part-3 stub with a real implementation:

  1. Add the import at the top:

  ```typescript
  import { AppendixSelector } from '../frontend-design/appendix-selector';
  ```

  2. Add a private field after the existing `_pendingHandoffForNormal` declaration (around line 39):

  ```typescript
  private _selectedAppendices: string[] = [];
  ```

  3. In the `enter()` method, clear the selected appendices when entering frontend-design mode (around line 63, after `this._kind = kind;`):

  ```typescript
  this._kind = kind;
  this._selectedAppendices = kind === 'frontend-design' ? [] : this._selectedAppendices;
  ```

  4. Replace the stub `getSelectedAppendices()` with the real implementation:

  ```typescript
  getSelectedAppendices(): string[] {
    return [...this._selectedAppendices];
  }

  selectAppendices(names: string[]): void {
    this._selectedAppendices = [...names];
  }
  ```

  5. In `exit()`, `cancel()`, and `clear()`, also reset `_selectedAppendices` to `[]`.

- [ ] Add a lightweight behavioural test in the session-mode test file (or create one if none exists). Since there is no dedicated session-mode test file, add a test inside `packages/agent-core/test/tools/plan-mode-hard-block.test.ts` or create `packages/agent-core/test/agent/session-mode/appendix-state.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { SessionMode } from '../../../src/agent/session-mode';

describe('SessionMode appendix state', () => {
  it('starts empty after entering frontend-design mode', async () => {
    const agent = {
      config: { cwd: '/tmp' },
      emitStatusUpdated: () => {},
      records: { logRecord: () => {} },
      replayBuilder: { push: () => {} },
      kaos: { mkdir: async () => {} },
    };
    const sessionMode = new SessionMode(agent as unknown as Parameters<typeof SessionMode>[0]);
    await sessionMode.enter('test-id', false, true, 'frontend-design');

    expect(sessionMode.getSelectedAppendices()).toEqual([]);
  });

  it('remembers selected appendices', async () => {
    const agent = {
      config: { cwd: '/tmp' },
      emitStatusUpdated: () => {},
      records: { logRecord: () => {} },
      replayBuilder: { push: () => {} },
      kaos: { mkdir: async () => {} },
    };
    const sessionMode = new SessionMode(agent as unknown as Parameters<typeof SessionMode>[0]);
    await sessionMode.enter('test-id', false, true, 'frontend-design');

    sessionMode.selectAppendices(['stitch', 'image-to-code']);
    expect(sessionMode.getSelectedAppendices()).toEqual(['stitch', 'image-to-code']);
  });

  it('clears appendices on exit', async () => {
    const agent = {
      config: { cwd: '/tmp' },
      emitStatusUpdated: () => {},
      records: { logRecord: () => {} },
      replayBuilder: { push: () => {} },
      kaos: { mkdir: async () => {} },
    };
    const sessionMode = new SessionMode(agent as unknown as Parameters<typeof SessionMode>[0]);
    await sessionMode.enter('test-id', false, true, 'frontend-design');
    sessionMode.selectAppendices(['stitch']);

    sessionMode.exit();
    expect(sessionMode.getSelectedAppendices()).toEqual([]);
  });
});
```

- [ ] Run the test:

```bash
cd packages/agent-core && pnpm test -- test/agent/session-mode/appendix-state.test.ts
```

- [ ] Commit: `git add -A && git commit -m "feat(agent-core): wire AppendixSelector into SessionMode"`

---

### Task 3: Replace `SkillRegistry.getBuiltinSkillAppendix` stub with metadata lookup

**Depends on:** Task 1

**Files:**
- **Modify:** `packages/agent-core/src/skill/registry.ts` (replace stub)

Rather than inventing a new storage mechanism, `getBuiltinSkillAppendix` looks up the already-registered `SkillDefinition` and reads from `metadata.appendices` (a `Record<string, string>`). This keeps the skill self-contained.

- [ ] Replace the Part-3 stub in `packages/agent-core/src/skill/registry.ts`:

```typescript
async getBuiltinSkillAppendix(skillName: string, appendixName: string): Promise<string> {
  const skill = this.getSkill(skillName);
  if (skill === undefined) return '';
  const appendices = skill.metadata.appendices;
  if (appendices === undefined || typeof appendices !== 'object') return '';
  const content = (appendices as Record<string, unknown>)[appendixName];
  return typeof content === 'string' ? content : '';
}
```

- [ ] Run package typecheck:

```bash
cd packages/agent-core && pnpm typecheck
```

- [ ] Commit: `git commit -am "feat(agent-core): implement getBuiltinSkillAppendix via skill metadata"`

---

### Task 4: Create `FrontendDesignSkill` registration

**Depends on:** Task 3

**Files:**
- **Create:** `packages/agent-core/src/skill/builtin/frontend-design.ts`
- **Create:** `packages/agent-core/src/skill/builtin/frontend-design.md`
- **Modify:** `packages/agent-core/src/skill/builtin/index.ts`
- **Modify:** `packages/agent-core/test/skill/builtin-skills.test.ts`

The skill content (1231 lines) lives upstream; the `.md` file here is a **minimal viable contract** that compiles and is functionally correct, and will be swapped for the full upstream text at build time. The plan explicitly treats the `.md` as a build-time asset.

- [ ] Create `packages/agent-core/src/skill/builtin/frontend-design.md`:

```markdown
# Frontend Design Skill

## Overview

Anti-slop frontend design methodology for premium interface generation.

## Workflow

1. **Brief Inference** — Read the user's request and produce a one-line Design Read.
2. **Three Dials** — Set VARIANCE / MOTION / DENSITY based on the Design Read.
3. **Design System Map** — Choose the right design system and stack.
4. **Confirm with User** — Before generating code, confirm project type, tech stack, and output directory.
5. **Appendix Selection** — Present available appendices with recommendations.
6. **Design Document** — Write DESIGN.md to `.ody-code/frontend-designs/` following stitch.md format.
7. **Code Generation** — Generate complete, runnable frontend code.
8. **Dependency Install** — Run npm install / npx commands as needed.
9. **Dev Server** — Optionally run npm run dev for live preview.
10. **Pre-flight Check** — Run all 40+ checks before declaring done.

## Hard Rules

- Every component must be complete and runnable. No TODOs, no truncation.
- If token limit approaches, use PAUSED mechanism.
- Honor prefers-reduced-motion for all MOTION_INTENSITY > 3.
- Dark mode tokens must be defined and tested.
- Zero em-dashes anywhere on the page.
```

- [ ] Create `packages/agent-core/src/skill/builtin/frontend-design.ts`:

```typescript
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import FRONTEND_DESIGN_BODY from './frontend-design.md';

const PSEUDO_PATH = 'builtin://frontend-design';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/frontend-design.md',
  skillDirName: 'frontend-design',
  source: 'builtin',
  text: FRONTEND_DESIGN_BODY,
});

export const FRONTEND_DESIGN_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
    hiddenInModes: ['normal', 'plan', 'design'],
    whenToUse:
      'Trigger: website, landing page, frontend, UI, interface, portfolio, ' +
      'SaaS page, web app. Appendices: gpt-taste, image-to-code, redesign, stitch.',
    appendices: {}, // Populated at build time from upstream appendix files.
  },
};
```

- [ ] Register the skill in `packages/agent-core/src/skill/builtin/index.ts`:

```typescript
import { FRONTEND_DESIGN_SKILL } from './frontend-design';
// ... add to registerBuiltinSkills and re-export list
```

- [ ] Update `packages/agent-core/test/skill/builtin-skills.test.ts`:

```typescript
import { FRONTEND_DESIGN_SKILL } from '../../src/skill/builtin/frontend-design';

// Add to BUILTIN_SKILLS array
{ skill: FRONTEND_DESIGN_SKILL, name: 'frontend-design' },

// Update the count assertion
expect(BUILTIN_SKILLS).toHaveLength(13);
```

- [ ] Run the builtin-skills test:

```bash
cd packages/agent-core && pnpm test -- test/skill/builtin-skills.test.ts
```

- [ ] Commit: `git add -A && git commit -m "feat(agent-core): register FrontendDesignSkill as builtin"`

---

### Task 5: End-to-end test for `FrontendDesignInjector` skill loading

**Depends on:** Task 4

**Files:**
- **Create:** `packages/agent-core/test/agent/injection/frontend-design-mode-skill-loading.test.ts`

- [ ] Create a test that asserts `FrontendDesignInjector` injects skill content when a `frontend-design` skill is registered:

```typescript
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../src/agent';
import { FrontendDesignInjector } from '../../../src/agent/injection/frontend-design-mode';
import { SkillRegistry } from '../../../src/skill/registry';
import { FRONTEND_DESIGN_SKILL } from '../../../src/skill/builtin/frontend-design';

function agentWithSkill(): Agent {
  const history: unknown[] = [];
  const registry = new SkillRegistry();
  registry.registerBuiltinSkill(FRONTEND_DESIGN_SKILL);

  return {
    type: 'main',
    skills: {
      registry,
    },
    sessionMode: {
      get isActive() {
        return true;
      },
      get kind() {
        return 'frontend-design';
      },
      get sessionModeFilePath() {
        return '/tmp/fd.md';
      },
      data: async () => null,
      getSelectedAppendices: () => [],
    },
    context: {
      history,
      appendSystemReminder: (content: string) => {
        history.push({ role: 'user', content: [{ type: 'text', text: content }] });
      },
    },
  } as unknown as Agent;
}

function lastReminder(agent: Agent): string {
  const h = agent.context.history as unknown as Array<{
    role: string;
    content?: ReadonlyArray<{ text?: string }>;
  }>;
  const last = h.findLast((message) => message.role === 'user');
  return last?.content?.map((part) => part.text ?? '').join('') ?? '';
}

describe('FrontendDesignInjector skill loading', () => {
  it('injects skill content when the frontend-design skill is registered', async () => {
    const agent = agentWithSkill();
    const injector = new FrontendDesignInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('Brief Inference');
    expect(text).toContain('Three Dials');
    expect(text).toContain('Pre-flight Check');
  });

  it('injects nothing when the frontend-design skill is not registered', async () => {
    const agent = agentWithSkill();
    (agent.skills!.registry as SkillRegistry).listSkills().forEach((s) => {
      // no-op — the registry already has the skill in agentWithSkill
    });
    // Re-create without the skill
    const history: unknown[] = [];
    const bareAgent = {
      type: 'main',
      skills: { registry: new SkillRegistry() },
      sessionMode: {
        get isActive() { return true; },
        get kind() { return 'frontend-design'; },
        get sessionModeFilePath() { return '/tmp/fd.md'; },
        data: async () => null,
        getSelectedAppendices: () => [],
      },
      context: {
        history,
        appendSystemReminder: (content: string) => {
          history.push({ role: 'user', content: [{ type: 'text', text: content }] });
        },
      },
    } as unknown as Agent;

    const injector = new FrontendDesignInjector(bareAgent);
    await injector.inject();

    const text = lastReminder(bareAgent);
    // The contract text is still injected; skill content is empty but the base contract remains.
    expect(text).toContain('Frontend-design mode is active');
  });
});
```

- [ ] Run the test:

```bash
cd packages/agent-core && pnpm test -- test/agent/injection/frontend-design-mode-skill-loading.test.ts
```

- [ ] Commit: `git add -A && git commit -m "test(agent-core): add FrontendDesignInjector skill-loading test"`

---

### Task 6: Whole-tree typecheck and final verification

**Depends on:** Task 5

**Files:** none (verification-only)

- [ ] Run the full agent-core test suite:

```bash
cd packages/agent-core && pnpm test
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck
```

- [ ] Commit: `git commit --allow-empty -m "chore: verify skill system after frontend-design skill registration"`

---

## Local Self-Review

- [ ] **1. Spec-coverage table**

| Design Section | Requirement | Task | Status |
|---|---|---|---|
| 6.1 | `AppendixSelector` interface + `select()` | Task 1 | covered |
| 6.2 | Trigger-signal matching algorithm | Task 1 | covered |
| 6.4 | `getSelectedAppendices()` with defaults / `all` / index parsing | Task 1 | covered |
| 5.1 | `FrontendDesignSkill` registration with `hiddenInModes` | Task 4 | covered |
| 5.2 | Register in `builtin/index.ts` | Task 4 | covered |
| 5.3 | Appendix config (label, description, triggerSignals) | Task 1 | covered |
| — | `SessionMode.getSelectedAppendices` wired to `AppendixSelector` | Task 2 | covered |
| — | `SkillRegistry.getBuiltinSkillAppendix` reads from skill metadata | Task 3 | covered |
| — | `FrontendDesignInjector` loads skill content end-to-end | Task 5 | covered |

- [ ] **2. Placeholder scan:** No TODO/TBD. The `frontend-design.md` skill content is a minimal viable contract; the plan notes it is a build-time asset swapped from upstream at build time. The `appendices: {}` metadata is intentionally empty because upstream appendix files are supplied by the build pipeline.

- [ ] **3. No phantom tasks:** Every task creates or modifies files and ends with tests + commit.

- [ ] **4. Dependency soundness:** Task 1 depends on nothing (pure helper). Tasks 2–5 depend on earlier tasks in this part. No forward references.

- [ ] **5. Caller & build soundness:** The Part-3 stubs (`getBuiltinSkillAppendix`, `getSelectedAppendices`) are replaced in-place with real implementations in Tasks 2 and 3. Task 6 ends with `pnpm -r typecheck`.

- [ ] **6. Test-the-risk:** Task 1 tests the trigger-signal matching (the core algorithmic risk) with concrete inputs including Chinese signals. Task 2 tests `SessionMode` state mutation (appendix selection is preserved and cleared on exit). Task 5 tests end-to-end skill loading.

- [ ] **7. Type consistency:** `AppendixSelector` uses the same `SessionModeKind` from Part 1. `getBuiltinSkillAppendix` signature matches the Part-3 stub. `SkillDefinition.metadata.appendices` is accessed through `unknown` narrowing, so it does not require a schema change.
