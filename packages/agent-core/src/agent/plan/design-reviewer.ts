/**
 * design-reviewer.ts — a second-model critique of a design document OR an
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

export interface ReviewFinding {
  readonly severity: Severity;
  readonly title: string;
  readonly detail: string;
  readonly location?: string;
  readonly suggestedFix?: string;
}

export interface DesignReviewResult {
  /** Audit level read from the design file (drives human escalation). */
  readonly auditLevel: AuditLevel;
  readonly findings: readonly ReviewFinding[];
  /** false when the reviewer could not run or its output was unusable. */
  readonly ok: boolean;
  /** Human-readable degradation reason when `ok` is false. */
  readonly note?: string;
}

export interface DesignReviewerOptions {
  /** Configured model alias to run the critique on (e.g. "ody-code/kimi-for-coding"). */
  readonly reviewerAlias: string;
  /**
   * Which document kind to attack. Selects the critic prompt's attack surface:
   * `design` (spec/architecture) or `plan` (execution plan). Defaults to `design`.
   */
  readonly kind?: 'plan' | 'design';
  /** Hard cap on the critique generation. Defaults to 60s. */
  readonly timeoutMs?: number;
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
 * Compose the adversarial critic prompt. The stance (attacker, win-by-breaking),
 * evidence rules, severity scale and STRICT-JSON envelope are shared verbatim so
 * the two document kinds can never drift; only the noun and attack surface differ.
 */
function composeCriticPrompt(docNoun: string, shortNoun: string, attackSurface: string): string {
  return `You are an ADVERSARY, not a reviewer. A different, less capable model wrote the ${docNoun} below, and your goal is to BREAK it. You win only by producing a concrete defect with a trigger that proves it. "Looks fine" / "no issues" is a LOSING answer — if you cannot break the ${shortNoun}, you have not attacked it hard enough. Stay honest, though: a fabricated or unfalsifiable defect is also a loss.

Attack surface — hunt along every one of these, because the author's correlated blind spots cluster here:
${attackSurface}

Rules of engagement: every finding MUST carry a CONCRETE input that breaks it (e.g. "the substring filter rejects 'auth-refactor' because it contains 'auth'") or a concrete trace (e.g. "a file written under fileStem is denied by the guard matching planId"). A finding without a concrete trigger does not count and must be dropped — it loses you the round.

Severity:
- high: will cause a real bug, wrong behaviour, security/data issue, or a self-contradiction that blocks implementation.
- med: a likely edge-case failure, a missing test case, or a maintainability/scope problem.
- low: a nit, naming, or minor clarity issue.

Output STRICT JSON and nothing else (no prose, no markdown fences):
{"findings":[{"severity":"high|med|low","title":"<short>","detail":"<what's wrong + the concrete trigger>","location":"<section/line if known, else omit>","suggestedFix":"<one line, else omit>"}]}
If — after a genuine attack — the ${shortNoun} truly has no breakable defect, output {"findings":[]}.`;
}

/** Adversarial critic prompt for a DESIGN document. */
export function buildCriticPrompt(): string {
  return composeCriticPrompt('DESIGN DOCUMENT', 'design', DESIGN_ATTACK_SURFACE);
}

/** Adversarial critic prompt for an EXECUTION PLAN (plan-mode output). */
export function buildPlanCriticPrompt(): string {
  return composeCriticPrompt('EXECUTION PLAN', 'plan', PLAN_ATTACK_SURFACE);
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
  return {
    severity,
    title: title.length > 0 ? title : detail.slice(0, 60),
    detail,
    ...(location.length > 0 ? { location } : {}),
    ...(suggestedFix.length > 0 ? { suggestedFix } : {}),
  };
}

function stripCodeFences(raw: string): string {
  const fenced = raw.match(/```(?:json)?\s*([\s\S]*?)```/i);
  return fenced?.[1] ?? raw;
}

export class DesignReviewer {
  constructor(
    private readonly agent: Agent,
    private readonly options: DesignReviewerOptions,
  ) {}

  async review(designContent: string): Promise<DesignReviewResult> {
    const auditLevel = parseAuditLevel(designContent);
    const fail = (note: string, reason: string): DesignReviewResult => {
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
      provider = createProvider(resolved.provider);
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
    const runOptions = { signal: AbortSignal.timeout(this.options.timeoutMs ?? 60_000) };
    const criticPrompt =
      this.options.kind === 'plan' ? buildPlanCriticPrompt() : buildCriticPrompt();
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
      return fail('Reviewer output could not be parsed as findings.', 'unparseable');
    }

    this.agent.telemetry.track('design_review_completed', {
      auditLevel,
      findingCount: String(findings.length),
    });
    return { auditLevel, findings, ok: true };
  }
}
