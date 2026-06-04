import { describe, expect, it, vi } from 'vitest';
import {
  handleLoginCommand,
  handleLogoutCommand,
} from '../../../src/tui/commands/auth';
import {
  getProviderLoginDefinition,
  isSupportedProviderLoginType,
} from '@odysseythink/kimi-code-oauth';

vi.mock('../../../src/tui/commands/prompts', async (importOriginal) => {
  const mod = await importOriginal<typeof import('../../../src/tui/commands/prompts')>();
  return {
    ...mod,
    promptCustomProviderName: vi.fn(async () => 'deepseek_1'),
    promptApiKey: vi.fn(async () => 'sk-test'),
    promptCustomBaseUrl: vi.fn(async () => 'https://api.deepseek.com/v1'),
    promptModelSelectionForProviderLogin: vi.fn(async () => ({
      model: { id: 'deepseek-chat', contextLength: 64000, supportsToolUse: true, supportsReasoning: false, supportsImageIn: false, supportsVideoIn: false },
      thinking: false,
    })),
  };
});

vi.mock('@odysseythink/kimi-code-oauth', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@odysseythink/kimi-code-oauth')>();
  return {
    ...mod,
    fetchProviderModels: vi.fn(async () => [
      { id: 'deepseek-chat', contextLength: 64000, supportsToolUse: true, supportsReasoning: false, supportsImageIn: false, supportsVideoIn: false },
    ]),
  };
});

describe('handleLoginCommand provider-type argument', () => {
  it('shows error for unsupported provider type', async () => {
    const showError = vi.fn();
    const host = makeMockHost({ showError });

    await handleLoginCommand(host, 'xyz');

    expect(showError).toHaveBeenCalledWith(
      expect.stringContaining('Unsupported provider type: "xyz"'),
    );
  });

  it('shows all supported providers when no arg', async () => {
    const showError = vi.fn();
    let capturedOptions: any[] | undefined;
    const mountEditorReplacement = vi.fn((component: any) => {
      if (typeof component.handleInput === 'function') {
        capturedOptions = component.opts?.options;
        component.handleInput('\u001B');
      }
    });
    const host = makeMockHost({ showError, mountEditorReplacement });

    await handleLoginCommand(host);

    expect(showError).not.toHaveBeenCalled();
    expect(mountEditorReplacement).toHaveBeenCalled();
    expect(capturedOptions).toBeDefined();
    const labels = capturedOptions!.map((o: any) => o.label);
    expect(labels).toContain('Kimi Code (OAuth)');
    expect(labels).toContain('DeepSeek');
    expect(labels).toContain('OpenAI');
    expect(labels).toContain('Kimi (Open Platform)');
    expect(labels).toContain('OpenAI (Responses API)');
    expect(labels).toContain('Anthropic');
  });

  it('saves deepseek_1 provider via setConfig', async () => {
    const setConfig = vi.fn(async (patch: any) => ({
      providers: patch.providers ?? {},
      models: patch.models ?? {},
      defaultModel: patch.defaultModel,
      defaultThinking: patch.defaultThinking,
    }));
    const getConfig = vi.fn(async () => ({
      providers: {},
      models: {},
    }));
    const host = makeMockHost({
      harness: {
        auth: { status: vi.fn(async () => ({ providers: [] })), login: vi.fn(), logout: vi.fn() },
        getConfig,
        setConfig,
        removeProvider: vi.fn(),
        track: vi.fn(),
      },
    });

    await handleLoginCommand(host, 'deepseek');

    expect(setConfig).toHaveBeenCalled();
    const patch = setConfig.mock.calls[0][0];
    expect(patch.providers?.deepseek_1).toMatchObject({
      type: 'deepseek',
      baseUrl: 'https://api.deepseek.com/v1',
      apiKey: 'sk-test',
    });
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

  it('shows deepseek_1 provider with correct label and description', async () => {
    let capturedOptions: any[] | undefined;
    const getConfig = vi.fn(async () => ({
      providers: {
        deepseek_1: { type: 'deepseek', baseUrl: 'https://api.deepseek.com/v1' },
      },
      models: {},
    }));
    const mountEditorReplacement = vi.fn((component: any) => {
      if (typeof component.handleInput === 'function') {
        capturedOptions = component.opts?.options;
        component.handleInput('\u001B');
      }
    });
    const host = makeMockHost({
      mountEditorReplacement,
      harness: {
        auth: { status: vi.fn(async () => ({ providers: [] })), login: vi.fn(), logout: vi.fn() },
        getConfig,
        setConfig: vi.fn(),
        removeProvider: vi.fn(),
        track: vi.fn(),
      },
    });

    await handleLogoutCommand(host);

    expect(mountEditorReplacement).toHaveBeenCalled();
    expect(capturedOptions).toBeDefined();
    expect(capturedOptions!.length).toBe(1);
    expect(capturedOptions![0]).toMatchObject({
      value: 'deepseek_1',
      label: 'deepseek_1',
      description: 'deepseek · https://api.deepseek.com/v1',
    });
  });

  it('filters by provider type when arg is given', async () => {
    let capturedOptions: any[] | undefined;
    const getConfig = vi.fn(async () => ({
      providers: {
        deepseek_1: { type: 'deepseek', baseUrl: 'https://api.deepseek.com/v1' },
        openai_main: { type: 'openai', baseUrl: 'https://api.openai.com/v1' },
      },
      models: {},
    }));
    const mountEditorReplacement = vi.fn((component: any) => {
      if (typeof component.handleInput === 'function') {
        capturedOptions = component.opts?.options;
        component.handleInput('\u001B');
      }
    });
    const host = makeMockHost({
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

    expect(capturedOptions).toBeDefined();
    expect(capturedOptions!.length).toBe(1);
    expect(capturedOptions![0]).toMatchObject({
      value: 'deepseek_1',
      label: 'deepseek_1',
    });
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
