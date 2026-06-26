# Fix: executing-plans skill visible in TUI slash commands despite hiddenInModes

## Scope

### In
- [C:USER] `executing-plans` skill hidden from TUI slash command autocomplete in plan/design modes
- [C:USER] Manual `/skill:executing-plans` input treated as plain message in plan/design modes
- [C:USER] Full chain: agent-core Session → RPC → node-sdk → TUI
- [C:USER] Any future skill with `hiddenInModes` automatically benefits

### Out
- [C:USER] Does not change agent-core's existing three-layer defense (system prompt filter, dynamic injection reminder, SkillTool guard)
- [C:USER] Does not add `hiddenInModes` to `SkillSummary`

## Architecture

### Data flow (fixed)

```
TUI init / mode switch
  │
  ▼
ody-tui.refreshSkillCommands()
  │ passes this.state.appState.sessionMode (undefined→all, 'plan'→filter plan-hidden)
  ▼
session.listSkills({ sessionMode })
  │
  ▼
Session.listSkills(options?)                               [session/index.ts:348]
  │ sessionMode ? Registry.listInvocableSkills(mode) : Registry.listSkills()
  ▼
SkillDefinition[] (filtered) → summarizeSkill() → SkillSummary[]
  │
  ▼
buildSkillSlashCommands(skills)                            [skills.ts:21]
  │ only skills visible in current mode
  ▼
KimiSlashCommand[] → skillCommandMap → getSlashCommands() → TUI autocomplete
                                             │
                                             ▼
                                   resolveSlashCommandInput()
                                     resolveSkillCommand(map, name)
                                       └─ hidden skill not in map → undefined → 'message'
```

### Root cause

The previous implementation covered agent-core (system prompt, injection, tool guard) but missed the TUI data pipeline:

```
SkillDefinition.hiddenInModes  ← exists ['plan','design']
  ↓ summarizeSkill()              ← drops it
SkillSummary                    ← no hiddenInModes field
  ↓ buildSkillSlashCommands()     ← never populates it
KimiSlashCommand.hiddenInModes  ← undefined
  ↓ getSlashCommands()            ← no isCommandVisibleInMode call
TUI autocomplete                  ← executing-plans still visible
```

## Interfaces

### Session.listSkills — new optional parameter

```
interface ListSkillsOptions {
  sessionMode?: 'normal' | 'plan' | 'design';
}

// Contract: when sessionMode is undefined or omitted, returns ALL skills (backward compat).
// When sessionMode is 'plan'/'design', returns only skills not hiddenInModes for that mode.
async listSkills(options?: ListSkillsOptions): Promise<readonly SkillSummary[]>
  await this.skillsReady
  if options?.sessionMode is not undefined
    return this.skills.listInvocableSkills(options.sessionMode).map(summarizeSkill)
  else
    return this.skills.listSkills().map(summarizeSkill)
```

### RPC — `packages/agent-core/src/session/rpc.ts` ~L91

```
listSkills(payload: { sessionMode?: 'normal' | 'plan' | 'design' }): Promise<readonly SkillSummary[]>
  const mode = payload.sessionMode
  if mode is not undefined
    return this.session.listSkills({ sessionMode: mode })
  else
    return this.session.listSkills()
```

### node-sdk Session type — `packages/node-sdk/src/session.ts` ~L225

```
// Type signature mirrors Session.listSkills:
listSkills(options?: { sessionMode?: 'normal' | 'plan' | 'design' }): Promise<readonly SkillSummary[]>
```

### node-sdk RPC client — `packages/node-sdk/src/rpc.ts` ~L425

```
async listSkills(input: SessionIdRpcInput & { sessionMode?: 'normal' | 'plan' | 'design' })
  → rpc.listSkills({ sessionId: input.sessionId, sessionMode: input.sessionMode })
```

## Call-sites

### 1. `apps/ody-code/src/tui/ody-tui.ts` — `refreshSkillCommands` ~L338

```
async refreshSkillCommands(session?: SkillListSession): Promise<void>
  if session is undefined
    this.skillCommands = []
    this.skillCommandMap.clear()
    this.setupAutocomplete()
    return

  let skills
  try
    const mode = this.state.appState.sessionMode   // 'plan' | 'design' | undefined(normal)
    skills = await session.listSkills(mode ? { sessionMode: mode } : undefined)
  catch
    return

  const skillCommands = buildSkillSlashCommands(skills)
  this.skillCommands = skillCommands.commands
  this.skillCommandMap.clear()
  for each [commandName, skillName] of skillCommands.commandMap
    this.skillCommandMap.set(commandName, skillName)
  this.setupAutocomplete()
```

### 2a. `apps/ody-code/src/tui/controllers/session-event-handler.ts` — add to `SessionEventHost` interface ~L78

```
export interface SessionEventHost {
  // ... existing fields ...
  refreshSkillCommands(session?: SkillListSession): Promise<void>;  // ← NEW
}
```

### 2b. `apps/ody-code/src/tui/controllers/session-event-handler.ts` — mode change trigger ~L548

```
handleStatusUpdate(event)
  ...
  if event.sessionMode is not undefined
    patch.sessionMode = event.sessionMode
  ...
  if Object.keys(patch).length > 0
    this.host.setAppState(patch)
    if event.sessionMode is not undefined
      void this.host.refreshSkillCommands(this.host.session)   // ← NEW
```

### 3. `apps/ody-code/src/tui/commands/skills.ts` — `SkillListSession` type ~L5

```
// SkillListSession already Picks from Session; Session's listSkills signature change
// automatically propagates. No code change needed here unless the Pick is explicit.
type SkillListSession = Pick<Session, 'listSkills'>;
```

### 4. `apps/ody-code/src/tui/commands/resolve.ts` — defense-in-depth (implicit)

```
// resolveSkillCommand already checks skillCommandMap presence.
// When server-side filtering removes hidden skills, they won't be in the map,
// so resolveSkillCommand returns undefined → falls through to {kind:'message'}.
// No code change needed — this is the natural defense-in-depth.
```

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `listSkills()` throws (network/RPC) | Caught in `refreshSkillCommands` catch, returns early (existing behavior) | `skillCommands` keeps last successful value; autocomplete may be briefly stale | Next successful `refreshSkillCommands` call |
| `listInvocableSkills` internal error | Registry's `listSkills()` + filter won't throw | No degradation needed | N/A |
| Mode switch while refresh in flight | `refreshSkillCommands` is async; `getSlashCommands()` may show stale data briefly | Stale commands are from previous mode's valid skills, not a security risk | Auto-corrected when async refresh completes |

## Test Plan

| # | Test | Assertions | File |
|---|---|---|---|
| 1 | `Session.listSkills()` no args returns all skills | Result length equals total registered skills | `packages/agent-core/src/session/__tests__/` |
| 2 | `Session.listSkills({sessionMode:'plan'})` filters plan-hidden skills | `executing-plans` NOT in result | Same |
| 3 | `Session.listSkills({sessionMode:'design'})` filters design-hidden skills | `executing-plans` NOT in result | Same |
| 4 | `Session.listSkills({sessionMode:'normal'})` does not filter | `executing-plans` IS in result | Same |
| 5 | `buildSkillSlashCommands` on filtered list | Command count equals input skill count | `apps/ody-code/src/tui/commands/__tests__/` |
| 6 | `getSlashCommands` excludes `skill:executing-plans` in plan/design mode | `skill:executing-plans` not in command list | TUI integration |
| 7 | Manual `/skill:executing-plans` in plan mode → plain message | `resolveSlashCommandInput` returns `{kind:'message'}` | `resolve.test.ts` |
| 8 | `handleStatusUpdate` triggers `refreshSkillCommands` on mode change | `skillCommandMap` updated after `sessionMode` change | TUI integration |

### Done criteria

```bash
pnpm --filter @odysseythink/agent-core test --run   # all session/skill tests pass
pnpm --filter @odysseythink/kimi-code test --run      # all TUI command tests pass
pnpm typecheck                                         # full typecheck passes
```

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | RPC serialization adds `sessionMode` field; old client incompatible | Low (internal only) | Medium (old TUI can't call new agent-core) | `sessionMode` is optional; old clients omit it → full list (backward compat) |
| 2 | Frequent mode switches cause many `listSkills` RPC calls | Low | Low (minor perf) | `listSkills` is lightweight in-memory op, no I/O |
| 3 | Future skill hidden in plan but not design (or vice versa); cache stale on switch | Low | Low (brief visibility) | `refreshSkillCommands` triggered immediately on mode switch |

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | [C:INFERRED] `SkillRegistry.listInvocableSkills` filters correctly for all three modes | High | Hidden skills leak through RPC | Covered by tests 2, 3, 4 |
| 2 | [C:INFERRED] `this.host.session` is always set when `handleStatusUpdate` fires | High → Verified: `SessionEventHost.session` is `Session \| undefined`, but `handleStatusUpdate` is called from within `session.onEvent` handler where a session is active | `refreshSkillCommands(undefined)` clears all commands | Verified by reading `SessionEventHost` interface and event handler code path |
| 3 | [C:INFERRED] `session.listSkills({sessionMode})` RPC path works for both undefined and string values | Medium | RPC deserialization may fail on undefined vs missing key | Test with actual RPC harness |
| 4 | [C:INFERRED] No other TUI consumers of `listSkills` need the unfiltered list | Medium → Verified: only `ody-tui.ts:348` calls `listSkills` | Other UI features lose access to hidden skills | Grep confirmed single caller |

## Self-Review

### Critical Decision Verification

**Decision 1: `listInvocableSkills` filter logic**

Verified via ephemeral `node -e`:

| Input `hiddenInModes` | Input `sessionMode` | Expected | Result |
|---|---|---|---|
| `['plan','design']` | `'plan'` | filtered out | ✅ |
| `['plan','design']` | `'design'` | filtered out | ✅ |
| `['plan','design']` | `'normal'` | kept | ✅ |
| `['plan','design']` | `undefined` | kept (backward compat) | ✅ |
| `['plan']` | `'design'` | kept | ✅ |
| `undefined` | `'plan'` | kept | ✅ |
| `[]` | `'plan'` | kept | ✅ |

All 7 real-world + adversarial inputs produce expected output.

### Four-lens sweep

- **Security**: No filter/regex for false positives/negatives beyond the proven `listInvocableSkills` filter. No secrets/PII involved. Nothing found.
- **Test**: Every behavior has must-pass + must-reject cases in the test plan. Test #7 (manual `/skill:executing-plans` → message) covers defense-in-depth. No constant-vs-assertion contradiction. Nothing found.
- **Ops**: Added call (`listSkills`) is in-memory registry read, negligible latency. One call per mode switch via `handleStatusUpdate`. No identifier collision (`sessionMode` parameter unique in `listSkills` context). No concurrency issue (async refresh, no debouncing needed). Nothing found.
- **Integration**: All 8 code points verified to exist (6 RPC/server files + 2 TUI files). One gap found and fixed: `SessionEventHost` interface needs `refreshSkillCommands` added (call-site 2a). No silent retargeting — all paths match user-named locations. Fixed.
- **Scope**: Single coherent design fixing one data pipeline gap. Does not grow into multiple subsystems. Clean.
