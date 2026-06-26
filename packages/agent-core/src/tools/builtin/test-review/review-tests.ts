import { z } from 'zod';
import type { Kaos } from '@odysseythink/kaos';
import type { Agent } from '#agent';
import type { BuiltinTool } from '#agent/tool';
import type { OdyConfig } from '@odysseythink/agent-core-shared';
import type { ExecutableToolResult, ToolExecution } from '#loop/types';
import { toInputJsonSchema } from '@odysseythink/agent-core-shared';
import { literalRulePattern, matchesGlobRuleSubject } from '#tools/support/rule-match';
import { parseGitStatusShort } from '@odysseythink/e2e-testing';
import {
  AdvancedSessionReviewer,
  shouldEscalate,
  type AdvancedSessionReviewResult,
  type MutationProbe,
  type ReviewFinding,
} from '#agent/session-mode/reviewer';
import DESCRIPTION from './review-tests.md';

/** Matches `.test.ts` / `.spec.tsx` / `.test.mjs` etc. */
const TEST_FILE_RE = /\.(test|spec)\.[cm]?[jt]sx?$/i;
/** Any JS/TS source file (used to gather implementation context). */
const SOURCE_FILE_RE = /\.[cm]?[jt]sx?$/i;
/** Rough context-window budget; mirrors the plan-review guard in agent/index.ts. */
const REVIEW_CONTENT_BUDGET_CHARS = 300_000;
const DEFAULT_TIMEOUT_MS = 120_000;

const ReviewTestsInputSchema = z.object({
  projectRoot: z.string().optional().describe('Optional project root; defaults to the agent workspace root.'),
}).strict();

export type ReviewTestsInput = z.infer<typeof ReviewTestsInputSchema>;

/**
 * Reviewer model alias for test review: a dedicated `mode_models.test_review`
 * alias when configured, otherwise the model the current mode is already using
 * (`activeModelAlias`), falling back to the configured default model. This keeps
 * the feature working out-of-the-box (default-on) with no extra config — at the
 * cost that, without a distinct `test_review` model, the reviewer runs on the
 * same model as the author, so independence comes only from the adversarial
 * prompt + fresh context, not a different model.
 */
export function resolveTestReviewerAlias(
  config: OdyConfig | undefined,
  activeModelAlias: string | undefined,
): string | undefined {
  return config?.modeModels?.testReview ?? activeModelAlias ?? config?.defaultModel;
}

/** One labelled file to feed the reviewer, in priority order. */
export interface ReviewEntry {
  readonly label: 'TEST FILE' | 'IMPLEMENTATION FILE';
  readonly path: string;
}

/**
 * Order the review context: each changed test file, immediately followed by its
 * same-directory sibling implementation (`foo.test.ts` → `foo.ts`), then any
 * remaining changed source files (these also cover cross-extension or split
 * `test/` ↔ `src/` layouts the sibling rule can't derive). Deduped, order-stable.
 * Pure so the selection/derivation can be tested without a filesystem.
 */
export function buildReviewEntries(
  testFiles: readonly string[],
  changedFiles: readonly string[],
): ReviewEntry[] {
  const entries: ReviewEntry[] = [];
  const seen = new Set<string>();
  const push = (label: ReviewEntry['label'], path: string): void => {
    if (seen.has(path)) return;
    seen.add(path);
    entries.push({ label, path });
  };
  for (const t of testFiles) {
    push('TEST FILE', t);
    const sibling = t.replace(/\.(test|spec)(\.[cm]?[jt]sx?)$/i, '$2');
    if (sibling !== t) push('IMPLEMENTATION FILE', sibling);
  }
  for (const f of changedFiles) {
    if (!TEST_FILE_RE.test(f) && SOURCE_FILE_RE.test(f)) push('IMPLEMENTATION FILE', f);
  }
  return entries;
}

export class ReviewTestsTool implements BuiltinTool<ReviewTestsInput> {
  readonly name = 'ReviewTests' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ReviewTestsInputSchema);

  constructor(
    private readonly kaos: Kaos,
    private readonly agent: Agent,
  ) {}

  resolveExecution(input: ReviewTestsInput): ToolExecution {
    return {
      description: 'Independently review the changed tests',
      approvalRule: literalRulePattern(this.name, '*'),
      matchesRule: (ruleArgs) => matchesGlobRuleSubject(ruleArgs, '*'),
      execute: (ctx) => this.execution(input, ctx),
    };
  }

  private async execution(
    input: ReviewTestsInput,
    ctx: { signal: AbortSignal },
  ): Promise<ExecutableToolResult> {
    const reviewerAlias = resolveTestReviewerAlias(this.agent.kimiConfig, this.agent.config.modelAlias);
    if (reviewerAlias === undefined || reviewerAlias.length === 0) {
      return {
        isError: true,
        output:
          'No reviewer model available. Configure mode_models.test_review (or a default model) in config.toml.',
      };
    }

    const workspaceRoot = this.kaos.getcwd();
    const projectRoot = input.projectRoot ?? workspaceRoot;
    const changedFiles = await this.getChangedFiles(projectRoot);

    const testFiles = changedFiles.filter((f) => TEST_FILE_RE.test(f));
    if (testFiles.length === 0) {
      return { output: 'No changed test files detected; nothing to review.' };
    }

    // Build review context as ordered (label, path) entries: each test file is
    // immediately followed by its sibling implementation (foo.test.ts → foo.ts)
    // so a single oversized test can't starve ALL implementation context under
    // the char budget; then any remaining changed source files. Deduped.
    const entries = buildReviewEntries(testFiles, changedFiles);

    const reviewContent = await this.buildReviewContent(projectRoot, entries);
    if (reviewContent.trim().length === 0) {
      return { output: 'Changed test files could not be read; nothing to review.' };
    }

    const result: AdvancedSessionReviewResult = await new AdvancedSessionReviewer(this.agent, {
      reviewerAlias,
      kind: 'tests',
      timeoutMs: DEFAULT_TIMEOUT_MS,
      signal: ctx.signal,
    }).review(reviewContent);

    if (ctx.signal.aborted) {
      return { isError: true, output: 'Test review cancelled.' };
    }

    if (!result.ok) {
      return { output: `Test review could not run: ${result.note ?? 'unknown error'} (reviewer: ${reviewerAlias}).` };
    }

    return { output: formatReport(result, reviewerAlias, testFiles) };
  }

  /**
   * Read each entry under projectRoot in order, skipping unreadable ones and
   * honouring the char budget. When the budget is hit, remaining files are
   * dropped but a VISIBLE marker is appended so the reviewer (and the reader of
   * the report) knows the input was truncated rather than silently thinned.
   */
  private async buildReviewContent(
    projectRoot: string,
    entries: readonly ReviewEntry[],
  ): Promise<string> {
    const k = this.kaos.withCwd(projectRoot);
    const sections: string[] = [];
    let total = 0;
    let omitted = 0;
    for (const { label, path } of entries) {
      if (total >= REVIEW_CONTENT_BUDGET_CHARS) {
        omitted++;
        continue;
      }
      let content: string;
      try {
        content = await k.readText(path);
      } catch {
        continue;
      }
      const section = `===== ${label}: ${path} =====\n\n${content}\n`;
      sections.push(section);
      total += section.length;
    }
    if (omitted > 0) {
      sections.push(`===== [truncated: ${omitted} file(s) omitted to fit the review budget] =====\n`);
    }
    return sections.join('\n');
  }

  private async getChangedFiles(projectRoot: string): Promise<string[]> {
    try {
      const k = this.kaos.withCwd(projectRoot);
      const proc = await k.exec('git', 'status', '--short', '--no-renames');
      const chunks: Buffer[] = [];
      proc.stdout.on('data', (chunk: Buffer) => chunks.push(chunk));
      await proc.wait();
      return parseGitStatusShort(Buffer.concat(chunks).toString('utf-8'));
    } catch {
      return [];
    }
  }
}

function formatFinding(f: ReviewFinding, auditLevel: AdvancedSessionReviewResult['auditLevel']): string {
  const escalate = shouldEscalate(f.severity, f.confidence, auditLevel);
  const tags = [
    f.severity.toUpperCase(),
    f.confidence ?? 'unrated',
    ...(escalate ? ['ESCALATE'] : []),
  ].join(' / ');
  const lines = [`- **[${tags}] ${f.title}**`, `  ${f.detail}`];
  if (f.location !== undefined) lines.push(`  _at ${f.location}_`);
  if (f.suggestedFix !== undefined) lines.push(`  _fix: ${f.suggestedFix}_`);
  return lines.join('\n');
}

function formatProbe(p: MutationProbe, i: number): string {
  const where = p.location.length > 0 ? ` at \`${p.location}\`` : '';
  const expect = p.expectedCatch.length > 0 ? ` — should be caught by: ${p.expectedCatch}` : '';
  return `${i + 1}. ${p.mutation}${where}${expect}`;
}

export function formatReport(
  result: AdvancedSessionReviewResult,
  reviewerAlias: string,
  testFiles: readonly string[],
): string {
  const out: string[] = [];
  out.push(`# Independent test review (reviewer: ${reviewerAlias})`);
  out.push(`Reviewed ${testFiles.length} changed test file(s): ${testFiles.join(', ')}`);

  if (result.findings.length === 0) {
    out.push('\n**Findings:** none — the reviewer could not break the tests.');
  } else {
    out.push(`\n## Findings (${result.findings.length})`);
    for (const f of result.findings) out.push(formatFinding(f, result.auditLevel));
  }

  const probes = result.mutationProbes ?? [];
  if (probes.length > 0) {
    out.push(`\n## Mutation probes (${probes.length}) — RUN THESE`);
    out.push(
      'The reviewer cannot run code. For EACH probe: apply the one-line break, run the named test, ' +
        'and observe the result. If the test stays GREEN under the break, that test is vacuous (a confirmed ' +
        'defect) — fix the test. Then REVERT the break. Report caught/missed for each.',
    );
    probes.forEach((p, i) => out.push(formatProbe(p, i)));
  } else {
    out.push('\n_No mutation probes were emitted._');
  }

  return out.join('\n');
}
