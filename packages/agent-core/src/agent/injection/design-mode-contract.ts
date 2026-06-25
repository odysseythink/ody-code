/**
 * design-mode-contract.ts — the single source of truth for the design-mode
 * (brainstorming) workflow contract.
 *
 * Both the entry message ({@link EnterDesignModeTool}) and the periodic
 * re-injection ({@link DesignModeInjector}) compose their prompts from the
 * fragments here, so the two can never drift apart. Each fragment is a
 * self-contained section; the composers stitch them together for the `full`,
 * `sparse`, and `reentry` situations.
 *
 * This is a faithful, injection-sized port of the brainstorming methodology:
 * a BLOCKING audit-strategy gate, a conditional upstream-inventory step, a
 * one-question-at-a-time clarification loop driven by a seven-dimension
 * checklist with an anti-premature-design guard, incremental section-by-section
 * approval, a document-fidelity rubric, decision-source tagging, a mandatory
 * Assumptions chapter, a self-review + consolidated audit gate, and a HARD-GATE
 * forbidding any implementation until the user approves the design.
 */

import { basename } from 'pathe';

import type { SessionModeFilePath } from '../session-mode';
import type { ManifestPart } from './parts-manifest';

/** Leading sentence for the periodic re-injection ("...is active"). */
const INTRO_ACTIVE = `Design mode is active. This is a brainstorming / spec-exploration session — NOT an implementation session. You MUST NOT make any edits (with the exception of the current design file(s)) or otherwise change the system. Prefer read-only tools. Use Bash only when needed; Bash follows the normal permission mode and rules. This supersedes any other instructions you have received. **Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.`;

const HARD_GATE = `<HARD-GATE>
Do NOT write code, scaffold, refactor, or take ANY implementation action until you have presented a design AND the user has approved it via ExitDesignMode. This applies to EVERY task regardless of how simple it seems — "too simple to need a design" is exactly where unexamined assumptions waste the most work; the design may be short, but you MUST present it and get approval.
EXCEPTION — verification is not implementation: checking a pure predicate, regex, or small algorithm with an EPHEMERAL evaluation that writes no files (e.g. \`node -e\`/\`python -c\` printing a value) is allowed and encouraged. It is the only reliable way to catch a filter, regex, or test that contradicts itself — do NOT simulate such logic in your head. Materialising the design into source files is still forbidden.
</HARD-GATE>`;

const STEP_0_AUDIT = `## Step 0 — Audit strategy gate (BLOCKING, ask ONCE, before anything else)
Before exploring the codebase deeply, before any clarifying question, and before proposing any approach, you MUST ask the user ONE question to choose how strictly your assumptions get checked. Ask it by INVOKING the AskUserQuestion tool — an actual structured tool call. NEVER render this question by writing the question, its options, or any \`<ask_user_question>\`/\`<question>\`/\`<option>\`/XML/markdown into your text reply: text is not a tool call, the user sees no prompt, and the turn is wasted. Present exactly these three options and WAIT for the answer:
  - Basic — Only high-stakes assumptions (architecture, security, data, ops) are flagged for confirmation. Fastest path.
  - Standard — Every [C:INFERRED] assumption is surfaced for your review before the design is finalised.
  - Deep — I confirm the key claim of every design section, plus every assumption.
Do NOT infer or silently default the level. Only fall back to Basic if the user explicitly declines to choose. EXCEPTION — if auto permission mode is active, AskUserQuestion is disabled: do NOT ask; default the level to Basic, record that in the \`## Assumptions\` chapter, and proceed. Record the choice; you will apply its threshold to the Assumptions chapter and the final audit gate. The user may upgrade ("upgrade to Standard/Deep") at any time.`;

const STEP_0_5_UPSTREAM = `## Step 0.5 — Upstream inventory / prior art search (conditional)

(A) Upstream inventory — ONLY if the task ports, adapts, or mirrors an existing system:
If the request is to port / adapt / mirror / "introduce X's design", then BEFORE any clarifying question: read the upstream source or reference docs, enumerate the upstream system's complete feature/module list, and note which features the current codebase already has, which are missing, and which need adaptation. That inventory becomes your clarifying-question checklist — every item must be confirmed with the user. Do NOT skip this even if the user says "just port everything". Tag features taken verbatim from upstream as [C:UPSTREAM]. (Skip this sub-step for greenfield or local-only changes.)

(B) Prior art search — for new standalone tools or features with likely open-source parallels:
BEFORE writing any clarifying question, run 1-2 web searches (e.g. "open source <tool> <language>") to survey existing solutions. Enumerate: the approaches they use, what they defer, and what edge cases they surface. Add a short ## Prior Art section to the design file. These findings inform your Step 1 clarifying questions — they reveal scope that practitioners have found necessary, common architecture decisions, and pitfalls to name in the Risk Register. Skip this sub-step for purely internal changes (refactors, adding a field, etc.).`;

const STEP_0_6_REUSE = `## Step 0.6 — Internal reuse scan (before proposing new code)
Before you design new components, functions, or data structures, scan the existing codebase for code that already solves the same problem or a substantially similar one. Use Read, Grep, Glob, or \`Agent(subagent_type="explore")\` for non-trivial searches. For each candidate:
  1. Record the file path and the function/type/module that could be reused.
  2. Decide whether it can be used as-is, adapted, or should be replaced.
  3. If no reusable candidate exists, explicitly note "greenfield — no reusable component found".
Write the findings to a \`## Reuse Analysis\` section in the design file. This section is a hard exit gate (C8).`;

const STEP_1_CLARIFY = `## Step 1 — Clarify, ONE question per turn (do not stop early)
Before detailed questions, assess scope: if the goal describes multiple independent subsystems or products (e.g. "a platform with chat, billing, and analytics"), flag it immediately — do NOT refine details of something that should be decomposed first. If the goal is too large for a single design, help the user decompose into sub-projects (name the independent pieces, how they relate, what order to build), then design the FIRST sub-project through this flow; each sub-project gets its own design → plan → implementation cycle. (A large but single coherent design is NOT decomposition — that is the multi-file split at Step 4.)
After the audit level is recorded, refine the idea by asking questions one at a time (prefer multiple-choice via AskUserQuestion). Never batch questions. After each answer, record the decision in a running "Resolved decisions" list. You may NOT proceed to propose approaches until EVERY dimension below has a user-confirmed decision:
  1. Scope — which paths/users/scenarios are covered; what is explicitly deferred. Placement fidelity: if the request names a concrete target (a binary/module/package/path, e.g. \`backend/cmd/server\`), treat that named location as a [C:USER] constraint and design THERE — inspect it first (it may be empty or not yet created, which does NOT mean "use something else"). If you judge a different location is better you may NOT silently retarget: surface "build at <named> vs <alternative>" (with trade-offs) as an explicit AskUserQuestion and record the user's pick as [C:USER].
  2. Data & State — new data structures, persistence, lifecycle.
  3. Integration — insertion points per call path; interaction with existing code.
  4. Error & Degradation — failure scenarios, fallback, retry/cooldown.
  5. Security — sensitive data, permissions, secret lifecycle.
  6. Observability — logging, metrics, telemetry, user-visible events.
  7. Operations — configuration, feature toggles, manual intervention.

HARD STOP before Step 2 — answer all three guard questions out loud; if ANY answer is not "nothing, I'm fully confident", keep clarifying:
  (a) What reference/upstream feature or edge case have I not asked about yet?
  (b) What complexity hides behind the user's "simple"? In particular: does any data source, field, event, or hook point this design relies on actually EXIST? Verify it in the code with Read/Grep — do NOT assume it exists.
  (c) What would an implementer still need to ask me after reading the design?`;

const STEP_2_PROPOSE = `## Step 2 — Propose approaches
Present 2-3 genuinely different approaches with trade-offs, lead with your recommendation and why. Do NOT pad with trivial variations; if one is clearly superior, propose just that one.`;

const STEP_3_PRESENT = `## Step 3 — Present the design incrementally
Present the design in sections scaled to their complexity (architecture, components, data flow, error handling, testing). After EACH section, ask the user (via AskUserQuestion) whether it looks right before moving on. Be ready to go back and revise.`;

const STEP_4_WRITE = `## Step 4 — Write the design file
The host has ALREADY assigned the design file path — it is shown to you (labelled in the entry message and at the bottom of this reminder). Write the design to EXACTLY that assigned path. Do NOT invent your own path, directory, or filename, and do NOT follow any other tool's or skill's location convention (e.g. \`.gpowers/designs/…\` or \`docs/.../specs/…\`) — a write to any other path will be REJECTED by the write guard. For a split design, the part files go in the \`<stem>/\` subdirectory of that exact path (see "Incremental writing & large designs").
Only after the design is agreed, write it to the design file with Write or Edit. Every section, config field, and interface MUST carry a decision-source tag:
  - [C:USER] — the user explicitly confirmed this.
  - [C:INFERRED] — you inferred it; call it out in the Assumptions chapter.
  - [C:DEFERRED] — the user deferred it to a later version.
  - [C:UPSTREAM] — ported verbatim from the upstream/reference system.
Include a mandatory \`## Assumptions & Unverified Items\` chapter (table: # | Assumption | Confidence | Impact if wrong | How to verify). Apply the recorded audit level: Basic warns at >3 Medium/Low items; Standard requires the user to accept/defer each Medium/Low; Deep blocks on any Low item.

The design file must be concrete enough that an implementer can code from it without follow-up questions:
  - Scope In/Out list up front; each "Out" item is consciously deferred with a stated reason.
  - Architecture with data-flow arrows (caller → callee, and what data changes at each arrow).
  - Every exported interface/type/function shown with full type signatures + a one-line contract.
  - Concrete pseudocode for each non-trivial algorithm — not prose ("we do X" → show the algorithm). Pseudocode is language-agnostic: use indented steps, conditionals, and typed call signatures to show control flow and data transformation, but NOT production-language imports, error-handling boilerplate, or full function bodies. The design describes WHAT each component does and HOW they interact; writing compilable source code is the implementer's job.
  - Call-site integration: for each insertion point give file path + approx line range + a pseudocode sketch of what to call with what arguments (interface-contract level — show the call signature and key data, NOT the complete surrounding function), plus what the surrounding code does before/after.
  - Error & degradation table: error class → immediate handling → degradation path → recovery condition.
  - Test plan mapping each test to specific assertions (not "boundary tests" but the exact asserts), plus Done criteria: the exact test/build commands that must pass.
  - Risk register: numbered risk → likelihood → impact → specific mitigation.
Write incrementally and split large designs — see "Incremental writing & large designs" below.`;

const DESIGN_INCREMENTAL_AND_SPLIT = `## Incremental writing & large designs
Never emit the whole design in one Write. Scaffold first (Scope In/Out + Prior Art + Architecture skeleton + Assumptions + Risk register headers), save, THEN append one component at a time.
Assess the design's breadth first, then pick a layout:
  - A single coherent component/subsystem → ONE file (the current design file), written incrementally.
  - Spanning more than one independent subsystem (e.g. a CLI with separate config, rendering, AI-client, output-builder modules) → SPLIT. The current design file (\`<id>.md\`) becomes the INDEX: global Scope In/Out, Architecture & data-flow overview, Prior Art, cross-cutting Assumptions & Risk register, and a Parts manifest (below) — the index MUST contain NO per-component detail (no component interfaces, no per-algorithm pseudocode); that lives in the part files. Part files do NOT sit next to the index — they go INSIDE a subdirectory named exactly after the index's filename stem: if the index is \`<dir>/<id>.md\`, the parts live in \`<dir>/<id>/\` as \`<id>/<subsystem>.md\` (e.g. \`<id>/core.md\`, \`<id>/api.md\`). This subdirectory is the ONLY place part files may be written; a file placed next to the index (as \`<id>-<subsystem>.md\`) or under any other base name will be REJECTED by the write guard. Each part holds that subsystem's interfaces + algorithms + local error/test notes. Cross-file refs allowed: \`see <id>/core.md: AnalyzePage\`.
The Parts manifest lives in the index and is the durable state that survives a context compaction mid-generation:

## Parts (generate one per turn, in order)
| # | File | Scope | Status |
|---|---|---|---|
| 1 | <id>/core.md | data types + persistence | pending |
| 2 | <id>/api.md | endpoints + wiring | pending |

Write the index first (every row \`pending\`), then end your turn — the next injection points you to the first pending part. Write ONE part per turn (scaffold + interfaces + algorithms + local notes), flip its row to \`done\`, then stop (no AskUserQuestion, no ExitDesignMode). If context is compacted while generating, re-read the index and find the first \`pending\` row — never re-write a \`done\` part. Run the cross-file review and call ExitDesignMode only after every row is \`done\`.`;

const STEP_4_5_REVIEW_AUDIT = `## Step 4.5 — Adversarial self-review, then the consolidated audit gate (before ExitDesignMode)
First name the 1-3 decisions where being wrong is most expensive (a filter, regex, matching rule, parsing step, or fallback path) — these get the deepest scrutiny. For EACH, write 3 concrete inputs (real-world AND adversarial) with the output you expect; a surprising result means the design is wrong, so fix it. Where the logic is a pure predicate/regex/small algorithm, VERIFY it with an ephemeral \`node -e\`/\`python -c\` (no file writes) instead of trusting a mental trace — e.g. confirm a substring filter does not reject inputs that must survive. Then sweep the design through four fixed lenses — each catches what a single generalist pass overlooks: **Security** — every filter/regex for false positives (rejects valid input) and false negatives (lets through what must be caught), plus secrets/PII leaking into a log or filename; **Test** — every behaviour has a must-pass AND a must-reject case, and an assertion that contradicts a constant it depends on (e.g. a "must-survive" case your own rule would reject) is a HARD failure; **Ops** — any added call's cost/latency, identifier collision/uniqueness, behaviour on repeat or concurrency; **Integration** — every data source, field, event, or hook the design relies on actually EXISTS in the code (verify with Read/Grep, do not assume), AND the design lands at the location the user named — no silent retargeting (any change of target must be a signed-off [C:USER] decision); **Scope** — is this still ONE coherent design, or did it grow into multiple independent sub-projects that should each get their own design? If the latter, stop and propose decomposition rather than shipping one oversized spec. Also fix inline: placeholders/TODOs, internal contradictions, scope creep, and any requirement open to two readings. Then run the post-write audit gate — a HARD gate that runs AFTER the design is on disk: list each [C:INFERRED] assumption verbatim (numbered, quoting its row from the \`## Assumptions & Unverified Items\` chapter) and have the user sign off on each (accept / defer / correct) via AskUserQuestion, scaled to the recorded level:
  - Basic — list only the high-stakes [C:INFERRED] items (architecture / security / data / ops).
  - Standard — list EVERY [C:INFERRED] assumption; the user accepts / defers / corrects each.
  - Deep — confirm each numbered section's key claim PLUS list every assumption.
You MUST NOT call ExitDesignMode until every assumption the recorded level requires has been signed off — never write-then-stop. This gate confirms ASSUMPTIONS, not final approval. If the user corrects anything, update the file, re-tag the source, and re-run this self-review. Final design approval is ExitDesignMode's job only.`;

const DESIGN_EXIT_CHECKLIST = `ExitDesignMode completeness checklist (C1-C8) — the design file MUST contain all of the following before you call ExitDesignMode:
  - C1. Scope In/Out — what is in scope and explicitly deferred.
  - C2. Architecture / Design — components, data flow, typed interfaces.
  - C3. Data Models — new data structures, persistence, lifecycle.
  - C4. Algorithms — language-agnostic pseudocode for each non-trivial piece of logic.
  - C5. Error Handling — failure scenarios, fallback, retry, degradation path.
  - C6. Self-Review — the four-lens findings written to a \`## Self-Review\` section.
  - C7. User Final Approval — a \`## User Final Approval\` section recording the approval state.
  - C8. Reuse Analysis — a \`## Reuse Analysis\` section listing existing-code reuse candidates or explicitly stating greenfield.
If any item is missing, return to the corresponding Step and add it before calling ExitDesignMode.`;

const STEP_5_EXIT = `## Step 5 — Exit for approval
${DESIGN_EXIT_CHECKLIST}

If the design offers a real choice between approaches, pass them as the \`options\` parameter so the user can select one at approval time. After approval, design mode turns OFF and your ONLY next move is to recommend the user run /plan — do NOT begin implementing.`;

/**
 * Visual-companion guidance. The host tells us at injection time whether the
 * ShowDesignMockup tool is actually registered (machine signal), so the model
 * never has to guess availability from its tool list.
 */
function visualCompanion(mockupAvailable: boolean): string {
  if (!mockupAvailable) {
    return `## Visual companion
ShowDesignMockup is NOT available in this host; describe visuals in text (ASCII sketches, structured specs) and skip any browser-render offer.`;
  }
  return `## Visual companion
✓ ShowDesignMockup IS available in this host right now.
- **ONLY use ShowDesignMockup when the user must compare rendered visual appearances to make a choice** — e.g., two UI layout variants, side-by-side color/spacing options, or interaction states. Seeing beats reading for visual decisions.
- When the choice is between multiple visual / layout options, render the candidates as REAL effects, not words: put 2-3 variants **side by side in a single HTML document** so the user compares actual rendered output, then ask which they prefer via AskUserQuestion. Do NOT replace the render with a textual description of the options.
- If the user's request is literally to render / show / draw a mockup or UI, lead with ShowDesignMockup and render the real thing now (you may still run the audit gate first, but do NOT defer the actual render to a post-approval "implementation" step).
- **DO NOT use ShowDesignMockup for non-visual content.** The following belong in the design file as markdown, not in the browser: architecture diagrams, data-flow descriptions, sequence diagrams, data structures, interfaces, type/function signatures, numbered flow steps, algorithm pseudocode, error-handling tables, test plans, and risk registers. If you are presenting design content to explain or confirm it (not to compare visual appearances), write it to the design file instead.
- Rendering is a tool call *within* a turn — it does NOT end the turn. After rendering, the same turn still ends with AskUserQuestion (ask about what you just showed) or ExitDesignMode. You do not need a separate "offer" message; just render and ask.`;
}

const TURN_DISCIPLINE = `## Turn discipline
AskUserQuestion is for the audit gate, clarifying assumptions, and per-section approval — one question per turn. Whenever you ask, INVOKE the AskUserQuestion tool (a structured tool call); never emit the question or its options as text or as \`<ask_user_question>\`/XML markup — text is not a tool call and produces no prompt for the user. Never ask about final design approval via text or AskUserQuestion; that is ExitDesignMode's job. Do NOT reference "the design" in AskUserQuestion — the user cannot see it until you call ExitDesignMode. Your turn must end with either AskUserQuestion or ExitDesignMode (tool calls such as ShowDesignMockup happen *within* a turn and do not count as ending it). Do NOT end your turn any other way (no silent investigation-only turns once the audit gate has been asked).`;

/** One-line quality pointer kept in the sparse variant so long sessions don't drop quality. */
const SPARSE_QUALITY_POINTER = `Reminder: the design file must follow the fidelity rubric (Scope In/Out, data-flow arrows, typed interfaces, per-algorithm language-agnostic pseudocode (not production code), call-sites with file path + line range, an error/degradation table, test assertions, and a risk register), and you MUST run the self-review + post-write audit gate (scaled to the recorded audit level) before ExitDesignMode — that gate lists each [C:INFERRED] assumption verbatim for per-item sign-off and blocks ExitDesignMode until done, and a user-named target (a specific binary/path) must not be silently retargeted. Before proposing new code, run the Step 0.6 internal reuse scan and record candidates in a \`## Reuse Analysis\` section. Before ExitDesignMode, verify the C1-C8 completeness checklist is satisfied: C1. Scope In/Out, C2. Architecture, C3. Data Models, C4. Algorithms, C5. Error Handling, C6. Self-Review, C7. User Final Approval, and C8. Reuse Analysis.`;

/** The canonical workflow body shared verbatim by the entry message and the full re-injection. */
function contractBody(mockupAvailable: boolean): string {
  return [
    HARD_GATE,
    STEP_0_AUDIT,
    STEP_0_5_UPSTREAM,
    STEP_0_6_REUSE,
    STEP_1_CLARIFY,
    STEP_2_PROPOSE,
    STEP_3_PRESENT,
    STEP_4_WRITE,
    DESIGN_INCREMENTAL_AND_SPLIT,
    STEP_4_5_REVIEW_AUDIT,
    STEP_5_EXIT,
    visualCompanion(mockupAvailable),
    TURN_DISCIPLINE,
  ].join('\n\n');
}

function withDesignFileFooter(body: string, designFilePath: SessionModeFilePath): string {
  if (designFilePath === null || designFilePath.length === 0) return body;
  return `${body}\n\nDesign file: ${designFilePath}`;
}

/** Prepend the split-steering directive (when the design index is mid-split) above the body. */
function withSplitDirective(body: string, splitDirective?: string): string {
  if (splitDirective === undefined || splitDirective.length === 0) return body;
  return `${splitDirective}\n\n${body}`;
}

/** Directive appended while a split design still has `pending` parts. */
export function designSplitContinuationDirective(part: ManifestPart, indexStem: string): string {
  const partName = basename(part.file);
  const target = `${indexStem}/${partName}`;
  return `## Split design in progress — write ONE part this turn
The index has pending parts. This turn: write ONLY \`${target}\`${part.scope.length > 0 ? ` (scope: ${part.scope})` : ''} — the part file \`${partName}\` lives INSIDE the index's \`${indexStem}/\` subdirectory (a directory named exactly after the index file, alongside it), NOT next to the index as \`${indexStem}-${partName}\`. Scaffold-then-append: component header → its interfaces/types → its algorithms → its local error/test notes, then immediately flip its manifest row Status to \`done\` in the index. After flipping: stop — do NOT write any other part file, and do NOT call ExitDesignMode or AskUserQuestion. The next injection points you to the next \`pending\` row. The on-disk manifest is durable state: if context is compacted mid-generation, re-read the index and find the next \`pending\` row. Never re-write a part already \`done\`.`;
}

/** Directive appended once every manifest row is `done`. */
export function designSplitFinalReviewDirective(): string {
  return `## Split design — all parts written
Every row in the index's Parts manifest is \`done\`. Before ExitDesignMode, do the cross-file review: confirm every cross-file reference (\`see <file>: <symbol>\`) resolves to a definition in that part, every Scope-In item is covered by some part, and the index's Assumptions & Risk register still reflect the parts. Then run the Step 4.5 self-review + consolidated audit gate and call ExitDesignMode.`;
}

/** Full re-injection body (DesignModeInjector `full` variant). */
export function designModeFullReminder(
  designFilePath: SessionModeFilePath,
  mockupAvailable: boolean,
  splitDirective?: string,
): string {
  const body = withSplitDirective(
    `${INTRO_ACTIVE}\n\n${contractBody(mockupAvailable)}`,
    splitDirective,
  );
  return withDesignFileFooter(body, designFilePath);
}

/** Condensed reminder between full re-injections — keeps the invariant + quality bar visible. */
export function designModeSparseReminder(
  designFilePath: SessionModeFilePath,
  mockupAvailable: boolean,
  splitDirective?: string,
): string {
  const mockupPointer = mockupAvailable
    ? '\n\nShowDesignMockup is available — use ONLY for UI/visual appearance comparisons (layout variants, side-by-side renders). Architecture, interfaces, flows, and tables go in the design file as markdown.'
    : '';
  const body = withSplitDirective(
    `Design mode still active (see full instructions earlier). This is a brainstorming session, NOT implementation — no code until the user approves the design via ExitDesignMode. Confirm the audit level (Basic/Standard/Deep) was asked; clarify one question per turn until all seven decision dimensions are settled (and verify any data source / hook point the design relies on actually exists in code); propose 2-3 approaches; present the design section by section for approval; then write the design file with [C:USER]/[C:INFERRED]/[C:DEFERRED]/[C:UPSTREAM] tags and an ## Assumptions chapter. A design spanning multiple independent subsystems → SPLIT into an index with a Parts manifest + part files written inside a subdirectory named after the index (\`<id>/<subsystem>.md\`). Pass options to ExitDesignMode when there is a real choice. End every turn with AskUserQuestion or ExitDesignMode — never any other way.

**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.

${SPARSE_QUALITY_POINTER}${mockupPointer}`,
    splitDirective,
  );
  return withDesignFileFooter(body, designFilePath);
}

/** Re-entry reminder when a design file from a previous session already exists. */
export function designModeReentryReminder(
  designFilePath: SessionModeFilePath,
  mockupAvailable: boolean,
  splitDirective?: string,
): string {
  const base = `Design mode is active. This is a brainstorming session, NOT implementation — no code until the user approves via ExitDesignMode. Prefer read-only tools; you may only write the current design file(s). **Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.

## Re-entering Design Mode
A design file from a previous session already exists.
  1. Read the existing design file to understand what was previously designed.
  2. Confirm (or re-ask) the audit level before continuing — ask by INVOKING the AskUserQuestion tool (a real structured tool call). NEVER write the question or its options as text or as \`<ask_user_question>\`/XML markup; text produces no prompt and wastes the turn. Present exactly these options: Basic (only high-stakes assumptions flagged — fastest), Standard (every [C:INFERRED] assumption surfaced), Deep (key claim of every section + every assumption). If auto permission mode is active, AskUserQuestion is disabled — do NOT ask; default to Basic and note it in \`## Assumptions\`.
  3. Evaluate the user's current request against that design. Same topic: update it. Different topic: replace it.
  4. If it is a split index, the Parts manifest is the source of truth — read the sibling files it lists, write the next \`pending\` part and flip its row to \`done\`; never re-write a \`done\` part.
  5. Clarify any newly-required decisions one question per turn (seven-dimension checklist); verify any data source / hook point the design relies on actually exists in code; if the request names a concrete target, design THERE — do not silently retarget.
  6. Maintain decision tags [C:USER]/[C:INFERRED]/[C:DEFERRED]/[C:UPSTREAM] and the ## Assumptions chapter; keep the fidelity rubric.
  7. Run the self-review + post-write audit gate (list each [C:INFERRED] assumption verbatim for sign-off; do not ExitDesignMode until signed off), then update the design file before calling ExitDesignMode.

${visualCompanion(mockupAvailable)}

Your turn must end with either AskUserQuestion (to clarify) or ExitDesignMode (to request approval).`;
  const body = withSplitDirective(base, splitDirective);
  return withDesignFileFooter(body, designFilePath);
}

/** Message shown the moment design mode is entered (EnterDesignModeTool). */
export function designModeEntryMessage(
  designFilePath: SessionModeFilePath,
  mockupAvailable: boolean,
): string {
  const fileLine =
    designFilePath === null || designFilePath.length === 0
      ? 'No design file path is assigned yet. Invent your own filename under `.ody-code/designs/` (format: `YYYY-MM-DD-<topic>.md`). The host will normalize and deduplicate it on first write.'
      : `Design file: ${designFilePath}\nWrite the design to EXACTLY this path (its split parts go in the matching \`<stem>/\` subdirectory). Do NOT invent your own path, directory, or filename, and do NOT follow another tool's or skill's convention (e.g. \`.gpowers/designs/…\`).`;

  return [
    'Design mode is now active. This is a brainstorming / spec-exploration session — NOT an',
    'implementation session. Do NOT write or edit code until the user approves a design via',
    'ExitDesignMode. You may only write the current design file(s).',
    '',
    '**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.',
    '',
    fileLine,
    '',
    'Follow this workflow. Your VERY FIRST action is the Step 0 audit-strategy gate.',
    '',
    contractBody(mockupAvailable),
  ].join('\n');
}
