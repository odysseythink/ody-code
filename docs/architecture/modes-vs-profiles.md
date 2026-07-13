# Modes vs Profiles

> **One-sentence definition**: A **mode** is an interaction phase (plan, design, product, game-design, or normal); a **profile** is a role/tool-set/system-prompt configuration loaded from `.ody-code/profiles/`.

## Responsibility split

| Concern | Mode | Profile |
|---|---|---|
| Determines output directory | Yes (`plans/`, `designs/`, `.ody-code/products/`, `.ody-code/game-design/`) | No |
| Switches the active model via `modeModels` | Yes | No |
| Owns an isolated context partition | Yes (`Agent._contexts[mode]`) | No |
| Provides the system prompt | No | Yes |
| Decides which tools are visible | No (except via `hiddenInModes`) | Yes |
| Can be entered/exited mid-session | Yes | No (profile is applied, not entered) |

## Current modes and handoff graph

```
        ┌─────────────────┐
        │   product  │
        └────────┬────────┘
                 │ enter/exit
                 ▼
        ┌─────────────────┐
        │   game-design   │
        └────────┬────────┘
                 │ enter/exit
                 ▼
        ┌─────────────────┐     handoffTo('plan')
        │     design      │ ───────────────────────►
        └────────┬────────┘                          │
                 │ enter/exit                        │
                 ▼                                   │
        ┌─────────────────┐                          │
        │      plan       │ ◄───────────────────────┘
        └────────┬────────┘
                 │ exit
                 ▼
        ┌─────────────────┐
        │     normal      │
        └─────────────────┘
```

- **normal**: free-form implementation; default partition.
- **plan**: write an implementation plan before coding; output goes to `.ody-code/plans/`.
- **design**: brainstorm/spec exploration; output goes to `.ody-code/designs/`; can hand off to `plan`.
- **product**: startup/builder diagnostic flow; output goes to `.ody-code/products/`.
- **game-design**: guided game-design session; output goes to `.ody-code/game-design/`.

## `SystemPromptContext.sessionMode` usage rules

Use `sessionMode` in a system prompt **only** when the text being rendered is specific to the interaction phase:

- ✅ "You are in plan mode; follow the plan-mode contract."
- ✅ "You are in design mode; do not write implementation code."
- ❌ Deciding which profile to load. Profile selection is a separate concern.
- ❌ Changing tool visibility. Use `hiddenInModes` in skill metadata instead.

## Decision matrix: adding a new mode vs adding a new profile

| If you want to… | Add a **mode** | Add a **profile** |
|---|---|---|
| Change where files are written | ✅ | ❌ |
| Change the active model alias | ✅ | ❌ |
| Add a new context partition | ✅ | ❌ |
| Change the system prompt | ❌ | ✅ |
| Change available tools | ❌ | ✅ |
| Change the agent's role/persona | ❌ | ✅ |

## Files to touch

### Adding a mode

1. `packages/agent-core/src/agent/session-mode/types.ts` — add to `SESSION_MODE_KINDS` / `RUNTIME_MODES`.
2. `packages/agent-core/src/agent/session-mode/behaviors/<mode>.ts` — implement `SessionModeBehavior`.
3. `packages/agent-core/src/agent/session-mode/behaviors/registry.ts` — register in default registry.
4. `packages/agent-core/src/agent/injection/<mode>-mode.ts` — implement `SessionModeInjector` if the mode needs injected reminders.
5. `packages/agent-core-shared/src/config.ts` — add `modeModels.<camelCaseKey>` if the mode has a dedicated model.
6. `apps/ody-code/src/tui/commands/types.ts` / `registry.ts` — update `SessionMode` and command visibility if needed.

### Adding a profile

1. Create `.ody-code/profiles/<name>.md` in the project or user profile directory.
2. Optionally create `.ody-code/profiles/<name>.toml` for tool lists.
3. No TypeScript changes required.

## Self-check questions

1. If a user enters `/plan`, which component decides that output goes to `.ody-code/plans/`?
2. Why should `SystemPromptContext.sessionMode` not be used to pick a profile?
3. Which mode can hand off to `plan`, and through which mechanism?
4. What is the difference between `SessionModeKind` and `RuntimeMode`?
5. To add a new interaction phase that needs its own model alias and output directory, would you add a mode or a profile?

## Answers

1. `PlanModeBehavior.outputSubdirectory`.
2. Profile selection is role/tool-set concern; mixing it with mode couples role to interaction phase.
3. `design` can hand off to `plan` via `DesignModeBehavior.handoffTarget`.
4. `SessionModeKind` = the four enterable interaction phases; `RuntimeMode` = `SessionModeKind` plus `normal`.
5. Add a mode.
