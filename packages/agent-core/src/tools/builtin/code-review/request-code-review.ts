import { z } from 'zod';
import type { Kaos } from '@odysseythink/kaos';
import type { Agent } from '#/agent';
import type { BuiltinTool } from '#/agent/tool';
import type { ExecutableToolResult, ToolExecution } from '#/loop/types';
import type { SessionSubagentHost } from '#/session/subagent-host';
import { toInputJsonSchema } from '#/tools/support/input-schema';
import { literalRulePattern, matchesGlobRuleSubject } from '#/tools/support/rule-match';
import { createDeadlineAbortSignal } from '#/utils/abort';
import { fetchDiff } from '#/code-review/diff';
import { buildReviewPrompt, parseReviewReport } from '#/code-review/prompt';
import { resolveCodeReviewModel } from '#/code-review/model-resolver';
import { renderCodeReviewReportToMarkdown } from '#/code-review/report';
import type { CodeReviewDiffSource, CodeReviewReport } from '#/code-review/types';
import DESCRIPTION from './request-code-review.md';

const RequestCodeReviewInputSchema = z.object({
  description: z.string().optional().describe('Short summary of what was built/changed.'),
  requirements: z.string().optional().describe('What the change is supposed to do (plan/requirements).'),
  model: z.string().optional().describe('Override the reviewer model alias. Defaults to the configured code-review model, else the default model.'),
  base: z.string().optional().describe('Base git ref. With head, reviews base..head; otherwise reviews the working tree (falling back to changes vs the default branch).'),
  head: z.string().optional().describe('Head git ref (use with base).'),
  pr: z.string().optional().describe('GitHub PR URL or number to review instead of local changes.'),
  timeout: z.number().int().min(30).max(3600).optional().describe('Optional timeout in seconds for the review (30-3600).'),
}).strict();

export type RequestCodeReviewInput = z.infer<typeof RequestCodeReviewInputSchema>;

export interface RunReviewerSubagentInput {
  readonly diff: string;
  readonly reviewerAlias: string;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly parentToolCallId: string;
  readonly signal: AbortSignal;
}

/**
 * The unified review engine: spawn the read-only `reviewer` subagent (on the
 * given model) seeded with the diff, then parse its structured final summary
 * into a {@link CodeReviewReport}. Shared so a deep/command path can reuse it.
 */
export async function runReviewerSubagent(
  subagentHost: SessionSubagentHost,
  input: RunReviewerSubagentInput,
): Promise<CodeReviewReport> {
  const handle = await subagentHost.spawn('reviewer', {
    parentToolCallId: input.parentToolCallId,
    prompt: buildReviewPrompt(input.diff, input.description, input.requirements),
    description: 'Code review',
    runInBackground: false,
    signal: input.signal,
    modelAlias: input.reviewerAlias,
  });
  const completion = await handle.completion;
  return parseReviewReport(completion.result, input.reviewerAlias);
}

export class RequestCodeReviewTool implements BuiltinTool<RequestCodeReviewInput> {
  readonly name = 'RequestCodeReview' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(RequestCodeReviewInputSchema);

  constructor(
    private readonly kaos: Kaos,
    private readonly agent: Agent,
  ) {}

  resolveExecution(input: RequestCodeReviewInput): ToolExecution {
    return {
      description: 'Request a second-model code review',
      approvalRule: literalRulePattern(this.name, '*'),
      matchesRule: (ruleArgs) => matchesGlobRuleSubject(ruleArgs, '*'),
      execute: (ctx) => this.execution(input, ctx),
    };
  }

  private async execution(
    input: RequestCodeReviewInput,
    ctx: { signal: AbortSignal; turnId: string; toolCallId: string },
  ): Promise<ExecutableToolResult> {
    const subagentHost = this.agent.subagentHost;
    if (subagentHost === undefined) {
      return { isError: true, output: 'Code review is unavailable: no subagent host in this context.' };
    }

    let reviewerAlias: string;
    try {
      reviewerAlias = resolveCodeReviewModel(
        'request',
        this.agent.kimiConfig?.modeModels,
        this.agent.kimiConfig?.defaultModel,
        { explicit: input.model },
        (alias) => {
          try {
            this.agent.modelProvider?.resolveProviderConfig(alias);
            return true;
          } catch {
            return false;
          }
        },
      );
    } catch (error) {
      return { isError: true, output: error instanceof Error ? error.message : String(error) };
    }

    const cwd = this.kaos.getcwd();
    let diff: string;
    try {
      diff = await this.resolveDiff(input, cwd, ctx.signal);
    } catch (error) {
      return { isError: true, output: `Failed to fetch diff: ${error instanceof Error ? error.message : String(error)}` };
    }
    if (diff.trim().length === 0) {
      return {
        output:
          'No changes to review: the working tree is clean and no committed changes were found against the default branch. Pass `base` and `head` (or `pr`) to review a specific range.',
      };
    }

    const deadline =
      input.timeout !== undefined ? createDeadlineAbortSignal(ctx.signal, input.timeout * 1000) : undefined;
    try {
      const report = await runReviewerSubagent(subagentHost, {
        diff,
        reviewerAlias,
        description: input.description,
        requirements: input.requirements,
        parentToolCallId: ctx.toolCallId,
        signal: deadline?.signal ?? ctx.signal,
      });

      if (!report.ok) {
        return { isError: true, output: report.note ?? 'Code review failed.' };
      }

      let output = renderCodeReviewReportToMarkdown(report);
      // A non-conforming reviewer message parses to zero findings + no summary,
      // which would otherwise look identical to a genuinely clean review.
      if (report.findings.length === 0 && (report.summary === undefined || report.summary.length === 0)) {
        output += '\n\n_(The reviewer returned no structured findings — this may be a clean review, or its output did not match the expected format.)_';
      }
      return { output };
    } catch (error) {
      if (deadline?.timedOut() === true) {
        return { isError: true, output: `Code review timed out after ${input.timeout}s.` };
      }
      throw error;
    } finally {
      deadline?.clear();
    }
  }

  /**
   * Resolve the diff to review. An explicit pr/base+head wins. Otherwise review
   * the working tree; if it is clean (common in per-task-commit workflows), fall
   * back to the branch's committed changes vs the default branch's merge-base.
   */
  private async resolveDiff(input: RequestCodeReviewInput, cwd: string, signal: AbortSignal): Promise<string> {
    if (input.pr !== undefined) {
      return fetchDiff({ kind: 'pr', prUrlOrNumber: input.pr }, cwd, signal);
    }
    if (input.base !== undefined && input.head !== undefined) {
      return fetchDiff({ kind: 'commits', base: input.base, head: input.head }, cwd, signal);
    }
    const workingTree = await fetchDiff({ kind: 'working-tree' }, cwd, signal);
    if (workingTree.trim().length > 0) return workingTree;

    const base = await this.resolveDefaultBase(cwd);
    if (base === undefined) return workingTree;
    const source: CodeReviewDiffSource = { kind: 'commits', base, head: 'HEAD' };
    return fetchDiff(source, cwd, signal);
  }

  /** Merge-base of HEAD with the default branch (prefers remote-tracking refs). */
  private async resolveDefaultBase(cwd: string): Promise<string | undefined> {
    const k = this.kaos.withCwd(cwd);
    let headSha: string;
    try {
      headSha = (await this.git(k, ['rev-parse', 'HEAD'])).trim();
    } catch {
      return undefined;
    }
    for (const ref of ['origin/HEAD', 'origin/main', 'origin/master', 'main', 'master']) {
      try {
        const base = (await this.git(k, ['merge-base', 'HEAD', ref])).trim();
        if (base.length > 0 && base !== headSha) return base;
      } catch {
        // try next ref
      }
    }
    return undefined;
  }

  private async git(k: Kaos, args: string[]): Promise<string> {
    const proc = await k.exec('git', ...args);
    const chunks: Buffer[] = [];
    proc.stdout.on('data', (chunk: Buffer) => chunks.push(chunk));
    await proc.wait();
    return Buffer.concat(chunks).toString('utf-8');
  }
}
