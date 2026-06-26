# T1-A.3 — Repo Knowledge Microagents Precedence & Budgeting

**Status**: Design (awaiting approval)  
**Audit level**: Deep  
**Scope**: Implement phase A.3 of roadmap item T1-A: add source-precedence ordering and a per-injection token budget to the knowledge-microagent injector that A.2 already built. Authoring UX and persistent cross-session state remain deferred to A.4.

---

## Scope In/Out

### In scope
- Add a `microagentBudget.maxTokens` configuration field to `OdyConfig` / `OdyConfigPatch`, defaulting to `1024`; `0` means unlimited. [C:USER]
- Read the budget from `agent.kimiConfig?.microagentBudget?.maxTokens` inside `KnowledgeMicroagentInjector`. [C:USER]
- Sort matched microagents by source precedence: `project > user > extra > builtin`; within the same source, sort by `name` lexicographically. [C:USER]
- Apply a single-injection token budget using `estimateTokens(skill.content)` on the trimmed microagent body. [C:USER]
- Skip complete lower-priority microagents when the budget would be exceeded; never truncate a body. [C:USER]
- Emit `microagent_injected` telemetry with `budget_used` and `budget_total` properties. [C:USER]
- Emit `microagent_skipped` telemetry with `reason: 'budget_exceeded'` for each omitted microagent. [C:USER]
- Append a short "omitted due to budget" note to the rendered reminder when any microagent is skipped. [C:USER]
- Update `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts` with precedence, budget, and telemetry assertions. [C:USER]

### Out of scope (deferred)
| Item | Reason |
|------|--------|
| Per-source or per-microagent budgets | Increases config surface; defer until telemetry shows it is needed. [C:DEFERRED] |
| Cross-turn / Agent-lifetime cumulative budget | A.3 scopes the cap to a single injection turn; cumulative accounting adds state machine complexity. [C:DEFERRED] |
| Truncating a single microagent to fit | User explicitly chose whole-microagent skipping to preserve markdown semantics. [C:USER] |
| Persistent budget state across sessions | Requires session-store schema changes; A.4/beyond. [C:DEFERRED] |
| UI/CLI status panel showing budget usage | Product-facing work, out of A.3 core-injector scope. [C:DEFERRED] |
| Changing A.2 trigger-matching or de-duplication semantics | A.3 layers on top without altering A.2 behavior. [C:USER] |

---

## Prior Art

This phase extends the A.2 injector; no new upstream feature is ported. The OpenHands repo-facing artifacts (`.openhands/microagents/*.md`) do not define token budgets or source precedence, so A.3 is an ody-code-specific guardrail rather than a mirror of upstream behavior. [C:INFERRED]

---

## Architecture

```text
Turn starts / InjectionManager.inject() called per step
        │
        ▼
KnowledgeMicroagentInjector.getInjection()
        │
        ├── flag enabled? ──► no: return undefined
        ├── normal mode? ──► no: return undefined
        ├── latest user message text exists?
        ├── registry.listKnowledgeMicroagents()
        ├── matchKnowledgeMicroagents(messageText, microagents, alreadyInjected)
        │
        ├── NEW: read maxTokens from agent.kimiConfig (default 1024)
        ├── NEW: sortBySourcePriority(matches)
        │        project (0) > user (1) > extra (2) > builtin (3)
        │        tie-breaker: skill.name localeCompare
        │
        ├── NEW: applyBudget(sortedMatches, maxTokens)
        │        loop: estimateTokens(body)
        │        fits? → inject, accumulate budget_used
        │        no?   → skip, emit microagent_skipped
        │
        ├── render reminder with injected bodies
        └── if skipped.length > 0: append omitted-note
        │
        ▼
ContextMemory.appendSystemReminder(reminder, { kind: 'injection', variant: 'knowledge_microagent' })
```

---

## Reuse Analysis

| File | Candidate | Verdict |
|------|-----------|---------|
| `packages/agent-core/src/utils/tokens.ts` | `estimateTokens(text)` | **Use as-is** — heuristic token estimator already used by compaction and context-size checks. |
| `packages/agent-core/src/config/schema.ts` | `OdyConfigSchema` / `OdyConfigPatchSchema` | **Adapt** — add `MicroagentBudgetConfigSchema` and wire it into both schemas. |
| `packages/agent-core/src/agent/injection/knowledge-microagent.ts` | `KnowledgeMicroagentInjector` | **Adapt** — extend `getInjection()` with sorting and budget logic; matcher and extraction helpers stay unchanged. |
| `packages/agent-core/src/agent/injection/injector.ts` | `DynamicInjector` base class | **Use as-is** — lifecycle hooks and `appendSystemReminder` wiring are sufficient. |
| `packages/agent-core/src/telemetry.ts` | `TelemetryClient.track` | **Use as-is** — add new event names/properties without changing the interface. |
| `packages/agent-core/src/skill/types.ts` | `SkillSource` union | **Use as-is** — `project \| user \| extra \| builtin` drives the priority map. |

No greenfield components are required for A.3 beyond the new config schema and the budget helper functions inside the existing injector file.

---

## Assumptions & Unverified Items

| # | Assumption | Source | Confidence | Impact if wrong | How to verify |
|---|------------|--------|------------|-----------------|---------------|
| A1 | `agent.kimiConfig` is available and carries user/project config values inside the Agent instance; reading `agent.kimiConfig.microagentBudget.maxTokens` is the right path. | [C:INFERRED] | High | Budget would fall back to default even when configured. | Inspect `Agent` constructor and `ConfigState` usage; verified `kimiConfig` field exists. |
| A2 | `estimateTokens` is an acceptable estimator for microagent body budgeting; its ~4 chars/ASCII heuristic is "good enough" for cap purposes. | [C:INFERRED] | Medium | Budget may under/over-shoot real tokenizer count by 10-30%. | Document as heuristic; no functional break. |
| A3 | Sorting by source precedence (`project > user > extra > builtin`) and then by name gives deterministic, user-predictable results. | [C:USER] | High | If users expect trigger-order or file-order, behavior feels non-deterministic. | Tests assert the chosen order. |
| A4 | A single-injection cap is sufficient; no per-Agent or per-session cumulative budget is needed for A.3. | [C:USER] | High | Without cumulative cap, a long session with many distinct triggers could still inject many tokens over time. | Roadmap explicitly says "cap injected microagent tokens" and A.3 is scoped to per-turn. |
| A5 | The existing `microagent_injected` telemetry event can safely carry extra numeric properties without breaking downstream consumers. | [C:INFERRED] | Medium | Telemetry schema consumers may ignore unknown properties or fail if strict. | Use flat numeric properties; no schema enforcement in code. |
| A6 | Appending an "omitted due to budget" note to the model-visible reminder does not itself push the prompt over the cap in a harmful way. | [C:INFERRED] | Medium | The note adds tokens but is bounded (list of names); acceptable overhead. | Count omitted names; cap note length if necessary in implementation. |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| R1 | Heuristic `estimateTokens` underestimates body size, causing real prompt to exceed intended budget | Medium | Low-Medium | Document as heuristic cap; default 1024 is conservative; user can lower it. |
| R2 | Source-priority order surprises users who expect their user-level microagent to override project-level | Low | Medium | Order is explicit and documented; users can raise budget to include both. |
| R3 | Skipped list in reminder adds noise to the model context | Low | Low | List only names, not bodies; omit entirely if skipped count is large (implementation detail). |
| R4 | Config schema mismatch (`OdyConfigPatchSchema` missing field) silently drops user config | Low | High | Add field to both `OdyConfigSchema` and `OdyConfigPatchSchema`; add schema test. |
| R5 | Budget logic suppresses all microagents when a single large project microagent exceeds cap | Medium | Medium | By design; logged/telemetry'd so user can see why and adjust budget or split microagent. |

---

## Selected Approach

**Approach A — Extend the existing injector** (chosen) [C:USER]
Keep all precedence and budget logic inside `KnowledgeMicroagentInjector.getInjection()`. Add pure helper functions `sortBySourcePriority` and `applyBudget` in the same file. Add `MicroagentBudgetConfigSchema` to `config/schema.ts`. This is the smallest change that satisfies all clarified requirements and reuses A.2 without new abstractions.

Alternatives considered:
- **Approach B — Separate `MicroagentBudget` class**: cleaner separation but introduces a new abstraction for a single call-site.
- **Approach C — Budget inside matcher**: makes `matchKnowledgeMicroagents` responsible for budgeting, conflating matching with resource policy.

---

## Data Models

### 6.1 New config schema

```ts
export const MicroagentBudgetConfigSchema = z.object({
  maxTokens: z.number().int().min(0).optional(),
});
export type MicroagentBudgetConfig = z.infer<typeof MicroagentBudgetConfigSchema>;
```

Wired as `microagentBudget: MicroagentBudgetConfigSchema.optional()` in both `OdyConfigSchema` and `OdyConfigPatchSchema`. [C:USER]

### 6.2 Budget result type (inside injector file)

```ts
interface MicroagentBudgetResult {
  readonly injected: readonly KnowledgeMicroagentMatch[];
  readonly skipped: readonly SkippedMicroagent[];
  readonly used: number;
  readonly total: number;   // Infinity represented as 0 for telemetry? Or store as number
}

interface SkippedMicroagent {
  readonly match: KnowledgeMicroagentMatch;
  readonly reason: 'budget_exceeded';
}
```

### 6.3 Telemetry event shapes

```ts
// microagent_injected (augmented from A.2)
{
  skill_name: string;
  trigger: string;
  skill_source: SkillSource;
  budget_used: number;
  budget_total: number;   // 0 when unlimited
}

// microagent_skipped (new)
{
  skill_name: string;
  trigger: string;
  skill_source: SkillSource;
  reason: 'budget_exceeded';
  budget_used: number;
  budget_total: number;
}
```

---

## Algorithms

### 7.1 `resolveBudgetLimit(agent)`

```text
function resolveBudgetLimit(agent):
    configured := agent.kimiConfig?.microagentBudget?.maxTokens
    if configured === undefined: return 1024     // default [C:USER]
    if configured === 0: return Infinity         // unlimited [C:USER]
    return configured
```

### 7.2 `sortBySourcePriority(matches)`

```text
priorityMap := { project: 0, user: 1, extra: 2, builtin: 3 }

function sortBySourcePriority(matches):
    return matches.toSorted((a, b) =>
        let pa := priorityMap[a.skill.source]
        let pb := priorityMap[b.skill.source]
        if pa != pb: return pa - pb
        return a.skill.name.localeCompare(b.skill.name)
    )
```

### 7.3 `applyBudget(sortedMatches, maxTokens)`

```text
function applyBudget(sortedMatches, maxTokens):
    budget := (maxTokens === Infinity) ? Infinity : maxTokens
    used := 0
    injected := []
    skipped := []

    for match in sortedMatches:
        body := match.skill.content.trim()
        if body.length === 0:
            continue            // A.2 already warns; do not count toward budget

        tokens := estimateTokens(body)
        if used + tokens <= budget:
            used += tokens
            injected.push(match)
        else:
            skipped.push({ match, reason: 'budget_exceeded' })

    return { injected, skipped, used, total: budget }
```

### 7.4 `KnowledgeMicroagentInjector.getInjection()` (delta over A.2)

```text
function getInjection():
    if !flags.enabled('repo-knowledge'): return undefined
    if agent.sessionMode.isActive: return undefined
    if agent.skills == null: return undefined

    text := extractLatestUserText(agent.context.history)
    if text == undefined or text.trim() == '': return undefined

    microagents := agent.skills.registry.listKnowledgeMicroagents()
    if microagents.length == 0: return undefined

    matches := matchKnowledgeMicroagents({ messageText: text, microagents, alreadyInjected: this.injectedNames })
    if matches.length == 0: return undefined

    maxTokens := resolveBudgetLimit(this.agent)
    sorted := sortBySourcePriority(matches)
    budget := applyBudget(sorted, maxTokens)

    if budget.injected.length == 0 and budget.skipped.length == 0: return undefined

    bodies := []
    for match in budget.injected:
        this.injectedNames.add(match.skill.name)
        this.agent.telemetry.track('microagent_injected', {
            skill_name: match.skill.name,
            trigger: match.trigger,
            skill_source: match.skill.source,
            budget_used: budget.used,
            budget_total: budget.total === Infinity ? 0 : budget.total,
        })
        bodies.push(`## ${match.skill.name}\n\n${match.skill.content.trim()}`)

    for skipped in budget.skipped:
        this.agent.telemetry.track('microagent_skipped', {
            skill_name: skipped.match.skill.name,
            trigger: skipped.match.trigger,
            skill_source: skipped.match.skill.source,
            reason: skipped.reason,
            budget_used: budget.used,
            budget_total: budget.total === Infinity ? 0 : budget.total,
        })

    if bodies.length == 0: return undefined

    lines := [
        "The following repo-specific conventions are relevant to your current task.",
        "Apply them without mentioning them to the user unless asked.",
        "",
        bodies.join("\n\n---\n\n")
    ]

    if budget.skipped.length > 0:
        omittedNames := budget.skipped.map(s => s.match.skill.name).join(", ")
        lines.push("")
        lines.push(`The following conventions were omitted due to the microagent token budget: ${omittedNames}.`)

    return lines.join("\n")
```

---

## Call-Site Integration

### 8.1 Config schema additions

**File**: `packages/agent-core/src/config/schema.ts`  
**Lines**: 296–326 (`OdyConfigSchema` definition), 348–378 (`OdyConfigPatchSchema` definition)  
**Change**: add `microagentBudget: MicroagentBudgetConfigSchema.optional()` to both objects.

```ts
export const MicroagentBudgetConfigSchema = z.object({
  maxTokens: z.number().int().min(0).optional(),
});

export const OdyConfigSchema = z.object({
  // ... existing fields ...
  microagentBudget: MicroagentBudgetConfigSchema.optional(),
});

export const OdyConfigPatchSchema = z.object({
  // ... existing fields ...
  microagentBudget: MicroagentBudgetConfigSchema.optional(),
}).strict();
```

Surrounding code: other optional top-level config fields; add in alphabetical order near `mergeAllAvailableSkills` / `modeModels` to keep the schema readable. [C:INFERRED]

### 8.2 Injector extension

**File**: `packages/agent-core/src/agent/injection/knowledge-microagent.ts`  
**Lines**: 136–178 (current `getInjection`)  
**Change**: insert sorting/budget helpers and use them between `matchKnowledgeMicroagents` and the rendering loop.

No changes to `InjectionManager` or feature flag wiring are required; A.2 already wires the injector. [C:USER]

### 8.3 Telemetry consumer

**File**: `packages/agent-core/src/telemetry.ts`  
**Change**: none; `TelemetryClient.track` is untyped and accepts the new properties. [C:INFERRED]

---

## Error Handling

| Error class / scenario | Trigger | Immediate handling | Degradation | Recovery |
|------------------------|---------|--------------------|-------------|----------|
| Config field missing | `microagentBudget` not set | Use default 1024 | Normal budget | User edits config |
| `maxTokens == 0` | User explicitly sets 0 | Treat as unlimited (`Infinity`) | No cap | User sets positive value |
| Single microagent exceeds entire budget | `estimateTokens(body) > maxTokens` | Skip it, emit `microagent_skipped`, include name in reminder note | That convention not shown | User raises budget or shortens microagent |
| All matched microagents exceed budget | Every body too large | Return `undefined` (no reminder) or a note-only reminder | No injection this turn | User adjusts budget/content |
| `estimateTokens` throws | Unexpected input | Catch, log warning, treat that body as unbudgeted/skip it | Slight over-budget risk | Fix input |
| Telemetry failure | `track()` throws | Catch and continue | Event lost | Telemetry backend recovers |

No retries are needed; all failures degrade to "inject less or nothing this turn."

---

## Test Plan

**Test file**: `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts`  
**Rationale**: A.3 is a behavioral extension of A.2; the same stub-based injector tests can assert budget and precedence behavior. [C:INFERRED]

### 10.1 Precedence tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| P1 | `project` wins over `builtin` under budget | With `maxTokens` fitting only one, reminder contains the `project` microagent body and `microagent_skipped` event names the `builtin` one. |
| P2 | `user` wins over `extra` under budget | Same pattern as P1 for `user` vs `extra`. |
| P3 | Same-source tie-breaker is name lexicographic | Two `project` microagents `alpha` and `beta` both fit; reminder lists `alpha` before `beta`. |

### 10.2 Budget tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| B1 | Default budget (1024) caps injection | `project` body 900 tokens + `user` body 300 tokens → only `project` injected; `user` skipped. |
| B2 | `maxTokens = 0` disables cap | Multiple large bodies all injected; no `microagent_skipped` events. |
| B3 | Custom `maxTokens` works | Set `maxTokens = 200`; only bodies ≤200 tokens inject. |
| B4 | Single oversized body skipped | One 2000-token body with budget 1024 → no injection, reminder note lists the omitted name. |
| B5 | Budget usage telemetry | `microagent_injected` properties include `budget_used` (number) and `budget_total` (number). |
| B6 | Skipped telemetry | `microagent_skipped` event has `reason === 'budget_exceeded'`, `skill_name`, `skill_source`, `budget_used`, `budget_total`. |
| B7 | Reminder includes omitted-note | When any microagent is skipped, reminder text contains "The following conventions were omitted due to the microagent token budget" and the skipped names. |
| B8 | Empty bodies still not counted | Empty-body microagent is skipped without telemetry and does not consume budget. |

### 10.3 Config schema tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| C1 | `OdyConfigSchema` accepts `microagentBudget.maxTokens` | `validateConfig({ microagentBudget: { maxTokens: 512 } })` succeeds. |
| C2 | `OdyConfigSchema` rejects negative tokens | `validateConfig({ microagentBudget: { maxTokens: -1 } })` throws. |
| C3 | `OdyConfigPatchSchema` accepts the field | `OdyConfigPatchSchema.parse({ microagentBudget: { maxTokens: 0 } })` succeeds. |

### 10.4 Done criteria

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/agent/injection/knowledge-microagent.test.ts
pnpm --filter @odysseythink/agent-core typecheck
```

Both must pass before A.3 is considered complete.

---

## Self-Review

Before the audit gate, the design was reviewed through four fixed lenses:

- **Security**: No new external input surface beyond the existing local `.ody-code/microagents/*.md` files. The only new user-controlled values are numeric `maxTokens` and already-parsed microagent bodies; both are handled with existing schema/trimming. The omitted-note only lists microagent names, not paths or secrets. Nothing found requiring a fix.

- **Test**: Every behavior has must-pass and must-reject cases. Adversarial checks for the budget predicate are below. One potential contradiction: a skipped list could itself exceed a very small budget if many names are omitted; the note is bounded by name length and count, and the cap applies to bodies, not the note. Documented as acceptable overhead. Nothing found requiring a fix.

- **Ops**: Sorting is O(M log M) per turn where M is the number of matched microagents; budget loop is O(M). `estimateTokens` is O(L) over body length. No network calls. No persistent state. Per-Agent de-dup state is unchanged from A.2. Nothing found requiring a fix.

- **Integration**: Verified that `estimateTokens` exists in `src/utils/tokens.ts`, `SkillSource` exists in `src/skill/types.ts`, `agent.kimiConfig` exists on the `Agent` class, `OdyConfigSchema`/`OdyConfigPatchSchema` exist in `src/config/schema.ts`, and the A.2 injector exists at `src/agent/injection/knowledge-microagent.ts`. The design lands at the named target and does not silently retarget. Nothing found requiring a fix.

- **Scope**: The design remains a single coherent component (budgeting + precedence on top of A.2). Per-source budgets, cumulative budgets, content truncation, and authoring UX remain explicitly deferred. No decomposition required.

### Adversarial verification

Three expensive decisions were tested:

1. **Budget predicate** (`used + tokens <= maxTokens`).
   - Input: maxTokens=100, bodies=[50, 51]. Expected: first injected, second skipped. Verified mentally: 50+51=101 > 100 → skip.
   - Input: maxTokens=0. Expected: all injected (unlimited). Handled by `Infinity` branch.
   - Input: maxTokens=100, single body=100. Expected: injected exactly.

2. **Source priority map**.
   - Input: sources [`builtin`, `project`, `user`, `extra`]. Expected order: `project`, `user`, `extra`, `builtin`.
   - Input: same source [`project:banana`, `project:apple`]. Expected order: `apple`, `banana`.
   - Input: empty matches. Expected: empty output.

3. **`0` means unlimited**.
   - Input: maxTokens=0, total body tokens=5000. Expected: all injected, `budget_total` telemetry = 0.
   - Input: maxTokens=undefined. Expected: cap=1024.
   - Input: maxTokens=-1. Expected: schema rejects (not a runtime path).

No contradictions found.

---

## User Final Approval

- [ ] User approved the design via `ExitDesignMode`.
- [x] Assumptions audit gate completed at Deep level: A1, A2, A3, A4, A5, A6 accepted.
