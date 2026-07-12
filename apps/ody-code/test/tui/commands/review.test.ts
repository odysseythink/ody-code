import { describe, expect, it, vi } from 'vitest';
import { handleReviewCommand } from '../../../src/tui/commands/review';
import type { SlashCommandHost } from '../../../src/tui/commands/dispatch';

function createMockHost(overrides: Partial<SlashCommandHost> = {}): SlashCommandHost {
  return {
    state: {
      appState: {
        model: 'default-model',
        sessionMode: 'normal',
        streamingPhase: 'idle',
      },
      theme: { colors: { primary: '#fff', text: '#fff', textMuted: '#888', textDim: '#666', success: '#0f0', error: '#f00', warning: '#ff0' } },
    },
    session: {
      id: 's1',
      setModel: vi.fn().mockResolvedValue(undefined),
      steer: vi.fn().mockResolvedValue(undefined),
    },
    harness: {
      getConfig: vi.fn().mockResolvedValue({
        modeModels: { codeReview: 'review-model', codeReviewReceive: 'receiver-model' },
        defaultModel: 'fallback',
        models: {
          'review-model': { provider: 'test-p', model: 'm1', maxContextSize: 8192 },
          'receiver-model': { provider: 'test-p', model: 'm2', maxContextSize: 8192 },
        },
        providers: { 'test-p': { type: 'openai', apiKey: 'sk-test' } },
      }),
      requestCodeReview: vi.fn().mockResolvedValue({
        ok: true,
        reviewerAlias: 'review-model',
        findings: [],
        summary: 'ok',
      }),
    },
    showStatus: vi.fn(),
    showError: vi.fn(),
    showNotice: vi.fn(),
    setAppState: vi.fn(),
    sendNormalUserInput: vi.fn(),
    mountEditorReplacement: vi.fn(),
    restoreEditor: vi.fn(),
    requireSession: vi.fn(function (this: SlashCommandHost) { return this.session; }),
    cancelInFlight: undefined,
    showProgressSpinner: vi.fn().mockReturnValue({
      updateLabel: vi.fn(),
      stop: vi.fn(),
    }),
    deferUserMessages: false,
    ...overrides,
  } as unknown as SlashCommandHost;
}

describe('handleReviewCommand', () => {
  it('shows a choice picker when called with no arguments', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '');
    expect(host.mountEditorReplacement).toHaveBeenCalledOnce();
    const picker = (host.mountEditorReplacement as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(picker.opts.title).toBe('Code Review');
    expect(picker.opts.options).toHaveLength(2);
    expect(picker.opts.options.map((o: { value: string }) => o.value)).toEqual(['request', 'receive']);
  });

  it('skips the picker and requests review when given arguments', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '--pr 123');
    expect(host.mountEditorReplacement).not.toHaveBeenCalled();
    expect(host.harness.requestCodeReview).toHaveBeenCalledOnce();
    const call = (host.harness.requestCodeReview as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(call.source).toEqual({ kind: 'pr', prUrlOrNumber: '123' });
  });

  it('shows error when there is no active session', async () => {
    const host = createMockHost({ session: undefined });
    await handleReviewCommand(host, '');
    expect(host.showError).toHaveBeenCalled();
    expect(host.mountEditorReplacement).not.toHaveBeenCalled();
  });
});
