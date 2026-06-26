# Part 4: TUI Integration

## Task 9: TUI types, command visibility, and OdyTUI game-design wiring

**Depends on:** Part 1 (core.md) Task 1, Part 3 (skills-cli.md) Tasks 7-8

**Files:**
- Modify: `apps/ody-code/src/tui/commands/types.ts:1-34` (SessionMode type)
- Modify: `apps/ody-code/src/tui/commands/registry.ts:8` (SPECIAL_MODE_HIDDEN constant, all hiddenInModes)
- Modify: `apps/ody-code/src/tui/ody-tui.ts:145,161,257,305-309,475-479,538-540,1039-1046,1740-1744`
- Test: `apps/ody-code/test/tui/commands/game-design-visibility.test.ts` (create)

### Step 1: Write the failing test

Create `apps/ody-code/test/tui/commands/game-design-visibility.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { isCommandVisibleInMode } from '../../src/tui/commands/registry';

describe('isCommandVisibleInMode with game-design', () => {
  it('hides most commands in game-design mode (like office-hours)', () => {
    // A typical command with SPECIAL_MODE_HIDDEN should be invisible
    const cmd = {
      name: '/clear',
      aliases: [],
      description: 'Clear screen',
      hiddenInModes: ['office-hours', 'game-design'] as const,
    };
    expect(isCommandVisibleInMode(cmd, 'game-design')).toBe(false);
  });

  it('shows /exit in game-design mode', () => {
    const cmd = {
      name: '/exit',
      aliases: [],
      description: 'Exit',
      // no hiddenInModes
    };
    expect(isCommandVisibleInMode(cmd, 'game-design')).toBe(true);
  });

  it('shows commands without hiddenInModes in game-design mode', () => {
    const cmd = {
      name: '/help',
      aliases: [],
      description: 'Help',
    };
    expect(isCommandVisibleInMode(cmd, 'game-design')).toBe(true);
  });
});
```

### Step 2: Run test and verify FAIL

```bash
pnpm --filter ody-code vitest run test/tui/commands/game-design-visibility.test.ts 2>&1 | tail -10
```

Expected: TypeScript error — `'game-design'` is not assignable to type `SessionMode` used by `isCommandVisibleInMode`.

### Step 3: Write the implementation

**3a. `apps/ody-code/src/tui/commands/types.ts` — line 1:**

Change the `SessionMode` type:
```ts
export type SessionMode = 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';
```

This is a shared signature — find all callers that use `SessionMode` type or exhaustively match on it.

Grep check:
```bash
grep -rn "SessionMode\|'office-hours'\|'game-design'" apps/ody-code/src/tui/ --include='*.ts' | grep -v node_modules | grep -v test
```

Key consumers to verify:
- `apps/ody-code/src/tui/commands/registry.ts:8` — `OFFICE_HOURS_HIDDEN` constant → rename to `SPECIAL_MODE_HIDDEN`
- `apps/ody-code/src/tui/components/chrome/footer.ts:31,51,62,73` — EMOJIS map, renderModeBadge parameter type, color logic
- `apps/ody-code/src/tui/components/messages/status-panel.ts:38` — props type
- `apps/ody-code/src/tui/ody-tui.ts` — multiple locations

**3b. `apps/ody-code/src/tui/commands/registry.ts`:**

Change line 8 — `OFFICE_HOURS_HIDDEN` → `SPECIAL_MODE_HIDDEN`:
```ts
const SPECIAL_MODE_HIDDEN: readonly SessionMode[] = ['office-hours', 'game-design'];
```

Now update ALL 34 `hiddenInModes: OFFICE_HOURS_HIDDEN` references to `hiddenInModes: SPECIAL_MODE_HIDDEN`. Use replace_all:

```ts
// Old:
hiddenInModes: OFFICE_HOURS_HIDDEN,
// New:
hiddenInModes: SPECIAL_MODE_HIDDEN,
```

Lines 299, 307 — these explicitly list `['plan', 'design', 'office-hours']`. Add `'game-design'`:
```ts
hiddenInModes: ['plan', 'design', 'office-hours', 'game-design'],
```

Lines 63, 71 — these spread `OFFICE_HOURS_HIDDEN`. Change to `SPECIAL_MODE_HIDDEN`:
```ts
hiddenInModes: ['plan', ...SPECIAL_MODE_HIDDEN],
hiddenInModes: ['design', ...SPECIAL_MODE_HIDDEN],
```

Lines 79, 87, 95 — these spread `OFFICE_HOURS_HIDDEN`:
```ts
hiddenInModes: ['plan', 'normal', ...SPECIAL_MODE_HIDDEN],
hiddenInModes: ['design', 'normal', ...SPECIAL_MODE_HIDDEN],
hiddenInModes: ['design', 'normal', ...SPECIAL_MODE_HIDDEN],
```

**3c. `apps/ody-code/src/tui/ody-tui.ts`:**

Add `gameDesign` boolean to `OdyTUIStartupInput` (line 145, after `officeHours`):
```ts
readonly officeHours: boolean;
readonly gameDesign: boolean;
```

Update `createInitialAppState` (line 161):
```ts
sessionMode: input.gameDesign
  ? 'game-design'
  : input.officeHours
    ? 'office-hours'
    : input.cliOptions.sessionMode,
```

Update startup state (line 257, after `officeHours`):
```ts
officeHours: startupInput.officeHours,
gameDesign: startupInput.gameDesign,
```

Update `getSlashCommands` (line 309):
```ts
// Game-design and office-hours are restricted modes: only built-in /exit is exposed.
const skillCommands =
  mode === 'office-hours' || mode === 'game-design' ? [] : this.skillCommands;
```

Update `createSessionOptions` (lines 474-479):
```ts
sessionMode:
  startup.gameDesign
    ? 'game-design'
    : startup.officeHours
      ? 'office-hours'
      : startup.sessionMode === 'normal'
        ? undefined
        : startup.sessionMode,
```

Update `setSessionMode` call (line 538):
```ts
if (session !== undefined && startup.gameDesign) {
  await session.setSessionMode('game-design');
} else if (session !== undefined && startup.officeHours) {
  await session.setSessionMode('office-hours');
}
```

Update `syncRuntimeState` (lines 1039-1046):
```ts
const isOfficeHours = this.state.appState.sessionMode === 'office-hours';
const isGameDesign = this.state.appState.sessionMode === 'game-design';
this.setAppState({
  sessionId: session.id,
  model: status.model ?? '',
  thinking: status.thinkingLevel !== 'off',
  permissionMode: status.permission,
  sessionMode: isOfficeHours ? 'office-hours'
    : isGameDesign ? 'game-design'
    : (status.sessionMode ?? 'normal'),
  sessionModeFilePath: (isOfficeHours || isGameDesign) ? null : (sessionModeFilePath ?? null),
  contextTokens: status.contextTokens,
  maxContextTokens: status.maxContextTokens,
  contextUsage: status.contextUsage,
  sessionTitle: session.summary?.title ?? null,
  goal: goalResult.goal,
  userLanguage: status.userLanguage,
});
```

Update `showHelpPanel` (lines 1742-1743):
```ts
const shortcuts =
  this.state.appState.sessionMode === 'office-hours' ||
  this.state.appState.sessionMode === 'game-design'
    ? DEFAULT_KEYBOARD_SHORTCUTS.filter(
        (s) => !s.description.toLowerCase().includes('cycle mode'),
      )
    : DEFAULT_KEYBOARD_SHORTCUTS;
```

### Step 4: Run tests and verify PASS

```bash
pnpm --filter ody-code vitest run test/tui/commands/game-design-visibility.test.ts 2>&1 | tail -10
```

Expected: All 3 command visibility tests pass.

### Step 5: Whole-tree typecheck

```bash
pnpm -r typecheck 2>&1 | tail -30
```

Expected: No type errors across the entire workspace. The `SessionMode` type in `apps/ody-code/src/tui/commands/types.ts` is separate from `SessionModeKind` in `packages/agent-core` — both need `'game-design'`. Verify all callers are updated.

### Step 6: Commit

```bash
git add -A && git commit -m "feat: extend TUI SessionMode type with game-design, update command visibility and OdyTUI wiring"
```

---

## Task 10: Footer badge, status panel, and i18n labels for game-design

**Depends on:** Part 4 Task 9

**Files:**
- Modify: `apps/ody-code/src/tui/components/chrome/footer.ts:31,51,62,73,115` (EMOJIS, renderModeBadge, tips)
- Modify: `apps/ody-code/src/tui/components/messages/status-panel.ts:38,111` (SessionMode type, label)
- Modify: `packages/agent-core/src/i18n/translations.ts` (add `tui.footer.gameDesign`, `tui.statusPanel.gameDesign`)
- Modify: `packages/agent-core/src/i18n/types.ts` (add type keys)

### Step 1: Build and verify FAIL (missing i18n keys)

The Task 9 typecheck passed, but the footer and status panel will reference `'game-design'` in their `SessionMode` type unions and the i18n keys `tui.footer.gameDesign` / `tui.statusPanel.gameDesign`. Build the ody-code app:

```bash
pnpm --filter ody-code build 2>&1 | tail -20
```

Expected: Build succeeds since the type change was in Task 9, but runtime will show raw i18n key names or fallback behavior for game-design mode badge/status.

### Step 2: Write the implementation

**2a. `apps/ody-code/src/tui/components/chrome/footer.ts`:**

Line 31 — add game-design to EMOJIS:
```ts
const EMOJIS: Record<string, string> = {
  normal: '⚒️', plan: '📝', design: '✏️', 'office-hours': '🏢', 'game-design': '🎮',
};
```

Line 51 — add `'game-design'` to the `renderModeBadge` parameter type:
```ts
function renderModeBadge(
  mode: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design',
  colors: ColorPalette,
  fileName?: string,
  userLanguage?: 'en' | 'zh' | undefined,
): string {
```

Lines 56-73 — add game-design to the `bgColor` and `displayLabel` logic:
```ts
const emoji = EMOJIS[mode] ?? '';
const bgColor =
  mode === 'design'
    ? colors.accent
    : mode === 'plan'
      ? colors.primary
      : mode === 'office-hours'
        ? colors.warning
        : mode === 'game-design'
          ? colors.info          // use info/cyan for game-design
          : colors.textMuted;

// ...
const displayLabel =
  mode === 'office-hours'
    ? t('tui.footer.officeHours', userLanguage)
    : mode === 'game-design'
      ? t('tui.footer.gameDesign', userLanguage)
      : mode;
```

Line 115 — update tips to also hide in game-design:
```ts
{ text: 'shift+tab: cycle plan/design mode', hiddenInModes: ['office-hours', 'game-design'] },
```

**2b. `apps/ody-code/src/tui/components/messages/status-panel.ts`:**

Line 38 — add `'game-design'` to the type:
```ts
readonly sessionMode: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';
```

After line 111 (the office-hours status line), add:
```ts
{ label: t('tui.statusPanel.gameDesign', lang), value: sessionMode === 'game-design' ? t('tui.statusPanel.on', lang) : t('tui.statusPanel.off', lang) },
```

**2c. `packages/agent-core/src/i18n/translations.ts`:**

Add to `en` block (after `tui.footer.officeHours` / before closing `}`):
```ts
'tui.footer.gameDesign': 'Game Design',
'tui.statusPanel.gameDesign': 'Game Design',
```

Add to `zh` block:
```ts
'tui.footer.gameDesign': '游戏设计',
'tui.statusPanel.gameDesign': '游戏设计',
```

**2d. `packages/agent-core/src/i18n/types.ts`:**

Add to the `MessageKey` type union:
```ts
  | 'tui.footer.gameDesign'
  | 'tui.statusPanel.gameDesign'
```

### Step 3: Build and verify

```bash
pnpm -r typecheck 2>&1 | tail -20
pnpm --filter ody-code build 2>&1 | tail -20
```

Expected: Typecheck passes. Build succeeds.

### Step 4: Manual verification

Run the full CLI (after all 10 tasks are complete):

```bash
node apps/ody-code/dist/cli/index.js --game-design
```

Manual checklist:
- [ ] Footer shows `🎮 Game Design` badge with info/cyan background
- [ ] Status panel shows `Game Design: on` row
- [ ] `/exit` command is available (only slash command exposed)
- [ ] All other slash commands hidden (like office-hours)
- [ ] Skill commands stripped (mode === 'game-design' → `[]`)
- [ ] `shift+tab` tip hidden
- [ ] Help panel hides cycle-mode shortcuts
- [ ] EnterGameDesignMode tool appears in available tools

### Step 5: Commit

```bash
git add -A && git commit -m "feat: add game-design footer badge, status panel, and i18n labels"
```

---

## Self-Review (Part 4)

- [ ] 1. Spec-coverage: Task 9 covers TUI types + command visibility + OdyTUI wiring (design items 8, 11, 16). Task 10 covers footer badge + status panel + i18n (design items 16).
- [ ] 2. Placeholder scan: No TODO/TBD. All code is exact modifications with line numbers.
- [ ] 3. No phantom tasks: Both tasks produce visible TUI changes (badge color, label, command hiding).
- [ ] 4. Dependency soundness: Task 9 depends on Part 3 (CLIOptions.gameDesign, runGameDesign). Task 10 depends on Task 9 (SessionMode type already extended).
- [ ] 5. Caller & build soundness: Task 9 changes `SessionMode` in tui/commands/types.ts — this is a shared signature used by registry.ts, footer.ts, status-panel.ts, ody-tui.ts. All callers updated in Tasks 9-10. Whole-tree typecheck verifies.
- [ ] 6. Test-the-risk: Task 9 has 3 tests for command visibility (hidden, /exit visible, unmarked visible). Task 10 is UI-only with manual verification checklist.
- [ ] 7. Type consistency: `SessionMode` type in TUI (`'normal' | 'plan' | 'design' | 'office-hours' | 'game-design'`) matches `SessionModeKind` in agent-core (`'plan' | 'design' | 'office-hours' | 'game-design'`). `OdyTUIStartupInput.gameDesign: boolean` matches `CLIOptions.gameDesign: boolean`.
