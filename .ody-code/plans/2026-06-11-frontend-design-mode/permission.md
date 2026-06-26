# Part 2: Permission Policies & Exit Tool

> Scope: refactor the binary plan/design guard into a mode-aware `SessionModeGuardPermissionPolicy`, extend the approve and review-ask policies for `frontend-design`, create `ExitFrontendDesignModeTool`, wire it into the builtin tool registry, and add tests.
>
> Depends on: `2026-06-11-frontend-design-mode/core.md` (SessionModeKind expanded to `'frontend-design'`, Agent context partition `'frontend-design'` exists).

---

### Task 1: Refactor `PlanModeGuardDenyPermissionPolicy` → `SessionModeGuardPermissionPolicy`

**Depends on:** `2026-06-11-frontend-design-mode/core.md`: Task 1 (SessionModeKind includes `'frontend-design'`)

**Files:**
- **Rename:** `packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts` → `session-mode-guard.ts`
- **Modify:** `packages/agent-core/src/agent/permission/policies/index.ts:14-15,34`
- **Modify:** `packages/agent-core/src/agent/session-mode/index.ts:306` (JSDoc link)
- **Modify:** `packages/agent-core/test/tools/plan-mode-hard-block.test.ts:10,74,149,152,170,207`
- **Modify:** `packages/agent-core/test/agent/permission.test.ts:670,1443`
- **Test:** `packages/agent-core/test/tools/plan-mode-hard-block.test.ts` (update + add cases)

This is a shared-signature rename: every textual reference to the old class name, file path, and policy `name` must be updated in the same task.

- [ ] Write the failing test additions first. In `packages/agent-core/test/tools/plan-mode-hard-block.test.ts`, add an `activeFrontendDesignAgent()` helper and tests that assert the guard **allows** Write/Edit to `.tsx`/`.css`, Bash, TaskStop, and CronCreate while in `frontend-design` mode:

```typescript
async function activeFrontendDesignAgent(): Promise<{ agent: Agent; sessionMode: SessionMode }> {
  const agent = {
    homedir: '/tmp/kimi-fd-test',
    config: { cwd: '/tmp/kimi-fd-test' },
    emitStatusUpdated: vi.fn(),
    records: { logRecord: vi.fn() },
    replayBuilder: { push: vi.fn() },
    kaos: {
      mkdir: vi.fn().mockResolvedValue(undefined),
    },
  } as unknown as Agent;
  const sessionMode = new SessionMode(agent);
  Object.assign(agent, { sessionMode });
  await sessionMode.enter('current-fd', false, true, 'frontend-design');
  (sessionMode as unknown as { _sessionModeFilePath: string })._sessionModeFilePath = '/tmp/kimi-fd-test/frontend-designs/2026-06-06-landing.md';
  return { agent, sessionMode };
}

describe('Frontend-design mode permission policy', () => {
  it('allows Write to any project file (not limited to .md)', async () => {
    const { agent } = await activeFrontendDesignAgent();
    expect(evaluatePlanPolicy(agent, 'Write', { path: '/tmp/kimi-fd-test/src/App.tsx', content: 'x' })).toBeUndefined();
    expect(evaluatePlanPolicy(agent, 'Write', { path: '/tmp/kimi-fd-test/src/styles.css', content: 'x' })).toBeUndefined();
  });

  it('allows Edit to any project file', async () => {
    const { agent } = await activeFrontendDesignAgent();
    expect(evaluatePlanPolicy(agent, 'Edit', { path: '/tmp/kimi-fd-test/src/App.tsx', old_string: 'A', new_string: 'B' })).toBeUndefined();
  });

  it.each(['manual', 'yolo', 'auto'] as const)('allows Bash in %s mode', async (mode) => {
    const { agent } = await activeFrontendDesignAgent();
    expect(evaluatePlanPolicy(agent, 'Bash', { command: 'npm install' }, mode)).toBeUndefined();
  });

  it.each(['manual', 'yolo', 'auto'] as const)('allows TaskStop in %s mode', async (mode) => {
    const { agent } = await activeFrontendDesignAgent();
    expect(evaluatePlanPolicy(agent, 'TaskStop', { task_id: 'dev-abc12345' }, mode)).toBeUndefined();
  });

  it('allows CronCreate and CronDelete', async () => {
    const { agent } = await activeFrontendDesignAgent();
    expect(evaluatePlanPolicy(agent, 'CronCreate', { cron: '*/5 * * * *', prompt: 'ping' })).toBeUndefined();
    expect(evaluatePlanPolicy(agent, 'CronDelete', { id: 'job_1' })).toBeUndefined();
  });

  it('allows read-only tools', async () => {
    const { agent } = await activeFrontendDesignAgent();
    expect(evaluatePlanPolicy(agent, 'Read', { path: '/workspace/src/main.ts' })).toBeUndefined();
    expect(evaluatePlanPolicy(agent, 'Grep', { pattern: 'TODO', path: '/workspace' })).toBeUndefined();
  });
});
```

Also update the import and all constructor calls in the same file from `PlanModeGuardDenyPermissionPolicy` to `SessionModeGuardPermissionPolicy`, and update the policy name string in `permission.test.ts` from `'plan-mode-guard-deny'` to `'session-mode-guard'`.

- [ ] Run the test and verify it FAILS because the old policy still denies TaskStop and CronCreate in `frontend-design` mode:

```bash
cd packages/agent-core && pnpm test -- test/tools/plan-mode-hard-block.test.ts
```

Expected failure: `expect(received).toBeUndefined()` for TaskStop, CronCreate, and non-.md Write/Edit calls.

- [ ] Rename the file and refactor the class. The new `packages/agent-core/src/agent/permission/policies/session-mode-guard.ts`:

```typescript
import { basename } from 'pathe';

import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';
import { writeFileAccesses } from './file-access-ask';

export class SessionModeGuardPermissionPolicy implements PermissionPolicy {
  readonly name = 'session-mode-guard';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!this.agent.sessionMode.isActive) return;

    const kind = this.agent.sessionMode.kind;
    const toolName = context.toolCall.name;

    if (kind === 'frontend-design') {
      return this.evaluateFrontendDesign(toolName);
    }

    return this.evaluatePlanOrDesign(context, toolName, kind);
  }

  private evaluateFrontendDesign(toolName: string): PermissionPolicyResult | undefined {
    // frontend-design mode allows all file writes, shell commands, background
    // task management, and cron operations. Other policies (cwd-guard,
    // sensitive-file, bash-deny-list) still apply.
    if (toolName === 'Write' || toolName === 'Edit') {
      return undefined;
    }
    if (toolName === 'Bash') {
      return undefined;
    }
    if (toolName === 'TaskStop') {
      return undefined;
    }
    if (toolName === 'CronCreate' || toolName === 'CronDelete') {
      return undefined;
    }
    return undefined;
  }

  private evaluatePlanOrDesign(
    context: PermissionPolicyContext,
    toolName: string,
    kind: 'plan' | 'design',
  ): PermissionPolicyResult | undefined {
    const modeLabel = kind;
    const exitTool = kind === 'design' ? 'ExitDesignMode' : 'ExitPlanMode';

    if (toolName === 'Write' || toolName === 'Edit') {
      const sessionModeFilePath = this.agent.sessionMode.sessionModeFilePath;
      if (sessionModeFilePath === null) {
        return {
          kind: 'deny',
          message: modeWriteDeniedMessage(modeLabel, sessionModeFilePath),
        };
      }
      if (writesOnlyPlanFileset(context, this.agent)) {
        return;
      }
      return {
        kind: 'deny',
        message: modeWriteDeniedMessage(modeLabel, sessionModeFilePath),
      };
    }

    if (toolName === 'TaskStop') {
      return {
        kind: 'deny',
        message: `TaskStop is not available in ${modeLabel} mode. Call ${exitTool} to exit ${modeLabel} mode before stopping a background task.`,
      };
    }

    if (toolName === 'CronCreate' || toolName === 'CronDelete') {
      return {
        kind: 'deny',
        message: `${toolName} is not available in ${modeLabel} mode because it would mutate scheduled work that runs after ${modeLabel} exit. Call ${exitTool} first.`,
      };
    }

    return;
  }
}

function writesOnlyPlanFileset(context: PermissionPolicyContext, agent: Agent): boolean {
  const writeAccesses = writeFileAccesses(context);
  if (writeAccesses.length === 0) return false;
  return writeAccesses.every((access) => agent.sessionMode.isWritableSessionModePath(access.path));
}

function modeWriteDeniedMessage(modeLabel: string, sessionModeFilePath: string | null): string {
  const Mode = modeLabel.charAt(0).toUpperCase() + modeLabel.slice(1);
  const exitTool = modeLabel === 'design' ? 'ExitDesignMode' : 'ExitPlanMode';
  if (sessionModeFilePath === null) {
    return (
      `${Mode} mode is active, but no ${modeLabel} file has been selected yet. ` +
      `Wait for the host to assign one before writing, or call ${exitTool} to exit ${modeLabel} mode.`
    );
  }
  const stem = basename(sessionModeFilePath).replace(/\.md$/, '');
  return (
    `${Mode} mode is active. You may only write to the assigned ${modeLabel} file (${sessionModeFilePath}) ` +
    `or .md files inside its "${stem}/" subdirectory (where split parts go) — write split parts there, do NOT merge them into the index and do NOT invent another path. ` +
    `Call ${exitTool} to exit ${modeLabel} mode before editing other files.`
  );
}
```

- [ ] Update `packages/agent-core/src/agent/permission/policies/index.ts`:
  - Replace `import { PlanModeGuardDenyPermissionPolicy } from './plan-mode-guard-deny';` with `import { SessionModeGuardPermissionPolicy } from './session-mode-guard';`
  - Replace `new PlanModeGuardDenyPermissionPolicy(agent)` with `new SessionModeGuardPermissionPolicy(agent)`
  - Update the inline comment from `plan mode: Write/Edit outside the plan file, or TaskStop → deny.` to `session mode: plan/design Write/Edit guard and TaskStop/Cron deny.`

- [ ] Update `packages/agent-core/src/agent/session-mode/index.ts:306` JSDoc: replace `{@link PlanModeGuardDenyPermissionPolicy}` with `{@link SessionModeGuardPermissionPolicy}`.

- [ ] Update `packages/agent-core/test/agent/permission.test.ts`:
  - Line 670: replace `'plan-mode-guard-deny'` with `'session-mode-guard'`
  - Line 1443: replace `policy_name: 'plan-mode-guard-deny'` with `policy_name: 'session-mode-guard'`

- [ ] Run tests and verify they PASS:

```bash
cd packages/agent-core && pnpm test -- test/tools/plan-mode-hard-block.test.ts
cd packages/agent-core && pnpm test -- test/agent/permission.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck
```

- [ ] Commit: `git add -A && git commit -m "refactor(agent-core): rename PlanModeGuardDenyPermissionPolicy to SessionModeGuardPermissionPolicy and allow frontend-design mode writes"`

---

### Task 2: Extend `PlanModeToolApprovePermissionPolicy` for `frontend-design`

**Depends on:** Task 1

**Files:**
- **Modify:** `packages/agent-core/src/agent/permission/policies/plan-mode-tool-approve.ts`
- **Modify:** `packages/agent-core/test/agent/permission.test.ts`

- [ ] Add `EnterFrontendDesignMode` → approve, and `ExitFrontendDesignMode` handling (same pattern as `ExitPlanMode`/`ExitDesignMode`). In `packages/agent-core/src/agent/permission/policies/plan-mode-tool-approve.ts`:

```typescript
import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';
import { writeFileAccesses } from './file-access-ask';

export class PlanModeToolApprovePermissionPolicy implements PermissionPolicy {
  readonly name = 'plan-mode-tool-approve';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const toolName = context.toolCall.name;

    if (
      toolName === 'EnterPlanMode' ||
      toolName === 'EnterDesignMode' ||
      toolName === 'EnterFrontendDesignMode'
    ) {
      return { kind: 'approve' };
    }

    if (
      (toolName === 'Write' || toolName === 'Edit') &&
      this.agent.sessionMode.isActive &&
      writesOnlyPlanFile(context, this.agent.sessionMode.sessionModeFilePath)
    ) {
      return { kind: 'approve' };
    }

    if (
      toolName === 'ExitPlanMode' ||
      toolName === 'ExitDesignMode' ||
      toolName === 'ExitFrontendDesignMode'
    ) {
      if (!this.agent.sessionMode.isActive) {
        return { kind: 'approve' };
      }
      if (context.execution.display?.kind !== 'plan_review') {
        return { kind: 'approve' };
      }
      if (context.execution.display.plan.trim().length > 0) return;
      return { kind: 'approve' };
    }
  }
}

function writesOnlyPlanFile(
  context: PermissionPolicyContext,
  sessionModeFilePath: string | null,
): boolean {
  if (sessionModeFilePath === null) return false;
  const writeAccesses = writeFileAccesses(context);
  return writeAccesses.every((access) => access.path === sessionModeFilePath);
}
```

- [ ] Add a lightweight behavioural test in `packages/agent-core/test/agent/permission.test.ts` (near the existing `EnterPlanMode` approval test around line 1360). Create a test helper that sets `sessionMode.kind` to `'frontend-design'` and asserts `EnterFrontendDesignMode` and `ExitFrontendDesignMode` (with no plan_review display) are approved:

```typescript
it('approves EnterFrontendDesignMode unconditionally', async () => {
  const { manager, requestApproval, telemetryTrack } = makePermissionManager(async () => ({
    decision: 'approved',
  }));

  await expect(
    manager.beforeToolCall(
      hookContext({
        id: 'call_enter_fd',
        toolName: 'EnterFrontendDesignMode',
        args: {},
      }),
    ),
  ).resolves.toBeUndefined();

  expect(requestApproval).not.toHaveBeenCalled();
  expect(telemetryTrack).toHaveBeenCalledWith(
    'permission_policy_decision',
    expect.objectContaining({
      policy_name: 'plan-mode-tool-approve',
      tool_name: 'EnterFrontendDesignMode',
      decision: 'approve',
    }),
  );
});

it('approves ExitFrontendDesignMode when mode is inactive', async () => {
  const { manager, requestApproval, telemetryTrack } = makePermissionManager(async () => ({
    decision: 'approved',
  }));

  await expect(
    manager.beforeToolCall(
      hookContext({
        id: 'call_exit_fd_inactive',
        toolName: 'ExitFrontendDesignMode',
        args: {},
        execution: planReviewExecution({ plan: '# Design', path: '/tmp/fd.md' }),
      }),
    ),
  ).resolves.toBeUndefined();

  expect(requestApproval).not.toHaveBeenCalled();
  expect(telemetryTrack).toHaveBeenCalledWith(
    'permission_policy_decision',
    expect.objectContaining({
      policy_name: 'plan-mode-tool-approve',
      tool_name: 'ExitFrontendDesignMode',
      decision: 'approve',
    }),
  );
});
```

- [ ] Run the permission tests:

```bash
cd packages/agent-core && pnpm test -- test/agent/permission.test.ts
```

- [ ] Commit: `git commit -am "feat(agent-core): approve EnterFrontendDesignMode and ExitFrontendDesignMode in plan-mode-tool-approve policy"`

---

### Task 3: Extend `ExitPlanModeReviewAskPermissionPolicy` for `ExitFrontendDesignMode`

**Depends on:** Task 2

**Files:**
- **Modify:** `packages/agent-core/src/agent/permission/policies/exit-plan-mode-review-ask.ts`
- **Modify:** `packages/agent-core/test/agent/permission.test.ts`

The `ExitPlanModeReviewAskPermissionPolicy` currently handles `ExitPlanMode` and `ExitDesignMode` with a shared `plan_review` approval surface. We add `ExitFrontendDesignMode` with the same surface but a direct `exit()` to normal mode (no handoff to plan).

- [ ] Refactor the guard logic at the top of `evaluate()` to use kind-matching instead of boolean isDesign. Update `packages/agent-core/src/agent/permission/policies/exit-plan-mode-review-ask.ts`:

```typescript
import type { Agent } from '../..';
import type { ApprovalResponse, PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

interface ExitPlanModeOption {
  readonly label: string;
  readonly description: string;
}

interface PlanReviewDisplay {
  readonly plan: string;
  readonly path?: string | undefined;
  readonly options?: readonly ExitPlanModeOption[] | undefined;
}

export class ExitPlanModeReviewAskPermissionPolicy implements PermissionPolicy {
  readonly name = 'exit-plan-mode-review-ask';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const toolName = context.toolCall.name;
    const kind = this.mapToolNameToModeKind(toolName);
    if (kind === undefined) return;
    if (this.agent.permission.mode === 'auto') return;
    if (!this.agent.sessionMode.isActive) return;
    if (this.agent.sessionMode.kind !== kind) return;

    const display = context.execution.display;
    if (display?.kind !== 'plan_review') return;
    if (display.plan.trim().length === 0) return;

    const eventPrefix = kind === 'design' ? 'design' : kind === 'frontend-design' ? 'frontend_design' : 'plan';
    this.agent.telemetry.track(`${eventPrefix}_submitted`, {
      has_options: display.options !== undefined && display.options.length >= 2,
    });

    return {
      kind: 'ask',
      reason: {
        has_options: display.options !== undefined,
      },
      resolveApproval: (result) =>
        this.exitModeApprovalResult(result, {
          plan: display.plan,
          path: display.path,
          options: display.options,
        }, kind),
    };
  }

  private mapToolNameToModeKind(
    toolName: string,
  ): 'plan' | 'design' | 'frontend-design' | undefined {
    if (toolName === 'ExitPlanMode') return 'plan';
    if (toolName === 'ExitDesignMode') return 'design';
    if (toolName === 'ExitFrontendDesignMode') return 'frontend-design';
    return undefined;
  }

  private exitModeApprovalResult(
    result: ApprovalResponse,
    display: PlanReviewDisplay,
    kind: 'plan' | 'design' | 'frontend-design',
  ) {
    if (result.decision !== 'approved') {
      return this.rejectedExitModeApprovalResult(result, kind);
    }

    if (kind === 'design') {
      if (result.selectedLabel !== undefined && result.selectedLabel.length > 0) {
        this.agent.telemetry.track('design_resolved', { outcome: 'approved', chosen_option: result.selectedLabel });
      } else {
        this.agent.telemetry.track('design_resolved', { outcome: 'approved' });
      }
      return {
        kind: 'approve' as const,
        executionMetadata: { selectedLabel: result.selectedLabel },
      };
    }

    if (kind === 'frontend-design') {
      if (result.selectedLabel !== undefined && result.selectedLabel.length > 0) {
        this.agent.telemetry.track('frontend_design_resolved', { outcome: 'approved', chosen_option: result.selectedLabel });
      } else {
        this.agent.telemetry.track('frontend_design_resolved', { outcome: 'approved' });
      }
      const failed = this.exitMode();
      if (failed !== undefined) {
        return { kind: 'result' as const, syntheticResult: failed };
      }
      const savedTo = display.path !== undefined ? `Design saved to: ${display.path}\n\n` : '';
      const formatted = `Frontend-design mode deactivated.\n${savedTo}## Approved Design:\n${display.plan}\n\nYour frontend design is complete. The generated code files are in your project directory.`;
      return {
        kind: 'result' as const,
        syntheticResult: {
          isError: false,
          stopTurn: true,
          output: `Exited frontend-design mode. ${formatted}`,
        },
      };
    }

    // Plan case
    const selected = selectedExitPlanModeOption(display.options, result.selectedLabel);
    const failed = this.exitMode();
    if (failed !== undefined) {
      return { kind: 'result' as const, syntheticResult: failed };
    }
    if (result.selectedLabel !== undefined && result.selectedLabel.length > 0) {
      this.agent.telemetry.track('plan_resolved', { outcome: 'approved', chosen_option: result.selectedLabel });
    } else {
      this.agent.telemetry.track('plan_resolved', { outcome: 'approved' });
    }
    const optionPrefix =
      selected === undefined
        ? ''
        : `Selected approach: ${selected.label}\nExecute ONLY the selected approach. Do not execute any unselected alternatives.\n\n`;
    const savedTo = display.path !== undefined ? `Plan saved to: ${display.path}\n\n` : '';
    const formattedPlan = `Plan mode deactivated.\n${savedTo}## Approved Plan:\n${display.plan}\n\nSTOP — do NOT begin executing now. This turn ends here so the planning context can be freed before execution. Do not write or edit code. The user will start execution themselves — typically by running /compact to free up context, then sending a message or invoking the gpowers executing-plans skill. Wait for them.`;
    return {
      kind: 'result' as const,
      syntheticResult: {
        isError: false,
        stopTurn: true,
        output: `Exited plan mode. ${optionPrefix}${formattedPlan}`,
      },
    };
  }

  private rejectedExitModeApprovalResult(
    result: ApprovalResponse,
    kind: 'plan' | 'design' | 'frontend-design',
  ) {
    this.trackRejectedModeResolution(result, kind);
    const modeLabel = kind === 'plan' ? 'Plan' : kind === 'design' ? 'Design' : 'Frontend-design';
    const modeActive = kind === 'plan' ? 'Plan mode' : kind === 'design' ? 'Design mode' : 'Frontend-design mode';

    if (result.decision === 'cancelled') {
      return {
        kind: 'result' as const,
        syntheticResult: {
          isError: false,
          output: `${modeLabel} approval dismissed. ${modeActive} remains active.`,
        },
      };
    }

    if (result.selectedLabel === 'Reject and Exit') {
      const failed = this.exitMode();
      return {
        kind: 'result' as const,
        syntheticResult:
          failed ?? {
            isError: true,
            stopTurn: true,
            output: `${modeLabel} rejected by user. ${modeActive} deactivated.`,
          },
      };
    }

    const feedback = result.feedback ?? '';
    if (result.selectedLabel === 'Revise' || feedback.length > 0) {
      return {
        kind: 'result' as const,
        syntheticResult: {
          isError: false,
          output:
            feedback.length > 0
              ? `User rejected the ${modeLabel.toLowerCase()}. Feedback:\n\n${feedback}`
              : `User requested revisions. ${modeActive} remains active.`,
        },
      };
    }

    return {
      kind: 'result' as const,
      syntheticResult: {
        isError: true,
        stopTurn: true,
        output: `${modeLabel} rejected by user. ${modeActive} remains active.`,
      },
    };
  }

  private exitMode(): { isError: true; output: string } | undefined {
    const kind = this.agent.sessionMode.kind;
    const modeLabel = kind === 'design' ? 'design' : kind === 'frontend-design' ? 'frontend-design' : 'plan';
    try {
      this.agent.sessionMode.exit();
    } catch (error) {
      const message = error instanceof Error ? error.message : `Unknown error.`;
      return {
        isError: true,
        output: `Failed to exit ${modeLabel} mode: ${message}`,
      };
    }
  }

  private trackRejectedModeResolution(
    result: ApprovalResponse,
    kind: 'plan' | 'design' | 'frontend-design',
  ): void {
    const eventPrefix = kind === 'design' ? 'design' : kind === 'frontend-design' ? 'frontend_design' : 'plan';
    if (result.decision === 'cancelled') {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, { outcome: 'dismissed' });
      return;
    }
    if (result.selectedLabel === 'Reject and Exit') {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, { outcome: 'rejected_and_exited' });
      return;
    }
    const feedback = result.feedback ?? '';
    if (result.selectedLabel === 'Revise' || feedback.length > 0) {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, {
        outcome: 'revise',
        has_feedback: feedback.length > 0,
      });
      return;
    }
    this.agent.telemetry.track(`${eventPrefix}_resolved`, { outcome: 'rejected' });
  }
}

function selectedExitPlanModeOption(
  options: readonly ExitPlanModeOption[] | undefined,
  label: string | undefined,
): ExitPlanModeOption | undefined {
  if (options === undefined || label === undefined) return;
  return options.find((option) => option.label === label);
}
```

- [ ] Add test in `packages/agent-core/test/agent/permission.test.ts` that asserts `ExitFrontendDesignMode` with a non-empty `plan_review` display is deferred to ask (not approved directly). Search for the existing plan/design test pattern around line 1537 (`defers non-empty plan reviews to the review approval policy`) and add a parallel case:

```typescript
it('defers non-empty frontend-design reviews to the review approval policy', async () => {
  const { manager, requestApproval, telemetryTrack } = makePermissionManager(
    async () => ({ decision: 'approved' }),
    { planModeActive: true, sessionModeFilePath: '/tmp/fd.md', sessionModeKind: 'frontend-design' },
  );

  const result = await manager.beforeToolCall(
    hookContext({
      id: 'call_exit_fd_review',
      toolName: 'ExitFrontendDesignMode',
      args: {},
      execution: planReviewExecution({ plan: '# Frontend Design', path: '/tmp/fd.md' }),
    }),
  );

  expect(result).toMatchObject({ block: true, reason: expect.any(String) });
  expect(requestApproval).toHaveBeenCalled();
  expect(telemetryTrack).toHaveBeenCalledWith(
    'frontend_design_submitted',
    expect.objectContaining({ has_options: false }),
  );
});
```

Note: the `makePermissionManager` helper may need a `sessionModeKind` option. If the helper does not support it, patch the helper in the same test file to accept `{ sessionModeKind?: SessionModeKind }` and default it to `'plan'`.

- [ ] Run permission tests:

```bash
cd packages/agent-core && pnpm test -- test/agent/permission.test.ts
```

- [ ] Commit: `git commit -am "feat(agent-core): handle ExitFrontendDesignMode in exit-plan-mode-review-ask policy"`

---

### Task 4: Create `ExitFrontendDesignModeTool`

**Depends on:** Task 3

**Files:**
- **Create:** `packages/agent-core/src/tools/builtin/planning/exit-frontend-design-mode.ts`
- **Create:** `packages/agent-core/src/tools/builtin/planning/exit-frontend-design-mode.md`

- [ ] Create the description markdown `packages/agent-core/src/tools/builtin/planning/exit-frontend-design-mode.md`:

```markdown
Use this tool when you are in frontend-design mode and have finished writing the design document and generating the frontend code, and are ready to present the result to the user.

## How This Tool Works
- You should have already written the design to the design file specified in the frontend-design mode reminder.
- This tool does NOT take the design content as a parameter — it reads the design from the file you wrote.
- The user will see the contents of your design file when they review it. In auto permission mode, the tool reads the file and exits frontend-design mode without asking the user.

## Before Using
- Make sure all code files have been generated and written to the project directory.
- Make sure dependencies have been installed (npm install, npx shadcn init, etc.).
- Run the pre-flight checklist before calling this tool.

## Multiple Approaches
If your design presents multiple alternative directions:
- Pass them via the `options` parameter so the user can choose which one to pursue.
- Each option should have a concise label and a brief description of trade-offs.
- If you recommend one option, append "(Recommended)" to its label.
- Provide up to 3 options; 2-3 distinct approaches work best when the design offers a real choice.
- Passing a single option is allowed and is equivalent to a plain approval.
- Do NOT use "Reject", "Reject and Exit", "Revise", or "Approve" as option labels — these are reserved by the system.

## After Exit
- Frontend-design mode deactivates and all tools become available again.
- The generated code files remain in the project directory.
- Suggest the user run `npm run dev` to preview the result.
- Do NOT use AskUserQuestion to ask "Is this design OK?" or "Should I proceed?" — that is exactly what ExitFrontendDesignMode does.
- If rejected, revise based on feedback and call ExitFrontendDesignMode again.
```

- [ ] Create the tool implementation `packages/agent-core/src/tools/builtin/planning/exit-frontend-design-mode.ts`:

```typescript
/**
 * ExitFrontendDesignModeTool — frontend-design mode exit tool.
 *
 * The LLM calls this tool to surface a finalised design document to the user
 * and exit frontend-design mode. The design must already be written to the
 * current design file; this tool reads that file and flips frontend-design
 * mode off. It mirrors {@link ExitPlanModeTool} (direct exit to normal)
 * rather than {@link ExitDesignModeTool} (handoff to plan).
 */

import type { Agent } from '#/agent';
import type { SessionModeData } from '#/agent/session-mode';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import type { ToolInputDisplay } from '../../display';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './exit-frontend-design-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export interface ExitFrontendDesignModeOption {
  label: string;
  description: string;
}

export interface ExitFrontendDesignModeInput {
  options?: readonly ExitFrontendDesignModeOption[] | undefined;
}

const RESERVED_OPTION_LABELS = new Set(
  ['Approve', 'Reject', 'Reject and Exit', 'Revise'].map(normalizeOptionLabel),
);

const ExitFrontendDesignModeOptionSchema = z
  .object({
    label: z
      .string()
      .min(1)
      .max(80)
      .describe(
        'Short name for this approach (1-8 words). Append "(Recommended)" if you recommend it.',
      ),
    description: z
      .string()
      .default('')
      .describe('Brief summary of this approach and its trade-offs.'),
  })
  .strict();

export const ExitFrontendDesignModeInputSchema: z.ZodType<ExitFrontendDesignModeInput> = z
  .object({
    options: z
      .array(ExitFrontendDesignModeOptionSchema)
      .min(1)
      .max(3)
      .refine(hasUniqueOptionLabels, 'Option labels must be unique.')
      .refine(hasNoReservedOptionLabels, 'Option labels must not use reserved approval labels.')
      .optional()
      .describe(
        'When the design presents multiple alternative directions, list them here so the user can choose which one to pursue. Provide up to 3 options; 2-3 distinct approaches work best when the design offers a real choice. Passing a single option is allowed and is equivalent to a plain approval. Do not use "Reject", "Revise", "Approve", or "Reject and Exit" as labels.',
      ),
  })
  .strict();

interface ResolveFrontendDesignResult {
  ok: boolean;
  design?: string;
  path?: string | undefined;
  error?: ExecutableToolResult;
}

// ── Implementation ───────────────────────────────────────────────────

export class ExitFrontendDesignModeTool implements BuiltinTool<ExitFrontendDesignModeInput> {
  readonly name = 'ExitFrontendDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitFrontendDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  async resolveExecution(args: ExitFrontendDesignModeInput): Promise<ToolExecution> {
    return {
      description: 'Presenting frontend design and exiting frontend-design mode',
      display: await this.resolveFrontendDesignReviewDisplay(args),
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async resolveFrontendDesignReviewDisplay(
    args: ExitFrontendDesignModeInput,
  ): Promise<ToolInputDisplay | undefined> {
    if (!this.agent.sessionMode.isActive) return undefined;
    if (this.agent.sessionMode.kind !== 'frontend-design') return undefined;
    let data: SessionModeData;
    try {
      data = await this.agent.sessionMode.data();
    } catch {
      return undefined;
    }
    if (data === null || data.content.trim().length === 0) return undefined;
    const display: ToolInputDisplay = {
      kind: 'plan_review',
      plan: data.content,
      path: data.path,
    };
    if (args.options !== undefined && args.options.length >= 2) {
      display.options = args.options;
    }
    return display;
  }

  private async execution(args: ExitFrontendDesignModeInput): Promise<ExecutableToolResult> {
    if (!this.agent.sessionMode.isActive) {
      return {
        isError: true,
        output:
          'ExitFrontendDesignMode can only be called while frontend-design mode is active. Use EnterFrontendDesignMode (or /frontend-design) first.',
      };
    }
    if (this.agent.sessionMode.kind !== 'frontend-design') {
      return {
        isError: true,
        output: `ExitFrontendDesignMode can only be called while frontend-design mode is active. Current mode is ${this.agent.sessionMode.kind}.`,
      };
    }

    const resolved = await this.resolveFrontendDesign();
    if (!resolved.ok) return resolved.error as ExecutableToolResult;

    this.agent.telemetry.track('frontend_design_submitted', {
      has_options: args.options !== undefined && args.options.length >= 2,
    });

    const failed = await this.exitMode();
    if (failed !== undefined) return failed;

    this.agent.telemetry.track('frontend_design_resolved', { outcome: 'auto_approved' });

    return {
      isError: false,
      stopTurn: true,
      output: `Exited frontend-design mode. ${formatFrontendDesignHandoffOutput(resolved.design ?? '', resolved.path)}`,
    };
  }

  private async exitMode(): Promise<ExecutableToolResult | undefined> {
    try {
      this.agent.sessionMode.exit();
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to exit frontend-design mode.';
      return {
        isError: true,
        output: `Failed to exit frontend-design mode: ${message}`,
      };
    }
  }

  private async resolveFrontendDesign(): Promise<ResolveFrontendDesignResult> {
    let data: SessionModeData;
    try {
      data = await this.agent.sessionMode.data();
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to read design file.';
      return {
        ok: false,
        error: { isError: true, output: `Failed to read design file: ${message}` },
      };
    }

    if (data !== null && data.content.trim().length > 0) {
      return { ok: true, design: data.content, path: data.path };
    }

    const path = data?.path ?? this.agent.sessionMode.sessionModeFilePath;
    return {
      ok: false,
      error: {
        isError: true,
        output:
          path === null
            ? 'No design file found. Write the design to the current design file first, then call ExitFrontendDesignMode.'
            : `No design file found. Write your design to ${path} first, then call ExitFrontendDesignMode.`,
      },
    };
  }
}

function hasUniqueOptionLabels(options: readonly ExitFrontendDesignModeOption[]): boolean {
  const labels = new Set<string>();
  for (const option of options) {
    const label = normalizeOptionLabel(option.label);
    if (labels.has(label)) return false;
    labels.add(label);
  }
  return true;
}

function hasNoReservedOptionLabels(options: readonly ExitFrontendDesignModeOption[]): boolean {
  return options.every((option) => !RESERVED_OPTION_LABELS.has(normalizeOptionLabel(option.label)));
}

function normalizeOptionLabel(label: string): string {
  return label.trim().toLowerCase();
}

function formatFrontendDesignHandoffOutput(design: string, path: string | undefined): string {
  const savedTo = path !== undefined ? `Design saved to: ${path}\n\n` : '';
  return `Frontend-design mode deactivated.\n${savedTo}## Approved Design:\n${design}\n\nYour frontend design is complete. The generated code files are in your project directory.`;
}
```

- [ ] Run the package typecheck to ensure the new file compiles:

```bash
cd packages/agent-core && pnpm typecheck
```

- [ ] Commit: `git add -A && git commit -m "feat(agent-core): add ExitFrontendDesignModeTool"`

---

### Task 5: Register `ExitFrontendDesignModeTool` in builtin tool registry

**Depends on:** Task 4

**Files:**
- **Modify:** `packages/agent-core/src/tools/builtin/index.ts`
- **Modify:** `packages/agent-core/src/agent/tool/index.ts:419-423`

- [ ] Add the export to `packages/agent-core/src/tools/builtin/index.ts` after `exit-design-mode`:

```typescript
export * from './planning/exit-frontend-design-mode';
```

- [ ] Register the tool in `packages/agent-core/src/agent/tool/index.ts` inside `initializeBuiltinTools()`, after `ExitDesignModeTool`:

```typescript
new b.ExitDesignModeTool(this.agent),
new b.ExitFrontendDesignModeTool(this.agent),
new b.EnterPlanModeTool(this.agent),
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck
```

- [ ] Commit: `git commit -am "feat(agent-core): register ExitFrontendDesignModeTool in builtin tool registry"`

---

### Task 6: Add `ExitFrontendDesignModeTool` tests

**Depends on:** Task 5

**Files:**
- **Create:** `packages/agent-core/test/tools/exit-frontend-design-mode.test.ts`

Pattern the test after `packages/agent-core/test/tools/exit-plan-mode.test.ts`.

- [ ] Create `packages/agent-core/test/tools/exit-frontend-design-mode.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import {
  ExitFrontendDesignModeInputSchema,
  ExitFrontendDesignModeTool,
} from '../../src/tools/builtin/planning/exit-frontend-design-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

function makeAgent(
  input: {
    readonly active?: boolean | undefined;
    readonly kind?: 'plan' | 'design' | 'frontend-design' | undefined;
    readonly plan?: string | null | undefined;
    readonly path?: string | undefined;
    readonly sessionModeFilePath?: string | null | undefined;
    readonly emit?: ((event: unknown) => void) | undefined;
  } = {},
): { agent: Agent; requestApproval: ReturnType<typeof vi.fn>; emit: ReturnType<typeof vi.fn> } {
  let active = input.active ?? true;
  const kind = input.kind ?? 'frontend-design';
  const requestApproval = vi.fn(async () => ({ decision: 'approved' }));
  const emit = vi.fn((event: unknown) => {
    input.emit?.(event);
    if ((event as { type?: string }).type === 'session_mode.exit') active = false;
  });
  const agent = {
    sessionMode: {
      get isActive() {
        return active;
      },
      get kind() {
        return kind;
      },
      get sessionModeFilePath() {
        return input.sessionModeFilePath ?? null;
      },
      data: vi.fn(async () => {
        if (input.plan === null) return null;
        return {
          content: input.plan ?? 'Step 1: brief inference\nStep 2: generate code',
          path: input.path ?? '/tmp/kimi-fd.md',
        };
      }),
      finalizeFileName: vi.fn().mockResolvedValue(null),
      exit: () => {
        emit({ type: 'session_mode.exit' });
      },
    },
    rpc: { requestApproval },
    telemetry: { track: vi.fn() },
    emit,
  } as unknown as Agent;
  return { agent, requestApproval, emit };
}

describe('ExitFrontendDesignModeTool', () => {
  it('has name, description, and parameters from the current schema', () => {
    const { agent } = makeAgent();
    const tool = new ExitFrontendDesignModeTool(agent);

    expect(tool.name).toBe('ExitFrontendDesignMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(ExitFrontendDesignModeInputSchema.safeParse({}).success).toBe(true);
    expect(ExitFrontendDesignModeInputSchema.safeParse({ options: [{ label: 'A' }] }).success).toBe(true);
    expect(tool.parameters).toMatchObject({
      type: 'object',
      properties: {
        options: { type: 'array' },
      },
    });
  });

  it('refuses to exit when frontend-design mode is inactive', async () => {
    const { agent, emit } = makeAgent({ active: false });

    const result = await executeTool(new ExitFrontendDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('frontend-design mode');
    expect(emit).not.toHaveBeenCalled();
  });

  it('refuses to exit when in a different mode', async () => {
    const { agent, emit } = makeAgent({ active: true, kind: 'plan' });

    const result = await executeTool(new ExitFrontendDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('Current mode is plan');
    expect(emit).not.toHaveBeenCalled();
  });

  it('exits with the current design without consulting permission approval', async () => {
    const { agent, requestApproval, emit } = makeAgent({
      plan: '# Frontend Design',
      path: '/tmp/kimi-fd.md',
    });

    const result = await executeTool(new ExitFrontendDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result.isError).toBe(false);
    expect(requestApproval).not.toHaveBeenCalled();
    expect(emit).toHaveBeenCalledWith({ type: 'session_mode.exit' });
    expect(result.output).toContain('Design saved to: /tmp/kimi-fd.md');
    expect(result.output).toContain('# Frontend Design');
    expect(result.stopTurn).toBe(true);
  });

  it('does not use inline fallback when no design file exists', async () => {
    const { agent, emit } = makeAgent({ plan: null });

    const result = await executeTool(new ExitFrontendDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_inline',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(emit).not.toHaveBeenCalled();
    expect(result.output).toContain('No design file found');
  });

  it('returns an error when no design content is available', async () => {
    const { agent, emit } = makeAgent({
      plan: '',
      path: '/tmp/kimi-fd.md',
    });

    const result = await executeTool(new ExitFrontendDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_empty',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('Write your design to /tmp/kimi-fd.md first');
    expect(emit).not.toHaveBeenCalled();
  });

  it('surfaces errors from session mode exit as a tool error', async () => {
    const { agent } = makeAgent({
      emit: () => {
        throw new Error('journal write failed');
      },
    });

    const result = await executeTool(new ExitFrontendDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_fail',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('journal write failed');
  });
});
```

- [ ] Run the new test:

```bash
cd packages/agent-core && pnpm test -- test/tools/exit-frontend-design-mode.test.ts
```

- [ ] Run the full agent-core test suite:

```bash
cd packages/agent-core && pnpm test
```

- [ ] Commit: `git add -A && git commit -m "test(agent-core): add ExitFrontendDesignModeTool tests"`

---

## Local Self-Review

- [ ] **1. Spec-coverage table**

| Design Section | Requirement | Task | Status |
|---|---|---|---|
| 3.2 | Refactor guard to mode-aware `SessionModeGuardPermissionPolicy` | Task 1 | covered |
| 3.2 | `frontend-design` mode allows Write/Edit (any file) | Task 1 | covered |
| 3.2 | `frontend-design` mode allows Bash | Task 1 | covered |
| 3.2 | `frontend-design` mode allows TaskStop | Task 1 | covered |
| 3.2 | `frontend-design` mode allows CronCreate/CronDelete | Task 1 | covered |
| 3.2 | `plan`/`design` mode keeps original strict restrictions | Task 1 | covered |
| — | `PlanModeToolApprovePermissionPolicy` handles `EnterFrontendDesignMode` | Task 2 | covered |
| — | `PlanModeToolApprovePermissionPolicy` handles `ExitFrontendDesignMode` | Task 2 | covered |
| — | `ExitPlanModeReviewAskPermissionPolicy` handles `ExitFrontendDesignMode` | Task 3 | covered |
| — | `ExitFrontendDesignMode` exits directly to normal (no handoff to plan) | Task 3, Task 4 | covered |
| — | Telemetry events for frontend-design (`frontend_design_submitted`, etc.) | Task 3, Task 4 | covered |
| — | `ExitFrontendDesignModeTool` created and registered | Task 4, Task 5 | covered |
| — | Tests for guard, approve policy, review-ask policy, and exit tool | Task 1–6 | covered |

- [ ] **2. Placeholder scan:** No TODO/TBD, no deferred-by-dependency excuses. Every file contains real code an engineer can copy-paste.

- [ ] **3. No phantom tasks:** Every task produces a verifiable change (file rename, class refactor, new tool file, test file, registry wiring). The rename in Task 1 is a real git mv + content edit, not a skip.

- [ ] **4. Dependency soundness:** Task 1 depends only on Part 1 (prerequisite). Tasks 2–6 depend on earlier tasks in this part. No forward references.

- [ ] **5. Caller & build soundness:** Task 1 updates every textual reference to the old class name, file path, and policy `name` (verified by `grep -rn` results in the plan). It ends with `pnpm -r typecheck`. The policy `name` string `'session-mode-guard'` is traced to `permission.test.ts` telemetry assertions and the policy chain order test.

- [ ] **6. Test-the-risk:** Task 1 tests that a non-.md Write in `frontend-design` mode is allowed (the core behavioural change). Task 3 tests that `ExitFrontendDesignMode` with a non-empty review is deferred to ask. Task 6 tests that the tool refuses to run when inactive or in the wrong mode.

- [ ] **7. Type consistency:** `SessionModeKind` from Part 1 is used throughout (`'frontend-design'`). `ToolInputDisplay['kind']` remains `'plan_review'` (shared surface). The `ExitFrontendDesignModeInput` schema mirrors `ExitPlanModeInput` structure.
