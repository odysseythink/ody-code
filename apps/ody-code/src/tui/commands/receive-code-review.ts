import { resolveCodeReviewModel } from '@odysseythink/ody-code-sdk';

import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/ody-tui';
import type { SlashCommandHost } from './dispatch';

export async function handleReceiveCodeReviewCommand(
  host: SlashCommandHost,
  _args: string,
): Promise<void> {
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

  // Save original state
  host.setAppState({
    receiveCodeReview: {
      originalModelAlias: currentModel,
      reviewModelAlias,
      active: true,
    },
  });

  // Switch model
  try {
    await session.setModel(reviewModelAlias);
  } catch (error) {
    host.showError(
      `Failed to switch model to ${reviewModelAlias}: ${error instanceof Error ? error.message : String(error)}`,
    );
    host.setAppState({ receiveCodeReview: undefined });
    return;
  }

  // Activate receiving-code-review skill
  try {
    await session.activateSkill('receiving-code-review');
  } catch (error) {
    host.showError(
      `Failed to load receiving-code-review skill: ${error instanceof Error ? error.message : String(error)}`,
    );
  }

  host.showStatus(
    `Switched to ${reviewModelAlias} and loaded receiving-code-review skill. Paste the review feedback and continue.`,
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
