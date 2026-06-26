# Fix: executing-plans skill visible in TUI slash commands despite hiddenInModes — Implementation Plan

**Goal:** Make `executing-plans` skill invisible from TUI slash command autocomplete in plan/design modes by adding `sessionMode` filtering to `Session.listSkills()`.

**Architecture:** Add optional `sessionMode` parameter to `Session.listSkills()`, propagating through RPC → node-sdk → TUI. The TUI's `refreshSkillCommands()` passes the current mode so server-side `listInvocableSkills()` filtering applies. The existing `resolveSlashCommandInput` defense-in-depth (skill not in map → plain message) catches manual input without code changes.

**Tech Stack:** TypeScript, pnpm monorepo, vitest

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| # | Path | Action |
|---|---|---|
| 1 | `packages/agent-core/src/session/index.ts:348-351` | Modify: add `ListSkillsOptions` + `sessionMode` param |
| 2 | `packages/agent-core/src/session/rpc.ts:91-93` | Modify: accept `sessionMode` from RPC payload |
| 3 | `packages/node-sdk/src/session.ts:225-228` | Modify: update `listSkills` type signature |
| 4 | `packages/node-sdk/src/rpc.ts:425-428` | Modify: pass `sessionMode` through RPC client |
| 5 | `packages/agent-core/test/harness/skill-session.test.ts` | Modify: add 4 tests for mode-filtered listSkills |
| 6 | `apps/ody-code/src/tui/ody-tui.ts:338-359` | Modify: pass `appState.sessionMode` to `listSkills` |
| 7 | `apps/ody-code/src/tui/controllers/session-event-handler.ts:78-96` | Modify: add `refreshSkillCommands` to `SessionEventHost` |
| 8 | `apps/ody-code/src/tui/controllers/session-event-handler.ts:543-555` | Modify: trigger `refreshSkillCommands` on mode change |
| 9 | `apps/ody-code/test/tui/commands/resolve.test.ts` | Modify: add test for skill-not-in-map → message |
| 10 | None (defense-in-depth implicit) | No-op: `resolveSlashCommandInput` already handles missing skills |

## Dependency Overview

```
Task 1 (core + SDK + tests)
  └─→ Task 2 (TUI refreshSkillCommands)
        └─→ Task 3 (SessionEventHost + mode trigger)
              └─→ Task 4 (resolve defense-in-depth test)
```

Task 1 is the shared-signature change — it touches agent-core Session, RPC, node-sdk Session, node-sdk RPC, and adds tests, ending with a whole-tree typecheck. Tasks 2–4 are incremental TUI-side changes, each independently typecheckable.

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| RPC wire protocol incompatibility between old client and new server | `sessionMode` is optional; old clients omit it → full list (backward compat) |
| `EmptyPayload = {}` type allows any extra fields in RPC payload | `sessionMode` will be silently accepted over the wire |

---

### Task 1: Add sessionMode to Session.listSkills (agent-core + node-sdk)

**Depends on:** none
**Files:**
- Modify: `packages/agent-core/src/session/index.ts:348-351`
- Modify: `packages/agent-core/src/session/rpc.ts:91-93`
- Modify: `packages/node-sdk/src/session.ts:225-228`
- Modify: `packages/node-sdk/src/rpc.ts:425-428`
- Modify: `packages/agent-core/test/harness/skill-session.test.ts` (add tests)

#### Step 1: Write the failing test

In `packages/agent-core/test/harness/skill-session.test.ts`, add a new `describe('listSkills with sessionMode', ...)` block after the existing `it('lists session skills without exposing content')` test. The test harness already has `createTestRpc()`, `writeSkill()`, and the `EXECUTING_PLANS_SKILL` is a built-in skill with `hiddenInModes: ['plan', 'design']`.

Add these four tests:

```typescript
import { EXECUTING_PLANS_SKILL } from '../../src/skill/builtin';

describe('listSkills with sessionMode', () => {
  it('returns all skills when no sessionMode is passed (backward compat)', async () => {
    const { rpc } = await createTestRpc();
    const created = await rpc.createSession({ id: 'ses_mode_all', workDir });
    // Without sessionMode, executing-plans should be included
    const skills = await rpc.listSkills({ sessionId: created.id });
    const names = skills.map((s) => s.name);
    expect(names).toContain('executing-plans');
  });

  it('filters skills hidden in plan mode when sessionMode=plan', async () => {
    const { rpc } = await createTestRpc();
    const created = await rpc.createSession({ id: 'ses_mode_plan', workDir });
    const skills = await rpc.listSkills({ sessionId: created.id, sessionMode: 'plan' });
    const names = skills.map((s) => s.name);
    expect(names).not.toContain('executing-plans');
  });

  it('filters skills hidden in design mode when sessionMode=design', async () => {
    const { rpc } = await createTestRpc();
    const created = await rpc.createSession({ id: 'ses_mode_design', workDir });
    const skills = await rpc.listSkills({ sessionId: created.id, sessionMode: 'design' });
    const names = skills.map((s) => s.name);
    expect(names).not.toContain('executing-plans');
  });

  it('does not filter when sessionMode=normal', async () => {
    const { rpc } = await createTestRpc();
    const created = await rpc.createSession({ id: 'ses_mode_normal', workDir });
    const skills = await rpc.listSkills({ sessionId: created.id, sessionMode: 'normal' });
    const names = skills.map((s) => s.name);
    expect(names).toContain('executing-plans');
  });
});
```

#### Step 2: Run and verify FAILS

```bash
cd /Users/ranwei/workspace/ody-code && nvm use 24 && pnpm --filter @odysseythink/agent-core test --run -- -t "listSkills with sessionMode"
```

Expected: TypeScript compilation error at `rpc.listSkills({ sessionId: created.id, sessionMode: 'plan' })` — `sessionMode` does not exist on the RPC input type.

#### Step 3: Write the minimal implementation

**3a.** `packages/agent-core/src/session/index.ts:348-351` — change `listSkills`:

```typescript
interface ListSkillsOptions {
  sessionMode?: 'normal' | 'plan' | 'design';
}

async listSkills(options?: ListSkillsOptions): Promise<readonly SkillSummary[]> {
  await this.skillsReady;
  if (options?.sessionMode !== undefined) {
    return this.skills.listInvocableSkills(options.sessionMode).map(summarizeSkill);
  }
  return this.skills.listSkills().map(summarizeSkill);
}
```

**3b.** `packages/agent-core/src/session/rpc.ts:91-93` — accept `sessionMode`:
Replace:
```typescript
listSkills(_payload: EmptyPayload): Promise<readonly SkillSummary[]> {
  return this.session.listSkills();
}
```
With:
```typescript
listSkills(payload: EmptyPayload & { sessionMode?: 'normal' | 'plan' | 'design' }): Promise<readonly SkillSummary[]> {
  if (payload.sessionMode !== undefined) {
    return this.session.listSkills({ sessionMode: payload.sessionMode });
  }
  return this.session.listSkills();
}
```

**3c.** `packages/node-sdk/src/session.ts:225-228` — update type signature:
```typescript
async listSkills(options?: { sessionMode?: 'normal' | 'plan' | 'design' }): Promise<readonly SkillSummary[]> {
  this.ensureOpen();
  return this.rpc.listSkills({ sessionId: this.id, ...(options?.sessionMode !== undefined ? { sessionMode: options.sessionMode } : {}) });
}
```

**3d.** `packages/node-sdk/src/rpc.ts:425-428` — update RPC client:
Replace:
```typescript
async listSkills(input: SessionIdRpcInput): Promise<readonly SkillSummary[]> {
  const rpc = await this.getRpc();
  return rpc.listSkills({ sessionId: input.sessionId });
}
```
With:
```typescript
async listSkills(input: SessionIdRpcInput & { sessionMode?: 'normal' | 'plan' | 'design' }): Promise<readonly SkillSummary[]> {
  const rpc = await this.getRpc();
  return rpc.listSkills({ sessionId: input.sessionId, sessionMode: input.sessionMode });
}
```

#### Step 4: Run and verify PASSES

```bash
cd /Users/ranwei/workspace/ody-code && nvm use 24 && pnpm --filter @odysseythink/agent-core test --run -- -t "listSkills with sessionMode"
```

Expected: 4 tests pass. `executing-plans` is present without mode and with `normal`, absent with `plan` and `design`.

#### Step 5: Whole-tree typecheck

```bash
cd /Users/ranwei/workspace/ody-code && nvm use 24 && pnpm -r typecheck
```

Expected: zero type errors across all packages.

#### Step 6: Commit

```bash
git add packages/agent-core/src/session/index.ts packages/agent-core/src/session/rpc.ts packages/node-sdk/src/session.ts packages/node-sdk/src/rpc.ts packages/agent-core/test/harness/skill-session.test.ts
git commit -m "feat: add optional sessionMode to Session.listSkills for hidden-in-mode skill filtering"

---

### Task 2: TUI refreshSkillCommands passes current sessionMode

**Depends on:** Task 1
**Files:**
- Modify: `apps/ody-code/src/tui/ody-tui.ts:346-348`

#### Step 1: Implement the change

In `apps/ody-code/src/tui/ody-tui.ts`, the `refreshSkillCommands` method at ~L338-359 currently calls `session.listSkills()` with no arguments on line 348:

```typescript
skills = await session.listSkills();
```

Change this to pass the current mode:

```typescript
const mode = this.state.appState.sessionMode; // 'plan' | 'design' | undefined (normal)
skills = await session.listSkills(mode && mode !== 'normal' ? { sessionMode: mode } : undefined);
```

The `sessionMode` is `undefined` for normal mode in `appState`, so `mode && mode !== 'normal'` handles both `undefined` and `'normal'` → both result in `undefined` (full list, backward compat). `'plan'` or `'design'` pass through.

#### Step 2: Verify builds

```bash
cd /Users/ranwei/workspace/ody-code && nvm use 24 && pnpm --filter @odysseythink/kimi-code typecheck
```

Expected: zero type errors in the kimi-code package.

#### Step 3: Manual verification

Start the TUI in a session, enter plan mode (`/plan`), and type `/ex` + Tab. Verify `skill:executing-plans` does NOT appear in autocomplete. Exit plan mode, verify it reappears.

#### Step 4: Commit

```bash
git add apps/ody-code/src/tui/ody-tui.ts
git commit -m "feat: pass current sessionMode to listSkills in TUI refreshSkillCommands"

---

### Task 3: Add refreshSkillCommands to SessionEventHost + mode change trigger

**Depends on:** Task 2
**Files:**
- Modify: `apps/ody-code/src/tui/controllers/session-event-handler.ts:78-96` (add to interface)
- Modify: `apps/ody-code/src/tui/controllers/session-event-handler.ts:543-555` (trigger refresh)

#### Step 1: Add refreshSkillCommands to SessionEventHost interface

In `apps/ody-code/src/tui/controllers/session-event-handler.ts`, add a new import at the top (if not already imported):

```typescript
import type { SkillListSession } from '../commands/skills';
```

Then add to the `SessionEventHost` interface (~L78-96), after `tasksBrowserController`:

```typescript
export interface SessionEventHost {
  state: TUIState;
  session: Session | undefined;
  aborted: boolean;
  sessionEventUnsubscribe: (() => void) | undefined;
  readonly streamingUI: StreamingUIController;

  requireSession(): Session;
  setAppState(patch: Partial<AppState>): void;
  patchLivePane(patch: Partial<LivePaneState>): void;
  resetLivePane(): void;
  showError(msg: string): void;
  showStatus(msg: string, color?: string): void;
  showNotice(title: string, detail?: string): void;
  appendTranscriptEntry(entry: TranscriptEntry): void;
  sendQueuedMessage(session: Session, item: QueuedMessage): void;
  shiftQueuedMessage(): QueuedMessage | undefined;
  readonly tasksBrowserController: TasksBrowserController;
  refreshSkillCommands(session?: SkillListSession): Promise<void>;  // ← NEW
}
```

#### Step 2: Trigger refreshSkillCommands on mode change

In the `handleStatusUpdate` method (~L543-555), after `this.host.setAppState(patch)`, add the refresh call:

```typescript
private handleStatusUpdate(event: AgentStatusUpdatedEvent): void {
  const patch: Partial<AppState> = {};
  if (event.contextUsage !== undefined) patch.contextUsage = event.contextUsage;
  if (event.contextTokens !== undefined) patch.contextTokens = event.contextTokens;
  if (event.maxContextTokens !== undefined) patch.maxContextTokens = event.maxContextTokens;
  if (event.sessionMode !== undefined) patch.sessionMode = event.sessionMode;
  if (event.sessionModeFilePath !== undefined) patch.sessionModeFilePath = event.sessionModeFilePath;
  if (event.permission !== undefined) {
    patch.permissionMode = event.permission;
  }
  if (event.model !== undefined) patch.model = event.model;
  if (Object.keys(patch).length > 0) {
    this.host.setAppState(patch);
    if (event.sessionMode !== undefined) {
      void this.host.refreshSkillCommands(this.host.session);
    }
  }
}
```

The `void` prefix intentionally fires the async refresh without awaiting — the skill list update is best-effort and should not block the status update handler.

#### Step 3: Verify builds

```bash
cd /Users/ranwei/workspace/ody-code && nvm use 24 && pnpm --filter @odysseythink/kimi-code typecheck
```

Expected: zero type errors.

#### Step 4: Manual verification

Start the TUI, enter plan mode (`/plan`). After the mode transition completes, type `/ex` + Tab. Verify `skill:executing-plans` is absent. Exit plan mode (`/plan` again). Verify it reappears. Repeat for design mode.

#### Step 5: Commit

```bash
git add apps/ody-code/src/tui/controllers/session-event-handler.ts
git commit -m "feat: trigger refreshSkillCommands on session mode change"

---

### Task 4: Resolve test — verify defense-in-depth for hidden skills

**Depends on:** Task 1 (uses updated types, but test is independent — only tests resolve logic)
**Files:**
- Modify: `apps/ody-code/test/tui/commands/resolve.test.ts`

#### Step 1: Write the test

In `apps/ody-code/test/tui/commands/resolve.test.ts`, add a new test inside the existing `describe('resolveSlashCommandInput', ...)` block:

```typescript
it('treats unknown skill command as plain message (defense-in-depth)', () => {
  // When a skill is not in the skillCommandMap (e.g. server filtered it out due to mode),
  // typing /skill:executing-plans should be treated as a plain message.
  const result = resolve('/skill:executing-plans', {
    skillCommandMap: new Map(), // empty map — skill not registered
  });
  expect(result).toEqual({ kind: 'message', input: '/skill:executing-plans' });
});
```

This test verifies that when `skillCommandMap` does not contain `skill:executing-plans` (as would be the case after server-side mode filtering), the resolve function falls through to `{kind:'message'}`, effectively rejecting the skill command.

#### Step 2: Run and verify PASSES

```bash
cd /Users/ranwei/workspace/ody-code && nvm use 24 && pnpm --filter @odysseythink/kimi-code test --run -- -t "treats unknown skill command"
```

Expected: test passes.

#### Step 3: Commit

```bash
git add apps/ody-code/test/tui/commands/resolve.test.ts
git commit -m "test: verify resolve treats unknown skills as plain messages"

---

## Self-Review

- [ ] 1. **Spec-coverage table**:

| Design Requirement | Task(s) | Status |
|---|---|---|
| `Session.listSkills({sessionMode})` filtering | Task 1 | covered |
| RPC `sessionMode` passthrough | Task 1 | covered |
| node-sdk type updates | Task 1 | covered |
| TUI `refreshSkillCommands` passes current mode | Task 2 | covered |
| `SessionEventHost` interface update | Task 3 | covered |
| Mode change triggers `refreshSkillCommands` | Task 3 | covered |
| Defense-in-depth (resolve fallthrough) | Task 4 | covered |
| `SkillListSession` auto-propagation | — | no-op (Pick auto-updates) |
| `skills.ts` no code change needed | — | no-op |
| `resolve.ts` no code change needed | — | no-op |

- [ ] 2. **Placeholder scan**: No TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders. All code is complete and inline.

- [ ] 3. **No phantom tasks**: Every task produces a verifiable change (file modifications + test additions). Two items are declared no-op with justification. Zero `--allow-empty` commits.

- [ ] 4. **Dependency soundness**: Task 1 → Task 2 → Task 3 → Task 4. Task 2 depends on Task 1's updated `Session.listSkills` signature. Task 3 depends on Task 2's TUI method. Task 4 is independently runnable but logically depends on Task 1 for type compatibility.

- [ ] 5. **Caller & build soundness**: Task 1 changes `Session.listSkills` signature — it updates all 4 callers (session/rpc.ts, node-sdk session.ts, node-sdk rpc.ts) in the same task, plus adds tests, and ends with `pnpm -r typecheck`. No other callers of `listSkills` exist outside these files (verified via grep). Tasks 2-4 each end with `pnpm --filter @odysseythink/kimi-code typecheck`. No signature split across tasks.

- [ ] 6. **Test-the-risk**: 
  - Task 1 tests the filter (state mutation: `listInvocableSkills` filtering) with 4 behavioral assertions covering all 3 modes + no-mode backward compat.
  - Task 4 tests the resolve fallthrough (defense-in-depth) with a behavioral assertion.
  - Filter must-survive check: `normal` mode and no-mode both must keep `executing-plans` — both verified in tests. `plan` and `design` must reject it — both verified. No constant-vs-assertion contradiction.

- [ ] 7. **Type consistency**: `sessionMode` type is `'normal' | 'plan' | 'design'` consistently across all 4 files in Task 1, and used as such in Tasks 2-3. `ListSkillsOptions` is defined in `session/index.ts` and consumed via RPC passthrough. `EmptyPayload` is `{}` so `& { sessionMode?: ... }` is valid. `SkillListSession` auto-derives from `Pick<Session, 'listSkills'>` — no manual type to go stale.
```
```
```
```
