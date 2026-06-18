import { renderCodeReviewReportToMarkdown } from '@odysseythink/ody-code-sdk';
import { resolveCodeReviewModel } from '@odysseythink/ody-code-sdk';

import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/ody-tui';
import type { SlashCommandHost } from './dispatch';

interface SlashArgs {
  readonly base?: string;
  readonly head?: string;
  readonly pr?: string;
  readonly model?: string;
  readonly description?: string;
  readonly requirements?: string;
  readonly deep?: boolean;
  readonly focus?: 'correctness' | 'simplicity';
  readonly scope?: 'diff' | 'repo';
}

function parseArgs(args: string): SlashArgs {
  const tokens = args.trim().split(/\s+/).filter((t) => t.length > 0);
  const result: Record<string, unknown> = {};
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;
    if (token === '--base' || token === '--head' || token === '--pr' || token === '--model' ||
        token === '--description' || token === '--requirements' || token === '--focus' || token === '--scope') {
      result[camelFromFlag(token)] = tokens[i + 1];
      i += 1;
    } else if (token === '--deep') {
      result['deep'] = true;
    } else {
      if (result['base'] === undefined) {
        result['base'] = token;
      } else if (result['head'] === undefined) {
        result['head'] = token;
      }
    }
  }
  return result as unknown as SlashArgs;
}

function camelFromFlag(flag: string): string {
  return flag.replace(/^--/, '').replace(/-([a-z])/g, (_, ch: string) => ch.toUpperCase());
}

function buildDiffSource(parsed: SlashArgs) {
  if (parsed.pr !== undefined) {
    return { kind: 'pr' as const, prUrlOrNumber: parsed.pr };
  }
  if (parsed.base !== undefined || parsed.head !== undefined) {
    return {
      kind: 'commits' as const,
      base: parsed.base ?? 'HEAD~1',
      head: parsed.head ?? 'HEAD',
    };
  }
  return { kind: 'working-tree' as const };
}

const STAGE_MAP: Record<string, string> = {
  'preparing': 'Preparing',
  'fetching-diff': 'Fetching diff',
  'audit-scanning': 'Scanning repo',
  'deep-review': 'Deep review in progress',
  'generating': 'Generating review',
  'completed': 'Complete',
  'failed': 'Failed',
};

function formatReviewProgressLabel(
  progress: { stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } },
  elapsedSeconds: number,
): string {
  const stageText = STAGE_MAP[progress.stage] ?? progress.stage;
  let base = `Code review on ${progress.modelAlias} — ${stageText}`;
  if (progress.detail) {
    const truncated = progress.detail.length > 40 ? progress.detail.slice(0, 37) + '…' : progress.detail;
    base += ` (${truncated})`;
  }
  if (progress.meta?.estimatedTokens !== undefined) {
    base += ` · ~${progress.meta.estimatedTokens} tokens`;
  }
  if (progress.meta?.filePath !== undefined) {
    const basename = progress.meta.filePath.split('/').pop() ?? progress.meta.filePath;
    base += ` · ${basename}`;
  }
  return `${base} (${elapsedSeconds}s)`;
}

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
  const config = await host.harness.getConfig();
  const currentModel = host.state.appState.model;

  const resolvedModel = resolveCodeReviewModel(
    'request',
    config.modeModels,
    config.defaultModel,
    {
      explicit: parsed.model,
      sessionModel: currentModel.length > 0 ? currentModel : undefined,
    },
    (alias: string) => {
      const models = config.models ?? {};
      const providers = config.providers;
      const modelEntry = models[alias];
      if (modelEntry === undefined) return false;
      return providers[modelEntry.provider] !== undefined;
    },
  );

  const source = buildDiffSource(parsed);

  // ── Progress spinner ──
  const controller = new AbortController();
  const cancel = () => controller.abort();
  host.cancelInFlight = cancel;

  let currentProgress: { stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } } = { stage: 'preparing', modelAlias: resolvedModel };
  const spinner = host.showProgressSpinner(formatReviewProgressLabel(currentProgress, 0));
  let elapsed = 0;
  const timer = setInterval(() => {
    elapsed += 1;
    spinner.updateLabel(formatReviewProgressLabel(currentProgress, elapsed));
  }, 1000);

  try {
    const report = await host.harness.requestCodeReview({
      source,
      modelAlias: resolvedModel,
      description: parsed.description,
      requirements: parsed.requirements,
      deep: parsed.deep,
      focus: parsed.focus,
      scope: parsed.scope,
    }, {
      signal: controller.signal,
      onProgress: (p) => {
        currentProgress = p;
        spinner.updateLabel(formatReviewProgressLabel(p, elapsed));
      },
    });

    if (!report.ok) {
      spinner.stop({ ok: false, label: report.note ?? 'Code review failed.' });
      host.showError(report.note ?? 'Code review failed.');
      return;
    }

    const markdown = renderCodeReviewReportToMarkdown(report);
    spinner.stop({ ok: true, label: `Code review complete (${report.reviewerAlias}).` });
    host.sendNormalUserInput(
      `Code review complete (${report.reviewerAlias}). Findings:\n\n${markdown}\n\nPlease act on the findings.`,
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    spinner.stop({ ok: false, label: `Code review failed: ${message}` });
    host.showError(`Code review failed: ${message}`);
  } finally {
    clearInterval(timer);
    if (host.cancelInFlight === cancel) {
      host.cancelInFlight = undefined;
    }
  }
}
