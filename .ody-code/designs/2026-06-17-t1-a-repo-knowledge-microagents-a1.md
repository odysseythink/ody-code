# T1-A.1 — Repo Knowledge Microagents Parser

**Status**: Design (awaiting approval)  
**Audit level**: Deep  
**Scope**: Implement phase A.1 of roadmap item T1-A: parse and load `type: knowledge` microagents from `.ody-code/microagents/` into the existing `SkillRegistry`. Trigger matching, injection, budgeting, and authoring UX are explicitly deferred to A.2–A.4.

---

## Scope In/Out

### In scope
- Recognize `type: knowledge` as a supported skill/metadata type in `packages/agent-core/src/skill`. [C:USER]
- Extend `SkillMetadata` / parsing to read a `triggers` array from YAML frontmatter and validate it. [C:USER]
- Discover top-level `.md` files in `.ody-code/microagents/` as a new project-local skill root. [C:USER]
- Load parsed microagents into the existing `SkillRegistry` alongside regular skills, but keep them out of the invocable skill listing. [C:USER]
- Ensure project skills in `.ody-code/skills/` take precedence over microagents of the same name. [C:USER]
- Add unit tests covering parsing, registry filtering, discovery order, and invalid-trigger handling. [C:USER]

### Out of scope (deferred)
| Item | Reason |
|------|--------|
| Keyword trigger matching against user/assistant turns | A.2 scope; needs turn-text API and token budget design. [C:DEFERRED] |
| Context injection into the prompt | A.2 scope; depends on matcher and de-duplication logic. [C:DEFERRED] |
| Token caps / precedence rules (project > user > builtin) | A.3 scope; requires telemetry and budget accounting. [C:DEFERRED] |
| `/microagent` authoring helper or starter `reuse-conventions` file | A.4 scope; product/docs work, not parser work. [C:DEFERRED] |
| Container sandbox, risk scoring, setup/verify hooks | Other roadmap items (T1-B/T1-D). [C:DEFERRED] |

---

## 2. Upstream Inventory (OpenHands)

OpenHands repo-facing artifacts that inform this design [C:UPSTREAM]:

| Upstream file | Feature | Maps to ody-code |
|---------------|---------|------------------|
| `skills/github.md` | `type: knowledge` frontmatter + `triggers: [github, git]` | `type: knowledge` + `triggers` schema |
| `.openhands/microagents/*.md` | Repo-local knowledge microagents | `.ody-code/microagents/*.md` |

A.1 takes only the **frontmatter schema** and **repo-local directory convention** verbatim. The trigger-matching semantics (case-insensitive, word-ish boundaries) are deferred to A.2.

---

## Architecture

```text
Session start / SkillRegistry.loadRoots()
        │
        ▼
resolveSkillRoots() ──► adds `.ody-code/microagents` after `.ody-code/skills`
        │                 in PROJECT_BRAND_DIRS [C:USER]
        ▼
discoverSkills() walks both roots
        │
        ├── .md files in .ody-code/skills/  ──► parsed as prompt/inline/flow skills
        │
        └── .md files in .ody-code/microagents/ ──► parsed as knowledge microagents
                          │
                          ▼
              parseSkillText() ──► normalizeMetadata()
                          │
                          ▼
              if type === 'knowledge': parseTriggers(metadata.triggers)
                          │
                          ▼
              SkillRegistry indexes by normalized name
                          │
                          ▼
              listInvocableSkills() filters via isInlineSkillType()
                          │
                          ▼
              listKnowledgeMicroagents() filters via isKnowledgeSkillType()
```

---

## Reuse Analysis

| File | Candidate | Verdict |
|------|-----------|---------|
| `packages/agent-core/src/skill/scanner.ts` | `resolveSkillRoots`, `discoverSkills`, `parseAndRegister` | **Adapt** — add `.ody-code/microagents` to `PROJECT_BRAND_DIRS` and rely on existing skip/warn behavior for invalid files. |
| `packages/agent-core/src/skill/parser.ts` | `parseSkillText`, `parseFrontmatter`, `normalizeMetadata`, `SkillParseError` | **Adapt** — accept `type: knowledge` and add `parseTriggers` validation. |
| `packages/agent-core/src/skill/types.ts` | `SkillDefinition`, `SkillMetadata`, `isSupportedSkillType`, `isInlineSkillType`, `isUserActivatableSkillType` | **Adapt** — add `isKnowledgeSkillType` helper and include knowledge in `isSupportedSkillType`. |
| `packages/agent-core/src/skill/registry.ts` | `SkillRegistry` index-by-name, `listInvocableSkills` | **Use as-is** — `listInvocableSkills` already excludes non-inline types; add a microagent-specific listing helper. |
| `packages/agent-core/src/agent/skill/index.ts` | `SkillManager.activate` | **Use as-is** — it blocks non-user-activatable types, so knowledge microagents cannot be invoked as skills. |

No greenfield components are required for A.1.

---

## 5. Assumptions & Unverified Items

| # | Assumption | Source | Confidence | Impact if wrong | How to verify |
|---|------------|--------|------------|-----------------|---------------|
| A1 | Microagents use frontmatter `type: knowledge` exactly (not aliases). | [C:INFERRED] | High | Parser would reject valid microagents or accept wrong types. | Add test; review YAML samples. |
| A2 | `triggers` should be stored trimmed, lowercased, and deduplicated at parse time. | [C:INFERRED] | Medium | A.2 matcher may need re-normalization or miss duplicates. | Unit test asserts normalized output. |
| A3 | `.ody-code/microagents/` is project-local only (no user-home mirror). | [C:INFERRED] | Medium | Users cannot have per-user microagents; may need user root later. | Confirm with roadmap / user in A.4 review. |
| A4 | Existing `SkillRegistry` first-wins semantics plus root ordering (skills before microagents) satisfies "skill priority". | [C:INFERRED] | High | A microagent could shadow a skill if ordering changes. | Add collision test. |
| A5 | No experimental flag is needed because A.1 only adds parsing and does not inject context. | [C:USER] | High | If policy changes, gated flag can be added later. | Confirmed in clarifying question. |

---

## 6. Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| R1 | `type: knowledge` files accidentally parsed where regular skills expected | Low | Medium | Keep `isInlineSkillType` unchanged; knowledge is supported but not invocable. |
| R2 | Invalid `triggers` cause a hard crash instead of graceful skip | Low | Medium | Reuse `parseAndRegister` catch block in `scanner.ts`; add test. |
| R3 | Microagent names collide with skills and shadow them | Low | High | Load `.ody-code/skills/` before `.ody-code/microagents/`; add collision test. |
| R4 | Future A.2 matcher assumes triggers stored differently than A.1 | Medium | Low | Document normalization in design; tests lock the shape. |

---

## 7. Selected Approach

Three approaches were considered during clarification:

1. **Reuse `SkillRegistry`** (chosen) [C:USER]  
   Treat microagents as `SkillDefinition` objects with `type: knowledge`, reuse discovery, parsing, and indexing. Add a filter helper for consumers. Lowest blast radius, minimal new code.
2. **Separate `MicroagentRegistry`**  
   Keep skills untouched; build a parallel registry. Cleaner conceptual separation, but duplicates discovery/loading logic for A.1.
3. **Ad-hoc parser only, no registry integration**  
   Parse files on demand at injection time. Simplest for A.1, but leaves no durable catalog for A.2–A.4 and re-parses on every turn.

---

## Data Models

### 8.1 Extended `SkillMetadata`

```ts
export interface SkillMetadata {
  readonly name?: string | undefined;
  readonly description?: string | undefined;
  readonly type?: string | undefined;
  readonly whenToUse?: string | undefined;
  readonly disableModelInvocation?: boolean | undefined;
  readonly hiddenInModes?: readonly string[] | undefined;
  readonly safe?: boolean | undefined;
  readonly arguments?: readonly unknown[] | string | undefined;
  readonly triggers?: readonly string[] | undefined;   // [C:USER] only meaningful for knowledge microagents
  readonly [key: string]: unknown;
}
```

### 8.2 Type helpers (in `packages/agent-core/src/skill/types.ts`)

```ts
// Returns true only for explicit knowledge microagents.
export function isKnowledgeSkillType(type: string | undefined): boolean {
  return type === 'knowledge';
}

// Knowledge is supported but NOT user-activatable and NOT inline.
export function isSupportedSkillType(type: string | undefined): boolean {
  return isUserActivatableSkillType(type) || isKnowledgeSkillType(type);   // [C:USER]
}
```

### 8.3 Registry listing helper (in `packages/agent-core/src/skill/registry.ts`)

```ts
listKnowledgeMicroagents(): readonly SkillDefinition[] {
  return this.listSkills().filter((skill) => isKnowledgeSkillType(skill.metadata.type));
}
```

---

## Algorithms

### 9.1 `parseTriggers(raw: unknown): readonly string[]`

Pseudocode:

```text
function parseTriggers(raw):
    if raw is not array OR raw.length == 0:
        throw SkillParseError("microagent 'triggers' must be a non-empty array of strings")

    seen := empty set
    result := empty list

    for item in raw:
        if item is not string OR item.trim() == "":
            throw SkillParseError("each trigger must be a non-empty string")
        normalized := item.trim().toLowerCase()
        if normalized already in seen:
            continue
        add normalized to seen
        append normalized to result

    return result sorted lexicographically
```

### 9.2 Integration in `parseSkillText`

After `const metadata = normalizeMetadata(frontmatter)` and the existing `isSupportedSkillType` check:

```text
if metadata.type == 'knowledge':
    metadata.triggers := parseTriggers(metadata.triggers)
```

For non-knowledge skills, `triggers` is left untouched (it will be ignored by callers).

### 9.3 Discovery order

`PROJECT_BRAND_DIRS` is updated to:

```ts
const PROJECT_BRAND_DIRS = ['.ody-code/skills', '.ody-code/microagents'] as const;   // [C:USER]
```

Because `pushBrandGroup` iterates in array order, `.ody-code/skills/` is registered first; a skill and microagent sharing the same file name will result in the skill winning via the existing first-wins map insertion in `discoverSkills`.

---

## 10. Call-Site Integration

### 10.1 Root discovery

**File**: `packages/agent-core/src/skill/scanner.ts`  
**Lines**: 8–10 (current)  
**Change**: append `.ody-code/microagents` to `PROJECT_BRAND_DIRS`.

```ts
const PROJECT_BRAND_DIRS = ['.ody-code/skills', '.ody-code/microagents'] as const;   // [C:USER]
```

Surrounding code: `resolveSkillRoots()` calls `pushBrandGroup(..., PROJECT_BRAND_DIRS, ..., 'project', mergeAllAvailableSkills, ...)` for the project source. No other changes needed; the existing `walkSkillDir` already scans top-level `.md` files.

### 10.2 Type support

**File**: `packages/agent-core/src/skill/types.ts`  
**Lines**: 65–75 (current)  
**Change**: add `isKnowledgeSkillType` and update `isSupportedSkillType`.

```ts
export function isKnowledgeSkillType(type: string | undefined): boolean {
  return type === 'knowledge';
}

export function isSupportedSkillType(type: string | undefined): boolean {
  return isUserActivatableSkillType(type) || isKnowledgeSkillType(type);
}
```

Surrounding code: `isInlineSkillType` and `isUserActivatableSkillType` remain unchanged, keeping knowledge microagents out of invocation paths.

### 10.3 Trigger validation

**File**: `packages/agent-core/src/skill/parser.ts`  
**Lines**: 135–148 (current `parseSkillText`)  
**Change**: after the supported-type check, validate triggers for knowledge microagents.

```ts
const metadata = normalizeMetadata(frontmatter);
if (!isSupportedSkillType(metadata.type)) {
  throw new UnsupportedSkillTypeError(metadata.type ?? String(frontmatter['type']));
}

if (metadata.type === 'knowledge') {
  metadata.triggers = parseTriggers(metadata.triggers);
}
```

Surrounding code: the function continues to derive `name`, `description`, `content`, etc., exactly as for regular skills.

### 10.4 Registry helper

**File**: `packages/agent-core/src/skill/registry.ts`  
**Lines**: after `listInvocableSkills()` (around 134)  
**Change**: add `listKnowledgeMicroagents()`.

```ts
listKnowledgeMicroagents(): readonly SkillDefinition[] {
  return this.listSkills().filter((skill) => isKnowledgeSkillType(skill.metadata.type));
}
```

Surrounding code: `SkillRegistry` already indexes all loaded skills by normalized name; this is a read-only filtered view.

---

## Error Handling

| Error class | Trigger | Immediate handling | Degradation | Recovery |
|-------------|---------|--------------------|-------------|----------|
| `FrontmatterError` | Invalid YAML fence or YAML parse | Wrapped in `SkillParseError` by `parseSkillText` | File skipped with warning | User fixes frontmatter |
| `UnsupportedSkillTypeError` | `type` not supported | Caught in `parseAndRegister`; file added to skipped list | Not loaded | User fixes `type` |
| `SkillParseError` (invalid `triggers`) | Missing/non-array/empty trigger for `type: knowledge` | Caught in `parseAndRegister`; warning logged | Microagent not loaded | User fixes `triggers` |
| Name collision | Skill and microagent share normalized name | First-wins via existing map insertion order | Later file ignored (skill wins) | Rename one of them |

No retries are needed; all failures are local parse/discovery errors.

---

## 12. Test Plan

**Test file**: `packages/agent-core/test/skill/microagent-parser.test.ts`  
**Rationale**: the `skill` component currently has no dedicated parser unit test; adding one focused file keeps the new assertions together. [C:INFERRED]

### 12.1 Parser tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| P1 | valid knowledge microagent | `skill.metadata.type === 'knowledge'`; `skill.metadata.triggers` deep-equals `['component', 'page', '模块']`; `skill.content` equals body. |
| P2 | triggers normalized (case + whitespace + duplicates) | Input `[' Page ', 'PAGE', 'component']` → output `['component', 'page']`. |
| P3 | missing triggers rejected | `parseSkillText` throws `SkillParseError` whose message contains `triggers`. |
| P4 | empty trigger string rejected | `parseSkillText` throws `SkillParseError`. |
| P5 | non-array triggers rejected | `parseSkillText` throws `SkillParseError`. |

### 12.2 Registry tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| R1 | invocable skills exclude knowledge | After loading a knowledge microagent, `registry.listInvocableSkills()` does not contain it. |
| R2 | knowledge listing helper | `registry.listKnowledgeMicroagents()` returns exactly the loaded knowledge microagent. |
| R3 | skill wins over same-name microagent | Load `foo.md` skill and `foo.md` microagent; `registry.getSkill('foo').metadata.type === 'prompt'` (or skill type). |

### 12.3 Discovery tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| D1 | `.ody-code/microagents/` root discovered | `resolveSkillRoots({ workDir: tmp })` returns a root whose path ends with `.ody-code/microagents` and source `project`. |
| D2 | microagents loaded via `discoverSkills` | A temporary `.ody-code/microagents/reuse.md` is parsed and indexed. |
| D3 | invalid microagent skipped with warning | `onWarning` is called once for a microagent with bad triggers; `discoverSkills` returns no entry for it. |

### 12.4 Activation guard test

| # | Test | Must-pass assertions |
|---|------|----------------------|
| A1 | knowledge microagent cannot be activated | `SkillManager.activate({ name: 'reuse' })` throws `OdyError` with code `SKILL_TYPE_UNSUPPORTED`. |

### 12.5 Done criteria

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/skill/microagent-parser.test.ts
pnpm --filter @odysseythink/agent-core typecheck
```

Both must pass before A.1 is considered complete.

---

## Self-Review

Before the audit gate, the design was reviewed through four fixed lenses:

- **Security**: The only new input surface is local `.ody-code/microagents/*.md` frontmatter. `triggers` values are normalized (trim/lowercase) but never executed or interpolated into shell commands. No secrets or PII are logged beyond existing warning paths that already include file paths. Nothing found requiring a fix.

- **Test**: Every behavior in scope has a must-pass and a must-reject case (valid/invalid triggers, inclusion/exclusion from invocation list, skill priority). The must-pass case for trigger normalization (`[' Page ', 'PAGE', 'component']` → `['component', 'page']`) was verified with an ephemeral check to ensure `trim().toLowerCase()` and set deduplication produce the expected order. Nothing found requiring a fix.

- **Ops**: Discovery is a one-time cost at session start; adding one extra directory root is O(files in `.ody-code/microagents/`). Identifier collisions are handled by existing first-wins insertion order. No retry, backoff, or concurrency concerns. Nothing found requiring a fix.

- **Integration**: Verified that `SkillRegistry.listInvocableSkills` already filters with `isInlineSkillType` (line 123), `SkillManager.activate` already blocks non-user-activatable types (line 23), and `discoverSkills` already catches `SkillParseError` in `parseAndRegister` (lines 364–375). The parser already supports arbitrary metadata keys via `SkillMetadata['[key: string]: unknown']`. No missing hook points. Nothing found requiring a fix.

- **Scope**: The design remains a single coherent component (skill parser/registry extension). Injection, matching, budgeting, and authoring UX are explicitly deferred. No decomposition required.

---

## User Final Approval

- [ ] User approved the design via `ExitDesignMode`.
- [x] Assumptions audit gate completed at Deep level (A1–A4 accepted; A5 was [C:USER]).

---

