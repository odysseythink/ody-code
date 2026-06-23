import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/ody-tui';
import type { SlashCommandHost } from './dispatch';
import type { CodeReviewDiffSource } from '@odysseythink/ody-code-sdk';

interface SlashArgs {
  readonly base?: string;
  readonly head?: string;
  readonly pr?: string;
  readonly model?: string;
  readonly description?: string;
  readonly requirements?: string;
}

function parseArgs(args: string): SlashArgs {
  const tokens = args.trim().split(/\s+/).filter((t) => t.length > 0);
  const result: Record<string, unknown> = {};
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;
    if (
      token === '--base' || token === '--head' || token === '--pr' || token === '--model' ||
      token === '--description' || token === '--requirements'
    ) {
      result[token.replace(/^--/, '')] = tokens[i + 1];
      i += 1;
    } else if (result['base'] === undefined) {
      result['base'] = token;
    } else if (result['head'] === undefined) {
      result['head'] = token;
    }
  }
  return result as unknown as SlashArgs;
}

function buildDiffSource(opts: {
  base?: string | undefined;
  head?: string | undefined;
  pr?: string | undefined;
}): CodeReviewDiffSource {
  if (opts.pr !== undefined) {
    return { kind: 'pr', prUrlOrNumber: opts.pr };
  }
  if (opts.base !== undefined || opts.head !== undefined) {
    return {
      kind: 'commits',
      base: opts.base ?? 'HEAD~1',
      head: opts.head ?? 'HEAD',
    };
  }
  return { kind: 'working-tree' };
}

function renderReviewSourceLabel(source: CodeReviewDiffSource): string {
  if (source.kind === 'pr') return `PR ${source.prUrlOrNumber}`;
  if (source.kind === 'commits') return `${source.base}..${source.head}`;
  return 'working tree';
}

/**
 * `/request-code-review` mirrors `/receive-code-review`: it requests a code
 * review for the current changes and streams the result back into the chat.
 * Unlike the skill path, this slash command invokes the review engine directly
 * through the harness so it works even when the `RequestCodeReview` tool is not
 * exposed in the active agent profile.
 */
export async function handleRequestCodeReviewCommand(
  host: SlashCommandHost,
  args: string,
): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const parsed = parseArgs(args);
  const source = buildDiffSource(parsed);
  const sourceLabel = renderReviewSourceLabel(source);

  const controller = new AbortController();
  host.cancelInFlight = () => {
    controller.abort();
  };

  const spinner = host.showProgressSpinner(`Code review on ${sourceLabel}`);

  try {
    const report = await host.harness.requestCodeReview(
      {
        source,
        modelAlias: parsed.model,
        description: parsed.description,
        requirements: parsed.requirements,
      },
      {
        signal: controller.signal,
        onProgress: (progress) => {
          if (progress.stage === 'generating') {
            spinner.updateLabel(`Generating review with ${progress.modelAlias}`);
          } else if (progress.stage === 'preparing') {
            spinner.updateLabel(`Preparing diff for review`);
          } else {
            spinner.updateLabel(`${progress.stage} (${progress.modelAlias})`);
          }
        },
      },
    );

    if (!report.ok) {
      spinner.stop({ ok: false, label: report.note ?? 'Code review failed.' });
      host.showError(report.note ?? 'Code review failed.');
      return;
    }

    spinner.stop({ ok: true, label: `Code review on ${sourceLabel} complete.` });

    const findings = report.findings ?? [];
    const summary = report.summary ?? '';
    const findingSummary = findings.length > 0
      ? `Found ${findings.length} finding${findings.length === 1 ? '' : 's'}.`
      : 'No findings reported.';

    host.sendNormalUserInput(
      `Code review complete on ${sourceLabel} using ${report.reviewerAlias}. ${findingSummary}\n\n${summary}`,
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    spinner.stop({ ok: false, label: `Code review failed: ${message}` });
    host.showError(`Code review failed: ${message}`);
  } finally {
    host.cancelInFlight = undefined;
  }
}
