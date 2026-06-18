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
  readonly fetchDiff: (
    source: CodeReviewDiffSource,
    cwd: string,
    signal?: AbortSignal,
  ) => Promise<string>;
  readonly generate: (options: {
    readonly modelAlias: string;
    readonly systemPrompt: string;
    readonly userPrompt: string;
    readonly signal?: AbortSignal | undefined;
  }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> }; usage?: unknown }>;
  readonly resolveProviderConfig: (alias: string) => unknown;
  readonly estimateTokens: (text: string) => number;
  readonly deepRunner?: ((
    diff: string,
    input: CodeReviewRequestInput,
    signal?: AbortSignal,
  ) => Promise<CodeReviewReport>) | undefined;
  readonly auditScanner?: ((workspaceDir: string, signal?: AbortSignal) => Promise<RepoAuditDigest>) | undefined;
}

const MAX_DIFF_TOKENS = 100_000;

function combineSignals(userSignal?: AbortSignal, timeoutMs?: number): AbortSignal | undefined {
  const timeoutSignal = timeoutMs !== undefined ? AbortSignal.timeout(timeoutMs) : undefined;
  if (userSignal === undefined && timeoutSignal === undefined) return undefined;
  if (userSignal === undefined) return timeoutSignal;
  if (timeoutSignal === undefined) return userSignal;
  return AbortSignal.any([userSignal, timeoutSignal]);
}

export function createCodeReviewExecutor(deps: CodeReviewExecutorDeps) {
  return {
    async review(input: CodeReviewRequestInput): Promise<CodeReviewReport> {
      const isSimplicity = input.focus === 'simplicity' || input.scope === 'repo';
      const signal = combineSignals(input.signal, input.timeoutMs);

      input.onProgress?.({ requestId: '', stage: 'preparing', modelAlias: input.modelAlias });

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
          input.onProgress?.({ requestId: '', stage: 'audit-scanning', modelAlias: input.modelAlias });
          const digest = await deps.auditScanner(deps.cwd, signal);
          input.onProgress?.({ requestId: '', stage: 'generating', modelAlias: input.modelAlias });
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
          const report = parseSimplicityReport(text, input.modelAlias);
          input.onProgress?.({ requestId: '', stage: report.ok ? 'completed' : 'failed', modelAlias: input.modelAlias, detail: report.note });
          return report;
        } catch (error) {
          const note = `Code review failed: ${error instanceof Error ? error.message : String(error)}`;
          input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
          return {
            ok: false,
            reviewerAlias: input.modelAlias,
            findings: [],
            note,
          };
        }
      }

      // ── Diff-based path ──
      input.onProgress?.({ requestId: '', stage: 'fetching-diff', modelAlias: input.modelAlias });
      let diff: string;
      try {
        diff = await deps.fetchDiff(input.source, deps.cwd, signal);
      } catch (error) {
        const note = `Failed to fetch diff: ${error instanceof Error ? error.message : String(error)}`;
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }

      const estimatedTokens = deps.estimateTokens(diff);
      input.onProgress?.({ requestId: '', stage: 'fetching-diff', modelAlias: input.modelAlias, meta: { estimatedTokens } });
      if (estimatedTokens > MAX_DIFF_TOKENS) {
        const note = `Diff too large (~${estimatedTokens} tokens, limit ${MAX_DIFF_TOKENS}). Try a smaller range or use --base/--head.`;
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }

      if (input.deep) {
        if (deps.deepRunner !== undefined) {
          input.onProgress?.({ requestId: '', stage: 'deep-review', modelAlias: input.modelAlias });
          return deps.deepRunner(diff, input, signal);
        }
        const note = 'Deep review is not available in this context. Try without --deep.';
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }

      try {
        input.onProgress?.({ requestId: '', stage: 'generating', modelAlias: input.modelAlias });
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

        const report = isSimplicity
          ? parseSimplicityReport(text, input.modelAlias)
          : parseReviewReport(text, input.modelAlias);
        input.onProgress?.({ requestId: '', stage: report.ok ? 'completed' : 'failed', modelAlias: input.modelAlias, detail: report.note });
        return report;
      } catch (error) {
        const note = `Code review failed: ${error instanceof Error ? error.message : String(error)}`;
        input.onProgress?.({ requestId: '', stage: 'failed', modelAlias: input.modelAlias, detail: note });
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note,
        };
      }
    },
  };
}
