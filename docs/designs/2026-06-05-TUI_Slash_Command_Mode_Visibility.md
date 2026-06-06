# TUI Slash Command Mode Visibility

## Scope

### In
- Add mode-based visibility filtering to slash commands in the TUI autocomplete and help panel.
- Built-in commands can declare `hiddenInModes` to hide themselves in specific modes (`design`, `plan`, `normal`). [C:USER]
- Manual input of a hidden command is also blocked with a clear error message. [C:USER]
- Help panel shows only currently-visible commands. [C:USER]
- Autocomplete refreshes when mode switches. [C:USER]

### Out
- Skill commands do not support `hiddenInModes` in this version; deferred to a future release. [C:USER]
- No changes to command execution logic beyond the block check. [C:DEFERRED]
- No changes to how modes are entered/exited. [C:DEFERRED]
- No persistent state changes. [C:DEFERRED]

## Resolved Decisions

1. **Scope** — Generic mechanism on `KimiSlashCommand` with `hiddenInModes`; applies to built-in commands only in this version. Skill command support deferred. [C:USER]
2. **Data & State** — `hiddenInModes?: readonly ('design' | 'plan' | 'normal')[]`; manual input blocked too. [C:USER]
3. **Error & Degradation** — New `blocked` reason `'mode-unavailable'` with message `"Not available in current mode"`. [C:USER]
4. **Integration** — Help panel syncs via `getSlashCommands()`; autocomplete refreshes on `sessionMode` change in `setAppState`. [C:USER]
5. **Filter Priority** — Mode filter first, then experimental flag. [C:INFERRED]
6. **SessionMode Source** — `AppState.sessionMode` is the single source of truth; no `resolveCurrentMode` helper needed. [C:USER]
7. **Telemetry** — Reuse existing `input_command_invalid` track event with `reason: 'mode-unavailable'`. [C:USER]

## Architecture

```
AppState.sessionMode ──► getSlashCommands() ──► filter by hiddenInModes
                              │
                              ├──► setupAutocomplete() ──► autocomplete list
                              │
                              ├──► showHelpPanel() ──► help list
                              │
                              └──► resolveSlashCommandInput() ──► blocked if hidden
```

## Data Types

```ts
// apps/ody-code/src/tui/commands/types.ts
export type SessionMode = 'normal' | 'plan' | 'design';

export interface KimiSlashCommand<Name extends string = string> extends SlashCommand {
  readonly name: Name;
  readonly aliases: readonly string[];
  readonly description: string;
  readonly priority?: number;
  readonly availability?: SlashCommandAvailability | ((args: string) => SlashCommandAvailability);
  readonly experimentalFlag?: FlagId;
  readonly completeArgs?: (argumentPrefix: string) => AutocompleteItem[] | null;
  /** [C:USER] Modes in which this command is hidden from the palette and blocked. */
  readonly hiddenInModes?: readonly SessionMode[];
}
```

```ts
// apps/ody-code/src/tui/commands/resolve.ts
export type SlashCommandBlockedReason = SlashCommandBusyReason | 'mode-unavailable';

export interface ResolveSlashCommandInput {
  readonly input: string;
  readonly skillCommandMap: ReadonlyMap<string, string>;
  readonly isStreaming: boolean;
  readonly isCompacting: boolean;
  /** [C:USER] Current session mode for visibility filtering. */
  readonly sessionMode: SessionMode;
}
```

## Core Functions

### `isCommandVisibleInMode`
```ts
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

## Call-Site Integration

### 1. `apps/ody-code/src/tui/commands/registry.ts` (lines 50–76)
Add `hiddenInModes` to the four commands:
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

### 2. `apps/ody-code/src/tui/ody-tui.ts` `getSlashCommands()` (lines 308–313)
Add mode filtering:
```ts
private getSlashCommands(): readonly KimiSlashCommand[] {
  const mode = this.state.appState.sessionMode;
  const builtins = sortSlashCommands(BUILTIN_SLASH_COMMANDS)
    .filter((command) => isCommandVisibleInMode(command, mode))
    .filter((command) => isExperimentalFlagEnabled(command.experimentalFlag));
  return [...builtins, ...this.skillCommands];
}
```

### 3. `apps/ody-code/src/tui/ody-tui.ts` `setAppState()` (lines 966–972)
Add autocomplete refresh on mode change:
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
  // ... rest unchanged
}
```

### 4. `apps/ody-code/src/tui/ody-tui.ts` `setupAutocomplete()` (lines 315–335)
No change needed; it already calls `getSlashCommands()`.

### 5. `apps/ody-code/src/tui/ody-tui.ts` `showHelpPanel()` (line 1717)
No change needed; it already calls `getSlashCommands()`.

### 6. `apps/ody-code/src/tui/commands/resolve.ts` `resolveSlashCommandInput()` (lines 48–99)
Add `sessionMode` to input and mode check:
```ts
export function resolveSlashCommandInput(options: ResolveSlashCommandInput): SlashCommandIntent {
  const parsed = parseSlashInput(options.input);
  if (parsed === null) return { kind: 'not-command' };

  const command = findBuiltInSlashCommand(parsed.name);
  if (
    command !== undefined &&
    isExperimentalFlagEnabled((command as KimiSlashCommand).experimentalFlag)
  ) {
    if (!isCommandVisibleInMode(command as KimiSlashCommand, options.sessionMode)) {
      return { kind: 'blocked', commandName: parsed.name, reason: 'mode-unavailable' };
    }
    const busyReason = slashCommandBusyReason(options);
    if (
      busyReason !== undefined &&
      resolveSlashCommandAvailability(command, parsed.args) === 'idle-only'
    ) {
      return { kind: 'blocked', commandName: parsed.name, reason: busyReason };
    }
    return { kind: 'builtin', command, name: command.name, args: parsed.args };
  }
  // ... rest unchanged
}
```

### 7. `apps/ody-code/src/tui/commands/dispatch.ts` `executeSlashCommand()` (lines 154–204)
Add handling for `'mode-unavailable'`:
```ts
async function executeSlashCommand(host: SlashCommandHost, input: string): Promise<void> {
  const parsedCommand = parseSlashInput(input);
  const intent = resolveSlashCommandInput({
    input,
    skillCommandMap: host.skillCommandMap,
    isStreaming: host.state.appState.streamingPhase !== 'idle',
    isCompacting: host.state.appState.isCompacting,
    sessionMode: host.state.appState.sessionMode,
  });

  switch (intent.kind) {
    // ... other cases
    case 'blocked':
      host.track('input_command_invalid', { reason: intent.reason, command: intent.commandName });
      if (intent.reason === 'mode-unavailable') {
        host.showError(`Not available in current mode`);
      } else {
        host.showError(slashBusyMessage(intent.commandName, intent.reason));
      }
      return;
    // ...
  }
}
```

## Error & Degradation

| Error | Immediate Handling | Degradation Path | Recovery |
|---|---|---|---|
| User types hidden command | `blocked` with `mode-unavailable` → `showError("Not available in current mode")` | Input rejected, nothing executed | User switches to correct mode |
| `hiddenInModes` is empty/undefined | `isCommandVisibleInMode` returns `true` | Command visible in all modes | N/A (default behavior) |

## Test Plan

1. **`isCommandVisibleInMode` unit tests**
   - `hiddenInModes: undefined` → visible in all modes
   - `hiddenInModes: ['design']` with mode `design` → `false`
   - `hiddenInModes: ['design']` with mode `plan` → `true`
   - `hiddenInModes: ['plan', 'normal']` with mode `design` → `true`
   - `hiddenInModes: ['plan', 'normal']` with mode `plan` → `false`
   - `hiddenInModes: ['plan', 'normal']` with mode `normal` → `false`

2. **Integration: `getSlashCommands` filtering**
   - In `normal` mode: `design-review` and `plan-review` are absent
   - In `design` mode: `design` is absent, `design-review` is present
   - In `plan` mode: `plan` is absent, `plan-review` is present

3. **Integration: `resolveSlashCommandInput` blocking**
   - `/design-review` in `normal` mode → `blocked` reason `'mode-unavailable'`
   - `/plan` in `plan` mode → `blocked` reason `'mode-unavailable'`

4. **Integration: `setAppState` mode-switch refresh**
   - Calling `setAppState({ sessionMode: 'plan' })` triggers `setupAutocomplete()`

5. **Help panel**
   - In `design` mode, help panel does not list `design` or `plan-review`

6. **Dispatch error message**
   - `/design-review` in `normal` mode shows `"Not available in current mode"`

Done criteria: `pnpm test --filter apps/ody-code` passes.

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | `getSlashCommands` filter accidentally hides commands that should be visible | Low | Medium | Unit tests for `isCommandVisibleInMode` cover all combinations; default is visible |
| 2 | Help panel cached stale list after mode switch | Low | Low | `showHelpPanel` re-calls `getSlashCommands` each time; `setAppState` refreshes autocomplete |
| 3 | Future skill commands unexpectedly hidden when `hiddenInModes` support is added | Low | Low | Skill support deferred; when added, default will be visible (no `hiddenInModes` = visible everywhere) |

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `AppState.sessionMode` is always `'normal'`, `'plan'`, or `'design'` at runtime | High | Medium | Verified by `AppState` type in `apps/ody-code/src/tui/types.ts` line 20 |
| 2 | `getSlashCommands` is the single source of truth for help panel commands | High | Low | Verified in `ody-tui.ts` line 1721 |
| 3 | `skillCommands` do not currently have `hiddenInModes` and will default to visible | High | Low | Verified in `commands/types.ts`; `hiddenInModes` is optional |
