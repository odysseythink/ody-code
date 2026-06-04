import { describe, expect, it, vi } from 'vitest';
import {
  handleLoginCommand,
  handleLogoutCommand,
} from '../../../src/tui/commands/auth';
import {
  getProviderLoginDefinition,
  isSupportedProviderLoginType,
} from '@odysseythink/kimi-code-oauth';

describe('handleLoginCommand provider-type argument', () => {
  it('shows error for unsupported provider type', async () => {
    const showError = vi.fn();
    const host = makeMockHost({ showError });

    await handleLoginCommand(host, 'xyz');

    expect(showError).toHaveBeenCalledWith(
      expect.stringContaining('Unsupported provider type: "xyz"'),
    );
  });

  it('delegates to legacy flow when no arg', async () => {
    const showError = vi.fn();
    const mountEditorReplacement = vi.fn();
    const host = makeMockHost({ showError, mountEditorReplacement });

    // Legacy flow shows platform selector; with no mocked dialog it hangs.
    // Start the command but don't await it; just verify no immediate error.
    const promise = handleLoginCommand(host);
    expect(showError).not.toHaveBeenCalled();
    expect(mountEditorReplacement).toHaveBeenCalled();
  });
});

describe('handleLogoutCommand provider-type argument', () => {
  it('falls back to all providers when filter matches nothing', async () => {
    const showStatus = vi.fn();
    const getConfig = vi.fn(async () => ({
      providers: {
        openai_main: { type: 'openai', baseUrl: 'https://api.openai.com/v1' },
      },
      models: {},
    }));
    const mountEditorReplacement = vi.fn((component: any) => {
      // Simulate user cancel to unblock the picker promise.
      if (typeof component.handleInput === 'function') {
        component.handleInput('\u001B');
      }
    });
    const host = makeMockHost({
      showStatus,
      mountEditorReplacement,
      harness: {
        auth: { status: vi.fn(async () => ({ providers: [] })), login: vi.fn(), logout: vi.fn() },
        getConfig,
        setConfig: vi.fn(),
        removeProvider: vi.fn(),
        track: vi.fn(),
      },
    });

    await handleLogoutCommand(host, 'deepseek');

    expect(showStatus).not.toHaveBeenCalledWith(expect.stringContaining('error'));
    expect(mountEditorReplacement).toHaveBeenCalled();
  });
});

function makeMockHost(partial: Record<string, unknown> = {}): Parameters<typeof handleLoginCommand>[0] {
  return {
    state: {
      appState: { model: '', availableModels: {}, availableProviders: {} },
      theme: { colors: {} as any },
    },
    session: undefined,
    harness: {
      auth: { status: vi.fn(async () => ({ providers: [] })), login: vi.fn(), logout: vi.fn() },
      getConfig: vi.fn(async () => ({ providers: {}, models: {} })),
      setConfig: vi.fn(),
      removeProvider: vi.fn(),
      track: vi.fn(),
    },
    cancelInFlight: undefined,
    deferUserMessages: false,
    setAppState: vi.fn(),
    resetLivePane: vi.fn(),
    showError: vi.fn(),
    showStatus: vi.fn(),
    showNotice: vi.fn(),
    track: vi.fn(),
    mountEditorReplacement: vi.fn(),
    restoreEditor: vi.fn(),
    restoreInputText: vi.fn(),
    requireSession: vi.fn(),
    switchToSession: vi.fn(),
    beginSessionRequest: vi.fn(),
    failSessionRequest: vi.fn(),
    sendQueuedMessage: vi.fn(),
    showLoginProgressSpinner: vi.fn(() => ({ stop: vi.fn() })),
    showLoginAuthorizationPrompt: vi.fn(() => ({ stop: vi.fn() })),
    showProgressSpinner: vi.fn(() => ({ stop: vi.fn() })),
    applyTheme: vi.fn(),
    refreshTerminalThemeTracking: vi.fn(),
    stop: vi.fn(),
    showHelpPanel: vi.fn(),
    createNewSession: vi.fn(),
    showSessionPicker: vi.fn(),
    sendNormalUserInput: vi.fn(),
    sendSkillActivation: vi.fn(),
    skillCommandMap: new Map(),
    streamingUI: {} as any,
    tasksBrowserController: {} as any,
    authFlow: {
      refreshConfigAfterLogin: vi.fn(),
      refreshConfigAfterLogout: vi.fn(),
      clearActiveSessionAfterLogout: vi.fn(),
      refreshAvailableModels: vi.fn(),
    } as any,
    ...partial,
  } as unknown as Parameters<typeof handleLoginCommand>[0];
}
