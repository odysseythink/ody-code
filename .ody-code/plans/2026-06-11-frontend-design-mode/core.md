# Part A: Core Types & Models

## Phase Dependency Overview

All tasks in this part are sequential (each builds on the previous).

```
Task 1 (shared-signature type expansion)
  → Task 2 (Agent context partitions)
  → Task 3 (resolveSessionModeDirectory)
  → Task 4 (handoffTo)
  → Task 5 (modeModels schema)
```

---

### Task 1: Extend SessionModeKind, ModeKey, and all session-mode type literals

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:21`
- Modify: `packages/agent-core/src/agent/index.ts:74`
- Modify: `apps/ody-code/src/tui/types.ts:20`
- Modify: `apps/ody-code/src/tui/commands/types.ts:4`
- Modify: `packages/agent-core/src/rpc/events.ts:50`
- Modify: `packages/agent-core/src/skill/registry.ts:112,143,156`
- Modify: `packages/agent-core/src/skill/types.ts:57,58`
- Modify: `packages/agent-core/src/session/index.ts:348`
- Modify: `packages/agent-core/src/session/rpc.ts:91`
- Modify: `packages/agent-core/src/rpc/core-api.ts:383`
- Modify: `packages/agent-core/src/profile/types.ts:45`
- Test: whole-workspace typecheck

This is a **shared-signature change**. Every literal union type that enumerates session modes must be expanded in the SAME task, and the task ends with a whole-tree `pnpm -r typecheck`.

- [ ] Apply the following edits. For each file, add `'frontend-design'` to the existing union (preserve order: normal, plan, design, frontend-design).

  ```typescript
  // packages/agent-core/src/agent/session-mode/index.ts:21
  export type SessionModeKind = 'plan' | 'design' | 'frontend-design';
  ```

  ```typescript
  // packages/agent-core/src/agent/index.ts:74
  export type ModeKey = 'normal' | 'plan' | 'design' | 'frontend-design';
  ```

  ```typescript
  // apps/ody-code/src/tui/types.ts:20
  sessionMode: 'normal' | 'plan' | 'design' | 'frontend-design';
  ```

  ```typescript
  // apps/ody-code/src/tui/commands/types.ts:4
  export type SessionMode = 'normal' | 'plan' | 'design' | 'frontend-design';
  ```

  ```typescript
  // packages/agent-core/src/rpc/events.ts:50
  readonly sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design' | undefined;
  ```

  ```typescript
  // packages/agent-core/src/skill/registry.ts:112
  listInvocableSkills(
    sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design',
  ): readonly SkillDefinition[]
  ```

  ```typescript
  // packages/agent-core/src/skill/registry.ts:143
  getModelSkillListing(sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design'): string {
  ```

  ```typescript
  // packages/agent-core/src/skill/registry.ts:156
  getUnavailableSkillsReminder(sessionMode: 'plan' | 'design' | 'frontend-design'): string {
  ```

  ```typescript
  // packages/agent-core/src/skill/types.ts:57-58
  listInvocableSkills(sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design'): readonly SkillDefinition[];
  getModelSkillListing(sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design'): string;
  ```

  ```typescript
  // packages/agent-core/src/session/index.ts:348
  async listSkills(options?: { sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design' }): Promise<readonly SkillSummary[]> {
  ```

  ```typescript
  // packages/agent-core/src/session/rpc.ts:91
  listSkills(payload: EmptyPayload & { sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design' }): Promise<readonly SkillSummary[]> {
  ```

  ```typescript
  // packages/agent-core/src/rpc/core-api.ts:383
  listSkills: (payload: EmptyPayload & { sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design' }) => readonly SkillSummary[];
  ```

  ```typescript
  // packages/agent-core/src/profile/types.ts:45
  readonly sessionMode?: 'normal' | 'plan' | 'design' | 'frontend-design';
  ```

- [ ] Run a grep to verify no literal union was missed (excluding reviewer/document-kind types which are intentionally unchanged):

  ```bash
  rg "'normal' \| 'plan' \| 'design'" packages/ apps/ --type ts --type tsx -n
  ```

  Expected: any remaining hits should be in `reviewer.ts` (document kind), `rpc/core-api.ts:317` (`ReviewDesignPayload.kind`), or `agent/index.ts:502` (reviewDesign local variable) — these are NOT session-mode unions and must NOT be changed.

- [ ] Run whole-tree typecheck:

  ```bash
  pnpm -r typecheck
  ```

  Expected: zero errors across all packages.

- [ ] Commit:

  ```bash
  git add -A && git commit -m "feat: extend SessionModeKind and ModeKey with frontend-design"
  ```

---

### Task 2: Add frontend-design context partition to Agent

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/agent/index.ts:181-195`
- Test: `packages/agent-core/test/agent/session-mode.test.ts` (update mock)

- [ ] Update the Agent constructor to instantiate a fourth partition for `'frontend-design'`:

  ```typescript
  // packages/agent-core/src/agent/index.ts:181-195
  this._contexts = {
    normal: new ContextMemory(this),
    plan: new ContextMemory(this),
    design: new ContextMemory(this),
    'frontend-design': new ContextMemory(this),
  };
  this._fullCompactions = {
    normal: new FullCompaction(this, options.compactionStrategy),
    plan: new FullCompaction(this, options.compactionStrategy),
    design: new FullCompaction(this, options.compactionStrategy),
    'frontend-design': new FullCompaction(this, options.compactionStrategy),
  };
  this._microCompactions = {
    normal: new MicroCompaction(this, options.microCompaction),
    plan: new MicroCompaction(this, options.microCompaction),
    design: new MicroCompaction(this, options.microCompaction),
    'frontend-design': new MicroCompaction(this, options.microCompaction),
  };
  ```

- [ ] Update the test mock in `packages/agent-core/test/agent/session-mode.test.ts:23`:

  ```typescript
  contexts: {
    normal: { history: [] },
    plan: { history: [] },
    design: { history: [] },
    'frontend-design': { history: [] },
  },
  ```

- [ ] Verify typecheck still passes:

  ```bash
  pnpm -r typecheck
  ```

- [ ] Commit:

  ```bash
  git add -A && git commit -m "feat: add frontend-design context partition to Agent"
  ```

---

### Task 3: Extend resolveSessionModeDirectory for frontend-design

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:515-528`
- Test: `packages/agent-core/test/agent/session-mode.test.ts`

- [ ] Refactor `resolveSessionModeDirectory` from a ternary expression to a mapping table:

  ```typescript
  // packages/agent-core/src/agent/session-mode/index.ts:515-528
  private async resolveSessionModeDirectory(kind: SessionModeKind): Promise<{ dir: string; isProjectScoped: boolean }> {
    const MODE_DIR_MAP: Record<SessionModeKind, string> = {
      plan: 'plans',
      design: 'designs',
      'frontend-design': 'frontend-designs',
    };
    const subDir = MODE_DIR_MAP[kind];
    const projectDir = join(this.agent.config.cwd, '.ody-code', subDir);
    try {
      await this.agent.kaos.mkdir(projectDir, { parents: true, existOk: true });
      return { dir: projectDir, isProjectScoped: true };
    } catch (error) {
      if (isPermissionError(error) && this.agent.homedir !== undefined) {
        const sessionDir = join(this.agent.homedir, subDir);
        await this.agent.kaos.mkdir(sessionDir, { parents: true, existOk: true });
        return { dir: sessionDir, isProjectScoped: false };
      }
      throw error;
    }
  }
  ```

- [ ] Add a test that verifies the directory resolution for `frontend-design`:

  ```typescript
  // Append to packages/agent-core/test/agent/session-mode.test.ts
  describe('resolveSessionModeDirectory', () => {
    it('resolves frontend-design mode to frontend-designs directory', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'frontend-design');
      // Trigger path resolution which calls resolveSessionModeDirectory internally
      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/frontend-designs/my-feature.md',
        '# My Feature\n\nContent',
      );
      expect(path).toMatch(/\.ody-code\/frontend-designs\//);
      expect(path).toMatch(/my-feature\.md$/);
    });
  });
  ```

- [ ] Run the specific test:

  ```bash
  pnpm test -- packages/agent-core/test/agent/session-mode.test.ts
  ```

  Expected: all tests pass, including the new one.

- [ ] Commit:

  ```bash
  git add -A && git commit -m "feat: resolve frontend-design mode to frontend-designs directory"
  ```

---

### Task 4: Extend handoffTo for frontend-design mode

**Depends on:** Task 3

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:38-39,248-290`
- Test: `packages/agent-core/test/agent/session-mode.test.ts`

- [ ] Add the pending-handoff field and consumer for frontend-design:

  ```typescript
  // packages/agent-core/src/agent/session-mode/index.ts:38-39
  // After _pendingHandoffForNormal:
  private _pendingHandoffForFrontendDesign: { content: string; path: string } | null = null;
  ```

  ```typescript
  // packages/agent-core/src/agent/session-mode/index.ts:248-259
  // After consumePendingHandoffForNormal:
  /** Consume and return the pending design→frontend-design handoff artifact (if any). */
  consumePendingHandoffForFrontendDesign(): { content: string; path: string } | null {
    const p = this._pendingHandoffForFrontendDesign;
    this._pendingHandoffForFrontendDesign = null;
    return p;
  }
  ```

- [ ] Update `handoffTo` signature and add the new branch:

  ```typescript
  // packages/agent-core/src/agent/session-mode/index.ts:270
  async handoffTo(target: 'plan' | 'normal' | 'frontend-design'): Promise<void> {
    const data = await this.data();
    const artifact =
      data !== null && data.content.trim().length > 0
        ? { content: data.content, path: data.path }
        : null;

    if (target === 'plan') {
      this._pendingHandoffForPlan = artifact;
      this.exit();
      try {
        await this.enter(this.createSessionModeId(), false, true, 'plan');
      } catch (error) {
        this._pendingHandoffForPlan = null;
        throw error;
      }
    } else if (target === 'frontend-design') {
      this._pendingHandoffForFrontendDesign = artifact;
      this.exit();
      try {
        await this.enter(this.createSessionModeId(), false, true, 'frontend-design');
      } catch (error) {
        this._pendingHandoffForFrontendDesign = null;
        throw error;
      }
    } else {
      this._pendingHandoffForNormal = artifact;
      this.exit();
    }
  }
  ```

- [ ] Add tests for the new handoff path:

  ```typescript
  // Append inside describe('handoffTo', ...) in packages/agent-core/test/agent/session-mode.test.ts
  it('handoffTo("frontend-design") exits design, enters frontend-design, stores artifact', async () => {
    const agent = makeAgent();
    vi.mocked(agent.kaos.readText).mockResolvedValue('# My Design\n\nSome content');
    const sm = new SessionMode(agent);
    await sm.enter('design-id', undefined, false, 'design');
    await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '# My Design\nSome content');

    vi.mocked(agent.records.logRecord).mockClear();

    await sm.handoffTo('frontend-design');

    expect(sm.isActive).toBe(true);
    expect(sm.kind).toBe('frontend-design');

    const handoff = sm.consumePendingHandoffForFrontendDesign();
    expect(handoff).not.toBeNull();
    expect(handoff?.content).toBe('# My Design\n\nSome content');
    expect(handoff?.path).toMatch(/my-feature\.md$/);

    expect(sm.consumePendingHandoffForFrontendDesign()).toBeNull();
  });

  it('handoffTo("frontend-design") stores null artifact when source file is empty', async () => {
    const agent = makeAgent();
    vi.mocked(agent.kaos.readText).mockResolvedValue('');
    const sm = new SessionMode(agent);
    await sm.enter('design-id', undefined, false, 'design');
    await sm.resolveFilePathFromModelRequest('.ody-code/designs/empty.md', '');

    await sm.handoffTo('frontend-design');

    expect(sm.kind).toBe('frontend-design');
    expect(sm.consumePendingHandoffForFrontendDesign()).toBeNull();
  });
  ```

- [ ] Run the tests:

  ```bash
  pnpm test -- packages/agent-core/test/agent/session-mode.test.ts
  ```

  Expected: all tests pass.

- [ ] Commit:

  ```bash
  git add -A && git commit -m "feat: support handoffTo frontend-design mode with artifact carry"
  ```

---

### Task 5: Extend modeModels config schema

**Depends on:** Task 4

**Files:**
- Modify: `packages/agent-core/src/config/schema.ts:215-218,258-261`

- [ ] Add `'frontend-design'` to both the full schema and the patch schema:

  ```typescript
  // packages/agent-core/src/config/schema.ts:215-218
  modeModels: z.object({
    plan: z.string().optional(),
    design: z.string().optional(),
    review: z.string().optional(),
    'frontend-design': z.string().optional(),
  }).optional(),
  ```

  ```typescript
  // packages/agent-core/src/config/schema.ts:258-261
  modeModels: z.object({
    plan: z.string().optional(),
    design: z.string().optional(),
    review: z.string().optional(),
    'frontend-design': z.string().optional(),
  }).optional(),
  ```

- [ ] Verify the schema accepts the new key with a runtime test. Since there is no dedicated config-schema test file, verify via typecheck and a quick Node validation:

  ```bash
  node -e "
    const { KimiConfigSchema } = require('./packages/agent-core/dist/config/schema.js');
    const result = KimiConfigSchema.safeParse({ modeModels: { 'frontend-design': 'kimi/k1' } });
    console.log(result.success ? 'PASS' : 'FAIL', result.error?.issues);
  "
  ```

  Note: if the dist file does not exist yet, run `pnpm -r build` first, or simply rely on the typecheck. The typecheck is the authoritative verification.

- [ ] Run whole-tree typecheck:

  ```bash
  pnpm -r typecheck
  ```

  Expected: zero errors.

- [ ] Commit:

  ```bash
  git add -A && git commit -m "feat: add frontend-design key to modeModels config schema"
  ```

---

## Local Self-Review

- [ ] 1. Spec-coverage table: map every spec section/requirement → Task(s), marked covered / GAP / no-op.
  | Spec Section | Task(s) | Status |
  |---|---|---|
  | SessionModeKind 扩展为 `'plan' \| 'design' \| 'frontend-design'` | Task 1 | covered |
  | ModeKey / Agent context partitions 扩展 | Task 1, Task 2 | covered |
  | AppState.sessionMode / TUI SessionMode 扩展 | Task 1 | covered |
  | resolveSessionModeDirectory 映射表 + frontend-designs 目录 | Task 3 | covered |
  | handoffTo 支持 frontend-design target | Task 4 | covered |
  | modeModels 配置支持 frontend-design key | Task 5 | covered |
  | isWritableSessionModePath 行为（DESIGN.md 可写）| no-op — 现有 `.md` 逻辑已满足 |

- [ ] 2. Placeholder scan: no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders. **Verified.**

- [ ] 3. No phantom tasks: every task produces a verifiable change; zero `--allow-empty`. **Verified.**

- [ ] 4. Dependency soundness: every `Depends on:` is satisfied by an earlier task. **Verified: 1 → 2 → 3 → 4 → 5.**

- [ ] 5. Caller & build soundness: Task 1 changed the shared `SessionModeKind` / `ModeKey` signatures and updated EVERY literal-union caller (skill registry, session RPC, core-api, profile types, TUI types, events) plus test mocks; ended with whole-tree `pnpm -r typecheck`. The same signature was not changed across multiple tasks. **Verified.**

- [ ] 6. Test-the-risk: Task 3 has a behavioral test asserting `frontend-design` resolves to `frontend-designs/` directory; Task 4 has behavioral tests asserting `handoffTo('frontend-design')` mutates `kind` to `'frontend-design'` and populates/consumes `_pendingHandoffForFrontendDesign`. These assert state mutations, not just compilation. **Verified.**

- [ ] 7. Type consistency: `SessionModeKind = 'plan' | 'design' | 'frontend-design'` is used consistently; `ModeKey = 'normal' | 'plan' | 'design' | 'frontend-design'` is used consistently. No mismatched property names. **Verified.**
