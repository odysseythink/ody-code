import type { SlashCommandHost } from './dispatch';
import { LLM_NOT_SET_MESSAGE } from '../constant/ody-tui';
import { isAbortError } from '../utils/errors';

export async function handleSetupCommand(host: SlashCommandHost): Promise<void> {
  const session = host.session;
  if (host.state.appState.model.trim().length === 0 || session === undefined) {
    host.showError(LLM_NOT_SET_MESSAGE);
    return;
  }

  host.deferUserMessages = true;
  host.beginSessionRequest();
  try {
    await session.setup();
    host.track('setup_script_manual');
    host.showStatus('Setup script completed. Check the agent response for details.');
    host.streamingUI.finalizeTurn((item) => {
      host.sendQueuedMessage(session, item);
    });
  } catch (error) {
    if (isAbortError(error)) {
      host.setAppState({ streamingPhase: 'idle' });
      host.resetLivePane();
      return;
    }
    const msg = error instanceof Error ? error.message : String(error);
    host.failSessionRequest(`Setup script failed: ${msg}`);
  } finally {
    host.deferUserMessages = false;
  }
}
