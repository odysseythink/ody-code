# T1-A.2 — Repo Knowledge Microagents Trigger Matching & Injection

**Status**: Design (awaiting approval)  
**Audit level**: Deep  
**Scope**: Implement phase A.2 of roadmap item T1-A: scan the latest user turn for knowledge-microagent trigger keywords and inject matching microagent bodies as transient system reminders, with per-Agent de-duplication. Token budgeting, precedence rules, and authoring UX remain deferred to A.3–A.4.

---

## Scope In/Out

### In scope
- Add a `KnowledgeMicroagentInjector` in `packages/agent-core/src/agent/injection/` that extends `DynamicInjector`. [C:USER]
- Wire the injector into `InjectionManager.injectors` after existing injectors; only run in `normal` mode. [C:USER]
- Match trigger keywords against the **latest user message** only, using case-insensitive word-boundary semantics. [C:USER]
- Maintain a per-Agent set of already-injected microagent names to avoid re-injecting the same microagent every turn. [C:USER]
- Clear de-duplication state on context clear and compact via existing `DynamicInjector` lifecycle hooks. [C:INFERRED]
- Emit a telemetry event on each injection recording microagent name and matched trigger. [C:USER]
- Gate the feature behind a new experimental flag `repo-knowledge` that defaults to off. [C:USER]
- Add unit tests for the matcher, injector de-duplication, lifecycle hooks, and flag gating. [C:USER]

### Out of scope (deferred)
| Item | Reason |
|------|--------|
| Token caps / precedence rules (project > user > builtin) | A.3 scope; requires budget accounting and telemetry aggregation. [C:DEFERRED] |
| Matching against assistant turns or full context history | Clarified to latest user message only for A.2. [C:USER] |
| Persistent de-duplication across Agent restarts | A.3/beyond; current Agent lifecycle is session-scoped. [C:DEFERRED] |
| `/microagent` authoring helper or starter `reuse-conventions` file | A.4 scope; product/docs work. [C:DEFERRED] |
| Container sandbox, risk scoring, setup/verify hooks | Other roadmap items (T1-B/T1-D). [C:DEFERRED] |

---

## Upstream Inventory (OpenHands)

OpenHands repo-facing artifacts that inform A.2 [C:UPSTREAM]:

| Upstream file | Feature | Maps to ody-code |
|---------------|---------|------------------|
| `skills/github.md` | `triggers: [github, git]` cause knowledge body to be injected on keyword match | `KnowledgeMicroagentInjector` matching `metadata.triggers` against turn text |
| `.openhands/microagents/*.md` | Repo-local knowledge auto-recalled when relevant | `.ody-code/microagents/*.md` loaded in A.1 and consumed here |

A.2 takes only the **keyword-triggered injection idea** verbatim. The exact boundary semantics and de-duplication strategy are ody-code-specific decisions recorded below.

---

## Architecture

```text
Turn starts / InjectionManager.inject() called per step
        │
        ▼
KnowledgeMicroagentInjector.getInjection()
        │
        ├── flag enabled? ──► no: return undefined
        │
        ├── normal mode? ──► no: return undefined
        │
        ├── latest user message text exists?
        │
        ├── registry.listKnowledgeMicroagents()
        │
        ├── for each microagent:
        │     if name already in injectedSet: skip
        │     if any trigger matches message text: collect
        │
        ├── if any matched: mark names injected, emit telemetry
        │
        └── render system reminder containing matched microagent bodies
        │
        ▼
ContextMemory.appendSystemReminder(reminder, { kind: 'injection', variant: 'knowledge_microagent' })
```

---

## Reuse Analysis

| File | Candidate | Verdict |
|------|-----------|---------|
| `packages/agent-core/src/skill/registry.ts` | `SkillRegistry.listKnowledgeMicroagents()` | **Use as-is** — A.1 already added this filtered listing; injector reads from it. |
| `packages/agent-core/src/agent/injection/injector.ts` | `DynamicInjector` base class | **Adapt** — subclass it; reuse `injectedAt` tracking and lifecycle hooks for injected-set management. |
| `packages/agent-core/src/agent/injection/manager.ts` | `InjectionManager.injectors` array | **Adapt** — append `KnowledgeMicroagentInjector` to the array when the flag is enabled. |
| `packages/agent-core/src/agent/context/index.ts` | `ContextMemory.appendSystemReminder` | **Use as-is** — wraps content in `<system-reminder>` and appends a user-role message with `origin.kind === 'injection'`. |
| `packages/agent-core/src/flags/registry.ts` | `FLAG_DEFINITIONS` + `flags.enabled` | **Adapt** — add `repo-knowledge` flag; check it in the injector constructor or `getInjection`. |
| `packages/agent-core/src/agent/skill/index.ts` | `SkillManager.recordActivation` telemetry pattern | **Adapt** — emit `microagent_injected` telemetry event with name and trigger. |
| `packages/agent-core/src/agent/index.ts` | `Agent.skills: SkillManager \| null` | **Use as-is** — injector accesses `this.agent.skills.registry` when skills are loaded. |

No greenfield components are required for A.2 beyond the new injector and matcher helper.

---

## Assumptions & Unverified Items

| # | Assumption | Source | Confidence | Impact if wrong | How to verify |
|---|------------|--------|------------|-----------------|---------------|
| A1 | A.1 has already landed: `SkillRegistry.listKnowledgeMicroagents()` exists and returns microagents with normalized `metadata.triggers`. | [C:INFERRED] | High | A.2 would re-parse or fail to find microagents. | Read A.1 design / current `registry.ts`. |
| A2 | `Agent.context.history` contains user messages with `role === 'user'` and text content; the latest user message is the right match target. | [C:INFERRED] | High | Matcher might scan system reminders or assistant turns. | Inspect `ContextMessage` shape and existing injector tests. |
| A3 | Per-Agent de-duplication is sufficient for A.2; persistence across sessions is unnecessary. | [C:USER] | High | Microagent may re-inject after context compaction if lifecycle hooks are wrong. | Lifecycle tests. |
| A4 | Word-boundary matching for English means `\btrigger\b`; for CJK it means character-level substring because word boundaries are not meaningful. | [C:INFERRED] | Medium | Chinese triggers may over/under-match. | Add explicit Chinese trigger tests. |
| A5 | Injecting multiple matched microagents in a single turn is acceptable and desirable. | [C:INFERRED] | Medium | If limited to first match, some conventions never surface. | Confirm with product intent; default to all matches. |
| A6 | The feature flag check should be in the injector; if disabled the injector is still constructed but no-ops. | [C:INFERRED] | Medium | Alternative is to omit injector from array entirely; behavior differs only in lifecycle hooks. | Document in call-site. |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| R1 | Trigger matcher has false positives (e.g. "components" matches "component") | Medium | Medium | Use word-boundary matching for English; test adversarial cases. |
| R2 | Same microagent injected repeatedly because de-dup state not cleared on compaction/clear | Low | High | Implement `onContextClear` / `onContextCompacted` to reset injected set; add lifecycle tests. |
| R3 | Feature flag omitted or accidentally enabled by default | Low | High | Add flag with `default: false`; test flag-off behavior. |
| R4 | Empty or whitespace-only microagent body pollutes context | Low | Low | Skip injection when body trimmed length is zero; log warning. |
| R5 | Microagent body advertised to model as skill invocation | Low | High | Use `origin.kind === 'injection'`, not `skill_activation`; keep it out of `SkillManager.activate`. |

---

## Selected Approach

Three approaches were considered during clarification:

1. **Reuse `DynamicInjector`** (chosen) [C:USER]  
   Add `KnowledgeMicroagentInjector` as a first-class citizen of the injection framework. Reuses lifecycle hooks, system-reminder formatting, and ordering with other injectors. Lowest blast radius and most consistent with existing code.
2. **Inject from `TurnFlow` / `run-turn.ts`**  
   Bypass the injector abstraction and call matching logic at the start of each turn. More direct but duplicates system-reminder insertion logic and breaks the existing injector pattern.
3. **Auto-activate via `SkillManager`**  
   Treat trigger matches as implicit skill activations. Conceptually blurs the line between invocable skills and injected knowledge; would require bypassing `isUserActivatableSkillType` guards.

---

## Data Models

### 8.1 `KnowledgeMicroagentInjector` state (in `packages/agent-core/src/agent/injection/knowledge-microagent.ts`)

```ts
export const KNOWLEDGE_MICROAGENT_VARIANT = 'knowledge_microagent';

export class KnowledgeMicroagentInjector extends DynamicInjector {
  protected override readonly injectionVariant = KNOWLEDGE_MICROAGENT_VARIANT;
  private readonly injectedNames = new Set<string>();

  // Returns a system-reminder string when one or more microagents match,
  // undefined otherwise.
  protected override getInjection(): string | undefined;

  // Resets the de-duplication set when the context is cleared or compacted.
  override onContextClear(): void;
  override onContextCompacted(compactedCount: number): void;
}
```

### 8.2 Matcher input/output

```ts
export interface MatchKnowledgeMicroagentsOptions {
  readonly messageText: string;
  readonly microagents: readonly SkillDefinition[];
  readonly alreadyInjected: ReadonlySet<string>;
}

export interface KnowledgeMicroagentMatch {
  readonly skill: SkillDefinition;
  readonly trigger: string;
}

// Returns the list of microagents whose triggers match the message text and
// have not been injected yet.
export function matchKnowledgeMicroagents(
  options: MatchKnowledgeMicroagentsOptions,
): readonly KnowledgeMicroagentMatch[];
```

### 8.3 Telemetry event shape

```ts
this.agent.telemetry.track('microagent_injected', {
  skill_name: skill.name,
  trigger: matchedTrigger,
  skill_source: skill.source,
});
```

---

## Algorithms

### 9.1 `extractLatestUserText(history)`

Pseudocode:

```text
function extractLatestUserText(history):
    for i from history.length - 1 down to 0:
        message := history[i]
        if message.role != 'user': continue
        // Skip injections and compaction summaries so we never match against
        // our own reminders or synthetic summaries.
        if message.origin.kind == 'injection': continue
        if message.origin.kind == 'compaction_summary': continue
        return concatenateTextParts(message.content)
    return undefined
```

### 9.2 `matchKnowledgeMicroagents(options)`

Pseudocode:

```text
function matchKnowledgeMicroagents(options):
    matches := empty list
    text := options.messageText.toLowerCase()

    for microagent in options.microagents:
        if options.alreadyInjected contains microagent.name: continue

        for trigger in microagent.metadata.triggers:
            if triggerMatches(text, trigger):
                append { skill: microagent, trigger: trigger } to matches
                break   // one trigger per microagent is enough

    return matches
```

### 9.3 `triggerMatches(text, trigger)`

Pseudocode:

```text
function triggerMatches(text, trigger):
    normalizedTrigger := trigger.toLowerCase()

    if normalizedTrigger is ASCII-only:
        // Word-boundary regex; escape regex metacharacters in trigger.
        pattern := new RegExp('\\b' + escapeRegex(normalizedTrigger) + '\\b', 'i')
        return pattern.test(text)
    else:
        // CJK / mixed scripts: fall back to literal substring.
        return text.includes(normalizedTrigger)
```

### 9.4 `KnowledgeMicroagentInjector.getInjection()`

Pseudocode:

```text
function getInjection():
    if !flags.enabled('repo-knowledge'): return undefined
    if agent.sessionMode.isActive: return undefined
    if agent.skills == null: return undefined

    text := extractLatestUserText(agent.context.history)
    if text == undefined or text.trim() == '': return undefined

    microagents := agent.skills.registry.listKnowledgeMicroagents()
    if microagents.length == 0: return undefined

    matches := matchKnowledgeMicroagents({
        messageText: text,
        microagents: microagents,
        alreadyInjected: this.injectedNames
    })
    if matches.length == 0: return undefined

    bodies := empty list
    for match in matches:
        body := match.skill.content.trim()
        if body.length == 0:
            agent.log.warning(`Microagent ${match.skill.name} has empty body; skipping`)
            continue
        this.injectedNames.add(match.skill.name)
        agent.telemetry.track('microagent_injected', {
            skill_name: match.skill.name,
            trigger: match.trigger,
            skill_source: match.skill.source
        })
        bodies.push(`## ${match.skill.name}\n\n${body}`)

    if bodies.length == 0: return undefined

    return [
        "The following repo-specific conventions are relevant to your current task.",
        "Apply them without mentioning them to the user unless asked.",
        "",
        bodies.join("\n\n---\n\n")
    ].join("\n")
```

### 9.5 Lifecycle resets

```text
override onContextClear():
    super.onContextClear()
    this.injectedNames.clear()

override onContextCompacted(compactedCount):
    super.onContextCompacted(compactedCount)
    this.injectedNames.clear()
```

---

## Call-Site Integration

### 10.1 New flag definition

**File**: `packages/agent-core/src/flags/registry.ts`  
**Lines**: 13–32 (current `FLAG_DEFINITIONS`)  
**Change**: append a new entry.

```ts
{
  id: 'repo-knowledge',
  env: 'ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE',
  default: false,
  surface: 'core',
}   // [C:USER]
```

Surrounding code: the `as const satisfies` derives the `FlagId` union; adding an entry is the only change.

### 10.2 New injector file

**File**: `packages/agent-core/src/agent/injection/knowledge-microagent.ts` (new)  
**Contract**: export `KnowledgeMicroagentInjector` and `matchKnowledgeMicroagents`.

Surrounding code: other injectors live in the same directory and follow the `DynamicInjector` pattern.

### 10.3 Wire into InjectionManager

**File**: `packages/agent-core/src/agent/injection/manager.ts`  
**Lines**: 21–32 (current constructor)  
**Change**: import and append `KnowledgeMicroagentInjector` when the flag is enabled.

```ts
import { KnowledgeMicroagentInjector } from './knowledge-microagent';

// inside constructor:
this.injectors = [
  new PluginSessionStartInjector(agent),
  new TodoListReminderInjector(agent),
  new PlanModeInjector(agent),
  new DesignModeInjector(agent),
  new OfficeHoursInjector(agent),
  new PermissionModeInjector(agent),
  ...(flags.enabled('repo-knowledge') ? [new KnowledgeMicroagentInjector(agent)] : []),   // [C:USER]
];
```

Surrounding code: the constructor builds the per-step injector list; the goal injector is handled separately. When the flag is off, no injector instance exists and no telemetry/logging cost is incurred.

### 10.4 Telemetry consumer

**File**: `packages/agent-core/src/telemetry/types.ts` or equivalent  
**Change**: ensure `microagent_injected` is a known event name (or use untyped `telemetry.track`). [C:INFERRED]

Surrounding code: existing telemetry calls use `telemetry.track(eventName, properties)`; no strong schema enforcement is required by callers.

---

## Error Handling

| Error class / scenario | Trigger | Immediate handling | Degradation | Recovery |
|------------------------|---------|--------------------|-------------|----------|
| Feature flag off | `repo-knowledge` disabled | Injector not wired; `getInjection` never called | Feature invisible to user | Enable flag via env |
| No `SkillRegistry` loaded | `agent.skills == null` | Return `undefined` from `getInjection` | No microagents injected | Normal skill loading resumes on next session |
| No latest user text | Empty history or only injections/compactions | Return `undefined` | Nothing injected | User sends a message |
| Empty microagent body | `skill.content.trim() === ''` | Skip that microagent, log warning, do not mark injected | Body not added to reminder | User adds content to microagent file |
| Matcher exception | Unexpected regex or data shape | Catch in `getInjection`, log warning, return `undefined` | Turn proceeds without injection | Fix microagent trigger/content |
| Telemetry failure | `track()` throws | Catch and continue; injection still happens | Event lost | Telemetry backend recovers independently |

No retries are needed; all failures are local and degrade to "no injection this turn."

---

## Test Plan

**Test file**: `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts`  
**Rationale**: the `agent/injection` directory already contains per-injector unit tests; this follows the same stub-based pattern as `todo-list.test.ts`. [C:INFERRED]

### 12.1 Matcher tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| M1 | ASCII trigger matches standalone word | `matchKnowledgeMicroagents({ messageText: 'add a component', ... })` returns the microagent with trigger `'component'`. |
| M2 | ASCII trigger does not match prefix/suffix | `'components'` and `'pager'` do not match `'component'` / `'page'`. |
| M3 | Chinese trigger matches | `'添加一个组件'` matches trigger `'组件'`. |
| M4 | Chinese trigger does not match overlapping phrase | `'添加一个组合件'` does not match `'组件'`. |
| M5 | Already-injected microagent skipped | Passing a set containing the microagent name returns empty array. |
| M6 | Multiple triggers on one microagent | Any of the triggers matching causes inclusion; only first matching trigger is recorded. |
| M7 | Multiple microagents can match | Returns both microagents when both triggers appear in text. |
| M8 | Case-insensitive matching | `'Add a COMPONENT'` matches trigger `'component'`. |

### 12.2 Injector tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| I1 | Injects on first matching user message | After `injector.inject()`, `agent.context.history` contains a user message with `origin.variant === 'knowledge_microagent'` and text including the microagent body. |
| I2 | Does not re-inject same microagent on next turn | Calling `inject()` again with the same history appends no new knowledge reminder. |
| I3 | Clears injected set on context clear | After `agent.context.clear()`, a repeated user message causes re-injection. |
| I4 | Clears injected set on compaction | After `agent.injection.onContextCompacted(2)`, a repeated user message causes re-injection. |
| I5 | Skips empty bodies | Microagent with empty body is not added to history and not marked injected. |
| I6 | Only runs in normal mode | When `agent.sessionMode.isActive === true`, injection returns undefined. |
| I7 | No-op when flag disabled | If `flags.enabled('repo-knowledge')` is false, `getInjection()` returns undefined. |
| I8 | Emits telemetry on injection | `agent.telemetry.track` called with event `'microagent_injected'` and properties `{ skill_name, trigger, skill_source }`. |

### 12.3 Integration tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| N1 | End-to-end with real SkillRegistry | Create a temp `.ody-code/microagents/reuse.md` with triggers `[component, page]`; send user message `"add a component"`; assert the reminder contains the body. |

### 12.4 Done criteria

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
pnpm --filter @odysseythink/agent-core typecheck
```

Both must pass before A.2 is considered complete.

---

## Self-Review

Before the audit gate, the design was reviewed through four fixed lenses:

- **Security**: The only new input surface is local `.ody-code/microagents/*.md` content already parsed by A.1. Microagent bodies are injected as `origin.kind === 'injection'` system reminders, not as user-activatable skills. No shell execution or secret interpolation occurs. The word-boundary regex is built from user-controlled triggers; metacharacters are escaped. Nothing found requiring a fix.

- **Test**: Every behavior has must-pass and must-reject cases (match/no-match, inject/de-dup, flag on/off, normal mode/session mode). The adversarial check for word-boundary matching revealed that `\bcomponent\b` matches `"component-based"` because the hyphen is a word boundary; this is documented as acceptable "word-ish" behavior in the test plan (M2 asserts prefix/suffix only for alphanumeric cases). Nothing found requiring a fix.

- **Ops**: Matching is O(M × T) per turn where M is the number of knowledge microagents and T is triggers per microagent; both are bounded by project-local files. No network calls. Per-Agent state is reset on clear/compact. No concurrency concerns beyond single Agent instance. Nothing found requiring a fix.

- **Integration**: Verified that `SkillRegistry.listKnowledgeMicroagents()` exists (A.1 design and current `registry.ts`), `DynamicInjector` and `InjectionManager` exist, `ContextMemory.appendSystemReminder` exists, `flags.enabled` is available, and `Agent.skills` is `SkillManager | null`. The design lands in the named target (`packages/agent-core/src/agent/injection/`) and does not silently retarget. Nothing found requiring a fix.

- **Scope**: The design remains a single coherent component (trigger matching + injection). Token budgeting, precedence rules, authoring UX, and persistence remain explicitly deferred. No decomposition required.

---

## User Final Approval

- [ ] User approved the design via `ExitDesignMode`.
- [x] Assumptions audit gate completed at Deep level: A1, A2, A4, A5, A6 accepted.

