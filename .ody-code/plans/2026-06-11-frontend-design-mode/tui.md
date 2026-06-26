# Part 5: TUI Integration

> Scope: extend the node-sdk `Session.setSessionMode` RPC surface for `'frontend-design'`, add the `/frontend-design` slash command with its handler and dispatch wiring, update the TUI footer badge for the new mode, create `EnterFrontendDesignModeTool`, register it in the builtin toolset, and add tests.
>
> Depends on: `2026-06-11-frontend-design-mode/core.md` (type unions expanded), `2026-06-11-frontend-design-mode/permission.md` (policies approve the new tool), `2026-06-11-frontend-design-mode/injection.md` (injector exists), `2026-06-11-frontend-design-mode/skill.md` (skill registered).

---

### Task 1: Extend `Session.setSessionMode` and RPC types in `node-sdk`

**Depends on:** `2026-06-11-frontend-design-mode/core.md`: Task 1 (type unions expanded)

**Files:**
- **Modify:** `packages/node-sdk/src/session.ts:148`
- **Modify:** `packages/node-sdk/src/rpc.ts:98`
- **Modify:** `packages/node-sdk/src/kimi-harness.ts:114-118`

This is a shared-signature change across the `node-sdk` package.

- [ ] Expand `SetSessionModeRpcInput.mode` in `packages/node-sdk/src/rpc.ts:98`:

```typescript
export interface SetSessionModeRpcInput extends SessionIdRpcInput {
  readonly mode: 'plan' | 'design' | 'frontend-design' | 'normal';
}
```

- [ ] Expand `Session.setSessionMode` in `packages/node-sdk/src/session.ts:148`:

```typescript
async setSessionMode(mode: 'plan' | 'design' | 'frontend-design' | 'normal'): Promise<void> {
  this.ensureOpen();
  if (mode === 'normal') {
    await this.rpc.setSessionMode({ sessionId: this.id, mode: 'normal' });
    return;
  }
  await this.rpc.setSessionMode({ sessionId: this.id, mode });
}
```

- [ ] Add `frontend-design` to the `createSession` branch in `packages/node-sdk/src/kimi-harness.ts:114`:

```typescript
if (sessionMode === 'plan') {
  await session.setSessionMode('plan');
} else if (sessionMode === 'design') {
  await session.setSessionMode('design');
} else if (sessionMode === 'frontend-design') {
  await session.setSessionMode('frontend-design');
}
```

- [ ] Run node-sdk typecheck:

```bash
cd packages/node-sdk && pnpm typecheck
```

- [ ] Commit: `git commit -am "feat(node-sdk): extend setSessionMode RPC for frontend-design mode"`

---

### Task 2: Add `/frontend-design` slash command

**Depends on:** Task 1

**Files:**
- **Modify:** `apps/ody-code/src/tui/commands/registry.ts`
- **Modify:** `apps/ody-code/src/tui/commands/config.ts`
- **Modify:** `apps/ody-code/src/tui/commands/dispatch.ts`

- [ ] Add the command definition in `apps/ody-code/src/tui/commands/registry.ts`, after the `design` command:

```typescript
{
  name: 'frontend-design',
  aliases: ['fd'],
  description: 'Toggle frontend-design mode (design document + code generation)',
  priority: 100,
  availability: (args) => (args.trim().toLowerCase() === 'clear' ? 'idle-only' : 'always'),
  hiddenInModes: ['frontend-design'],
},
```

- [ ] Add the handler in `apps/ody-code/src/tui/commands/config.ts`, after `applyDesignMode`:

```typescript
export async function handleFrontendDesignCommand(host: SlashCommandHost, args: string): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const subcmd = args.trim().toLowerCase();
  if (subcmd === 'clear') {
    await session.clearPlan();
    host.showNotice('Frontend design cleared');
    return;
  }

  let enabled: boolean;
  if (subcmd.length === 0) enabled = host.state.appState.sessionMode !== 'frontend-design';
  else if (subcmd === 'on') enabled = true;
  else if (subcmd === 'off') enabled = false;
  else {
    host.showError(`Unknown frontend-design subcommand: ${subcmd}`);
    return;
  }

  await applyFrontendDesignMode(host, session, enabled);
}

async function applyFrontendDesignMode(host: SlashCommandHost, session: Session, enabled: boolean): Promise<void> {
  try {
    await session.setSessionMode(enabled ? 'frontend-design' : 'normal');
    host.setAppState({ sessionMode: enabled ? 'frontend-design' : 'normal' });
    if (enabled) {
      const plan = await session.getPlan().catch(() => null);
      log.debug('Mode toggled', { mode: 'frontend-design', enabled, sessionModeFilePath: plan?.path ?? null });
      host.showNotice('Frontend-design mode: ON');
      return;
    }
    log.debug('Mode toggled', { mode: 'frontend-design', enabled });
    host.showNotice('Frontend-design mode: OFF');
  } catch (error) {
    const msg = formatErrorMessage(error);
    host.showError(`Failed to set frontend-design mode: ${msg}`);
  }
}
```

Also export `handleFrontendDesignCommand` from `config.ts` and re-export it from `dispatch.ts`.

- [ ] Wire the dispatch in `apps/ody-code/src/tui/commands/dispatch.ts`:

```typescript
import {
  handleAutoCommand,
  handleCompactCommand,
  handleDesignCommand,
  handleEditorCommand,
  handleFrontendDesignCommand,
  handleModelCommand,
  handlePlanCommand,
  // ...
} from './config';
```

```typescript
export {
  // ... existing exports
  handleFrontendDesignCommand,
} from './config';
```

```typescript
case 'frontend-design':
  await handleFrontendDesignCommand(host, args);
  return;
```

- [ ] Build the ody-code app to verify the TUI compiles:

```bash
cd apps/ody-code && pnpm typecheck
```

- [ ] Commit: `git commit -am "feat(ody-code): add /frontend-design slash command and handler"`

---

### Task 3: Update TUI footer badge for `frontend-design` mode

**Depends on:** Task 2

**Files:**
- **Modify:** `apps/ody-code/src/tui/components/chrome/footer.ts:29,48-68,398-400`

- [ ] Add the emoji in `apps/ody-code/src/tui/components/chrome/footer.ts:29`:

```typescript
const EMOJIS: Record<string, string> = { normal: '⚒️', plan: '📝', design: '✏️', 'frontend-design': '🎨' };
```

- [ ] Update `renderModeBadge` to accept the new mode and add a distinct colour for `frontend-design`. In `apps/ody-code/src/tui/components/chrome/footer.ts`, change the function signature and colour logic:

```typescript
function renderModeBadge(
  mode: 'normal' | 'plan' | 'design' | 'frontend-design',
  colors: ColorPalette,
  fileName?: string,
): string {
  const emoji = EMOJIS[mode] ?? '';
  const bgColor =
    mode === 'frontend-design'
      ? colors.warning
      : mode === 'design'
        ? colors.accent
        : mode === 'plan'
          ? colors.primary
          : colors.textMuted;

  let textColor: string;
  try {
    textColor = luminance(bgColor) > 0.5 ? '#000000' : '#ffffff';
  } catch {
    textColor = '#ffffff';
  }

  const label = fileName ? `${emoji} ${mode} · ${fileName}` : `${emoji} ${mode}`;
  const padded = ` ${label} `;

  return chalk.bgHex(bgColor).hex(textColor)(`【${padded}】`);
}
```

- [ ] Update the call site around line 398-400. The `mode` variable already comes from `state.sessionMode` which is typed as `'normal' | 'plan' | 'design' | 'frontend-design'` after Part 1. The `fileName` logic currently reads:

```typescript
const fileName = mode === 'normal' ? planFileName(state.sessionModeFilePath) : null;
```

Change it to show the filename for `frontend-design` as well (the DESIGN.md path):

```typescript
const fileName = mode === 'normal' || mode === 'frontend-design' ? planFileName(state.sessionModeFilePath) : null;
```

- [ ] Build the ody-code app:

```bash
cd apps/ody-code && pnpm typecheck
```

- [ ] Commit: `git commit -am "feat(ody-code): add frontend-design badge to TUI footer"`

---

### Task 4: Create `EnterFrontendDesignModeTool`

**Depends on:** Task 2

**Files:**
- **Create:** `packages/agent-core/src/tools/builtin/planning/enter-frontend-design-mode.ts`
- **Create:** `packages/agent-core/src/tools/builtin/planning/enter-frontend-design-mode.md`
- **Modify:** `packages/agent-core/src/tools/builtin/index.ts`
- **Modify:** `packages/agent-core/src/agent/tool/index.ts` (register in `initializeBuiltinTools`)

- [ ] Create the description markdown `packages/agent-core/src/tools/builtin/planning/enter-frontend-design-mode.md`:

```markdown
Use this tool when the user's request involves frontend design, website creation, landing pages, UI/UX work, or web app interfaces.

Frontend-design mode is a specialized workflow for complete frontend design and code generation. It combines design document creation (DESIGN.md) with runnable code generation in a single session.

## When to Use
Use it when ANY of these apply:

1. **Website / Landing page** — e.g. "Build a SaaS landing page"
2. **Portfolio** — e.g. "Create my design portfolio website"
3. **Web app UI** — e.g. "Build a dashboard interface"
4. **Frontend component** — e.g. "Create a reusable modal component"
5. **UI/UX redesign** — e.g. "Redesign my existing website"

## What Happens in Frontend-Design Mode
1. Brief Inference — produce a one-line Design Read from the user's request.
2. Three Dials — set VARIANCE / MOTION / DENSITY.
3. Design System Map — choose the right stack (React, Vue, Svelte, etc.).
4. Confirm with User — new or existing project, tech stack, output directory.
5. Appendix Selection — present available appendices with recommendations.
6. DESIGN.md — write the design document to `.ody-code/frontend-designs/`.
7. Code Generation — generate complete, runnable frontend code.
8. Dependency Install — run npm install / npx commands.
9. Dev Server — optionally run `npm run dev` for live preview.
10. Pre-flight Check — run all 40+ checks before declaring done.

Permission mode notes:
- EnterFrontendDesignMode enters the mode automatically without an approval prompt in all permission modes.
- In yolo and manual modes, ExitFrontendDesignMode presents the design to the user for approval.
- In auto permission mode, ExitFrontendDesignMode exits without asking the user.
```

- [ ] Create the tool implementation `packages/agent-core/src/tools/builtin/planning/enter-frontend-design-mode.ts`:

```typescript
/**
 * EnterFrontendDesignModeTool — frontend-design mode entry tool.
 *
 * The LLM calls this tool to enter frontend-design mode for complete
 * frontend design and code generation workflows.
 */

import type { Agent } from '#/agent';
import { z } from 'zod';

import { frontendDesignEntryMessage } from '../../../agent/injection/frontend-design-mode-contract';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-frontend-design-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export const EnterFrontendDesignModeInputSchema = z.object({}).strict();
export type EnterFrontendDesignModeInput = z.infer<typeof EnterFrontendDesignModeInputSchema>;

export class EnterFrontendDesignModeTool implements BuiltinTool<EnterFrontendDesignModeInput> {
  readonly name = 'EnterFrontendDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterFrontendDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterFrontendDesignModeInput): ToolExecution {
    return {
      description: 'Requesting to enter frontend-design mode',
      approvalRule: this.name,
      execute: async () => {
        if (this.agent.sessionMode.isActive) {
          const active =
            this.agent.sessionMode.kind === 'frontend-design'
              ? 'Frontend-design'
              : this.agent.sessionMode.kind === 'design'
                ? 'Design'
                : 'Plan';
          return {
            isError: true,
            output: `${active} mode is already active. Use ExitFrontendDesignMode when done, or exit first.`,
          };
        }

        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'frontend-design');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter frontend-design mode.';
          return { isError: true, output: `Failed to enter frontend-design mode: ${message}` };
        }

        this.agent.telemetry.track('frontend_design_enter_resolved', { outcome: 'auto_approved' });
        return {
          output: frontendDesignEntryMessage(this.agent.sessionMode.sessionModeFilePath),
        };
      },
    };
  }
}
```

- [ ] Export the tool in `packages/agent-core/src/tools/builtin/index.ts`:

```typescript
export * from './planning/enter-frontend-design-mode';
```

- [ ] Register the tool in `packages/agent-core/src/agent/tool/index.ts` inside `initializeBuiltinTools()`, after `EnterDesignModeTool`:

```typescript
new b.EnterDesignModeTool(this.agent),
new b.EnterFrontendDesignModeTool(this.agent),
new b.ExitDesignModeTool(this.agent),
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck
```

- [ ] Commit: `git add -A && git commit -m "feat(agent-core): add EnterFrontendDesignModeTool"`

---

### Task 5: Add `EnterFrontendDesignModeTool` test

**Depends on:** Task 4

**Files:**
- **Create:** `packages/agent-core/test/tools/enter-frontend-design-mode.test.ts`

- [ ] Create `packages/agent-core/test/tools/enter-frontend-design-mode.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import {
  EnterFrontendDesignModeInputSchema,
  EnterFrontendDesignModeTool,
} from '../../src/tools/builtin/planning/enter-frontend-design-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

function makeAgent(
  input: {
    readonly active?: boolean | undefined;
    readonly kind?: 'plan' | 'design' | 'frontend-design' | undefined;
  } = {},
): { agent: Agent; emit: ReturnType<typeof vi.fn> } {
  const emit = vi.fn();
  const agent = {
    sessionMode: {
      get isActive() {
        return input.active ?? false;
      },
      get kind() {
        return input.kind ?? 'plan';
      },
      get sessionModeFilePath() {
        return '/tmp/fd.md';
      },
      enter: vi.fn().mockResolvedValue(undefined),
    },
    telemetry: { track: vi.fn() },
    emit,
  } as unknown as Agent;
  return { agent, emit };
}

describe('EnterFrontendDesignModeTool', () => {
  it('has name, description, and empty parameter schema', () => {
    const { agent } = makeAgent();
    const tool = new EnterFrontendDesignModeTool(agent);

    expect(tool.name).toBe('EnterFrontendDesignMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(EnterFrontendDesignModeInputSchema.safeParse({}).success).toBe(true);
  });

  it('enters frontend-design mode when inactive', async () => {
    const { agent } = makeAgent({ active: false });
    const tool = new EnterFrontendDesignModeTool(agent);

    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result.isError).toBe(false);
    expect(agent.sessionMode.enter).toHaveBeenCalledWith(undefined, undefined, undefined, 'frontend-design');
    expect(result.output).toContain('Frontend-design mode is now active');
  });

  it('refuses to enter when already in frontend-design mode', async () => {
    const { agent } = makeAgent({ active: true, kind: 'frontend-design' });
    const tool = new EnterFrontendDesignModeTool(agent);

    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('already active');
    expect(agent.sessionMode.enter).not.toHaveBeenCalled();
  });

  it('refuses to enter when in plan mode', async () => {
    const { agent } = makeAgent({ active: true, kind: 'plan' });
    const tool = new EnterFrontendDesignModeTool(agent);

    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('Plan mode is already active');
  });

  it('surfaces enter errors as tool errors', async () => {
    const { agent } = makeAgent({ active: false });
    (agent.sessionMode.enter as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('disk full'));
    const tool = new EnterFrontendDesignModeTool(agent);

    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('disk full');
  });
});
```

- [ ] Run the test:

```bash
cd packages/agent-core && pnpm test -- test/tools/enter-frontend-design-mode.test.ts
```

- [ ] Commit: `git add -A && git commit -m "test(agent-core): add EnterFrontendDesignModeTool tests"`

---

### Task 6: Whole-tree verification

**Depends on:** Task 5

**Files:** none (verification-only)

- [ ] Run the full agent-core test suite:

```bash
cd packages/agent-core && pnpm test
```

- [ ] Run the ody-code typecheck:

```bash
cd apps/ody-code && pnpm typecheck
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck
```

- [ ] Commit: `git commit --allow-empty -m "chore: verify whole tree after TUI integration"`

---

## Local Self-Review

- [ ] **1. Spec-coverage table**

| Design Section | Requirement | Task | Status |
|---|---|---|---|
| 4.1 | `/frontend-design` slash command | Task 2 | covered |
| 4.1 | `aliases: ['fd']` | Task 2 | covered |
| 4.1 | `hiddenInModes: ['frontend-design']` | Task 2 | covered |
| 4.2 | `EnterFrontendDesignModeTool` | Task 4 | covered |
| 4.4 | TUI footer badge for `frontend-design` | Task 3 | covered |
| 4.4 | Emoji `🎨` for frontend-design | Task 3 | covered |
| 4.4 | Distinct badge colour (`colors.warning`) | Task 3 | covered |
| — | `node-sdk` `setSessionMode` accepts `'frontend-design'` | Task 1 | covered |
| — | `kimi-harness` creates sessions in `frontend-design` mode | Task 1 | covered |
| — | Tool registered in `ToolManager` and `builtin/index.ts` | Task 4 | covered |
| — | Tests for entry tool | Task 5 | covered |

- [ ] **2. Placeholder scan:** No TODO/TBD. Every handler, badge, and tool contains real code.

- [ ] **3. No phantom tasks:** Every task creates/modifies files and ends with tests + commit.

- [ ] **4. Dependency soundness:** Task 1 depends on Part 1 (types already expanded). Tasks 2–6 depend on earlier tasks in this part. No forward references.

- [ ] **5. Caller & build soundness:** Task 1 updates `Session.setSessionMode`, `SetSessionModeRpcInput`, and `kimi-harness.createSession`; all callers pass literal strings and remain valid. Task 3 updates `renderModeBadge` signature and the call site that passes `mode` from `state.sessionMode`; the consumer (`renderModeBadge`) and the producer (`AppState.sessionMode`) both include `'frontend-design'` after Part 1. Task 6 ends with `pnpm -r typecheck`.

- [ ] **6. Test-the-risk:** Task 5 tests that the tool refuses to enter when already active (state-mutation guard). Task 5 tests that errors from `sessionMode.enter` are surfaced as tool errors.

- [ ] **7. Type consistency:** `SessionModeKind` from Part 1 is used throughout (`'frontend-design'`). `EnterFrontendDesignModeTool` mirrors `EnterDesignModeTool` structure. `handleFrontendDesignCommand` mirrors `handleDesignCommand` structure.
