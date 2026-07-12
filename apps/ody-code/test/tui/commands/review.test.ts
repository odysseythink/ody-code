import { describe, expect, it, vi } from 'vitest';
import {
  handleReviewCommand,
  maybeRestoreModelAfterReceiveReview,
} from '../../../src/tui/commands/review';
import { NO_ACTIVE_SESSION_MESSAGE } from '../../../src/tui/constant/ody-tui';
import type { SlashCommandHost } from '../../../src/tui/commands/dispatch';

function createMockHost(overrides: Partial<SlashCommandHost> = {}): SlashCommandHost {
  return {
    state: {
      appState: {
        model: 'default-model',
        sessionMode: 'normal',
        streamingPhase: 'idle',
      },
      theme: {
        colors: {
          primary: '#fff',
          text: '#fff',
          textMuted: '#888',
          textDim: '#666',
          success: '#0f0',
          error: '#f00',
          warning: '#ff0',
        },
      },
    },
    session: {
      id: 's1',
      setModel: vi.fn().mockResolvedValue(undefined),
      steer: vi.fn().mockResolvedValue(undefined),
      activateSkill: vi.fn(),
    },
    harness: {
      getConfig: vi.fn().mockResolvedValue({
        modeModels: { codeReview: 'review-model', codeReviewReceive: 'receiver-model' },
        defaultModel: 'fallback',
        models: {
          'review-model': { provider: 'test-p', model: 'm1', maxContextSize: 8192 },
          'receiver-model': { provider: 'test-p', model: 'm2', maxContextSize: 8192 },
          fallback: { provider: 'test-p', model: 'm0', maxContextSize: 8192 },
        },
        providers: { 'test-p': { type: 'openai', apiKey: 'sk-test' } },
      }),
      requestCodeReview: vi.fn().mockResolvedValue({
        ok: true,
        reviewerAlias: 'review-model',
        findings: [{ severity: 'important', title: 'test finding', detail: 'detail' }],
        summary: 'one strength',
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

async function flushMicrotasks(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe('handleReviewCommand', () => {
  it('shows a choice picker with request and receive options when called with no arguments', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '');
    expect(host.mountEditorReplacement).toHaveBeenCalledOnce();
    const picker = (host.mountEditorReplacement as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(picker.opts.title).toBe('Code Review');
    expect(picker.opts.options).toHaveLength(2);
    expect(picker.opts.options.map((o: { value: string; label: string }) => o.value)).toEqual([
      'request',
      'receive',
    ]);
    expect(picker.opts.options[0]!.label).toContain('请求 code review');
    expect(picker.opts.options[1]!.label).toContain('处理收到的 review 反馈');
  });

  it('restores editor and requests review when picker selects request', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '');
    const picker = (host.mountEditorReplacement as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    picker.opts.onSelect('request');
    await flushMicrotasks();
    expect(host.restoreEditor).toHaveBeenCalled();
    expect(host.harness.requestCodeReview).toHaveBeenCalledOnce();
    const call = (host.harness.requestCodeReview as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(call.source).toEqual({ kind: 'working-tree' });
  });

  it('restores editor and does nothing when picker is cancelled', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '');
    const picker = (host.mountEditorReplacement as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    picker.opts.onCancel();
    await flushMicrotasks();
    expect(host.restoreEditor).toHaveBeenCalled();
    expect(host.harness.requestCodeReview).not.toHaveBeenCalled();
    expect((host.session as any).setModel).not.toHaveBeenCalled();
  });

  it('skips picker and requests review with pr args', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '--pr 123');
    expect(host.mountEditorReplacement).not.toHaveBeenCalled();
    expect(host.harness.requestCodeReview).toHaveBeenCalledOnce();
    const call = (host.harness.requestCodeReview as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(call.source).toEqual({ kind: 'pr', prUrlOrNumber: '123' });
  });

  it('send normal user input with findings on successful request review', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '--base HEAD~1 --head HEAD');
    expect(host.sendNormalUserInput).toHaveBeenCalledWith(
      expect.stringContaining('Code review complete'),
    );
    expect(host.sendNormalUserInput).toHaveBeenCalledWith(
      expect.stringContaining('Found 1 finding'),
    );
  });

  it('shows error when request review report is not ok', async () => {
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
    await handleReviewCommand(host, '--pr 123');
    expect(host.showError).toHaveBeenCalledWith('Diff too large');
  });

  it('switches model, steers guidance, and sets active state in receive branch', async () => {
    const host = createMockHost();
    await handleReviewCommand(host, '');
    const picker = (host.mountEditorReplacement as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    picker.opts.onSelect('receive');
    await flushMicrotasks();

    expect(host.restoreEditor).toHaveBeenCalled();
    expect(host.setAppState).toHaveBeenCalledWith(
      expect.objectContaining({
        receiveCodeReview: expect.objectContaining({ active: true, reviewModelAlias: 'receiver-model' }),
      }),
    );
    expect((host.session as any).setModel).toHaveBeenCalledWith('receiver-model');
    expect((host.session as any).steer).toHaveBeenCalledWith(
      expect.stringContaining('Verify before implementing'),
    );
    expect((host.session as any).activateSkill).not.toHaveBeenCalled();
    expect(host.showStatus).toHaveBeenCalledWith(
      expect.stringContaining('Switched to receiver-model'),
    );
  });

  it('shows error and does not switch model when receive model resolution fails', async () => {
    const host = createMockHost({
      state: {
        appState: {
          model: '',
          sessionMode: 'normal',
          streamingPhase: 'idle',
        },
        theme: createMockHost().state.theme,
      } as any,
      harness: {
        ...createMockHost().harness,
        getConfig: vi.fn().mockResolvedValue({
          modeModels: {},
          defaultModel: '',
          models: {},
          providers: {},
        }),
      } as any,
    });
    await handleReviewCommand(host, '');
    const picker = (host.mountEditorReplacement as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    picker.opts.onSelect('receive');
    await flushMicrotasks();

    expect(host.showError).toHaveBeenCalled();
    expect((host.session as any).setModel).not.toHaveBeenCalled();
    expect((host.session as any).steer).not.toHaveBeenCalled();
  });

  it('shows error and clears state when setModel fails in receive branch', async () => {
    const host = createMockHost();
    (host.session as any).setModel = vi.fn().mockRejectedValue(new Error('model unavailable'));
    await handleReviewCommand(host, '');
    const picker = (host.mountEditorReplacement as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    picker.opts.onSelect('receive');
    await flushMicrotasks();

    expect(host.showError).toHaveBeenCalledWith(expect.stringContaining('model unavailable'));
    expect(host.setAppState).toHaveBeenLastCalledWith({ receiveCodeReview: undefined });
  });

  it('shows error for both branches when there is no active session', async () => {
    const hostNoArgs = createMockHost({ session: undefined });
    await handleReviewCommand(hostNoArgs, '');
    expect(hostNoArgs.showError).toHaveBeenCalledWith(NO_ACTIVE_SESSION_MESSAGE);
    expect(hostNoArgs.mountEditorReplacement).not.toHaveBeenCalled();

    const hostWithArgs = createMockHost({ session: undefined });
    await handleReviewCommand(hostWithArgs, '--pr 123');
    expect(hostWithArgs.showError).toHaveBeenCalledWith(NO_ACTIVE_SESSION_MESSAGE);
    expect(hostWithArgs.harness.requestCodeReview).not.toHaveBeenCalled();
  });
});

describe('maybeRestoreModelAfterReceiveReview', () => {
  it('restores model and deactivates when active', () => {
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
    expect((host.session as any).setModel).toHaveBeenCalledWith('original');
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
