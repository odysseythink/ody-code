import type { CodeReviewRequestInput, CodeReviewReport } from './types';
import { buildReviewPrompt, parseReviewReport } from './prompt';
import {
  buildSimplicityReviewPrompt,
  buildSimplicityAuditPrompt,
  parseSimplicityReport,
  type RepoAuditDigest,
} from './simplicity';
import type { CodeReviewDiffSource } from './types';

export interface CodeReviewExecutorDeps {
  readonly cwd: string;
  readonly fetchDiff: (source: CodeReviewDiffSource, cwd: string) => Promise<string>;
  readonly generate: (options: {
    readonly modelAlias: string;
    readonly systemPrompt: string;
    readonly userPrompt: string;
    readonly signal?: AbortSignal | undefined;
  }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> }; usage?: unknown }>;
  readonly resolveProviderConfig: (alias: string) => unknown;
  readonly estimateTokens: (text: string) => number;
  readonly deepRunner?: ((diff: string, input: CodeReviewRequestInput) => Promise<CodeReviewReport>) | undefined;
  readonly auditScanner?: ((workspaceDir: string, signal?: AbortSignal) => Promise<RepoAuditDigest>) | undefined;
}

const MAX_DIFF_TOKENS = 100_000;

export function createCodeReviewExecutor(deps: CodeReviewExecutorDeps) {
  return {
    async review(input: CodeReviewRequestInput): Promise<CodeReviewReport> {
      const isSimplicity = input.focus === 'simplicity' || input.scope === 'repo';
      const signal = input.timeoutMs !== undefined ? AbortSignal.timeout(input.timeoutMs) : undefined;

      // ── Repo audit path ──
      if (input.scope === 'repo') {
        if (deps.auditScanner === undefined) {
          return {
            ok: false,
            reviewerAlias: input.modelAlias,
            findings: [],
            note: 'Repo audit is not available in this context.',
          };
        }
        try {
          const digest = await deps.auditScanner(deps.cwd, signal);
          const userPrompt = buildSimplicityAuditPrompt(digest);
          const response = await deps.generate({
            modelAlias: input.modelAlias,
            systemPrompt: '',
            userPrompt,
            signal,
          });
          const text = response.message.content
            .filter((p) => p.type === 'text')
            .map((p) => p.text)
            .join('');
          return parseSimplicityReport(text, input.modelAlias);
        } catch (error) {
          return {
            ok: false,
            reviewerAlias: input.modelAlias,
            findings: [],
            note: `Code review failed: ${error instanceof Error ? error.message : String(error)}`,
          };
        }
      }

      // ── Diff-based path ──
      let diff: string;
      try {
        diff = await deps.fetchDiff(input.source, deps.cwd);
      } catch (error) {
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: `Failed to fetch diff: ${error instanceof Error ? error.message : String(error)}`,
        };
      }

      const estimatedTokens = deps.estimateTokens(diff);
      if (estimatedTokens > MAX_DIFF_TOKENS) {
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: `Diff too large (~${estimatedTokens} tokens, limit ${MAX_DIFF_TOKENS}). Try a smaller range or use --base/--head.`,
        };
      }

      if (input.deep) {
        if (deps.deepRunner !== undefined) {
          return deps.deepRunner(diff, input);
        }
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: 'Deep review is not available in this context. Try without --deep.',
        };
      }

      try {
        const userPrompt = isSimplicity
          ? buildSimplicityReviewPrompt(diff, input.description, input.requirements)
          : buildReviewPrompt(diff, input.description, input.requirements);

        const response = await deps.generate({
          modelAlias: input.modelAlias,
          systemPrompt: '',
          userPrompt,
          signal,
        });
        const text = response.message.content
          .filter((p) => p.type === 'text')
          .map((p) => p.text)
          .join('');

        return isSimplicity
          ? parseSimplicityReport(text, input.modelAlias)
          : parseReviewReport(text, input.modelAlias);
      } catch (error) {
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: `Code review failed: ${error instanceof Error ? error.message : String(error)}`,
        };
      }
    },
  };
}
