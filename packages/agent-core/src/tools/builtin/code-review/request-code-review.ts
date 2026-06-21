import { z } from 'zod';
import type { Kaos } from '@odysseythink/kaos';
import type { Agent } from '#/agent';
import type { BuiltinTool } from '#/agent/tool';
import type { ExecutableToolResult, ToolExecution } from '#/loop/types';
import type { SessionSubagentHost } from '#/session/subagent-host';
import { toInputJsonSchema } from '#/tools/support/input-schema';
import { literalRulePattern, matchesGlobRuleSubject } from '#/tools/support/rule-match';
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
  base: z.string().optional().describe('Base git ref. With head, reviews base..head; otherwise reviews the working tree.'),
  head: z.string().optional().describe('Head git ref (use with base).'),
  pr: z.string().optional().describe('GitHub PR URL or number to review instead of local changes.'),
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

    const source: CodeReviewDiffSource =
      input.pr !== undefined
        ? { kind: 'pr', prUrlOrNumber: input.pr }
        : input.base !== undefined && input.head !== undefined
          ? { kind: 'commits', base: input.base, head: input.head }
          : { kind: 'working-tree' };

    let diff: string;
    try {
      diff = await fetchDiff(source, this.kaos.getcwd(), ctx.signal);
    } catch (error) {
      return { isError: true, output: `Failed to fetch diff: ${error instanceof Error ? error.message : String(error)}` };
    }

    const report = await runReviewerSubagent(subagentHost, {
      diff,
      reviewerAlias,
      description: input.description,
      requirements: input.requirements,
      parentToolCallId: ctx.toolCallId,
      signal: ctx.signal,
    });

    const isError = !report.ok;
    return {
      output: report.ok
        ? renderCodeReviewReportToMarkdown(report)
        : (report.note ?? 'Code review failed.'),
      isError,
    };
  }
}
