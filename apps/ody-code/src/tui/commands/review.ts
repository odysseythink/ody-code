import { resolveCodeReviewModel } from '@odysseythink/ody-code-sdk';

import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/ody-tui';
import { ChoicePickerComponent, type ChoiceOption } from '../components/dialogs/choice-picker';
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

const REVIEW_OPTIONS: readonly ChoiceOption[] = [
  {
    value: 'request',
    label: '请求 code review',
    description: '用第二个模型审查当前 diff（等价于原 /request-code-review）',
  },
  {
    value: 'receive',
    label: '处理收到的 review 反馈',
    description: '切换 review 模型并注入反馈处理准则，然后粘贴反馈',
  },
];

const RECEIVE_REVIEW_GUIDANCE = `You are now handling externally received code review feedback.

Core principle: Verify before implementing. Ask before assuming. Technical correctness over social comfort.

When receiving feedback:
1. Read all feedback completely before reacting.
2. Restate the technical requirement in your own words; ask if anything is unclear.
3. Verify the claim against the actual codebase (tests, callers, invariants).
4. Evaluate whether the suggestion is technically sound for THIS codebase.
5. Respond with a technical acknowledgment or reasoned pushback, never performative agreement.
6. Implement one item at a time and test each fix individually.

Forbidden responses: "You're absolutely right!", "Great point!", "Excellent feedback!", "Let me implement that now" (before verification), or any gratitude expression.

If a suggestion seems wrong, push back with technical reasoning and reference working code/tests. If you cannot verify something, say so and ask for direction. If feedback conflicts with prior architectural decisions, stop and discuss with your human partner first.`;

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
 * `/review` is the single entry point for code review flows. With arguments it
 * requests a review directly (mirroring the old `/request-code-review`);
 * without arguments it opens a picker to choose between requesting a review
 * and entering receive-feedback mode (the old `/receive-code-review`).
 */
export async function handleReviewCommand(
  host: SlashCommandHost,
  args: string,
): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const trimmed = args.trim();
  if (trimmed.length > 0) {
    await runRequestReview(host, args);
    return;
  }

  host.mountEditorReplacement(
    new ChoicePickerComponent({
      title: 'Code Review',
      options: REVIEW_OPTIONS,
      colors: host.state.theme.colors,
      onSelect: (value) => {
        host.restoreEditor();
        if (value === 'request') {
          void runRequestReview(host, '');
        } else {
          void runReceiveReview(host);
        }
      },
      onCancel: () => {
        host.restoreEditor();
      },
    }),
  );
}

async function runRequestReview(host: SlashCommandHost, args: string): Promise<void> {
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
        onProgress: (progress: any) => {
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

async function runReceiveReview(host: SlashCommandHost): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const config = await host.harness.getConfig({ reload: true });
  const currentModel = host.state.appState.model;

  let reviewModelAlias: string;
  try {
    reviewModelAlias = resolveCodeReviewModel(
      'receive',
      config.modeModels,
      config.defaultModel,
      {
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
  } catch (error) {
    host.showError(
      `Cannot enter receive-code-review mode: ${error instanceof Error ? error.message : String(error)}`,
    );
    return;
  }

  host.setAppState({
    receiveCodeReview: {
      originalModelAlias: currentModel,
      reviewModelAlias,
      active: true,
    },
  });

  try {
    await session.setModel(reviewModelAlias);
  } catch (error) {
    host.showError(
      `Failed to switch model to ${reviewModelAlias}: ${error instanceof Error ? error.message : String(error)}`,
    );
    host.setAppState({ receiveCodeReview: undefined });
    return;
  }

  try {
    await session.steer(`<system-reminder>\n${RECEIVE_REVIEW_GUIDANCE}\n</system-reminder>`);
  } catch (error) {
    host.showError(
      `Failed to inject review-handling guidance: ${error instanceof Error ? error.message : String(error)}`,
    );
  }

  host.showStatus(
    `Switched to ${reviewModelAlias} and loaded review-handling guidance. Paste the review feedback and continue.`,
  );
}

export function maybeRestoreModelAfterReceiveReview(host: SlashCommandHost): void {
  const state = (host.state as any).appState.receiveCodeReview;
  if (state?.active !== true) return;

  const session = host.session;
  if (session !== undefined && state.originalModelAlias.length > 0) {
    void session.setModel(state.originalModelAlias).catch(() => {});
  }

  host.setAppState({
    model: state.originalModelAlias,
    receiveCodeReview: { ...state, active: false },
  });
}
