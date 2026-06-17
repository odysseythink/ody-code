import { describe, expect, it, vi } from 'vitest';
import {
  handleReceiveCodeReviewCommand,
  maybeRestoreModelAfterReceiveReview,
} from '../../../src/tui/commands/receive-code-review';
import type { SlashCommandHost } from '../../../src/tui/commands/dispatch';

function createMockHost(overrides: Partial<SlashCommandHost> = {}): SlashCommandHost {
  return {
    state: {
      appState: {
        model: 'default-model',
        sessionMode: 'normal',
        streamingPhase: 'idle',
      },
    },
    session: {
      id: 's1',
      setModel: vi.fn().mockResolvedValue(undefined),
      activateSkill: vi.fn(),
    },
    harness: {
      getConfig: vi.fn().mockResolvedValue({
        modeModels: { codeReviewReceive: 'receiver-model' },
        defaultModel: 'fallback',
        models: { 'receiver-model': { provider: 'test-p', model: 'm1', maxContextSize: 8192 } },
        providers: { 'test-p': { type: 'openai', apiKey: 'sk-test' } },
      }),
    },
    showStatus: vi.fn(),
    showError: vi.fn(),
    setAppState: vi.fn(),
    sendNormalUserInput: vi.fn(),
    requireSession: vi.fn(function (this: SlashCommandHost) { return this.session; }),
    cancelInFlight: undefined,
    deferUserMessages: false,
    ...overrides,
  } as unknown as SlashCommandHost;
}

describe('handleReceiveCodeReviewCommand', () => {
  it('switches model and activates skill', async () => {
    const host = createMockHost();
    await handleReceiveCodeReviewCommand(host, '');
    expect(host.setAppState).toHaveBeenCalledWith(
      expect.objectContaining({
        receiveCodeReview: expect.objectContaining({ active: true }),
      }),
    );
    const setAppStateCall = (host.setAppState as ReturnType<typeof vi.fn>).mock.calls[0]?.[0];
    expect(setAppStateCall?.receiveCodeReview?.reviewModelAlias).toBe('receiver-model');
    expect((host.session as any).setModel).toHaveBeenCalledWith('receiver-model');
    expect((host.session as any).activateSkill).toHaveBeenCalledWith('receiving-code-review');
  });

  it('shows error when no active session', async () => {
    const host = createMockHost({ session: undefined });
    (host.requireSession as ReturnType<typeof vi.fn>).mockImplementation(() => undefined);
    await handleReceiveCodeReviewCommand(host, '');
    expect(host.showError).toHaveBeenCalled();
  });
});

describe('maybeRestoreModelAfterReceiveReview', () => {
  it('restores model when active', () => {
    const host = createMockHost({
      state: {
        appState: {
          model: 'receiver-model',
          sessionMode: 'normal' as const,
          streamingPhase: 'idle' as const,
          receiveCodeReview: {
            originalModelAlias: 'original',
            reviewModelAlias: 'receiver-model',
            active: true,
          },
        },
      } as any,
    });
    maybeRestoreModelAfterReceiveReview(host);
    expect(host.setAppState).toHaveBeenCalledWith(
      expect.objectContaining({
        model: 'original',
        receiveCodeReview: expect.objectContaining({ active: false }),
      }),
    );
  });

  it('no-ops when not active', () => {
    const host = createMockHost();
    maybeRestoreModelAfterReceiveReview(host);
    expect(host.setAppState).not.toHaveBeenCalled();
  });
});
