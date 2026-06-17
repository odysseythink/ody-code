import { describe, expect, it, vi } from 'vitest';
import { handleRequestCodeReviewCommand } from '../../../src/tui/commands/request-code-review';
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
        modeModels: { codeReview: 'review-model' },
        defaultModel: 'fallback',
        models: { 'review-model': { provider: 'test-p', model: 'm1', maxContextSize: 8192 } },
        providers: { 'test-p': { type: 'openai', apiKey: 'sk-test' } },
      }),
      requestCodeReview: vi.fn().mockResolvedValue({
        ok: true,
        reviewerAlias: 'review-model',
        findings: [
          { severity: 'important', title: 'test finding', detail: 'detail' },
        ],
        summary: 'one strength',
      }),
    },
    showStatus: vi.fn(),
    showError: vi.fn(),
    sendNormalUserInput: vi.fn(),
    requireSession: vi.fn(function (this: SlashCommandHost) { return this.session; }),
    cancelInFlight: undefined,
    deferUserMessages: false,
    ...overrides,
  } as unknown as SlashCommandHost;
}

describe('handleRequestCodeReviewCommand', () => {
  it('shows error when no active session', async () => {
    const host = createMockHost({ session: undefined });
    (host.requireSession as ReturnType<typeof vi.fn>).mockImplementation(() => undefined);
    await handleRequestCodeReviewCommand(host, '');
    expect(host.showError).toHaveBeenCalled();
  });

  it('calls harness.requestCodeReview and sends result to chat', async () => {
    const host = createMockHost();
    await handleRequestCodeReviewCommand(host, '--base HEAD~1 --head HEAD');
    expect(host.harness.requestCodeReview).toHaveBeenCalledOnce();
    expect(host.sendNormalUserInput).toHaveBeenCalledWith(
      expect.stringContaining('Code review complete'),
    );
  });

  it('shows error when report is not ok', async () => {
    const host = createMockHost({
      harness: {
        ...createMockHost().harness,
        requestCodeReview: vi.fn().mockResolvedValue({
          ok: false,
          reviewerAlias: 'x',
          findings: [],
          note: 'Diff too large',
        }),
      } as any,
    });
    await handleRequestCodeReviewCommand(host, '');
    expect(host.showError).toHaveBeenCalledWith('Diff too large');
  });
});
