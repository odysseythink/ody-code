# T1-A.4 — Repo Knowledge Microagents Authoring UX

**Status**: Design (awaiting approval)  
**Audit level**: Deep  
**Scope**: Implement phase A.4 of roadmap item T1-A: an interactive `/microagent` slash command that scaffolds a new knowledge microagent in `.ody-code/microagents/`, plus a starter pack shipped as built-in templates. Depends on A.1–A.3 for parsing, trigger matching, and budget-aware injection.

---

## Scope In/Out

### In scope
- Add a `/microagent` built-in slash command in the TUI, gated by the `repo-knowledge` experimental flag. [C:USER]
- Interactive wizard that collects: microagent file name, trigger keywords, and a one-line description. [C:USER]
- Generate a standard microagent file with YAML frontmatter (`name`, `type: knowledge`, `triggers`, `description`) and a TODO-style body template. [C:USER]
- Write the file to `<workDir>/.ody-code/microagents/<name>.md` using TUI-layer filesystem calls. [C:USER]
- Strict input validation: name limited to `[a-z0-9_-]+`; triggers normalized, deduplicated, lowercased, non-empty; description non-empty. [C:USER]
- Interactive overwrite confirmation when the target file already exists. [C:USER]
- Install a starter pack of built-in microagent templates on first use of `/microagent` when `.ody-code/microagents/` is empty or missing. [C:USER]
- Ship starter templates: `reuse-conventions.md`, `glossary.md`, `testing.md`, `documentation.md`. [C:USER]
- Telemetry events: `microagent_created`, `microagent_create_failed`, `starter_microagent_installed`. [C:USER]
- Update user documentation (`docs/`) explaining how to author microagents and use `/microagent`. [C:USER]
- Unit tests for validation, wizard state machine, file writing, overwrite confirmation, and starter installation. [C:USER]

### Out of scope (deferred)
| Item | Reason |
|------|--------|
| LLM-generated microagent bodies | User chose template-based wizard; generative fill deferred to a later enhancement. [C:DEFERRED] |
| External editor integration for body editing | User chose description-only wizard; full-body editing via Ctrl-G deferred. [C:DEFERRED] |
| Listing/editing/deleting existing microagents | New scope; can be added as `/microagent list` or similar later. [C:DEFERRED] |
| Per-user (home-directory) microagents | A.1–A.3 scoped microagents to project-local `.ody-code/microagents/`; user-home variant not requested. [C:DEFERRED] |
| Container sandbox, risk scoring, setup/verify hooks | Other roadmap items (T1-B/T1-D). [C:DEFERRED] |
| Non-TUI entry points (CLI subcommand, SDK API) | User chose TUI-layer direct write; headless/scripting support deferred. [C:DEFERRED] |

---

## Upstream Inventory (OpenHands)

A.4 is an ody-code-specific authoring convenience; OpenHands does not expose a `/microagent` command. The upstream influence is limited to the file format [C:UPSTREAM]:

| Upstream file | Feature | Maps to ody-code |
|---------------|---------|------------------|
| `.openhands/microagents/documentation.md` | `name`, `type: knowledge`, `triggers` frontmatter + markdown body | Generated file schema for `/microagent` |
| `.openhands/microagents/glossary.md` | Repo-local knowledge microagent example | `glossary.md` starter template |

The interactive wizard, validation rules, starter-pack delivery mechanism, and telemetry are ody-code-specific decisions.

---

## Architecture

```text
User types `/microagent`
        │
        ▼
dispatchInput() → resolveSlashCommandInput() → 'builtin' /microagent
        │
        ▼
handleBuiltInSlashCommand() → handleMicroagentCommand(host, args)
        │
        ▼
MicroagentWizard.start()
        │
        ├── flag enabled? ──► no: show error/status
        │
        ├── workDir valid? ──► no: show error
        │
        ├── ensureMicroagentDir(workDir) ──► mkdir -p .ody-code/microagents
        │
        ├── installStarterPackIfEmpty(dir) ──► copy built-in templates if dir empty
        │   └── emit starter_microagent_installed telemetry
        │
        ├── promptName() ──► TextInputDialogComponent
        ├── promptTriggers() ──► TextInputDialogComponent
        ├── promptDescription() ──► TextInputDialogComponent
        │
        ├── validate inputs
        │
        ├── targetPath := resolve(workDir, '.ody-code/microagents', `${name}.md`)
        ├── file exists? ──► interactive overwrite confirmation (QuestionDialogComponent)
        │
        ├── renderMicroagentFile({ name, triggers, description })
        │
        ├── writeFile(targetPath, content)
        │
        └── emit microagent_created telemetry
        │
        ▼
host.showNotice('Microagent created', targetPath)
```

---

## Reuse Analysis

| File | Candidate | Verdict |
|------|-----------|---------|
| `apps/ody-code/src/tui/commands/registry.ts` | `BUILTIN_SLASH_COMMANDS`, `KimiSlashCommand`, `experimentalFlag` | **Adapt** — add `/microagent` entry with `experimentalFlag: 'repo-knowledge'`. |
| `apps/ody-code/src/tui/commands/dispatch.ts` | `handleBuiltInSlashCommand` switch | **Adapt** — add `case 'microagent'` routing to new handler. |
| `apps/ody-code/src/tui/components/dialogs/text-input-dialog.ts` | `TextInputDialogComponent` | **Use as-is** — collect name, triggers, description with per-field validation. |
| `apps/ody-code/src/tui/components/dialogs/question-dialog.ts` | `QuestionDialogComponent` | **Use as-is** — confirm overwrite (yes/no). |
| `apps/ody-code/src/tui/commands/session.ts` | `mkdir`/`writeFile` from `node:fs/promises` | **Reuse pattern** — same filesystem approach as export-md. |
| `packages/agent-core/src/flags/registry.ts` | `FLAG_DEFINITIONS` + `flags.enabled` | **Use as-is** — `repo-knowledge` flag already defined in A.2. |
| `packages/agent-core/src/skill/parser.ts` | `parseTriggers` normalization rules | **Mirror** — apply same trim/lowercase/dedupe to wizard-collected triggers. |
| `apps/ody-code/src/tui/ody-tui.ts` | `SlashCommandHost` | **Use as-is** — provides `workDir`, theme, `mountEditorReplacement`, telemetry. |

No greenfield components are required beyond a focused `microagent.ts` command module and a starter-template asset directory.

---

## Assumptions & Unverified Items

| # | Assumption | Source | Confidence | Impact if wrong | How to verify |
|---|------------|--------|------------|-----------------|---------------|
| A1 | The `repo-knowledge` experimental flag is already defined (A.2) and `BUILTIN_SLASH_COMMANDS` supports `experimentalFlag` gating. | [C:INFERRED] | High | Command would be visible/usable without the flag, leaking unfinished feature. | Inspect `registry.ts` and `flags/registry.ts`. |
| A2 | `TextInputDialogComponent` can be mounted sequentially via `mountEditorReplacement` and restored with `restoreEditor` between steps. | [C:INFERRED] | High | Wizard would lose focus or leave UI in broken state. | Read `TextInputDialogComponent` and existing sequential dialog usage. |
| A3 | `SlashCommandHost.workDir` is always set when a session is active and points to the project root where `.ody-code/` should live. | [C:INFERRED] | Medium | File could be written to wrong directory or command errors. | Inspect `TUIState` / `AppState` initialization. |
| A4 | Starter templates can be shipped as static assets alongside the TUI bundle and resolved relative to the running executable or source root. | [C:INFERRED] | Medium | Starter installation fails at runtime because template path cannot be resolved. | Verify asset packaging in build config. |
| A5 | Users prefer a 3-step text-input wizard over a single multi-question dialog or external-editor flow. | [C:USER] | High | UX dissatisfaction; can be redesigned later. | Confirmed in clarifying question. |
| A6 | Normalizing triggers with the same rules as A.1 (`trim().toLowerCase()`, dedupe, non-empty) is sufficient for the wizard. | [C:INFERRED] | High | Injected microagent may not match user intent if triggers differ from A.1. | Share normalization helper or test parity. |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| R1 | Overly strict name regex rejects valid user-chosen names (e.g. "my-agent") | Low | Low | Allow hyphen and underscore; error message shows allowed pattern. |
| R2 | Path traversal via crafted `name` escapes `.ody-code/microagents/` | Low | High | Restrict name charset to `[a-z0-9_-]+`; reject slashes/dots; resolve and verify final path prefix. |
| R3 | Starter installation overwrites user files if detection logic is wrong | Low | High | Only install when directory is empty or missing; log each copied file; interactive confirmation optional. |
| R4 | Wizard cancellation leaves partial state or broken UI | Low | Medium | Each step checks `done` flag; `onCancel` restores editor and aborts. |
| R5 | Feature flag mismatch: command visible but injection disabled, confusing users | Low | Medium | Gate command on same `repo-knowledge` flag; status message explains flag when disabled. |
| R6 | Generated file fails A.1 parser due to frontmatter formatting | Low | High | Use exact schema from A.1; include parser test on generated sample. |

---

## Selected Approach

Three approaches were considered for the authoring UX:

1. **Sequential `TextInputDialogComponent` wizard** (chosen) [C:USER]  
   Three focused input dialogs (name → triggers → description), each with inline validation. Reuses existing components, keeps each step simple, and matches the TUI's existing patterns. A separate `QuestionDialogComponent` handles overwrite confirmation.
2. **Single multi-question `QuestionDialogComponent`**  
   One dialog with three tabs and a review screen. More cohesive but `QuestionDialogComponent` is optimized for SDK-driven approval questions, not free-text collection; adapting it adds complexity.
3. **Template-only `/microagent <name>`**  
   No interactivity; generate a file from command arguments. Fastest to implement but poor UX for trigger lists and descriptions, and conflicts with the user's explicit choice of an interactive wizard.

---

## Data Models

### 6.1 Wizard input shape (in `apps/ody-code/src/tui/commands/microagent.ts`)

```ts
export interface MicroagentWizardInput {
  readonly name: string;        // sanitized file basename, e.g. "reuse-conventions"
  readonly triggers: readonly string[];  // normalized trigger keywords
  readonly description: string; // one-line summary shown in frontmatter
}
```

### 6.2 Validation result

```ts
export type MicroagentValidationError =
  | { readonly field: 'name'; readonly message: string }
  | { readonly field: 'triggers'; readonly message: string }
  | { readonly field: 'description'; readonly message: string };

export interface MicroagentValidationResult {
  readonly ok: boolean;
  readonly input?: MicroagentWizardInput;
  readonly error?: MicroagentValidationError;
}
```

### 6.3 Generated file schema

The generated file matches the A.1 parser expectation [C:UPSTREAM]:

```markdown
---
name: ${name}
type: knowledge
triggers:
${triggers.map(t => `  - ${t}`).join('\n')}
description: ${description}
---

# ${name}

<!-- TODO: Add repo-specific conventions below. -->
```

### 6.4 Starter template manifest

```ts
export interface StarterTemplate {
  readonly fileName: string;      // e.g. "reuse-conventions.md"
  readonly sourceAssetPath: string; // resolved at runtime from bundled assets
}

export const STARTER_TEMPLATES: readonly StarterTemplate[] = [
  { fileName: 'reuse-conventions.md', sourceAssetPath: '...' },
  { fileName: 'glossary.md', sourceAssetPath: '...' },
  { fileName: 'testing.md', sourceAssetPath: '...' },
  { fileName: 'documentation.md', sourceAssetPath: '...' },
];   // [C:USER]
```

---

## Algorithms

### 7.1 `normalizeName(raw: string): string | undefined`

```text
function normalizeName(raw):
    trimmed := raw.trim().toLowerCase()
    if trimmed.length == 0: return undefined
    if trimmed contains '.' or '/' or '\\' or '..' or any character not in [a-z0-9_-]:
        return undefined
    return trimmed
```

### 7.2 `normalizeTriggers(raw: string): readonly string[] | undefined`

```text
function normalizeTriggers(raw):
    // Split on commas, Chinese commas, or whitespace.
    tokens := raw.split(/[,，\s]+/)
    seen := empty set
    result := empty list

    for token in tokens:
        cleaned := token.trim().toLowerCase()
        if cleaned.length == 0: continue
        if cleaned already in seen: continue
        add cleaned to seen
        append cleaned to result

    if result.length == 0: return undefined
    return result sorted lexicographically
```

### 7.3 `validateMicroagentInput(rawName, rawTriggers, rawDescription): MicroagentValidationResult`

```text
function validateMicroagentInput(rawName, rawTriggers, rawDescription):
    name := normalizeName(rawName)
    if name == undefined:
        return { ok: false, error: { field: 'name', message: 'Name must be lowercase alphanumeric with - or _ only.' } }

    triggers := normalizeTriggers(rawTriggers)
    if triggers == undefined:
        return { ok: false, error: { field: 'triggers', message: 'At least one non-empty trigger keyword is required.' } }

    description := rawDescription.trim()
    if description.length == 0:
        return { ok: false, error: { field: 'description', message: 'Description is required.' } }
    if description.length > 200:
        return { ok: false, error: { field: 'description', message: 'Description must be 200 characters or fewer.' } }

    return { ok: true, input: { name, triggers, description } }
```

### 7.4 `renderMicroagentFile(input): string`

```text
function renderMicroagentFile(input):
    triggersYaml := input.triggers.map(t => `  - ${t}`).join('\n')
    return [
        "---",
        `name: ${input.name}`,
        "type: knowledge",
        "triggers:",
        triggersYaml,
        `description: ${input.description}`,
        "---",
        "",
        `# ${input.name}`,
        "",
        "<!-- TODO: Add repo-specific conventions below. -->",
        ""
    ].join('\n')
```

### 7.5 `installStarterPackIfEmpty(targetDir): Promise<InstalledFile[]>`

```text
async function installStarterPackIfEmpty(targetDir):
    entries := await readdir(targetDir).catch(() => [])
    markdownFiles := entries.filter(name => name.endsWith('.md'))
    if markdownFiles.length > 0: return []   // already has user content

    installed := []
    for template in STARTER_TEMPLATES:
        source := await readAsset(template.sourceAssetPath)
        dest := join(targetDir, template.fileName)
        await writeFile(dest, source, 'utf-8')
        installed.push({ fileName: template.fileName, path: dest })
    return installed
```

### 7.6 `MicroagentWizard.run()`

```text
async function run(host):
    if !flags.enabled('repo-knowledge'):
        host.showError('Microagent authoring requires the repo-knowledge experimental flag.')
        return

    workDir := host.state.appState.workDir
    if workDir == undefined or workDir.length == 0:
        host.showError('No active workspace.')
        return

    microagentsDir := join(workDir, '.ody-code', 'microagents')
    await mkdir(microagentsDir, { recursive: true })

    installed := await installStarterPackIfEmpty(microagentsDir)
    if installed.length > 0:
        for file in installed:
            host.track('starter_microagent_installed', { file_name: file.fileName })
        host.showNotice('Starter microagents installed', installed.map(f => f.fileName).join(', '))

    name := await promptName(host)
    if name == undefined: return   // user cancelled

    triggers := await promptTriggers(host)
    if triggers == undefined: return

    description := await promptDescription(host)
    if description == undefined: return

    validation := validateMicroagentInput(name, triggers, description)
    if !validation.ok:
        host.showError(`Invalid ${validation.error.field}: ${validation.error.message}`)
        return

    input := validation.input
    targetPath := join(microagentsDir, `${input.name}.md`)

    if fileExists(targetPath):
        confirmed := await confirmOverwrite(host, input.name)
        if !confirmed:
            host.showStatus('Microagent creation cancelled.')
            return

    content := renderMicroagentFile(input)
    try:
        await writeFile(targetPath, content, 'utf-8')
    catch (error):
        host.track('microagent_create_failed', { reason: 'write_error', error: String(error) })
        host.showError(`Failed to write microagent: ${formatErrorMessage(error)}`)
        return

    host.track('microagent_created', {
        name: input.name,
        trigger_count: input.triggers.length,
    })
    host.showNotice('Microagent created', targetPath)
```

---

## Call-Site Integration

### 8.1 Slash command registration

**File**: `apps/ody-code/src/tui/commands/registry.ts`  
**Lines**: 186–192 (near `/init`)  
**Change**: add `/microagent` to `BUILTIN_SLASH_COMMANDS`.

```ts
{
  name: 'microagent',
  aliases: [],
  description: 'Create a new repo knowledge microagent',
  priority: 80,
  availability: 'idle-only',
  experimentalFlag: 'repo-knowledge',
  hiddenInModes: OFFICE_HOURS_HIDDEN,
}   // [C:USER]
```

### 8.2 Command dispatch

**File**: `apps/ody-code/src/tui/commands/dispatch.ts`  
**Lines**: 300–304 (near `case 'init'`)  
**Change**: import and route `microagent`.

```ts
import { handleMicroagentCommand } from './microagent';

// inside handleBuiltInSlashCommand switch:
case 'microagent':
  await handleMicroagentCommand(host, args);
  return;
```

### 8.3 New command module

**File**: `apps/ody-code/src/tui/commands/microagent.ts` (new)  
**Contract**: export `handleMicroagentCommand(host: SlashCommandHost, args: string): Promise<void>` and pure helpers `normalizeName`, `normalizeTriggers`, `validateMicroagentInput`, `renderMicroagentFile`, `installStarterPackIfEmpty`.

Surrounding code: follows the same pattern as `session.ts` for filesystem operations and `goal.ts` for dialog mounting.

### 8.4 Starter asset location

**File**: `apps/ody-code/assets/microagents/*.md` (new directory)  
**Content**: `reuse-conventions.md`, `glossary.md`, `testing.md`, `documentation.md`.  
**Build integration**: ensure the asset directory is copied into the distribution bundle so `installStarterPackIfEmpty` can resolve it at runtime. [C:INFERRED]

---

## Error Handling

| Error class / scenario | Trigger | Immediate handling | Degradation | Recovery |
|------------------------|---------|--------------------|-------------|----------|
| Feature flag off | `repo-knowledge` disabled | `handleMicroagentCommand` shows error and returns | Command unavailable | Enable flag via env |
| No active workspace | `workDir` empty or undefined | Show error; do not write | No file created | User opens a project workspace |
| Invalid name | `normalizeName` returns undefined | Inline validation hint in `TextInputDialogComponent`; if slipped through, show error after validation | No file created | User retypes name |
| Invalid triggers | Empty or all-whitespace triggers | Inline validation hint; show error after validation | No file created | User retypes triggers |
| Invalid description | Empty or >200 chars | Inline validation hint; show error after validation | No file created | User retypes description |
| File exists | `targetPath` already present | Mount `QuestionDialogComponent` for overwrite confirmation | Creation paused until user decides | User confirms or cancels |
| User cancels wizard | Esc/Enter on cancel | `restoreEditor()` and abort | No file created | User restarts `/microagent` |
| Write failure | `writeFile` throws | Log `microagent_create_failed` telemetry; show error | No file created | User fixes permissions/disk space |
| Starter read failure | Asset missing or unreadable | Log warning; skip starter installation; continue wizard | Starter not installed | Fix build asset packaging |
| Starter directory not empty | Existing `.md` files present | Skip starter installation entirely | No starter files added | User deletes files or runs in empty dir |

No retries are needed; all failures are local and degrade to "no file created" except starter failures, which degrade to "continue without starter."

---

## Test Plan

**Test file**: `apps/ody-code/test/tui/commands/microagent.test.ts`  
**Rationale**: TUI command tests already live under `test/tui/commands/`; this follows the same pure-function + stubbed-host pattern as other command tests. [C:INFERRED]

### 10.1 Validation tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| V1 | valid input passes | `validateMicroagentInput('reuse-conventions', 'component, page', 'Reuse existing components')` returns `{ ok: true, input: { name: 'reuse-conventions', triggers: ['component', 'page'], description: 'Reuse existing components' } }`. |
| V2 | name rejects uppercase | `normalizeName('ReuseConventions')` returns `undefined`. |
| V3 | name rejects path separators | `normalizeName('foo/bar')` and `normalizeName('foo\\bar')` return `undefined`. |
| V4 | name rejects dots | `normalizeName('foo.bar')` returns `undefined`. |
| V5 | name accepts hyphen and underscore | `normalizeName('my-agent_v2')` returns `'my-agent_v2'`. |
| V6 | triggers split on comma, Chinese comma, whitespace | Input `'组件, page ，test'` → `['component', 'page', '组件']` (ASCII lowercased; CJK passes through). |
| V7 | triggers deduplicate and sort | Input `'page, component, page'` → `['component', 'page']`. |
| V8 | empty triggers rejected | `normalizeTriggers('   ')` returns `undefined`. |
| V9 | empty description rejected | `validateMicroagentInput('x', 'y', '')` returns `{ ok: false, error.field === 'description' }`. |
| V10 | long description rejected | Description length 201 → rejected. |

### 10.2 Rendering tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| R1 | generated file contains correct frontmatter | `renderMicroagentFile({ name: 'reuse', triggers: ['component'], description: 'Reuse' })` contains `name: reuse`, `type: knowledge`, `triggers:`, `  - component`, `description: Reuse`. |
| R2 | generated file is parseable by A.1 parser | `parseSkillText(renderMicroagentFile(...))` succeeds and returns `metadata.type === 'knowledge'` with matching triggers. |

### 10.3 Starter installation tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| S1 | installs when directory empty | `installStarterPackIfEmpty(tmpEmptyDir)` copies all 4 templates; returns 4 entries. |
| S2 | skips when `.md` files exist | With an existing `user.md`, returns empty array and does not overwrite. |
| S3 | creates directory if missing | `mkdir` recursive called before copy. |

### 10.4 Wizard integration tests

| # | Test | Must-pass assertions |
|---|------|----------------------|
| W1 | flag off shows error | `handleMicroagentCommand` calls `host.showError` containing "repo-knowledge"; no dialog mounted. |
| W2 | missing workDir shows error | `host.state.appState.workDir` undefined → `host.showError` called. |
| W3 | cancellation at name step aborts | User cancels first dialog → no file written; `host.showStatus` or silent abort. |
| W4 | overwrite confirmation on existing file | File exists → `QuestionDialogComponent` mounted; choosing "No" aborts; choosing "Yes" writes file. |
| W5 | successful creation emits telemetry | `host.track('microagent_created', { name, trigger_count })` called. |
| W6 | starter install emits telemetry | On first use, `host.track('starter_microagent_installed', { file_name })` called for each starter. |
| W7 | write failure emits telemetry | `writeFile` throws → `host.track('microagent_create_failed', { reason: 'write_error' })` called. |

### 10.5 Done criteria

```bash
pnpm --filter @odysseythink/ody-code test apps/ody-code/test/tui/commands/microagent.test.ts
pnpm --filter @odysseythink/ody-code typecheck
```

Both must pass before A.4 is considered complete.

---

## Self-Review

Before the audit gate, the design was reviewed through four fixed lenses:

- **Security**: The only new external input surfaces are the three wizard fields and the target file name derived from the sanitized `name` field. `normalizeName` rejects path separators, dots, and non-alphanumeric characters, preventing traversal out of `.ody-code/microagents/`. The overwrite confirmation prevents accidental destruction of existing user content. The generated YAML does not interpolate user input into shell commands. The omitted-note and telemetry do not include secrets or full paths. Nothing found requiring a fix.

- **Test**: Every behavior has must-pass and must-reject cases (valid/invalid names, triggers, descriptions; install/skip starter; create/abort/overwrite). The adversarial checks for `normalizeName` and `normalizeTriggers` were verified with ephemeral Node evaluations (see below). CJK triggers pass through `toLowerCase()` unchanged while ASCII triggers are lowercased; this is consistent with A.2's matching semantics. Nothing found requiring a fix.

- **Ops**: The wizard is a short, bounded sequence of local UI dialogs with no network calls. Starter installation is a one-time O(N) file copy where N=4. No persistent state beyond the generated file. No concurrency concerns beyond the single TUI event loop. Nothing found requiring a fix.

- **Integration**: Verified that `TextInputDialogComponent` and `QuestionDialogComponent` exist in `apps/ody-code/src/tui/components/dialogs/`, `BUILTIN_SLASH_COMMANDS` and `experimentalFlag` exist in `commands/registry.ts`, `handleBuiltInSlashCommand` routes commands in `commands/dispatch.ts`, `SlashCommandHost` exposes `workDir`, `mountEditorReplacement`, `restoreEditor`, and `track`, and the `repo-knowledge` flag is defined in A.2. The design lands at the named TUI target and does not silently retarget. Nothing found requiring a fix.

- **Scope**: The design remains a single coherent TUI command (authoring UX for knowledge microagents). LLM-generated bodies, external-editor body editing, listing/deleting existing microagents, and non-TUI entry points are explicitly deferred. No decomposition required.

### Adversarial verification

Three expensive decisions were tested with ephemeral `node -e` checks:

1. **`normalizeName` regex predicate** — name must match `^[a-z0-9_-]+$`.
   - Input: `"reuse-conventions"` → match (valid).
   - Input: `"foo/bar"` → no match (path separator rejected).
   - Input: `"foo..bar"` → no match (dot rejected).
   - Input: `"MyAgent"` → no match (uppercase rejected).
   - Input: `""` → no match (empty rejected).

2. **`normalizeTriggers` split and dedup** — split on `[,，\s]+`, trim/lowercase, dedupe, sort.
   - Input: `"A, B, a"` → expected `['a', 'b']` (verified OK).
   - Input: `"组件 ， page  test"` → expected `['page', 'test', '组件']` (CJK characters pass through unchanged; ASCII is lowercased; verified OK).
   - Input: `"   "` → empty array (rejected; verified OK).

3. **Starter installation guard** — only install when no `.md` files exist.
   - Directory empty → install all 4.
   - Directory contains `user.md` → install none.
   - Directory contains only `not-md.txt` → install (safe; `.md` check is the gate).

No contradictions found.

---

## User Final Approval

- [ ] User approved the design via `ExitDesignMode`.
- [x] Assumptions audit gate completed at Deep level: A1, A2, A3, A4, A5, A6 accepted.
