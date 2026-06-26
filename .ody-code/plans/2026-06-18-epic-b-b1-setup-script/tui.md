# Part 2: apps/ody-code — /setup TUI Slash Command + RPC Pipeline

**Depends on:** `2026-06-18-epic-b-b1-setup-script/core.md: Task 4` (Session.createMain wiring must exist)

---

## Task 6: /setup slash command — full RPC pipeline + TUI handler + registry + dispatch

**Depends on:** `core.md: Task 4`  
**Files:**
- Modify `packages/agent-core/src/rpc/core-api.ts:419` (add `runSetupScript` to CoreAPI)
- Modify `packages/agent-core/src/rpc/core-impl.ts:776` (implement `runSetupScript`)
- Modify `packages/node-sdk/src/rpc.ts:255` (proxy `runSetupScript`)
- Modify `packages/node-sdk/src/session.ts:107` (add `setup()` method)
- Create `apps/ody-code/src/tui/commands/setup.ts`
- Modify `apps/ody-code/src/tui/commands/registry.ts` (register `/setup`)
- Modify `apps/ody-code/src/tui/commands/dispatch.ts` (add case + import)
- Modify `apps/ody-code/src/tui/commands/session.ts` (re-export handler)

### Design

Follow the existing `/init` RPC pipeline pattern exactly:
- `core-api.ts`: define `runSetupScript` method on `CoreAPI`
- `core-impl.ts`: implement — looks up session, gets main agent, calls `runSetupScriptIfNeeded(force: true)`
- `node-sdk/rpc.ts`: proxy to core RPC
- `node-sdk/session.ts`: expose `setup()` convenience method
- TUI: handler calls `session.setup()`, shows result as status

### Steps

- [ ] Step 1: Add `runSetupScript` to core-api.ts

Modify `packages/agent-core/src/rpc/core-api.ts` — add method to `CoreAPI` interface (after the `generateAgentsMd` line ~419):

```typescript
  generateAgentsMd: (payload: EmptyPayload) => void;
  /** Run the repository setup script (.ody-code/setup.sh) with force=true. */
  runSetupScript: (payload: EmptyPayload) => void;
```

- [ ] Step 2: Implement `runSetupScript` in core-impl.ts

In `packages/agent-core/src/rpc/core-impl.ts`, first add the import at the top of the file (near other session imports):

```typescript
import { runSetupScriptIfNeeded } from '../session/setup-script';
```

Then add the implementation method after `generateAgentsMd` (~line 776):

```typescript
  generateAgentsMd({ sessionId, ...payload }: SessionScopedPayload<EmptyPayload>): Promise<void> {
    return this.sessionApi(sessionId).generateAgentsMd(payload);
  }

  async runSetupScript({ sessionId, ...payload }: SessionScopedPayload<EmptyPayload>): Promise<void> {
    const session = this.getSession(sessionId);
    const mainAgent = session.agents.get('main');
    if (mainAgent === undefined) {
      throw new OdyError(ErrorCodes.AGENT_NOT_FOUND, 'Main agent was not found');
    }
    await runSetupScriptIfNeeded(session, mainAgent, { force: true });
  }
```

- [ ] Step 3: Proxy in node-sdk rpc.ts

Modify `packages/node-sdk/src/rpc.ts` — add `runSetupScript` to `SDKSessionRPC` proxy (after `generateAgentsMd` around line 255):

```typescript
  async generateAgentsMd(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.generateAgentsMd({ sessionId: input.sessionId });
  }

  async runSetupScript(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.runSetupScript({ sessionId: input.sessionId });
  }
```

- [ ] Step 4: Add `setup()` method to node-sdk Session

Modify `packages/node-sdk/src/session.ts` — add `setup()` convenience method after `init()` (~line 109):

```typescript
  async init(): Promise<void> {
    this.ensureOpen();
    await this.rpc.generateAgentsMd({ sessionId: this.id });
  }

  /** Manually run the repository setup script, even if it already ran at startup. */
  async setup(): Promise<void> {
    this.ensureOpen();
    await this.rpc.runSetupScript({ sessionId: this.id });
  }
```

- [ ] Run compile check for all modified packages

```bash
# Typecheck agent-core first
pnpm --filter @odysseythink/agent-core typecheck
# Expected: PASS

# Typecheck node-sdk
pnpm --filter @odysseythink/node-sdk typecheck
# Expected: PASS (or check build since typecheck script may differ)
```

- [ ] Step 5: Create TUI `/setup` handler

Create `apps/ody-code/src/tui/commands/setup.ts`:

```typescript
import type { SlashCommandHost } from './dispatch';
import { LLM_NOT_SET_MESSAGE } from '../constant/ody-tui';
import { isAbortError } from '../utils/errors';
import { formatErrorMessage } from '../utils/event-payload';

export async function handleSetupCommand(host: SlashCommandHost): Promise<void> {
  const session = host.session;
  if (host.state.appState.model.trim().length === 0 || session === undefined) {
    host.showError(LLM_NOT_SET_MESSAGE);
    return;
  }

  host.deferUserMessages = true;
  host.beginSessionRequest();
  try {
    await session.setup();
    host.track('setup_script_manual');
    host.showStatus('Setup script completed. Check the agent response for details.');
    host.streamingUI.finalizeTurn((item) => {
      host.sendQueuedMessage(session, item);
    });
  } catch (error) {
    if (isAbortError(error)) {
      host.setAppState({ streamingPhase: 'idle' });
      host.resetLivePane();
      return;
    }
    const msg = error instanceof Error ? error.message : String(error);
    host.failSessionRequest(`Setup script failed: ${msg}`);
  } finally {
    host.deferUserMessages = false;
  }
}
```

- [ ] Step 6: Register `/setup` in BUILTIN_SLASH_COMMANDS

Modify `apps/ody-code/src/tui/commands/registry.ts` — add entry after the `init` command block (~line 220):

Find the `init` block (approximately lines 233-239):

```typescript
  {
    name: 'init',
    aliases: [],
    description: 'Analyze the codebase and create/update AGENTS.md',
    priority: 80,
    availability: 'idle-only',
    hiddenInModes: SPECIAL_MODE_HIDDEN,
  },
```

Add after it:

```typescript
  {
    name: 'setup',
    aliases: [],
    description: 'Manually run the repository setup script',
    priority: 70,
    availability: 'idle-only',
    hiddenInModes: SPECIAL_MODE_HIDDEN,
  },
```

- [ ] Step 7: Add dispatch case

Modify `apps/ody-code/src/tui/commands/dispatch.ts`:

Add import in the imports section (after the `init` import from session ~line 50):

```typescript
import {
  handleExportDebugZipCommand,
  handleExportMdCommand,
  handleForkCommand,
  handleInitCommand,
  handleTitleCommand,
} from './session';
import { handleSetupCommand } from './setup';
```

Add case in `handleBuiltInSlashCommand` switch (after `case 'init':` ~line 307):

```typescript
    case 'init':
      await handleInitCommand(host);
      return;
    case 'setup':
      await handleSetupCommand(host);
      return;
```

- [ ] Step 8: Re-export from session.ts

Modify `apps/ody-code/src/tui/commands/session.ts` — add re-export:

```typescript
export { handleSetupCommand } from './setup';
```

- [ ] Step 9: Build verification

```bash
# Typecheck apps/ody-code
cd apps/ody-code && npx tsc --noEmit 2>&1 | head -30
# Expected: no errors related to setup command

# Or use the package's typecheck script
pnpm --filter @odysseythink/ody-code typecheck
# Expected: PASS
```

- [ ] Commit

```bash
git add packages/agent-core/src/rpc/core-api.ts packages/agent-core/src/rpc/core-impl.ts \
        packages/node-sdk/src/rpc.ts packages/node-sdk/src/session.ts \
        apps/ody-code/src/tui/commands/setup.ts \
        apps/ody-code/src/tui/commands/registry.ts \
        apps/ody-code/src/tui/commands/dispatch.ts \
        apps/ody-code/src/tui/commands/session.ts
git commit -m "feat: add /setup TUI slash command with full RPC pipeline"
```

---

## Part 2 Self-Review

- [x] 1. Spec-coverage table (TUI scope): T6 covers `/setup` slash command registration (7.3) + RPC pipeline. All TUI spec items are covered.
- [x] 2. Placeholder scan: no TODO/TBD — the implementation follows the exact `/init` pattern with concrete line numbers.
- [x] 3. No phantom tasks: T6 creates a real file (`setup.ts`), modifies 4 existing files, and ends with a commit. Zero `--allow-empty`.
- [x] 4. Dependency soundness: T6 depends on core.md:Task 4 (which defines `runSetupScriptIfNeeded` and wires it into `Session.createMain()`). The RPC implementation in core-impl.ts imports `runSetupScriptIfNeeded` from `../session/setup-script` — this file exists after T1-T2 of core.md. No forward references.
- [x] 5. Caller & build soundness: T6 adds to `CoreAPI` interface (shared signature). It must update `core-impl.ts` (implements the method) and `node-sdk/rpc.ts` (proxies it), plus `node-sdk/session.ts` (wraps it). All callers are updated within this single task. Step 9 verifies `pnpm --filter @odysseythink/ody-code typecheck` which covers all transitive dependencies.
- [x] 6. Test-the-risk: TUI commands are non-testable code (UI wiring). The verification step is a manual typecheck + visual confirmation. The RPC pipeline is type-safe — verified by `pnpm -r typecheck` in step 9. The actual runtime behavior (setup script execution) is tested in core.md tasks (T2 behavioral tests with mocked Kaos).
- [x] 7. Type consistency: `runSetupScriptIfNeeded` signature used in `core-impl.ts` matches what core.md T2 defines. The `EmptyPayload` type matches existing pattern from `generateAgentsMd`. The node-sdk `Session.setup()` method matches the existing `init()` pattern exactly.
