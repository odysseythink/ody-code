# Part 1: agent-core — SessionMode + StateStore

## Task 1: Extend SessionModeKind, ModeKey, context partitions, directory resolution, and write guard

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:1-771`
- Modify: `packages/agent-core/src/agent/index.ts:80,204-221,238`
- Modify: `packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts:16-19,24,71-72`
- Modify: `packages/agent-core/src/agent/records/types.ts:41-46`
- Modify: `packages/agent-core/src/rpc/core-api.ts:168`
- Test: `packages/agent-core/test/agent/session-mode.test.ts`

### Step 1: Write the failing test

Add to `packages/agent-core/test/agent/session-mode.test.ts` after existing imports:

```ts
it('resolves game-design directory to .ody-code/game-design/', async () => {
  const agent = makeAgent();
  const sm = new SessionMode(agent);
  await sm.enter('id-gd', undefined, false, 'game-design');
  expect(agent.kaos.mkdir).toHaveBeenCalledWith(
    join(CWD, '.ody-code', 'game-design'),
    { parents: true, existOk: true },
  );
});

it('game-design mode is writable by isWritableSessionModePath', async () => {
  const gdFile = join(CWD, '.ody-code', 'game-design', '2026-06-18-topic.md');
  const existing = new Set([gdFile]);
  const agent = makeAgent({ existingPaths: existing });
  const sm = new SessionMode(agent);
  // directly set state for testing isWritableSessionModePath
  (sm as any)._sessionModeFilePath = gdFile;
  (sm as any)._sessionModeKind = 'game-design';
  expect(sm.isWritableSessionModePath(gdFile)).toBe(true);
  // allow split subdirectory
  const splitFile = join(CWD, '.ody-code', 'game-design', '2026-06-18-topic', 'appendix.md');
  expect(sm.isWritableSessionModePath(splitFile)).toBe(true);
  // deny unrelated paths
  expect(sm.isWritableSessionModePath(join(CWD, 'src', 'index.ts'))).toBe(false);
  // deny other game-design files with different stem
  expect(sm.isWritableSessionModePath(
    join(CWD, '.ody-code', 'game-design', 'other.md'),
  )).toBe(false);
});

it('PlanModeGuardDeny allows game-design mode writes', async () => {
  const gdFile = join(CWD, '.ody-code', 'game-design', '2026-06-18-topic.md');
  const agent = makeAgent();
  const sm = new SessionMode(agent);
  (sm as any)._sessionModeFilePath = gdFile;
  (sm as any)._sessionModeKind = 'game-design';
  (sm as any)._isActive = true;

  const policy = new PlanModeGuardDenyPermissionPolicy(agent as any);
  // Write to the main file should be allowed (returns undefined = not denied)
  const result = policy.evaluate({
    toolCall: { name: 'Write' } as any,
    args: { file_path: gdFile, content: 'hello' },
    rawArgs: '{}',
  });
  expect(result).toBeUndefined();
});
```

Add import at top of test file:
```ts
import { PlanModeGuardDenyPermissionPolicy } from '../../src/agent/permission/policies/plan-mode-guard-deny';
```

### Step 2: Run test and verify FAIL

```bash
pnpm --filter @odysseythink/agent-core vitest run test/agent/session-mode.test.ts 2>&1 | tail -20
```

Expected: TypeScript compilation fails because `'game-design'` is not assignable to `SessionModeKind`.

### Step 3: Write the minimal implementation

**3a. `packages/agent-core/src/agent/session-mode/index.ts`:**

Line 1 — change:
```ts
export type SessionModeKind = 'plan' | 'design' | 'office-hours' | 'game-design';
```

Line 672 — change `resolveSessionModeDirectory`:
```ts
private async resolveSessionModeDirectory(kind: SessionModeKind): Promise<{ dir: string; isProjectScoped: boolean }> {
  const subdir =
    kind === 'office-hours' ? 'office-hours' :
    kind === 'game-design' ? 'game-design' :
    kind === 'design' ? 'designs' :
    'plans';
  const projectDir = join(this.agent.config.cwd, '.ody-code', subdir);
  try {
    await this.agent.kaos.mkdir(projectDir, { parents: true, existOk: true });
    return { dir: projectDir, isProjectScoped: true };
  } catch (error) {
    if (isPermissionError(error) && this.agent.homedir !== undefined) {
      const sessionDir = join(this.agent.homedir, subdir);
      await this.agent.kaos.mkdir(sessionDir, { parents: true, existOk: true });
      return { dir: sessionDir, isProjectScoped: false };
    }
    throw error;
  }
}
```

**3b. `packages/agent-core/src/agent/index.ts`:**

Line 80 — change:
```ts
export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';
```

Lines 204-221 — add `'game-design'` entries:
```ts
this._contexts = {
  normal: new ContextMemory(this),
  plan: new ContextMemory(this),
  design: new ContextMemory(this),
  'office-hours': new ContextMemory(this),
  'game-design': new ContextMemory(this),
} as Record<ModeKey, ContextMemory>;
this._fullCompactions = {
  normal: new FullCompaction(this, options.compactionStrategy),
  plan: new FullCompaction(this, options.compactionStrategy),
  design: new FullCompaction(this, options.compactionStrategy),
  'office-hours': new FullCompaction(this, options.compactionStrategy),
  'game-design': new FullCompaction(this, options.compactionStrategy),
} as Record<ModeKey, FullCompaction>;
this._microCompactions = {
  normal: new MicroCompaction(this, options.microCompaction),
  plan: new MicroCompaction(this, options.microCompaction),
  design: new MicroCompaction(this, options.microCompaction),
  'office-hours': new MicroCompaction(this, options.microCompaction),
  'game-design': new MicroCompaction(this, options.microCompaction),
} as Record<ModeKey, MicroCompaction>;
```

Line 238 — add gameDesignStateStore initialization (after officeHoursStateStore):
```ts
this.officeHoursStateStore = options.officeHoursStateStore ?? new NoopOfficeHoursStateStore();
this.gameDesignStateStore = options.gameDesignStateStore ?? new NoopGameDesignStateStore();
```

Also add to imports near top of agent/index.ts (after office-hours imports):
```ts
import { NoopGameDesignStateStore } from '../office-hours/state';
```
And the `GameDesignStateStore` import will come in Task 2, but Task 1 needs a placeholder. Since `NoopGameDesignStateStore` won't exist yet, use `NoopGameDesignStateStore` in Task 1 as `undefined` and let Task 2 add the real class. Specifically, in Task 1, use:

```ts
this.gameDesignStateStore = options.gameDesignStateStore ?? (null as any);
```

Then Task 2 replaces `(null as any)` with `new NoopGameDesignStateStore()`.

**3c. `packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts`:**

Line 16-19 — add game-design alongside office-hours:
```ts
const kind = this.agent.sessionMode.kind;
const isOfficeHours = kind === 'office-hours';
const isGameDesign = kind === 'game-design';
const isDesign = kind === 'design';
const modeLabel = isOfficeHours ? 'office-hours' : isGameDesign ? 'game-design' : isDesign ? 'design' : 'plan';
```

Line 19 — change exitTool:
```ts
const exitTool = isOfficeHours
  ? 'ExitOfficeHoursMode'
  : isGameDesign
    ? 'ExitGameDesignMode'
    : isDesign
      ? 'ExitDesignMode'
      : 'ExitPlanMode';
```

Line 71-72 — change `modeWriteDeniedMessage`:
```ts
function modeWriteDeniedMessage(modeLabel: string, sessionModeFilePath: string | null): string {
  const Mode = modeLabel === 'game-design'
    ? 'Game-design'
    : modeLabel.charAt(0).toUpperCase() + modeLabel.slice(1);
  const exitTool = modeLabel === 'office-hours' ? 'ExitOfficeHoursMode'
    : modeLabel === 'game-design' ? 'ExitGameDesignMode'
    : modeLabel === 'design' ? 'ExitDesignMode'
    : 'ExitPlanMode';
```

**3d. `packages/agent-core/src/agent/records/types.ts`** — no code change needed. The `SessionModeKind` type is imported, and the record field `kind?: SessionModeKind` already accepts any valid kind. No change required.

**3e. `packages/agent-core/src/rpc/core-api.ts`** — no code change needed. `EnterPlanPayload.kind?: SessionModeKind` already accepts any valid kind.

**3f. `packages/agent-core/src/rpc/resumed.ts`** — no code change needed. `SessionModeKind` is re-exported and used in `AgentReplayRecord.kind?: SessionModeKind`.

**3g. Search for and update ALL callers that exhaustively check `SessionModeKind`:**

```bash
grep -rn "kind === 'office-hours'" packages/agent-core/src/
grep -rn "kind === 'plan'" packages/agent-core/src/agent/session-mode/
```

In `session-mode/index.ts`, look for `kind === 'plan' || kind === 'design'` style checks. The `enter()` method at approximately line 96 has:

```ts
if (kind === 'plan' || kind === 'design') {
  // switch to modeModels[kind] model
}
```

No change needed — game-design doesn't switch models, same as office-hours.

The `data()` method returns `SessionModeData | null` with `kind: SessionModeKind`. No change needed.

### Step 4: Run test and verify PASS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/agent/session-mode.test.ts 2>&1 | tail -20
```

Expected: All tests pass including the new game-design tests.

### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck 2>&1 | tail -40
```

Expected: No type errors. This verifies all callers of `SessionModeKind` and `ModeKey` across the entire workspace.

### Step 6: Commit

```bash
git add -A && git commit -m "feat: extend SessionModeKind and ModeKey with game-design, add directory resolution and write guard"
```

---

## Task 2: GameDesignStateStore interface + FileSystem implementation + Agent wiring

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/office-hours/state.ts` (add types + interface + classes)
- Modify: `packages/agent-core/src/agent/index.ts:238` (replace placeholder with real store)
- Modify: `packages/agent-core/src/agent/index.ts` (add AgentOptions.gameDesignStateStore)
- Test: `packages/agent-core/test/tools/builtin/game-design/state-store.test.ts` (create)

### Step 1: Write the failing test

Create `packages/agent-core/test/tools/builtin/game-design/state-store.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest';
import { join } from 'pathe';
import {
  FileSystemGameDesignStateStore,
  NoopGameDesignStateStore,
} from '../../../src/office-hours/state';

describe('GameDesignStateStore', () => {
  const mockKaos = () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeText: vi.fn().mockResolvedValue(undefined),
    readText: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
  });

  describe('FileSystemGameDesignStateStore', () => {
    it('stores to .ody-code/game-design/ in the project directory', async () => {
      const kaos = mockKaos();
      const store = new FileSystemGameDesignStateStore(kaos as any, '/fake/project');
      await store.appendProfile({
        date: '2026-06-18',
        mode: 'startup',
        projectSlug: 'test-game',
        pillars: 'Explore, Build, Survive',
        audience: 'Casual',
        platform: 'Mobile',
        genre: 'Adventure',
        signals: [],
        designDoc: 'game-design.md',
      });
      expect(kaos.mkdir).toHaveBeenCalledWith(
        join('/fake/project', '.ody-code', 'game-design'),
        { parents: true, existOk: true },
      );
      expect(kaos.writeText).toHaveBeenCalledWith(
        expect.stringContaining('.ody-code/game-design/builder-profile.jsonl'),
        expect.stringContaining('"projectSlug":"test-game"'),
        { mode: 'a' },
      );
    });

    it('searchLearnings returns most recent entries first within limit', async () => {
      const entries = [
        { ts: '2026-01-01', skill: 'game-design', type: 'operational' as const, key: 'k1', insight: 'a', confidence: 0.5, source: 'observed' as const },
        { ts: '2026-01-02', skill: 'game-design', type: 'eureka' as const, key: 'k2', insight: 'b', confidence: 0.8, source: 'observed' as const },
        { ts: '2026-01-03', skill: 'game-design', type: 'operational' as const, key: 'k3', insight: 'c', confidence: 0.6, source: 'observed' as const },
      ];
      const kaos = {
        ...mockKaos(),
        readText: vi.fn().mockResolvedValue(entries.map(e => JSON.stringify(e)).join('\n')),
      };
      const store = new FileSystemGameDesignStateStore(kaos as any, '/fake/project');
      const result = await store.searchLearnings({ limit: 2 });
      expect(result).toHaveLength(2);
      expect(result[0].key).toBe('k3');
      expect(result[1].key).toBe('k2');
    });

    it('searchLearnings filters by branch when provided', async () => {
      const entries = [
        { ts: '2026-01-01', skill: 'game-design', type: 'operational' as const, key: 'k1', insight: 'a', confidence: 0.5, source: 'observed' as const, branch: 'main' },
        { ts: '2026-01-02', skill: 'game-design', type: 'eureka' as const, key: 'k2', insight: 'b', confidence: 0.8, source: 'observed' as const, branch: 'feature/x' },
        { ts: '2026-01-03', skill: 'game-design', type: 'operational' as const, key: 'k3', insight: 'c', confidence: 0.6, source: 'observed' as const, branch: 'main' },
      ];
      const kaos = {
        ...mockKaos(),
        readText: vi.fn().mockResolvedValue(entries.map(e => JSON.stringify(e)).join('\n')),
      };
      const store = new FileSystemGameDesignStateStore(kaos as any, '/fake/project');
      const result = await store.searchLearnings({ limit: 10, branch: 'main' });
      expect(result).toHaveLength(2);
      expect(result.every(e => e.branch === 'main')).toBe(true);
    });
  });

  describe('NoopGameDesignStateStore', () => {
    it('all methods are no-ops returning empty/zero values', async () => {
      const store = new NoopGameDesignStateStore();
      await expect(store.appendProfile({} as any)).resolves.toBeUndefined();
      await expect(store.readProfile()).resolves.toEqual([]);
      await expect(store.searchLearnings({ limit: 5 })).resolves.toEqual([]);
      const summary = await store.getSessionSummary();
      expect(summary).toEqual({ sessionCount: 0, tier: 'introduction' });
    });
  });
});
```

### Step 2: Run test and verify FAIL

```bash
pnpm --filter @odysseythink/agent-core vitest run test/tools/builtin/game-design/state-store.test.ts 2>&1 | tail -20
```

Expected: Test fails because `FileSystemGameDesignStateStore` and `NoopGameDesignStateStore` are not exported.

### Step 3: Write the minimal implementation

Append to `packages/agent-core/src/office-hours/state.ts` (after line 185):

```ts
// ── Game-Design State Store ──────────────────────────────────────────────

export interface GameDesignProfileEntry {
  readonly date: string;
  readonly mode: 'startup' | 'builder';
  readonly projectSlug: string;
  readonly pillars: string;
  readonly audience: string;
  readonly platform: string;
  readonly genre: string;
  readonly signals: readonly string[];
  readonly designDoc: string;
}

export type GameDesignTier = 'introduction' | 'welcome_back' | 'regular' | 'inner_circle';

export interface GameDesignLearningEntry {
  readonly ts: string;
  readonly skill: 'game-design';
  readonly type: 'operational' | 'eureka';
  readonly key: string;
  readonly insight: string;
  readonly confidence: number;
  readonly source: 'observed';
  readonly branch?: string;
}

export interface GameDesignStateStore {
  appendProfile(entry: GameDesignProfileEntry): Promise<void>;
  readProfile(): Promise<readonly GameDesignProfileEntry[]>;
  appendLearning(entry: GameDesignLearningEntry): Promise<void>;
  searchLearnings(options: {
    limit: number;
    branch?: string;
  }): Promise<readonly GameDesignLearningEntry[]>;
  getSessionSummary(): Promise<{ sessionCount: number; tier: GameDesignTier }>;
}

export class FileSystemGameDesignStateStore implements GameDesignStateStore {
  private readonly baseDir: string;

  constructor(
    private readonly kaos: Kaos,
    projectDir: string,
  ) {
    this.baseDir = join(projectDir, '.ody-code', 'game-design');
  }

  private profilePath(): string {
    return join(this.baseDir, 'builder-profile.jsonl');
  }

  private learningsPath(): string {
    return join(this.baseDir, 'learnings.jsonl');
  }

  private async ensureDir(): Promise<void> {
    await this.kaos.mkdir(this.baseDir, { parents: true, existOk: true });
  }

  async appendProfile(entry: GameDesignProfileEntry): Promise<void> {
    await this.ensureDir();
    await this.kaos.writeText(this.profilePath(), JSON.stringify(entry) + '\n', {
      mode: 'a',
    });
  }

  async readProfile(): Promise<readonly GameDesignProfileEntry[]> {
    try {
      const text = await this.kaos.readText(this.profilePath());
      return text
        .split('\n')
        .filter((line) => line.trim().length > 0)
        .map((line) => JSON.parse(line) as GameDesignProfileEntry);
    } catch {
      return [];
    }
  }

  async appendLearning(entry: GameDesignLearningEntry): Promise<void> {
    await this.ensureDir();
    await this.kaos.writeText(this.learningsPath(), JSON.stringify(entry) + '\n', {
      mode: 'a',
    });
  }

  async searchLearnings(options: {
    limit: number;
    branch?: string;
  }): Promise<readonly GameDesignLearningEntry[]> {
    try {
      const text = await this.kaos.readText(this.learningsPath());
      let entries = text
        .split('\n')
        .filter((line) => line.trim().length > 0)
        .map((line) => JSON.parse(line) as GameDesignLearningEntry);
      if (options.branch !== undefined) {
        entries = entries.filter((e) => e.branch === options.branch);
      }
      return entries.slice(-options.limit).reverse();
    } catch {
      return [];
    }
  }

  async getSessionSummary(): Promise<{ sessionCount: number; tier: GameDesignTier }> {
    const entries = await this.readProfile();
    const sessionCount = entries.length;
    if (sessionCount === 0) return { sessionCount, tier: 'introduction' };
    if (sessionCount <= 3) return { sessionCount, tier: 'welcome_back' };
    if (sessionCount <= 7) return { sessionCount, tier: 'regular' };
    return { sessionCount, tier: 'inner_circle' };
  }
}

export class NoopGameDesignStateStore implements GameDesignStateStore {
  async appendProfile(_entry: GameDesignProfileEntry): Promise<void> {}
  async readProfile(): Promise<readonly GameDesignProfileEntry[]> {
    return [];
  }
  async appendLearning(_entry: GameDesignLearningEntry): Promise<void> {}
  async searchLearnings(_options: {
    limit: number;
    branch?: string;
  }): Promise<readonly GameDesignLearningEntry[]> {
    return [];
  }
  async getSessionSummary(): Promise<{ sessionCount: number; tier: GameDesignTier }> {
    return { sessionCount: 0, tier: 'introduction' };
  }
}
```

Now wire into Agent. In `packages/agent-core/src/agent/index.ts`:

Add to `AgentOptions` interface (near the `officeHoursStateStore` line):
```ts
readonly gameDesignStateStore?: GameDesignStateStore;
```

Add property declaration near `officeHoursStateStore`:
```ts
readonly gameDesignStateStore!: GameDesignStateStore;
```

Replace the Task 1 placeholder on line 238:
```ts
// Before (Task 1 placeholder):
this.gameDesignStateStore = options.gameDesignStateStore ?? (null as any);
// After:
this.gameDesignStateStore = options.gameDesignStateStore ?? new NoopGameDesignStateStore();
```

Add imports at top of `agent/index.ts`:
```ts
import { NoopGameDesignStateStore, type GameDesignStateStore } from '../office-hours/state';
```

### Step 4: Run tests and verify PASS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/tools/builtin/game-design/state-store.test.ts 2>&1 | tail -15
pnpm --filter @odysseythink/agent-core vitest run test/agent/session-mode.test.ts 2>&1 | tail -15
```

Expected: All state store tests pass. Session mode tests still pass.

### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck 2>&1 | tail -20
```

Expected: No type errors.

### Step 6: Commit

```bash
git add -A && git commit -m "feat: add GameDesignStateStore with FileSystem and Noop implementations"
```

---

## Self-Review (Part 1)

- [ ] 1. Spec-coverage: Task 1 covers SessionModeKind + ModeKey extension, directory resolution, write guard. Task 2 covers state store interface + persistence. Both map to design In Scope items 2, 8, 15.
- [ ] 2. Placeholder scan: Task 1 uses `(null as any)` placeholder that Task 2 replaces. No TODO/TBD.
- [ ] 3. No phantom tasks: Both tasks produce verifiable changes (new union member, new class, new tests).
- [ ] 4. Dependency soundness: Task 2 depends on Task 1 (needs ModeKey extended, placeholder in Agent). Nothing references symbols from later tasks.
- [ ] 5. Caller & build soundness: Task 1 changes `SessionModeKind` (shared signature) — grep confirms 4 files import it; all are either the definition site or use `kind?: SessionModeKind` (which needs no change). Ends with whole-tree typecheck. `ModeKey` changes in agent/index.ts only. `PlanModeGuardDeny` updated.
- [ ] 6. Test-the-risk: Task 1 tests directory resolution to `.ody-code/game-design/`, write guard allow/deny behavior, guard policy integration. Task 2 tests append, search (with branch filter), noop defaults. All state mutations are asserted behaviorally.
- [ ] 7. Type consistency: `GameDesignStateStore` interface used in Task 4 tools matches: `appendProfile(GameDesignProfileEntry)`, `appendLearning(GameDesignLearningEntry)`, `searchLearnings({limit, branch?})`, `getSessionSummary()`.
