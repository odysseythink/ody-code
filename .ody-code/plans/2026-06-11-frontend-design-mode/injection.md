# Part 3: Injection System

> Scope: fix `PlanModeInjector` so it no longer misclassifies `frontend-design` as plan mode; create `FrontendDesignInjector` with its contract and skill-loading surface; register the injector in `InjectionManager`; add tests.
>
> Depends on: `2026-06-11-frontend-design-mode/core.md` (SessionModeKind includes `'frontend-design'`, context partition exists).

---

### Task 1: Fix `PlanModeInjector` plan-mode detection

**Depends on:** `2026-06-11-frontend-design-mode/core.md`: Task 1 (SessionModeKind includes `'frontend-design'`)

**Files:**
- **Modify:** `packages/agent-core/src/agent/injection/plan-mode.ts:33,37`
- **Test:** `packages/agent-core/test/agent/injection/plan-mode.test.ts` (add regression case)

The current code uses `kind !== 'design'` to detect "plan mode active", which incorrectly fires for `frontend-design` mode.

- [ ] Add a regression test in `packages/agent-core/test/agent/injection/plan-mode.test.ts` that asserts the injector stays silent when the session mode is `frontend-design`:

```typescript
it('does not inject when session mode is frontend-design', async () => {
  const agent = planAgent({ isActive: true, kind: 'frontend-design', sessionModeFilePath: '/tmp/fd.md' });
  // Patch the stub kind to frontend-design
  Object.defineProperty(agent.sessionMode, 'kind', { get: () => 'frontend-design' });
  const injector = new PlanModeInjector(agent);

  await injector.inject();

  expect(history(agent).length).toBe(0);
});
```

- [ ] Run the test and verify it FAILS (the old `kind !== 'design'` still triggers injection):

```bash
cd packages/agent-core && pnpm test -- test/agent/injection/plan-mode.test.ts
```

- [ ] Apply the fix in `packages/agent-core/src/agent/injection/plan-mode.ts`:

```typescript
override onContextClear(): void {
  super.onContextClear();
  this.wasActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'plan';
}

override async getInjection(): Promise<string | undefined> {
  const isPlanActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'plan';
  // ... rest unchanged
}
```

- [ ] Re-run the test and verify it PASSES.

- [ ] Commit: `git commit -am "fix(agent-core): PlanModeInjector only fires for kind === 'plan'"`

---

### Task 2: Extend `SkillRegistry` and `SessionMode` with minimal frontend-design stubs

**Depends on:** Task 1

**Files:**
- **Modify:** `packages/agent-core/src/skill/registry.ts:71-73,156`
- **Modify:** `packages/agent-core/src/agent/session-mode/index.ts` (add `getSelectedAppendices` stub)

These are lightweight forward-declarations so that `FrontendDesignInjector` (Task 3) can compile and run without waiting for Part 4 (skill system). The stubs return safe defaults; Part 4 will replace them with real behaviour.

- [ ] In `packages/agent-core/src/skill/registry.ts`:
  1. After `getSkill()`, add `getBuiltinSkill(name: string): SkillDefinition | undefined` that delegates to `getSkill(name)` (builtin skills are already indexed by name).
  2. After `getBuiltinSkill()`, add `getBuiltinSkillAppendix(_skillName: string, _appendixName: string): Promise<string>` that returns `Promise.resolve('')` (stub; Part 4 loads from disk).
  3. Expand `getUnavailableSkillsReminder(sessionMode: 'plan' | 'design')` → `getUnavailableSkillsReminder(sessionMode: 'plan' | 'design' | 'frontend-design')`.

```typescript
getBuiltinSkill(name: string): SkillDefinition | undefined {
  return this.getSkill(name);
}

async getBuiltinSkillAppendix(_skillName: string, _appendixName: string): Promise<string> {
  return '';
}
```

- [ ] In `packages/agent-core/src/agent/session-mode/index.ts`, add the stub after the `findUniqueStem` method (end of public API):

```typescript
getSelectedAppendices(): string[] {
  return [];
}
```

- [ ] Run whole-tree typecheck to confirm the signature expansions do not break existing callers:

```bash
pnpm -r typecheck
```

- [ ] Commit: `git commit -am "feat(agent-core): add SkillRegistry/SessionMode stubs for frontend-design skill loading"`

---

### Task 3: Create `frontend-design-mode-contract.ts`

**Depends on:** Task 2

**Files:**
- **Create:** `packages/agent-core/src/agent/injection/frontend-design-mode-contract.ts`

This contract file parallels `plan-mode-contract.ts` and `design-mode-contract.ts`. It composes reminder text from static fragments + dynamically loaded skill content.

- [ ] Create `packages/agent-core/src/agent/injection/frontend-design-mode-contract.ts`:

```typescript
import type { SessionModeFilePath } from '../session-mode';

const INTRO_ACTIVE = `Frontend-design mode is active. This is a frontend design and code generation session.
You are equipped with the frontend-design skill. Follow its methodology precisely:

1. BRIEF INFERENCE: Read the user's request and produce a one-line Design Read.
2. THREE DIALS: Set VARIANCE / MOTION / DENSITY based on the Design Read.
3. DESIGN SYSTEM MAP: Choose the right design system and stack.
4. CONFIRM WITH USER: Before generating code, confirm:
   - Is this a new project or existing project?
   - If existing, what is the current tech stack?
   - Where should the code files be placed?
5. APPENDIX SELECTION: Present available appendices with recommendations.
6. DESIGN DOCUMENT: Write DESIGN.md to .ody-code/frontend-designs/ following stitch.md format.
7. CODE GENERATION: Generate complete, runnable frontend code.
8. DEPENDENCY INSTALL: Run npm install / npx commands as needed.
9. DEV SERVER: Optionally run npm run dev for live preview.
10. PRE-FLIGHT CHECK: Run all 40+ checks before declaring done.

HARD RULES:
- Every component must be complete and runnable. No TODOs, no truncation.
- If token limit approaches, use PAUSED mechanism.
- Honor prefers-reduced-motion for all MOTION_INTENSITY > 3.
- Dark mode tokens must be defined and tested.
- Zero em-dashes anywhere on the page.`;

const SPARSE_QUALITY_POINTER = `Reminder: the design document must follow the stitch.md format; every component must be complete and runnable; honor prefers-reduced-motion; define and test dark mode tokens.`;

function withDesignFileFooter(body: string, designFilePath: SessionModeFilePath): string {
  if (designFilePath === null || designFilePath.length === 0) return body;
  return `${body}\n\nDesign file: ${designFilePath}`;
}

function appendSkillContent(body: string, skillContent: string, appendixContent: string): string {
  const parts: string[] = [body];
  if (skillContent.trim().length > 0) {
    parts.push(skillContent);
  }
  if (appendixContent.trim().length > 0) {
    parts.push(appendixContent);
  }
  return parts.join('\n\n---\n\n');
}

/** Full re-injection body (FrontendDesignInjector `full` variant). */
export function frontendDesignFullReminder(
  designFilePath: SessionModeFilePath,
  skillContent: string,
  appendixContent: string,
): string {
  const body = appendSkillContent(INTRO_ACTIVE, skillContent, appendixContent);
  return withDesignFileFooter(body, designFilePath);
}

/** Condensed reminder between full re-injections. */
export function frontendDesignSparseReminder(
  designFilePath: SessionModeFilePath,
  skillContent: string,
  appendixContent: string,
): string {
  const base = `Frontend-design mode still active (see full instructions earlier). You are equipped with the frontend-design skill. Follow its methodology: Brief Inference → Three Dials → Design System Map → Confirm with User → Appendix Selection → DESIGN.md → Code Generation → Dependency Install → Dev Server → Pre-flight Check.

${SPARSE_QUALITY_POINTER}`;
  const body = appendSkillContent(base, skillContent, appendixContent);
  return withDesignFileFooter(body, designFilePath);
}

/** Re-entry reminder when a design file from a previous session already exists. */
export function frontendDesignReentryReminder(
  designFilePath: SessionModeFilePath,
  skillContent: string,
  appendixContent: string,
): string {
  const base = `Frontend-design mode is active. This is a frontend design and code generation session.

## Re-entering Frontend-Design Mode
A design file from a previous session already exists.
  1. Read the existing design file to understand what was previously designed.
  2. Evaluate the user's current request against it. Same topic: update it. Different topic: replace it.
  3. Follow the methodology: Brief Inference → Three Dials → Design System Map → Confirm with User → Appendix Selection → DESIGN.md → Code Generation → Dependency Install → Dev Server → Pre-flight Check.
  4. Every component must be complete and runnable. No TODOs, no truncation.
  5. Run the pre-flight checklist before calling ExitFrontendDesignMode.

Your turn must end with either AskUserQuestion (to clarify) or ExitFrontendDesignMode (to request approval).`;
  const body = appendSkillContent(base, skillContent, appendixContent);
  return withDesignFileFooter(body, designFilePath);
}

/** Message shown the moment frontend-design mode is entered (EnterFrontendDesignModeTool). */
export function frontendDesignEntryMessage(designFilePath: SessionModeFilePath): string {
  const fileLine =
    designFilePath === null || designFilePath.length === 0
      ? 'No design file path is assigned yet. Invent your own filename under `.ody-code/frontend-designs/` (format: `YYYY-MM-DD-<topic>.md`). The host will normalize and deduplicate it on first write.'
      : `Design file: ${designFilePath}\nWrite the design to EXACTLY this path. Do NOT invent your own path, directory, or filename.`;

  return [
    'Frontend-design mode is now active. This is a frontend design and code generation session.',
    '',
    fileLine,
    '',
    'Follow the frontend-design skill methodology precisely.',
    '',
    INTRO_ACTIVE,
  ].join('\n');
}
```

- [ ] Run package typecheck:

```bash
cd packages/agent-core && pnpm typecheck
```

- [ ] Commit: `git add -A && git commit -m "feat(agent-core): add frontend-design-mode contract reminders"`

---

### Task 4: Create `FrontendDesignInjector`

**Depends on:** Task 3

**Files:**
- **Create:** `packages/agent-core/src/agent/injection/frontend-design-mode.ts`

- [ ] Create `packages/agent-core/src/agent/injection/frontend-design-mode.ts`:

```typescript
import type { SessionModeFilePath } from '../session-mode';
import { DynamicInjector } from './injector';
import {
  frontendDesignFullReminder,
  frontendDesignReentryReminder,
  frontendDesignSparseReminder,
} from './frontend-design-mode-contract';

const FRONTEND_DESIGN_DEDUP_MIN_TURNS = 2;
const FRONTEND_DESIGN_FULL_REFRESH_TURNS = 5;

export type FrontendDesignVariant = 'full' | 'sparse' | 'reentry';

export class FrontendDesignInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'frontend_design';
  private wasActive = false;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'frontend-design';
  }

  override async getInjection(): Promise<string | undefined> {
    const isActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'frontend-design';
    const { sessionModeFilePath } = this.agent.sessionMode;

    if (!isActive) {
      if (!this.wasActive) return undefined;
      this.wasActive = false;
      this.injectedAt = null;
      return exitReminder();
    }

    const skillContent = await this.loadSkillContent();
    const appendixContent = await this.loadSelectedAppendices();
    const skillsReminder = this.agent.skills?.registry.getUnavailableSkillsReminder('frontend-design') ?? '';

    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      const content = await this.currentDesignContent();
      if (content.trim().length > 0) {
        return appendSkillsReminder(
          frontendDesignReentryReminder(sessionModeFilePath, skillContent, appendixContent),
          skillsReminder,
        );
      }
    }

    const variant = this.getVariant();
    if (variant === null) return undefined;
    if (variant === 'reentry') {
      return appendSkillsReminder(
        frontendDesignReentryReminder(sessionModeFilePath, skillContent, appendixContent),
        skillsReminder,
      );
    }

    const body =
      variant === 'full'
        ? frontendDesignFullReminder(sessionModeFilePath, skillContent, appendixContent)
        : frontendDesignSparseReminder(sessionModeFilePath, skillContent, appendixContent);
    return appendSkillsReminder(body, skillsReminder);
  }

  protected getVariant(): FrontendDesignVariant | null {
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
    if (assistantTurnsSince >= FRONTEND_DESIGN_FULL_REFRESH_TURNS) return 'full';
    if (assistantTurnsSince >= FRONTEND_DESIGN_DEDUP_MIN_TURNS) return 'sparse';
    return null;
  }

  private async loadSkillContent(): Promise<string> {
    const skill = this.agent.skills?.registry.getBuiltinSkill('frontend-design');
    return skill?.content ?? '';
  }

  private async loadSelectedAppendices(): Promise<string> {
    const selected = this.agent.sessionMode.getSelectedAppendices();
    if (selected.length === 0) return '';
    const parts: string[] = [];
    for (const name of selected) {
      const content = await this.agent.skills?.registry.getBuiltinSkillAppendix('frontend-design', name);
      if (content) parts.push(`## Appendix: ${name}\n\n${content}`);
    }
    return parts.join('\n\n---\n\n');
  }

  private async currentDesignContent(): Promise<string> {
    try {
      const data = await this.agent.sessionMode.data();
      return data?.content ?? '';
    } catch {
      return '';
    }
  }
}

function exitReminder(): string {
  return `Frontend-design mode was cancelled — no design was approved. The frontend-design restrictions no longer apply. Continue with normal operation.`;
}

function appendSkillsReminder(body: string, reminder: string): string {
  return reminder.length > 0 ? `${body}\n\n${reminder}` : body;
}
```

- [ ] Run package typecheck:

```bash
cd packages/agent-core && pnpm typecheck
```

- [ ] Commit: `git add -A && git commit -m "feat(agent-core): add FrontendDesignInjector"`

---

### Task 5: Register `FrontendDesignInjector` in `InjectionManager`

**Depends on:** Task 4

**Files:**
- **Modify:** `packages/agent-core/src/agent/injection/manager.ts:1-27`
- **Test:** `packages/agent-core/test/agent/injection/manager.test.ts` (update injector count)

- [ ] Add the import and register the injector in `packages/agent-core/src/agent/injection/manager.ts`:

```typescript
import type { Agent } from '..';
import { flags } from '../../flags';
import { DesignModeInjector } from './design-mode';
import { FrontendDesignInjector } from './frontend-design-mode';
import { GoalInjector } from './goal';
import type { DynamicInjector } from './injector';
import { PermissionModeInjector } from './permission-mode';
import { PluginSessionStartInjector } from './plugin-session-start';
import { PlanModeInjector } from './plan-mode';
import { TodoListReminderInjector } from './todo-list';

export class InjectionManager {
  // ... existing fields ...

  constructor(protected readonly agent: Agent) {
    this.injectors = [
      new PluginSessionStartInjector(agent),
      new TodoListReminderInjector(agent),
      new PlanModeInjector(agent),
      new DesignModeInjector(agent),
      new FrontendDesignInjector(agent),
      new PermissionModeInjector(agent),
    ];
    // ... rest unchanged
  }
  // ...
}
```

- [ ] Update `packages/agent-core/test/agent/injection/manager.test.ts` if it asserts the number of injectors (search for any test that counts injectors or lists them).

- [ ] Run manager tests:

```bash
cd packages/agent-core && pnpm test -- test/agent/injection/manager.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck
```

- [ ] Commit: `git commit -am "feat(agent-core): register FrontendDesignInjector in InjectionManager"`

---

### Task 6: Add `FrontendDesignInjector` tests

**Depends on:** Task 5

**Files:**
- **Create:** `packages/agent-core/test/agent/injection/frontend-design-mode.test.ts`

Pattern the test after `packages/agent-core/test/agent/injection/design-mode.test.ts`.

- [ ] Create `packages/agent-core/test/agent/injection/frontend-design-mode.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../src/agent';
import { FrontendDesignInjector } from '../../../src/agent/injection/frontend-design-mode';
import { frontendDesignEntryMessage } from '../../../src/agent/injection/frontend-design-mode-contract';

interface FrontendDesignStub {
  isActive: boolean;
  sessionModeFilePath?: string | null;
  content?: string;
}

function frontendDesignAgent(stub: FrontendDesignStub): Agent {
  const history: unknown[] = [];
  return {
    type: 'main',
    skills: {
      registry: {
        getBuiltinSkill: () => undefined,
        getBuiltinSkillAppendix: async () => '',
        getUnavailableSkillsReminder: () => '',
      },
    },
    sessionMode: {
      get isActive() {
        return stub.isActive;
      },
      get kind() {
        return 'frontend-design';
      },
      get sessionModeFilePath() {
        return stub.sessionModeFilePath ?? null;
      },
      data: async () =>
        stub.content === undefined
          ? null
          : { id: 'fd1', content: stub.content, path: stub.sessionModeFilePath ?? '', kind: 'frontend-design' },
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

function history(agent: Agent): Array<{ role: string; content?: ReadonlyArray<{ text?: string }> }> {
  return agent.context.history as unknown as Array<{
    role: string;
    content?: ReadonlyArray<{ text?: string }>;
  }>;
}

function lastReminder(agent: Agent): string {
  const last = history(agent).findLast((message) => message.role === 'user');
  return last?.content?.map((part) => part.text ?? '').join('') ?? '';
}

describe('FrontendDesignInjector content', () => {
  it('injects the full reminder with the frontend-design contract and file footer', async () => {
    const agent = frontendDesignAgent({ isActive: true, sessionModeFilePath: '/tmp/fd.md' });
    const injector = new FrontendDesignInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('Frontend-design mode is active');
    expect(text).toContain('BRIEF INFERENCE');
    expect(text).toContain('THREE DIALS');
    expect(text).toContain('DESIGN SYSTEM MAP');
    expect(text).toContain('PRE-FLIGHT CHECK');
    expect(text).toContain('HARD RULES');
    expect(text).toContain('Design file: /tmp/fd.md');
  });

  it('tells the model to invent its own filename when path is null', () => {
    const entry = frontendDesignEntryMessage(null);
    expect(entry).toContain('Invent your own filename');
  });

  it('shows assigned path and "do not invent" when path is non-null', () => {
    const entry = frontendDesignEntryMessage('/workspace/.ody-code/frontend-designs/2026-06-10-my-topic.md');
    expect(entry).toContain('Design file:');
    expect(entry).toContain('Do NOT invent your own path');
  });

  it('keeps the entry message and the full reminder in sync (shared contract)', async () => {
    const agent = frontendDesignAgent({ isActive: true, sessionModeFilePath: '/tmp/fd.md' });
    const injector = new FrontendDesignInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = frontendDesignEntryMessage('/tmp/fd.md');

    for (const marker of [
      'BRIEF INFERENCE',
      'THREE DIALS',
      'DESIGN SYSTEM MAP',
      'PRE-FLIGHT CHECK',
      'HARD RULES',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
    expect(entry).toContain('Frontend-design mode is now active');
  });

  it('injects nothing when frontend-design mode is inactive', async () => {
    const agent = frontendDesignAgent({ isActive: false });
    const injector = new FrontendDesignInjector(agent);

    await injector.inject();

    expect(history(agent).length).toBe(0);
  });

  it('uses reentry variant when a design file already has content', async () => {
    const agent = frontendDesignAgent({
      isActive: true,
      sessionModeFilePath: '/tmp/fd.md',
      content: '# Existing Design',
    });
    const injector = new FrontendDesignInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('Re-entering Frontend-Design Mode');
  });

  it('injects exit reminder when mode deactivates after having been active', async () => {
    const agent = frontendDesignAgent({ isActive: true, sessionModeFilePath: '/tmp/fd.md' });
    const injector = new FrontendDesignInjector(agent);

    await injector.inject();
    expect(history(agent).length).toBe(1);

    Object.defineProperty(agent.sessionMode, 'isActive', { get: () => false });
    await injector.inject();

    expect(history(agent).length).toBe(2);
    const exitText = lastReminder(agent);
    expect(exitText).toContain('Frontend-design mode was cancelled');
  });
});
```

- [ ] Run the new test:

```bash
cd packages/agent-core && pnpm test -- test/agent/injection/frontend-design-mode.test.ts
```

- [ ] Run the full agent-core test suite:

```bash
cd packages/agent-core && pnpm test
```

- [ ] Commit: `git add -A && git commit -m "test(agent-core): add FrontendDesignInjector tests"`

---

## Local Self-Review

- [ ] **1. Spec-coverage table**

| Design Section | Requirement | Task | Status |
|---|---|---|---|
| 2.5 | Fix PlanModeInjector `kind !== 'design'` → `kind === 'plan'` | Task 1 | covered |
| 2.2 | Create `FrontendDesignInjector` class | Task 4 | covered |
| 2.2 | `FrontendDesignInjector` loads skill content via `getBuiltinSkill` | Task 4 | covered |
| 2.2 | `FrontendDesignInjector` loads selected appendices | Task 4 | covered |
| 2.2 | `FrontendDesignInjector` variant logic (full/sparse/reentry) | Task 4 | covered |
| 2.3 | `frontendDesignFullReminder` contract content | Task 3 | covered |
| 2.3 | `frontendDesignSparseReminder` contract content | Task 3 | covered |
| 2.3 | `frontendDesignReentryReminder` contract content | Task 3 | covered |
| 2.3 | `frontendDesignEntryMessage` entry message | Task 3 | covered |
| 2.4 | Register `FrontendDesignInjector` in `InjectionManager` | Task 5 | covered |
| — | `getUnavailableSkillsReminder` accepts `'frontend-design'` | Task 2 | covered |
| — | `SkillRegistry.getBuiltinSkill` stub delegates to `getSkill` | Task 2 | covered |
| — | `SkillRegistry.getBuiltinSkillAppendix` stub returns `''` | Task 2 | covered |
| — | `SessionMode.getSelectedAppendices` stub returns `[]` | Task 2 | covered |
| — | Tests for injector content, reentry, exit, and silence | Task 6 | covered |

- [ ] **2. Placeholder scan:** No TODO/TBD. The `getBuiltinSkillAppendix` and `getSelectedAppendices` stubs return safe defaults and are explicitly marked as expanded in Part 4; their signatures are real and compile-clean.

- [ ] **3. No phantom tasks:** Every task creates/modifies files and ends with a test run + commit.

- [ ] **4. Dependency soundness:** Task 1 depends on Part 1. Tasks 2–6 depend on earlier tasks in this part. No forward references.

- [ ] **5. Caller & build soundness:** Task 2 expands `getUnavailableSkillsReminder` signature and adds stubs. All existing callers (`plan-mode.ts`, `design-mode.ts`) pass literal strings that remain valid. Task 2 ends with `pnpm -r typecheck`. The `PlanModeInjector` fix in Task 1 is traced to the regression test that asserts silence when `kind === 'frontend-design'`.

- [ ] **6. Test-the-risk:** Task 1's regression test asserts that `PlanModeInjector` no longer fires for `frontend-design` (the key behavioural fix). Task 6 tests that `FrontendDesignInjector` injects the correct contract text, uses reentry when content exists, and stays silent when inactive.

- [ ] **7. Type consistency:** `SessionModeKind` from Part 1 is used (`'frontend-design'`). `getUnavailableSkillsReminder` parameter is expanded consistently. The `FrontendDesignInjector` uses the same `DynamicInjector` base class and lifecycle patterns as `PlanModeInjector`/`DesignModeInjector`.
