# TUI Slash Command Mode Visibility

## Scope

### In
- Add mode-based visibility filtering to slash commands in the TUI autocomplete and help panel.
- Commands can declare `hiddenInModes` to hide themselves in specific modes (`design`, `plan`, `normal`).
- Manual input of a hidden command is also blocked with a clear error message.
- Both built-in commands and skill commands support `hiddenInModes`.
- Help panel shows only currently-visible commands.
- Autocomplete refreshes when mode switches.

### Out
- No changes to command execution logic beyond the block check.
- No changes to how modes are entered/exited.
- No persistent state changes.

## Resolved Decisions

1. **Scope** — Generic mechanism on `KimiSlashCommand` with `hiddenInModes`; applies to built-in and skill commands. [C:USER]
2. **Data & State** — `hiddenInModes?: ('design' | 'plan' | 'normal')[]`; manual input blocked too. [C:USER]
3. **Error & Degradation** — New `blocked` reason `'mode-unavailable'` with message `"Not available in current mode"`. [C:USER]
4. **Integration** — Skill commands support `hiddenInModes`; help panel syncs; autocomplete refreshes on mode switch. [C:USER]
5. **Filter Priority** — Mode filter first, then experimental flag. [C:USER]
6. **Mode Exclusivity** — `plan` and `design` are mutually exclusive; `normal` means neither is active. [C:INFERRED]

## Architecture

```
AppState (planMode, designMode)
    │
    ▼
resolveCurrentMode(appState) ──► 'design' | 'plan' | 'normal'
    │
    ▼
isCommandVisibleInMode(command, mode)
    │
    ├──► getSlashCommands() ──► autocomplete list (filtered)
    │
    ├──► resolveSlashCommandInput() ──► blocked if hidden
    │
    └──► showHelpPanel() ──► help list (filtered via getSlashCommands)
```

## Data Types

```ts
// apps/ody-code/src/tui/commands/types.ts
export type TuiMode = 'design' | 'plan' | 'normal';

export interface KimiSlashCommand<Name extends string = string> extends SlashCommand {
  readonly name: Name;
  readonly aliases: readonly string[];
  readonly description: string;
  readonly priority?: number;
  readonly availability?: SlashCommandAvailability | ((args: string) => SlashCommandAvailability);
  readonly experimentalFlag?: FlagId;
  readonly completeArgs?: (argumentPrefix: string) => AutocompleteItem[] | null;
  /** [C:USER] Modes in which this command is hidden from the palette and blocked. */
  readonly hiddenInModes?: readonly TuiMode[];
}
```

```ts
// apps/ody-code/src/tui/commands/resolve.ts
export type SlashCommandBlockedReason = SlashCommandBusyReason | 'mode-unavailable';

export type SlashCommandIntent =
  | { readonly kind: 'not-command' }
  | { readonly kind: 'builtin'; readonly command: BuiltinSlashCommand; readonly name: BuiltinSlashCommandName; readonly args: string }
  | { readonly kind: 'skill'; readonly commandName: string; readonly skillName: string; readonly args: string }
  | { readonly kind: 'message'; readonly input: string }
  | { readonly kind: 'blocked'; readonly commandName: string; readonly reason: SlashCommandBlockedReason }
  | { readonly kind: 'invalid'; readonly commandName: string; readonly reason: SlashCommandInvalidReason };
```

## Core Functions

### `resolveCurrentMode`
```ts
export function resolveCurrentMode(appState: Pick<AppState, 'planMode' | 'designMode'>): TuiMode {
  if (appState.planMode) return 'plan';
  if (appState.designMode) return 'design';
  return 'normal';
}
```

### `isCommandVisibleInMode`
```ts
export function isCommandVisibleInMode(
  command: Pick<KimiSlashCommand, 'hiddenInModes'>,
  mode: TuiMode,
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

### 2. `apps/ody-code/src/tui/kimi-tui.ts` `getSlashCommands()` (lines 309–314)
Change to accept `appState` and filter by mode:
```ts
private getSlashCommands(appState: AppState = this.state.appState): readonly KimiSlashCommand[] {
  const mode = resolveCurrentMode(appState);
  const builtins = sortSlashCommands(BUILTIN_SLASH_COMMANDS)
    .filter((command) => isCommandVisibleInMode(command, mode))
    .filter((command) => isExperimentalFlagEnabled(command.experimentalFlag));
  const skills = this.skillCommands.filter((command) => isCommandVisibleInMode(command, mode));
  return [...builtins, ...skills];
}
```

### 3. `apps/ody-code/src/tui/kimi-tui.ts` `setupAutocomplete()` (lines 316–335)
No change needed; it already calls `getSlashCommands()`.

### 4. `apps/ody-code/src/tui/kimi-tui.ts` `showHelpPanel()` (line 1724)
No change needed; it already calls `getSlashCommands()`.

### 5. `apps/ody-code/src/tui/commands/resolve.ts` `resolveSlashCommandInput()` (lines 48–99)
Add `appState` to input and mode check:
```ts
export interface ResolveSlashCommandInput {
  readonly input: string;
  readonly skillCommandMap: ReadonlyMap<string, string>;
  readonly isStreaming: boolean;
  readonly isCompacting: boolean;
  readonly planMode: boolean;
  readonly designMode: boolean;
}

export function resolveSlashCommandInput(options: ResolveSlashCommandInput): SlashCommandIntent {
  const parsed = parseSlashInput(options.input);
  if (parsed === null) return { kind: 'not-command' };

  const command = findBuiltInSlashCommand(parsed.name);
  if (
    command !== undefined &&
    isExperimentalFlagEnabled((command as KimiSlashCommand).experimentalFlag)
  ) {
    const mode = resolveCurrentMode(options);
    if (!isCommandVisibleInMode(command as KimiSlashCommand, mode)) {
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

### 6. `apps/ody-code/src/tui/commands/dispatch.ts` `executeSlashCommand()` (lines 154–204)
Add handling for `'mode-unavailable'`:
```ts
async function executeSlashCommand(host: SlashCommandHost, input: string): Promise<void> {
  const parsedCommand = parseSlashInput(input);
  const intent = resolveSlashCommandInput({
    input,
    skillCommandMap: host.skillCommandMap,
    isStreaming: host.state.appState.streamingPhase !== 'idle',
    isCompacting: host.state.appState.isCompacting,
    planMode: host.state.appState.planMode,
    designMode: host.state.appState.designMode ?? false,
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

### 7. Mode-switch refresh
In `apps/ody-code/src/tui/kimi-tui.ts` `setAppState()` (line 968), add mode-change detection to auto-refresh autocomplete:
```ts
setAppState(patch: Partial<AppState>): void {
  if (!hasPatchChanges(this.state.appState, patch)) return;
  const busyChanged = 'streamingPhase' in patch || 'isCompacting' in patch;
  const modeChanged = 'planMode' in patch || 'designMode' in patch;
  Object.assign(this.state.appState, patch);
  if ('planMode' in patch) this.updateEditorBorderHighlight();
  if (modeChanged) this.setupAutocomplete();
  this.state.footer.setState(this.state.appState);
  // ... rest unchanged
}
```
This avoids exposing `setupAutocomplete` through `SlashCommandHost` and covers all mode-switch paths (`/plan`, `/design`, Shift-Tab, session restore) automatically.

## Error & Degradation

| Error | Immediate Handling | Degradation Path | Recovery |
|---|---|---|---|
| User types hidden command | `blocked` with `mode-unavailable` → `showError("Not available in current mode")` | Input rejected, nothing executed | User switches to correct mode |
| Mode resolve fails (both true) | `resolveCurrentMode` picks `plan` first [C:INFERRED] | `plan` wins over `design` | User toggles modes to correct state |

## Test Plan

1. **`isCommandVisibleInMode` unit tests**
   - `hiddenInModes: undefined` → visible in all modes
   - `hiddenInModes: ['design']` with mode `design` → false
   - `hiddenInModes: ['design']` with mode `plan` → true
   - `hiddenInModes: ['plan', 'normal']` with mode `design` → true
   - `hiddenInModes: ['plan', 'normal']` with mode `plan` → false
   - `hiddenInModes: ['plan', 'normal']` with mode `normal` → false

2. **`resolveCurrentMode` unit tests**
   - `planMode: true, designMode: true` → `'plan'` (mutual-exclusivity fallback)
   - `planMode: true, designMode: false` → `'plan'`
   - `planMode: false, designMode: true` → `'design'`
   - `planMode: false, designMode: false` → `'normal'`

3. **Integration: `getSlashCommands` filtering**
   - In `normal` mode: `design-review` and `plan-review` are absent
   - In `design` mode: `design` is absent, `design-review` is present
   - In `plan` mode: `plan` is absent, `plan-review` is present

4. **Integration: `resolveSlashCommandInput` blocking**
   - `/design-review` in `normal` mode → `blocked` reason `'mode-unavailable'`
   - `/plan` in `plan` mode → `blocked` reason `'mode-unavailable'`

5. **Help panel**
   - In `design` mode, help panel does not list `design` or `plan-review`

Done criteria: `pnpm test --filter apps/ody-code` passes.

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | `resolveCurrentMode` picks wrong mode if both flags are true | Low | Medium | Document fallback order (`plan` > `design` > `normal`); add unit test |
| 2 | Skill commands from future skills unexpectedly hidden | Low | Low | Default is visible; only hidden when explicitly declared |
| 3 | Help panel cached stale list after mode switch | Medium | Low | Ensure `showHelpPanel` re-calls `getSlashCommands` each time |

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `planMode` and `designMode` are mutually exclusive in practice | Medium | Low | Check mode toggle logic in `config.ts` and `editor-keyboard.ts` |
| 2 | `getSlashCommands` is the single source of truth for help panel commands | High | Low | Verified in `kimi-tui.ts` line 1724 |
| 3 | Skill commands do not currently have `hiddenInModes` and will default to visible | High | Low | Verified in `skills.ts` line 21–31 |
