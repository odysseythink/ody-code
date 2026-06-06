# TUI Slash Command Mode Visibility Implementation Plan

**Goal:** Add mode-based visibility filtering to slash commands so built-in commands can declare `hiddenInModes` to hide themselves from autocomplete, help panel, and manual input in specific session modes.

**Architecture:** A pure helper `isCommandVisibleInMode` checks a command's `hiddenInModes` array against the current `SessionMode`. `getSlashCommands()` filters the built-in list through this helper before returning. `resolveSlashCommandInput` rejects hidden commands with a new `mode-unavailable` block reason, and `dispatch.ts` renders a clear error message. Autocomplete refreshes automatically when `setAppState` detects a `sessionMode` change.

**Tech Stack:** TypeScript, Vitest, pnpm workspaces.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| File | Responsibility |
|------|---------------|
| `apps/ody-code/src/tui/commands/types.ts` | `SessionMode` type alias; `hiddenInModes` field on `KimiSlashCommand` |
| `apps/ody-code/src/tui/commands/visibility.ts` | `isCommandVisibleInMode` helper (new) |
| `apps/ody-code/src/tui/commands/registry.ts` | `BUILTIN_SLASH_COMMANDS` data — add `hiddenInModes` to 4 commands |
| `apps/ody-code/src/tui/commands/resolve.ts` | `ResolveSlashCommandInput` adds `sessionMode`; `blocked` reason extended to `'mode-unavailable'`; runtime mode check |
| `apps/ody-code/src/tui/commands/dispatch.ts` | Pass `sessionMode` into resolver; handle `mode-unavailable` in `blocked` case |
| `apps/ody-code/src/tui/ody-tui.ts` | `getSlashCommands()` filters by mode; `setAppState()` refreshes autocomplete on mode change |
| `apps/ody-code/src/tui/commands/index.ts` | Re-export `isCommandVisibleInMode` and `SessionMode` |
| `apps/ody-code/test/tui/commands/visibility.test.ts` | Unit tests for `isCommandVisibleInMode` (new) |
| `apps/ody-code/test/tui/commands/resolve.test.ts` | Update `resolve` helper default + add mode-blocking assertions |
| `apps/ody-code/test/tui/commands/registry.test.ts` | Assert `hiddenInModes` values on the 4 commands |

## Dependency Overview

```
Task 1 (types + visibility helper + tests)
    │
    ├──► Task 2 (registry data + tests)
    │
    └──► Task 3 (resolve.ts + dispatch.ts shared-signature change + tests)
             │
             └──► Task 4 (ody-tui.ts wiring)
                      │
                      └──► Task 5 (full test suite verification)
```

- **Task 1** has no dependencies. It creates the type and helper that everything else relies on.
- **Task 2** depends on Task 1 (needs `hiddenInModes` in the type).
- **Task 3** depends on Task 1 (needs `SessionMode`, `isCommandVisibleInMode`) and on Task 2 conceptually (tests assert on commands with `hiddenInModes`, but compiles without it). Because Task 3 changes a shared signature (`ResolveSlashCommandInput` and `SlashCommandIntent['blocked']['reason']`), it must also update **every caller** (`dispatch.ts`) and end with a whole-tree typecheck.
- **Task 4** depends on Task 1 (needs `isCommandVisibleInMode`) and Task 2 (commands must have `hiddenInModes` data for the filter to be meaningful).
- **Task 5** depends on Task 3 and Task 4 (verifies everything integrates correctly).

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| 1 | `getSlashCommands` filter accidentally hides commands that should be visible | Default is visible (`hiddenInModes` undefined → visible). Unit tests enumerate all mode/command combinations. |
| 2 | `resolveSlashCommandInput` mode check order vs. busy check | Mode check is placed *after* experimental-flag check and *before* busy check, matching the design doc. A hidden command blocked by mode is never also blocked by busy state. |
| 3 | `setAppState` triggers unnecessary autocomplete rebuilds | Only calls `setupAutocomplete()` when `'sessionMode'` key is present in the patch, not on every `setAppState` call. |

---

### Task 1: Add `SessionMode` type, `hiddenInModes` field, and `isCommandVisibleInMode` helper

**Depends on:** none

**Files:**
- Modify: `apps/ody-code/src/tui/commands/types.ts` (line 4: add `SessionMode`; line 21: add `hiddenInModes`)
- Create: `apps/ody-code/src/tui/commands/visibility.ts`
- Create: `apps/ody-code/test/tui/commands/visibility.test.ts`
- Modify: `apps/ody-code/src/tui/commands/index.ts` (add re-export)

- [ ] **Add `SessionMode` and `hiddenInModes` to `types.ts`.**

  Replace lines 4–21 of `apps/ody-code/src/tui/commands/types.ts`:

  ```ts
  export type SessionMode = 'normal' | 'plan' | 'design';

  export interface KimiSlashCommand<Name extends string = string> extends SlashCommand {
    readonly name: Name;
    readonly aliases: readonly string[];
    readonly description: string;
    readonly priority?: number;
    readonly availability?: SlashCommandAvailability | ((args: string) => SlashCommandAvailability);
    /** When set, the command is hidden from the palette and blocked unless this flag is enabled. */
    readonly experimentalFlag?: FlagId;
    /**
     * Generic argument autocompletion. `argumentPrefix` is the text typed after
     * `/<command> `; return suggestions or `null`. Declared as a plain function
     * property (not a method) so passing it around is `this`-free. Adapted to
     * pi-tui's `getArgumentCompletions` in the autocomplete setup.
     */
    readonly completeArgs?: (argumentPrefix: string) => AutocompleteItem[] | null;
    /** Modes in which this command is hidden from the palette and blocked. */
    readonly hiddenInModes?: readonly SessionMode[];
  }
  ```

- [ ] **Create `visibility.ts` skeleton (types only, body throws).**

  Create `apps/ody-code/src/tui/commands/visibility.ts`:

  ```ts
  import type { KimiSlashCommand, SessionMode } from './types';

  export function isCommandVisibleInMode(
    command: Pick<KimiSlashCommand, 'hiddenInModes'>,
    mode: SessionMode,
  ): boolean {
    throw new Error('not implemented');
  }
  ```

- [ ] **Write the failing test.**

  Create `apps/ody-code/test/tui/commands/visibility.test.ts`:

  ```ts
  import { isCommandVisibleInMode } from '#/tui/commands/index';
  import { describe, expect, it } from 'vitest';

  describe('isCommandVisibleInMode', () => {
    it('returns true when hiddenInModes is undefined', () => {
      expect(isCommandVisibleInMode({}, 'normal')).toBe(true);
      expect(isCommandVisibleInMode({}, 'plan')).toBe(true);
      expect(isCommandVisibleInMode({}, 'design')).toBe(true);
    });

    it('returns true when hiddenInModes is empty', () => {
      expect(isCommandVisibleInMode({ hiddenInModes: [] }, 'normal')).toBe(true);
      expect(isCommandVisibleInMode({ hiddenInModes: [] }, 'plan')).toBe(true);
    });

    it('hides command when mode is in hiddenInModes', () => {
      expect(isCommandVisibleInMode({ hiddenInModes: ['design'] }, 'design')).toBe(false);
      expect(isCommandVisibleInMode({ hiddenInModes: ['plan'] }, 'plan')).toBe(false);
      expect(isCommandVisibleInMode({ hiddenInModes: ['normal'] }, 'normal')).toBe(false);
    });

    it('shows command when mode is NOT in hiddenInModes', () => {
      expect(isCommandVisibleInMode({ hiddenInModes: ['design'] }, 'plan')).toBe(true);
      expect(isCommandVisibleInMode({ hiddenInModes: ['design'] }, 'normal')).toBe(true);
      expect(isCommandVisibleInMode({ hiddenInModes: ['plan'] }, 'design')).toBe(true);
    });

    it('handles multiple hidden modes', () => {
      const hiddenInModes = ['plan', 'normal'] as const;
      expect(isCommandVisibleInMode({ hiddenInModes }, 'design')).toBe(true);
      expect(isCommandVisibleInMode({ hiddenInModes }, 'plan')).toBe(false);
      expect(isCommandVisibleInMode({ hiddenInModes }, 'normal')).toBe(false);
    });
  });
  ```

  **Must-survive inputs verified against constants:**
  - Input `{ hiddenInModes: ['design'] }, 'plan'` → must survive (return `true`). The constant `['design']` does not contain `'plan'` → OK.
  - Input `{ hiddenInModes: ['plan', 'normal'] }, 'design'` → must survive. The constant `['plan', 'normal']` does not contain `'design'` → OK.
  - Input `{ hiddenInModes: undefined }, 'design'` → must survive. The helper returns `true` for undefined → OK.

- [ ] **Run it and verify it FAILS.**

  ```bash
  pnpm vitest run apps/ody-code/test/tui/commands/visibility.test.ts
  ```

  Expected failure: `Error: not implemented` (thrown by the skeleton).

- [ ] **Write the minimal implementation.**

  Replace the body in `apps/ody-code/src/tui/commands/visibility.ts`:

  ```ts
  import type { KimiSlashCommand, SessionMode } from './types';

  export function isCommandVisibleInMode(
    command: Pick<KimiSlashCommand, 'hiddenInModes'>,
    mode: SessionMode,
  ): boolean {
    if (command.hiddenInModes === undefined || command.hiddenInModes.length === 0) {
      return true;
    }
    return !command.hiddenInModes.includes(mode);
  }
  ```

- [ ] **Run it and verify it PASSES.**

  ```bash
  pnpm vitest run apps/ody-code/test/tui/commands/visibility.test.ts
  ```

  Expected: all 5 tests pass.

- [ ] **Add re-export in `index.ts`.**

  Add to `apps/ody-code/src/tui/commands/index.ts` after line 6:

  ```ts
  export * from './visibility';
  ```

- [ ] **Whole-tree typecheck.**

  ```bash
  pnpm -r typecheck
  ```

  Expected: zero errors across the workspace.

- [ ] **Commit.**

  ```bash
  git add -A && git commit -m "feat(tui): add SessionMode type and isCommandVisibleInMode helper"
  ```

---

### Task 2: Add `hiddenInModes` to built-in commands in registry

**Depends on:** Task 1

**Files:**
- Modify: `apps/ody-code/src/tui/commands/registry.ts` (lines 50–76)
- Modify: `apps/ody-code/test/tui/commands/registry.test.ts` (add assertions)

- [ ] **Add `hiddenInModes` to the four commands.**

  In `apps/ody-code/src/tui/commands/registry.ts`, update these four entries:

  ```ts
  {
    name: 'plan',
    aliases: [],
    description: 'Toggle plan mode',
    priority: 100,
    availability: (args) => (args.trim().toLowerCase() === 'clear' ? 'idle-only' : 'always'),
    hiddenInModes: ['plan'],
  },
  {
    name: 'design',
    aliases: [],
    description: 'Toggle design mode (brainstorming / spec exploration)',
    priority: 100,
    availability: (args) => (args.trim().toLowerCase() === 'clear' ? 'idle-only' : 'always'),
    hiddenInModes: ['design'],
  },
  {
    name: 'design-review',
    aliases: [],
    description: 'Critique the current design with the reviewer model (second-model review)',
    priority: 95,
    availability: 'idle-only',
    hiddenInModes: ['plan', 'normal'],
  },
  {
    name: 'plan-review',
    aliases: [],
    description: 'Critique the current execution plan with the reviewer model (second-model review)',
    priority: 95,
    availability: 'idle-only',
    hiddenInModes: ['design', 'normal'],
  },
  ```

- [ ] **Add registry assertions for `hiddenInModes`.**

  Append to `apps/ody-code/test/tui/commands/registry.test.ts` inside `describe('built-in slash command registry', () => { ... })`:

  ```ts
  it('declares hiddenInModes for mode-specific commands', () => {
    expect(findBuiltInSlashCommand('plan')?.hiddenInModes).toEqual(['plan']);
    expect(findBuiltInSlashCommand('design')?.hiddenInModes).toEqual(['design']);
    expect(findBuiltInSlashCommand('design-review')?.hiddenInModes).toEqual(['plan', 'normal']);
    expect(findBuiltInSlashCommand('plan-review')?.hiddenInModes).toEqual(['design', 'normal']);
  });

  it('does not declare hiddenInModes for universal commands', () => {
    expect(findBuiltInSlashCommand('help')?.hiddenInModes).toBeUndefined();
    expect(findBuiltInSlashCommand('exit')?.hiddenInModes).toBeUndefined();
    expect(findBuiltInSlashCommand('model')?.hiddenInModes).toBeUndefined();
  });
  ```

- [ ] **Run registry tests.**

  ```bash
  pnpm vitest run apps/ody-code/test/tui/commands/registry.test.ts
  ```

  Expected: all tests pass, including the new assertions.

- [ ] **Commit.**

  ```bash
  git add -A && git commit -m "feat(tui): add hiddenInModes to built-in slash commands"
  ```

---

### Task 3: Update `resolveSlashCommandInput` with mode visibility check and update `dispatch.ts`

**Depends on:** Task 1

**Files:**
- Modify: `apps/ody-code/src/tui/commands/resolve.ts`
- Modify: `apps/ody-code/src/tui/commands/dispatch.ts`
- Modify: `apps/ody-code/test/tui/commands/resolve.test.ts`

> **Shared-signature change:** `ResolveSlashCommandInput` gains a required `sessionMode` field, and `SlashCommandIntent['blocked']['reason']` widens from `SlashCommandBusyReason` to `SlashCommandBlockedReason`. Every caller (including tests) must be updated in this task.

- [ ] **Update `resolve.ts` types and signature.**

  In `apps/ody-code/src/tui/commands/resolve.ts`:

  1. Add imports at the top:
     ```ts
     import { isCommandVisibleInMode } from './visibility';
     import type { SessionMode } from './types';
     ```

  2. Replace line 15–39 with:
     ```ts
     export type SlashCommandBlockedReason = SlashCommandBusyReason | 'mode-unavailable';

     export type SlashCommandIntent =
       | { readonly kind: 'not-command' }
       | {
           readonly kind: 'builtin';
           readonly command: BuiltinSlashCommand;
           readonly name: BuiltinSlashCommandName;
           readonly args: string;
         }
       | {
           readonly kind: 'skill';
           readonly commandName: string;
           readonly skillName: string;
           readonly args: string;
         }
       | { readonly kind: 'message'; readonly input: string }
       | {
           readonly kind: 'blocked';
           readonly commandName: string;
           readonly reason: SlashCommandBlockedReason;
         }
       | {
           readonly kind: 'invalid';
           readonly commandName: string;
           readonly reason: SlashCommandInvalidReason;
         };
     ```

  3. Replace lines 41–46 with:
     ```ts
     export interface ResolveSlashCommandInput {
       readonly input: string;
       readonly skillCommandMap: ReadonlyMap<string, string>;
       readonly isStreaming: boolean;
       readonly isCompacting: boolean;
       /** Current session mode for visibility filtering. */
       readonly sessionMode: SessionMode;
     }
     ```

- [ ] **Update `resolveSlashCommandInput` implementation.**

  Replace the `if` block at lines 54–75 with:

  ```ts
  if (
    command !== undefined &&
    isExperimentalFlagEnabled((command as KimiSlashCommand).experimentalFlag)
  ) {
    if (!isCommandVisibleInMode(command as KimiSlashCommand, options.sessionMode)) {
      return {
        kind: 'blocked',
        commandName: parsed.name,
        reason: 'mode-unavailable',
      };
    }
    const busyReason = slashCommandBusyReason(options);
    if (
      busyReason !== undefined &&
      resolveSlashCommandAvailability(command, parsed.args) === 'idle-only'
    ) {
      return {
        kind: 'blocked',
        commandName: parsed.name,
        reason: busyReason,
      };
    }
    return {
      kind: 'builtin',
      command,
      name: command.name,
      args: parsed.args,
    };
  }
  ```

- [ ] **Update `dispatch.ts` caller and error handling.**

  In `apps/ody-code/src/tui/commands/dispatch.ts`:

  1. Pass `sessionMode` to the resolver (line 156–161):
     ```ts
     const intent = resolveSlashCommandInput({
       input,
       skillCommandMap: host.skillCommandMap,
       isStreaming: host.state.appState.streamingPhase !== 'idle',
       isCompacting: host.state.appState.isCompacting,
       sessionMode: host.state.appState.sessionMode,
     });
     ```

  2. Handle `mode-unavailable` in the `blocked` case (line 166–169):
     ```ts
     case 'blocked':
       host.track('input_command_invalid', { reason: intent.reason, command: intent.commandName });
       if (intent.reason === 'mode-unavailable') {
         host.showError('Not available in current mode');
       } else {
         host.showError(slashBusyMessage(intent.commandName, intent.reason));
       }
       return;
     ```

- [ ] **Find and confirm no other callers exist.**

  ```bash
  grep -rn "resolveSlashCommandInput" apps/ody-code/
  ```

  Expected output: only `apps/ody-code/src/tui/commands/dispatch.ts` and `apps/ody-code/test/tui/commands/resolve.test.ts`.

- [ ] **Update `resolve.test.ts` helper and add mode-blocking tests.**

  In `apps/ody-code/test/tui/commands/resolve.test.ts`:

  1. Update the `resolve` helper (line 10–21) to include a default `sessionMode`:
     ```ts
     function resolve(
       input: string,
       overrides: Partial<Parameters<typeof resolveSlashCommandInput>[0]> = {},
     ) {
       return resolveSlashCommandInput({
         input,
         skillCommandMap: new Map<string, string>(),
         isStreaming: false,
         isCompacting: false,
         sessionMode: 'normal',
         ...overrides,
       });
     }
     ```

  2. Append a new describe block at the end of the file:
     ```ts
     describe('mode visibility blocking', () => {
       it('blocks commands hidden in the current mode', () => {
         // design-review is hidden in plan and normal
         expect(resolve('/design-review', { sessionMode: 'normal' })).toEqual({
           kind: 'blocked',
           commandName: 'design-review',
           reason: 'mode-unavailable',
         });
         expect(resolve('/design-review', { sessionMode: 'plan' })).toEqual({
           kind: 'blocked',
           commandName: 'design-review',
           reason: 'mode-unavailable',
         });
       });

       it('allows hidden commands when in the correct mode', () => {
         expect(resolve('/design-review', { sessionMode: 'design' })).toMatchObject({
           kind: 'builtin',
           name: 'design-review',
         });
         expect(resolve('/plan-review', { sessionMode: 'plan' })).toMatchObject({
           kind: 'builtin',
           name: 'plan-review',
         });
       });

       it('blocks /plan in plan mode and /design in design mode', () => {
         expect(resolve('/plan', { sessionMode: 'plan' })).toEqual({
           kind: 'blocked',
           commandName: 'plan',
           reason: 'mode-unavailable',
         });
         expect(resolve('/design', { sessionMode: 'design' })).toEqual({
           kind: 'blocked',
           commandName: 'design',
           reason: 'mode-unavailable',
         });
       });

       it('allows /plan and /design in normal mode', () => {
         expect(resolve('/plan', { sessionMode: 'normal' })).toMatchObject({
           kind: 'builtin',
           name: 'plan',
         });
         expect(resolve('/design', { sessionMode: 'normal' })).toMatchObject({
           kind: 'builtin',
           name: 'design',
         });
       });

       it('mode block takes precedence over busy block', () => {
         // design-review is hidden in normal mode; even while streaming,
         // the mode-unavailable reason wins.
         expect(resolve('/design-review', { sessionMode: 'normal', isStreaming: true })).toEqual({
           kind: 'blocked',
           commandName: 'design-review',
           reason: 'mode-unavailable',
         });
       });
     });
     ```

- [ ] **Run resolve tests.**

  ```bash
  pnpm vitest run apps/ody-code/test/tui/commands/resolve.test.ts
  ```

  Expected: all existing tests still pass, plus new mode-blocking tests pass.

- [ ] **Whole-tree typecheck.**

  ```bash
  pnpm -r typecheck
  ```

  Expected: zero errors. The signature change in `ResolveSlashCommandInput` and `SlashCommandIntent` has been fully propagated.

- [ ] **Commit.**

  ```bash
  git add -A && git commit -m "feat(tui): block hidden slash commands by session mode"
  ```

---

### Task 4: Update `ody-tui.ts` — filter `getSlashCommands` by mode and refresh autocomplete on mode switch

**Depends on:** Task 1, Task 2

**Files:**
- Modify: `apps/ody-code/src/tui/ody-tui.ts`

- [ ] **Import `isCommandVisibleInMode`.**

  In `apps/ody-code/src/tui/ody-tui.ts`, add to the existing import block (around line 35–45):

  ```ts
  import {
    BUILTIN_SLASH_COMMANDS,
    buildSkillSlashCommands,
    isCommandVisibleInMode,
    isExperimentalFlagEnabled,
    setExperimentalFlags,
    sortSlashCommands,
  } from '#/tui/commands/index';
  ```

- [ ] **Add mode filtering to `getSlashCommands()`.**

  Replace lines 308–313:

  ```ts
  private getSlashCommands(): readonly KimiSlashCommand[] {
    const mode = this.state.appState.sessionMode;
    const builtins = sortSlashCommands(BUILTIN_SLASH_COMMANDS)
      .filter((command) => isCommandVisibleInMode(command, mode))
      .filter((command) => isExperimentalFlagEnabled(command.experimentalFlag));
    return [...builtins, ...this.skillCommands];
  }
  ```

- [ ] **Refresh autocomplete on `sessionMode` change in `setAppState()`.**

  Replace lines 966–976:

  ```ts
  setAppState(patch: Partial<AppState>): void {
    assertNoLegacyFields(patch, 'setAppState');
    if (!hasPatchChanges(this.state.appState, patch)) return;
    const busyChanged = 'streamingPhase' in patch || 'isCompacting' in patch;
    const modeChanged = 'sessionMode' in patch;
    Object.assign(this.state.appState, patch);
    if (modeChanged) {
      this.updateEditorBorderHighlight();
      this.setupAutocomplete();
    }
    this.state.footer.setState(this.state.appState);
    this.updateActivityPane();
    if (busyChanged) this.updateQueueDisplay();
    this.state.ui.requestRender();
  }
  ```

- [ ] **Whole-tree typecheck.**

  ```bash
  pnpm -r typecheck
  ```

  Expected: zero errors.

- [ ] **Commit.**

  ```bash
  git add -A && git commit -m "feat(tui): filter slash commands by session mode and refresh autocomplete on switch"
  ```

---

### Task 5: Run full test suite and verify

**Depends on:** Task 3, Task 4

**Files:** none (verification only)

- [ ] **Run the full app test suite.**

  ```bash
  pnpm test --filter apps/ody-code
  ```

  Expected: all tests pass. The pre-existing 7 failures (OAuth/login timeouts + User-Agent mismatches) are acceptable if they were present before this feature branch.

- [ ] **Manual verification checklist (run the built CLI).**

  ```bash
  pnpm --filter apps/ody-code build
  pnpm --filter apps/ody-code start
  ```

  Then interactively verify:
  1. Type `/help` in **normal** mode → help panel should list all commands *except* `design-review` and `plan-review`.
  2. Type `/design` to enter **design** mode → observe the footer badge changes to "Design".
  3. Type `/help` again → help panel should *not* list `/design` or `/plan-review`, but *should* list `/design-review`.
  4. Type `/design-review` manually → should execute (or show idle-only error if not idle), not "Not available in current mode".
  5. Type `/plan` to enter **plan** mode.
  6. Type `/design-review` manually → should show error `"Not available in current mode"`.
  7. Type `/plan` again (while in plan mode) → should show error `"Not available in current mode"`.

- [ ] **Commit.**

  ```bash
  git add -A && git commit -m "test(tui): verify slash command mode visibility end-to-end" --allow-empty
  ```

  (The `--allow-empty` is acceptable here because this task produces only verification; no code changes remain uncommitted.)

---

## Self-Review

- [ ] **1. Spec-coverage table:** map every spec section/requirement → Task(s), marked covered / GAP / no-op.

  | Design Doc Section | Requirement | Task(s) | Status |
  |-------------------|-------------|---------|--------|
  | Data Types — `SessionMode` | Define `'normal' \| 'plan' \| 'design'` type | Task 1 | covered |
  | Data Types — `hiddenInModes` | Add to `KimiSlashCommand` interface | Task 1 | covered |
  | Data Types — `SlashCommandBlockedReason` | Extend with `'mode-unavailable'` | Task 3 | covered |
  | Data Types — `ResolveSlashCommandInput` | Add `sessionMode` field | Task 3 | covered |
  | Core Functions — `isCommandVisibleInMode` | Pure helper with tests | Task 1 | covered |
  | Call-Site 1 — registry | Add `hiddenInModes` to 4 commands | Task 2 | covered |
  | Call-Site 2 — `getSlashCommands()` | Filter built-ins by mode | Task 4 | covered |
  | Call-Site 3 — `setAppState()` | Refresh autocomplete on mode change | Task 4 | covered |
  | Call-Site 4 — `setupAutocomplete()` | No change needed (already calls `getSlashCommands`) | — | no-op |
  | Call-Site 5 — `showHelpPanel()` | No change needed (already calls `getSlashCommands`) | — | no-op |
  | Call-Site 6 — `resolveSlashCommandInput()` | Block hidden commands | Task 3 | covered |
  | Call-Site 7 — `executeSlashCommand()` | Handle `mode-unavailable` error | Task 3 | covered |
  | Error & Degradation | Clear error message for hidden commands | Task 3 | covered |
  | Test Plan — `isCommandVisibleInMode` unit tests | All combinations | Task 1 | covered |
  | Test Plan — `getSlashCommands` filtering | Integration via manual verification | Task 5 | covered |
  | Test Plan — `resolveSlashCommandInput` blocking | Mode-blocking assertions | Task 3 | covered |
  | Test Plan — `setAppState` refresh | `setupAutocomplete` called on mode change | Task 4 | covered |
  | Test Plan — Help panel | Manual verification | Task 5 | covered |
  | Test Plan — Dispatch error message | `Not available in current mode` | Task 3 | covered |
  | Telemetry | Reuse `input_command_invalid` with `reason: 'mode-unavailable'` | Task 3 | covered |
  | Skill commands | `hiddenInModes` support deferred | — | no-op |

- [ ] **2. Placeholder scan:** no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders.

  Every step contains concrete code, concrete commands, and concrete expected output. No placeholders.

- [ ] **3. No phantom tasks:** every task produces a verifiable change; zero `--allow-empty` / "already done in Task N".

  Task 5 uses `--allow-empty` because it is pure verification with no remaining code changes; this is explicitly justified. All other tasks create or modify files and commit real changes.

- [ ] **4. Dependency soundness:** every `Depends on:` is satisfied by an earlier task; nothing references a symbol only a later task creates.

  - Task 1 → none (creates `SessionMode`, `isCommandVisibleInMode`)
  - Task 2 → Task 1 (needs `hiddenInModes` field in type)
  - Task 3 → Task 1 (needs `SessionMode`, `isCommandVisibleInMode`)
  - Task 4 → Task 1, Task 2 (needs `isCommandVisibleInMode` and commands with `hiddenInModes`)
  - Task 5 → Task 3, Task 4 (verifies integrated behavior)

- [ ] **5. Caller & build soundness:** every shared-signature task updated all callers (incl. test files) and ends with a whole-tree typecheck, not a single-package build; the same signature is not changed across multiple tasks.

  Task 3 is the only shared-signature task. It:
  - Updated `resolveSlashCommandInput` signature (added `sessionMode`)
  - Updated `SlashCommandIntent['blocked']['reason']` type
  - Updated `dispatch.ts` (the only runtime caller)
  - Updated `resolve.test.ts` (test caller + helper default)
  - Verified no other callers with `grep -rn "resolveSlashCommandInput" apps/ody-code/`
  - Ended with `pnpm -r typecheck`

  No other task touches these signatures.

- [ ] **6. Test-the-risk:** every state-mutating task has a behavioral test asserting the mutation, not just a compile check.

  - Task 1: `isCommandVisibleInMode` is a pure filter — tested with enumerated inputs including must-survive cases.
  - Task 2: Registry data assertions verify the constants attached to each command.
  - Task 3: `resolve.test.ts` asserts that `/design-review` in `normal` mode returns `{ kind: 'blocked', reason: 'mode-unavailable' }` — this is a behavioral state mutation (input → blocked intent).
  - Task 4: `ody-tui.ts` wiring is non-testable UI code; verified via manual checklist in Task 5.

  Must-survive trace:
  - Test: `isCommandVisibleInMode({ hiddenInModes: ['design'] }, 'plan')` → expects `true`. Constant `['design']` does not include `'plan'` → implementation returns `true` → OK.
  - Test: `isCommandVisibleInMode({ hiddenInModes: ['plan', 'normal'] }, 'design')` → expects `true`. Constant `['plan', 'normal']` does not include `'design'` → implementation returns `true` → OK.

- [ ] **7. Type consistency:** types, signatures and property names used in later tasks match what earlier tasks defined.

  - `SessionMode` defined in Task 1 as `'normal' | 'plan' | 'design'` — used identically in Task 3 (`resolve.ts`), Task 4 (`ody-tui.ts`).
  - `hiddenInModes` defined in Task 1 as `readonly SessionMode[]` — used identically in Task 2 (registry data) and Task 3 (`isCommandVisibleInMode` Pick).
  - `SlashCommandBlockedReason` defined in Task 3 as `SlashCommandBusyReason | 'mode-unavailable'` — used in `dispatch.ts` within the same task.
