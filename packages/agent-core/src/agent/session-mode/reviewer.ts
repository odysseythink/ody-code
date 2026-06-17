/**
 * reviewer.ts — a second-model critique of a design document OR an
 * execution plan (the `kind` option selects the attack surface).
 *
 * Plan/design mode runs on a cheap model; this runs a SINGLE pass of an
 * independent, usually more capable model over the finished document to catch
 * what the authoring model's own (correlated) blind spots miss. The reviewer is
 * primed as an
 * ADVERSARY, not a neutral reviewer — its win condition is to break the design
 * with a concrete trigger, which fights the confirmation bias a same-stance
 * review inherits. It only flags — it never edits — and findings are
 * severity-tagged so the caller can escalate the risky ones to a human while
 * merely listing the rest.
 *
 * The reviewer model is a different configured alias than the session model, so
 * we resolve ITS provider + auth explicitly (the session model's auth must not
 * leak onto the reviewer's provider) and call {@link Agent.rawGenerate} directly.
 */

import { createProvider } from '@odysseythink/kosong';
import type { ProviderRequestAuth } from '@odysseythink/kosong';

import type { Agent } from '..';

export type Severity = 'high' | 'med' | 'low';
export type AuditLevel = 'Basic' | 'Standard' | 'Deep';
export type Confidence = 'certain' | 'likely' | 'speculative';

export interface ReviewFinding {
  readonly severity: Severity;
  readonly confidence?: Confidence;
  readonly title: string;
  readonly detail: string;
  readonly location?: string;
  readonly suggestedFix?: string;
}

/**
 * An executable perturbation the reviewer believes a sound test must catch.
 * Because the reviewer is a pure single-shot text model (no tools, cannot run
 * code), it only *describes* the mutation; the controller agent applies it,
 * runs the named test, and confirms it goes red. A test that stays green under
 * its probe is proven to be vacuous — this turns "the tests look weak" from an
 * assertion into a runnable check. Only emitted for `kind: 'tests'`.
 */
export interface MutationProbe {
  /** Where to inject the fault, ideally `file:line`. */
  readonly location: string;
  /** The exact one-line break to apply (negate a condition, change a constant, early-return). */
  readonly mutation: string;
  /** Which existing test SHOULD turn red once the mutation is applied. */
  readonly expectedCatch: string;
}

export interface AdvancedSessionReviewResult {
  /** Audit level read from the design file (drives human escalation). */
  readonly auditLevel: AuditLevel;
  readonly findings: readonly ReviewFinding[];
  /**
   * Executable perturbations the controller should run to confirm the tests
   * actually catch regressions. Only populated for `kind: 'tests'`; empty
   * otherwise.
   */
  readonly mutationProbes?: readonly MutationProbe[];
  /** false when the reviewer could not run or its output was unusable. */
  readonly ok: boolean;
  /** Human-readable degradation reason when `ok` is false. */
  readonly note?: string;
}

export interface AdvancedSessionReviewerOptions {
  /** Configured model alias to run the critique on (e.g. "ody-code/kimi-for-coding"). */
  readonly reviewerAlias: string;
  /**
   * Which document kind to attack. Selects the critic prompt's attack surface:
   * `design` (spec/architecture), `plan` (execution plan), or `tests` (the test
   * code the implementation model wrote). Defaults to `design`.
   */
  readonly kind?: 'plan' | 'design' | 'tests';
  /** Hard cap on the critique generation. Defaults to 120s. */
  readonly timeoutMs?: number;
  /**
   * Optional external abort signal (e.g. a tool's cancellation). Combined with
   * the internal timeout via `AbortSignal.any`, so a caller cancellation aborts
   * the in-flight generation rather than only relabelling a finished result.
   */
  readonly signal?: AbortSignal;
}

export const DEFAULT_AUDIT_LEVEL: AuditLevel = 'Standard';

/**
 * Severities that get escalated to the human for key verification, scaled to the
 * recorded audit level. Monotonic: the stricter the level, the more it escalates.
 */
export function escalatedSeverities(level: AuditLevel): readonly Severity[] {
  switch (level) {
    case 'Basic':
      return ['high'];
    case 'Standard':
      return ['high', 'med'];
    case 'Deep':
      return ['high', 'med', 'low'];
  }
}

/**
 * Whether a finding should be escalated to the human for confirmation.
 * `speculative` findings never escalate regardless of severity — the reviewer
 * itself flagged them as unverified assumptions, so they must not block the
 * human sign-off path. `undefined` confidence is treated as non-speculative
 * (preserves existing behaviour for reviewers that omit the field).
 */
export function shouldEscalate(
  severity: Severity,
  confidence: Confidence | undefined,
  level: AuditLevel,
): boolean {
  return escalatedSeverities(level).includes(severity) && confidence !== 'speculative';
}

/** Reads the `## Audit Level` line the design contract requires; falls back to Standard. */
export function parseAuditLevel(content: string): AuditLevel {
  const match = content.match(
    /##\s*Audit Level[\s\S]{0,300}?\*\*\s*(Basic|Standard|Deep)\s*\*\*/i,
  );
  if (match !== null) {
    const value = (match[1] ?? '').toLowerCase();
    if (value === 'basic') return 'Basic';
    if (value === 'deep') return 'Deep';
    return 'Standard';
  }
  return DEFAULT_AUDIT_LEVEL;
}

/** Attack surface for a DESIGN document (spec / architecture exploration). */
const DESIGN_ATTACK_SURFACE = `- Security: every filter/regex/matching rule for false positives (rejects valid input) and false negatives (lets through what must be caught); secrets or PII leaking into a log or filename.
- Logic & correctness: algorithms that produce a wrong result on a realistic input; off-by-one, ordering, null/empty, concurrency, collision/uniqueness of generated identifiers.
- Tests: every behaviour needs a must-pass AND a must-reject case; an assertion that contradicts a constant it depends on is a defect.
- Integration: any data source, field, event, hook, or guard the design relies on that may not exist — or that keys off a DIFFERENT value than the producer writes (e.g. a file written under one identifier but authorized by a guard matching another). Open it and trace one concrete value through; do not assume it "continues to work".
- Internal consistency: sections that contradict each other; a decision stated one way here and another way there.`;

/**
 * Attack surface for an EXECUTION PLAN (plan-mode output). The failure modes are
 * deliberately different from a design's — they target task structure, dependency
 * ordering, placeholders, caller soundness, and the filter/consumer blades.
 */
const PLAN_ATTACK_SURFACE = `- Dependency soundness: a task that uses a symbol, type, or file only a LATER task defines; a \`Depends on:\` not satisfied by an earlier task; for a split plan, a cross-file \`Depends on: <file>: Task N\` whose target part comes later.
- Phantom tasks: a task that produces no verifiable change — \`--allow-empty\`, "already done in Task N", a manufactured no-op.
- Placeholders: TODO/TBD, "add appropriate error handling/validation", "write tests for the above" without the test code, "similar to Task N" without repeating the code, or a reference to a type/function no task defines.
- Shared-signature & callers: a task that changes a shared signature/type/field without updating EVERY caller (incl. test files) and without a whole-tree typecheck; the same signature churned across multiple tasks.
- Test-the-risk: a state-mutating task with only a compile check, not a behavioral assert; a filter/word-list that rejects a must-survive input (e.g. the list contains a substring of an input that must pass — 'auth-refactor' eaten by 'auth'); a consumer that keys off a DIFFERENT value than the producer writes (e.g. a file written under \`fileStem\` but authorized by a guard matching \`planId\`).
- Spec coverage: a spec-coverage table row marked "covered" that no task actually implements, or a spec requirement with no task and no GAP marker.
- Execution realism: stale \`path:line\` references; a "replace the whole file/function" step that silently drops existing logic; a step whose command or expected output would not run as written.`;

/**
 * Attack surface for the TEST CODE the implementation model wrote. The author and
 * the test author are the SAME model, so its blind spots are perfectly correlated:
 * a test written to confirm the code it just wrote tends to encode the bug rather
 * than catch it. These are the shapes that make a test pass while proving nothing.
 */
const TEST_CODE_ATTACK_SURFACE = `- Tautology: an assertion that re-states a value the implementation itself computed (e.g. \`expect(result).toBe(result)\`, or snapshotting the current output as the "expected" value) — it can never fail on a wrong implementation.
- Mock theatre: the test asserts a mock was called / returned a stubbed value instead of exercising real behaviour, so it tests the mock, not the code.
- Happy-path only: every behaviour needs a must-pass AND a must-reject case; flag any behaviour with no negative/edge/error case (empty, null, boundary, duplicate, failure path).
- Weak assertions: assertions so loose the implementation could be wrong and still pass (e.g. \`toBeDefined()\` where a value matters, \`toBeTruthy()\` on a number, asserting length but not contents, no assertion at all).
- Unguarded behaviour: a behaviour stated in the spec / implied by the changed code that NO test pins down at all.
- Assertion-vs-constant contradiction: an expected value that contradicts a constant or type the code depends on is itself a defect.`;

/**
 * Compose the adversarial critic prompt. The stance (attacker, win-by-breaking),
 * evidence rules, severity scale and STRICT-JSON envelope are shared verbatim so
 * the document kinds can never drift; only the noun and attack surface differ.
 *
 * When `withMutationProbes` is set (test-code reviews), the prompt also asks for
 * up to three executable perturbations and extends the JSON envelope with a
 * `mutationProbes` array — the reviewer cannot run code, so the controller agent
 * executes these to PROVE a weak test stays green under a real fault.
 */
function composeCriticPrompt(
  docNoun: string,
  shortNoun: string,
  attackSurface: string,
  withMutationProbes = false,
): string {
  const mutationInstruction = withMutationProbes
    ? `\n\nMutation probes: you cannot run code, so additionally hand the controller up to THREE executable perturbations that a SOUND test must catch — pick the riskiest logic in the implementation under test. Each probe is a single concrete break (negate a condition, change a constant, early-return) at a \`file:line\`, plus the name of the existing test that SHOULD turn red. If the suite is so weak you cannot name a test that would catch a probe, that is itself a high-severity finding.`
    : '';
  const mutationEnvelope = withMutationProbes
    ? `,"mutationProbes":[{"location":"<file:line>","mutation":"<the one-line break to apply>","expectedCatch":"<name of the test that should fail>"}]`
    : '';
  const mutationEmptyNote = withMutationProbes
    ? ' Always include the "mutationProbes" array (use [] only if there is genuinely no riskable logic in the change).'
    : '';
  return `You are an ADVERSARY, not a reviewer. A different, less capable model wrote the ${docNoun} below, and your goal is to BREAK it. You win only by producing a concrete defect with a trigger that proves it. "Looks fine" / "no issues" is a LOSING answer — if you cannot break the ${shortNoun}, you have not attacked it hard enough. Stay honest, though: a fabricated or unfalsifiable defect is also a loss.

Attack surface — hunt along every one of these, because the author's correlated blind spots cluster here:
${attackSurface}

Rules of engagement: every finding MUST carry a CONCRETE input that breaks it (e.g. "the substring filter rejects 'auth-refactor' because it contains 'auth'") or a concrete trace (e.g. "a file written under fileStem is denied by the guard matching planId"). A finding without a concrete trigger does not count and must be dropped — it loses you the round. Self-falsification gate: before writing down any finding, mentally execute its trigger and read the result — if the output you describe is actually correct or expected, you have NOT broken anything; discard the finding. A finding whose own trigger produces the right answer is self-contradictory and counts as a fabricated defect (a loss).${mutationInstruction}

Severity:
- high: will cause a real bug, wrong behaviour, security/data issue, or a self-contradiction that blocks implementation.
- med: a likely edge-case failure, a missing test case, or a maintainability/scope problem.
- low: a nit, naming, or minor clarity issue.

Confidence — rate how thoroughly you verified the trigger (be honest; over-claiming burns credibility):
- certain: you traced the trigger end-to-end through the text and confirmed it would fire.
- likely: strong evidence but you did not fully trace every branch.
- speculative: an untested assumption ("if X then…" where you did not verify X exists or holds).

Output STRICT JSON and nothing else (no prose, no markdown fences):
{"findings":[{"severity":"high|med|low","confidence":"certain|likely|speculative","title":"<short>","detail":"<what's wrong + the concrete trigger>","location":"<section/line if known, else omit>","suggestedFix":"<one line, else omit>"}]${mutationEnvelope}}
If — after a genuine attack — the ${shortNoun} truly has no breakable defect, output {"findings":[]}.${mutationEmptyNote}`;
}

/**
 * Adversarial critic prompt for the given document kind. `design` (default)
 * targets spec/architecture failure modes; `plan` targets execution-plan failure
 * modes (dependency soundness, phantom tasks, placeholders, callers, filter
 * blades, spec-coverage GAPs); `tests` attacks the test code itself (tautology,
 * mock theatre, happy-path-only, weak assertions) and additionally emits runnable
 * mutation probes.
 */
export function buildCriticPrompt(kind: 'plan' | 'design' | 'tests' = 'design'): string {
  switch (kind) {
    case 'plan':
      return composeCriticPrompt('EXECUTION PLAN', 'plan', PLAN_ATTACK_SURFACE);
    case 'tests':
      return composeCriticPrompt('TEST SUITE', 'test suite', TEST_CODE_ATTACK_SURFACE, true);
    case 'design':
      return composeCriticPrompt('DESIGN DOCUMENT', 'design', DESIGN_ATTACK_SURFACE);
    default: {
      const _exhaustive: never = kind;
      return _exhaustive;
    }
  }
}

/**
 * Tolerant JSON parse of the reviewer's output. Returns null when the output is
 * not usable so the caller can degrade gracefully rather than invent findings.
 */
export function parseFindings(raw: string): ReviewFinding[] | null {
  const stripped = stripCodeFences(raw).trim();
  if (stripped.length === 0) return null;

  let parsed: unknown;
  try {
    parsed = JSON.parse(stripped);
  } catch {
    // Some models still wrap in prose; try to salvage the first {...} block.
    const start = stripped.indexOf('{');
    const end = stripped.lastIndexOf('}');
    if (start === -1 || end <= start) return null;
    try {
      parsed = JSON.parse(stripped.slice(start, end + 1));
    } catch {
      return null;
    }
  }

  const rawFindings = (parsed as { findings?: unknown })?.findings;
  if (!Array.isArray(rawFindings)) return null;

  const findings: ReviewFinding[] = [];
  for (const entry of rawFindings) {
    const finding = coerceFinding(entry);
    if (finding !== null) findings.push(finding);
  }
  return findings;
}

/**
 * Tolerant parse of the optional `mutationProbes` array from the reviewer's
 * output, mirroring {@link parseFindings}. Returns an empty array when absent or
 * malformed — probes are a best-effort bonus, never a reason to fail the review.
 */
export function parseMutationProbes(raw: string): MutationProbe[] {
  const stripped = stripCodeFences(raw).trim();
  if (stripped.length === 0) return [];

  let parsed: unknown;
  try {
    parsed = JSON.parse(stripped);
  } catch {
    const start = stripped.indexOf('{');
    const end = stripped.lastIndexOf('}');
    if (start === -1 || end <= start) return [];
    try {
      parsed = JSON.parse(stripped.slice(start, end + 1));
    } catch {
      return [];
    }
  }

  const rawProbes = (parsed as { mutationProbes?: unknown })?.mutationProbes;
  if (!Array.isArray(rawProbes)) return [];

  const probes: MutationProbe[] = [];
  for (const entry of rawProbes) {
    const probe = coerceProbe(entry);
    if (probe !== null) probes.push(probe);
  }
  return probes;
}

function coerceProbe(entry: unknown): MutationProbe | null {
  if (typeof entry !== 'object' || entry === null) return null;
  const record = entry as Record<string, unknown>;
  const location = typeof record['location'] === 'string' ? record['location'].trim() : '';
  const mutation = typeof record['mutation'] === 'string' ? record['mutation'].trim() : '';
  const expectedCatch = typeof record['expectedCatch'] === 'string' ? record['expectedCatch'].trim() : '';
  // A probe with no mutation is useless; location/expectedCatch may be coarse.
  if (mutation.length === 0) return null;
  return { location, mutation, expectedCatch };
}

function coerceFinding(entry: unknown): ReviewFinding | null {
  if (typeof entry !== 'object' || entry === null) return null;
  const record = entry as Record<string, unknown>;
  const severity = record['severity'];
  if (severity !== 'high' && severity !== 'med' && severity !== 'low') return null;
  const title = typeof record['title'] === 'string' ? record['title'].trim() : '';
  const detail = typeof record['detail'] === 'string' ? record['detail'].trim() : '';
  if (title.length === 0 && detail.length === 0) return null;
  const location = typeof record['location'] === 'string' ? record['location'].trim() : '';
  const suggestedFix = typeof record['suggestedFix'] === 'string' ? record['suggestedFix'].trim() : '';
  const rawConfidence = record['confidence'];
  const confidence: Confidence | undefined =
    rawConfidence === 'certain' || rawConfidence === 'likely' || rawConfidence === 'speculative'
      ? rawConfidence
      : undefined;
  return {
    severity,
    ...(confidence !== undefined ? { confidence } : {}),
    title: title.length > 0 ? title : detail.slice(0, 60),
    detail,
    ...(location.length > 0 ? { location } : {}),
    ...(suggestedFix.length > 0 ? { suggestedFix } : {}),
  };
}

function stripCodeFences(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed.startsWith('```')) return raw;
  const end = trimmed.lastIndexOf('```');
  if (end <= 3) return raw;
  const firstNewline = trimmed.indexOf('\n');
  const start = firstNewline === -1 ? 3 : firstNewline + 1;
  return trimmed.slice(start, end).trimEnd();
}

export class AdvancedSessionReviewer {
  constructor(
    private readonly agent: Agent,
    private readonly options: AdvancedSessionReviewerOptions,
  ) {}

  async review(designContent: string): Promise<AdvancedSessionReviewResult> {
    const auditLevel = parseAuditLevel(designContent);
    const fail = (note: string, reason: string): AdvancedSessionReviewResult => {
      this.agent.telemetry.track('design_review_failed', { reason });
      return { auditLevel, findings: [], ok: false, note };
    };

    if (this.agent.modelProvider === undefined) {
      return fail('No model provider available for review.', 'no_model_provider');
    }

    let provider;
    let withAuth;
    try {
      const resolved = this.agent.modelProvider.resolveProviderConfig(this.options.reviewerAlias);
      // Disable extended thinking on the reviewer. The critique is a single-shot
      // structured task (emit a JSON findings array) — the model can still reason
      // inside its answer, but a long streamed reasoning_content trace is pure
      // latency here and is the usual cause of the review tripping its timeout
      // (e.g. GLM-5.1, which defaults thinking ON). Any reasoning the model needs
      // happens in the response itself, not a separate exposed CoT stream.
      provider = createProvider(resolved.provider).withThinking('off');
      withAuth = this.agent.modelProvider.resolveAuth?.(this.options.reviewerAlias, {
        log: this.agent.log,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'unknown error';
      return fail(`Reviewer model "${this.options.reviewerAlias}" unavailable: ${message}`, 'alias_unresolved');
    }

    const messages = [
      { role: 'user' as const, content: [{ type: 'text' as const, text: designContent }], toolCalls: [] },
    ];
    const timeoutSignal = AbortSignal.timeout(this.options.timeoutMs ?? 120_000);
    const runOptions = {
      signal:
        this.options.signal === undefined
          ? timeoutSignal
          : AbortSignal.any([this.options.signal, timeoutSignal]),
    };
    const criticPrompt = buildCriticPrompt(this.options.kind);
    const call = (auth?: ProviderRequestAuth) =>
      this.agent.rawGenerate(
        provider,
        criticPrompt,
        [],
        messages,
        undefined,
        auth === undefined ? runOptions : { ...runOptions, auth },
      );

    let raw: string;
    try {
      const result = withAuth === undefined ? await call() : await withAuth((auth) => call(auth));
      raw = result.message.content
        .filter((part) => part.type === 'text')
        .map((part) => part.text)
        .join('')
        .trim();
    } catch (error) {
      const reason = error instanceof Error ? error.name : 'unknown_error';
      return fail(`Review generation failed (${reason}).`, reason);
    }

    const findings = parseFindings(raw);
    if (findings === null) {
      let dumpPath: string | undefined;
      if (this.agent.homedir !== undefined) {
        dumpPath = `${this.agent.homedir}/logs/reviewer-fail-${Date.now()}.txt`;
        try {
          await this.agent.kaos.mkdir(`${this.agent.homedir}/logs`, { parents: true, existOk: true });
          await this.agent.kaos.writeText(dumpPath, raw);
        } catch {
          dumpPath = undefined;
        }
      }
      this.agent.log.warn(
        'AdvancedSessionReviewer: reviewer output could not be parsed as findings.',
        { rawLength: raw.length, dumpPath },
      );
      return fail('Reviewer output could not be parsed as findings.', 'unparseable');
    }

    const mutationProbes = this.options.kind === 'tests' ? parseMutationProbes(raw) : [];

    this.agent.telemetry.track('advanced_session_review_completed', {
      auditLevel,
      findingCount: String(findings.length),
      ...(this.options.kind === 'tests' ? { mutationProbeCount: String(mutationProbes.length) } : {}),
    });
    return { auditLevel, findings, ok: true, ...(mutationProbes.length > 0 ? { mutationProbes } : {}) };
  }
}
