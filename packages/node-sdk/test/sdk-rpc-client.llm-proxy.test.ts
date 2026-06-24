import { describe, it, expect, vi, beforeEach, type MockedFunction } from 'vitest';

import type { ChatProvider, ProviderConfig, StreamedMessagePart, TokenUsage } from '@odysseythink/kosong';
import { createProvider } from '@odysseythink/kosong';

import { ClientAPI, type ResolvedCoreAPI } from '../src/rpc';
import type { SDKRpcClient } from '../src/rpc';
import type { ChatStreamRequest } from '@odysseythink/agent-core';

vi.mock('@odysseythink/kosong', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@odysseythink/kosong')>();
  return {
    ...mod,
    createProvider: vi.fn(),
  };
});

function createFakeProvider(parts: StreamedMessagePart[]): ChatProvider {
  return {
    name: 'fake',
    modelName: 'fake-model',
    thinkingEffort: null,
    async generate() {
      const usage: TokenUsage = {
        inputOther: 1,
        output: 1,
        inputCacheRead: 0,
        inputCacheCreation: 0,
      };
      return {
        [Symbol.asyncIterator]: async function* () {
          for (const part of parts) {
            yield part;
          }
        },
        id: null,
        usage,
        finishReason: 'completed',
        rawFinishReason: 'stop',
      } as unknown as ReturnType<ChatProvider['generate']> extends Promise<infer T> ? T : never;
    },
    withThinking: (effort) => ({ ...createFakeProvider([]), thinkingEffort: effort }),
    getCapability: () => ({
      thinking: false,
      image_in: false,
      video_in: false,
      audio_in: false,
      tool_use: true,
      max_context_tokens: 0,
      max_output_tokens: 0,
    }),
  };
}

function waitForStreamTermination(rpc: ResolvedCoreAPI): Promise<void> {
  return new Promise<void>((resolve) => {
    (rpc.chatStreamEnd as MockedFunction<ResolvedCoreAPI['chatStreamEnd']>).mockImplementation(() => {
      resolve();
      return Promise.resolve();
    });
    (rpc.chatStreamError as MockedFunction<ResolvedCoreAPI['chatStreamError']>).mockImplementation(() => {
      resolve();
      return Promise.resolve();
    });
  });
}

describe('ClientAPI LLM proxy', () => {
  const mockedCreateProvider = createProvider as MockedFunction<typeof createProvider>;

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('streams deltas and sends an end payload', async () => {
    const fakeProvider = createFakeProvider([
      { type: 'text', text: 'hello ' },
      { type: 'text', text: 'world' },
      { type: 'think', think: '<think>' },
    ]);
    mockedCreateProvider.mockReturnValue(fakeProvider);

    const rpc = {
      chatStreamDelta: vi.fn().mockResolvedValue(undefined),
      chatStreamEnd: vi.fn().mockResolvedValue(undefined),
      chatStreamError: vi.fn().mockResolvedValue(undefined),
    } as unknown as ResolvedCoreAPI;

    const client = {} as unknown as SDKRpcClient;
    const api = new ClientAPI(client, async () => rpc);

    const request: ChatStreamRequest = {
      modelName: 'fake-model',
      systemPrompt: 's',
      messages: [{ role: 'user', content: [{ type: 'text', text: 'hi' }] }],
      tools: [],
      provider: { type: 'openai', model: 'fake-model', apiKey: 'key' } as ProviderConfig,
    };

    const { streamId } = await api.chatStreamInit({ request, sessionId: 's1', agentId: 'a1' });
    expect(streamId).toBeDefined();

    await waitForStreamTermination(rpc);

    expect(mockedCreateProvider).toHaveBeenCalledWith(request.provider);
    // KosongLLM forwards raw per-part deltas before generate merges them.
    expect(rpc.chatStreamDelta).toHaveBeenCalledTimes(3);
    expect(rpc.chatStreamDelta).toHaveBeenNthCalledWith(1, {
      streamId,
      delta: { type: 'text', text: 'hello ' },
    });
    expect(rpc.chatStreamDelta).toHaveBeenNthCalledWith(2, {
      streamId,
      delta: { type: 'text', text: 'world' },
    });
    expect(rpc.chatStreamDelta).toHaveBeenNthCalledWith(3, {
      streamId,
      delta: { type: 'think', think: '<think>' },
    });

    expect(rpc.chatStreamEnd).toHaveBeenCalledTimes(1);
    expect(rpc.chatStreamEnd).toHaveBeenCalledWith(
      expect.objectContaining({
        streamId,
        result: expect.objectContaining({
          providerFinishReason: 'completed',
          rawFinishReason: 'stop',
          usage: { inputOther: 1, output: 1, inputCacheRead: 0, inputCacheCreation: 0 },
        }),
      }),
    );
    expect(rpc.chatStreamError).not.toHaveBeenCalled();
  });

  it('cancels an active stream', async () => {
    const abortSignals: AbortSignal[] = [];
    const fakeProvider = createFakeProvider([]);
    // Hang after the first delta so the stream stays active until cancelled.
    fakeProvider.generate = async (...args) => {
      abortSignals.push(args[3]?.signal as AbortSignal);
      return {
        [Symbol.asyncIterator]: async function* () {
          yield { type: 'text', text: 'x' };
          await new Promise(() => {
            // Intentionally hang until the signal aborts.
          });
        },
        id: null,
        usage: { inputOther: 0, output: 0, inputCacheRead: 0, inputCacheCreation: 0 },
        finishReason: null,
        rawFinishReason: null,
      } as unknown as ReturnType<ChatProvider['generate']> extends Promise<infer T> ? T : never;
    };
    mockedCreateProvider.mockReturnValue(fakeProvider);

    const rpc = {
      chatStreamDelta: vi.fn().mockResolvedValue(undefined),
      chatStreamEnd: vi.fn().mockResolvedValue(undefined),
      chatStreamError: vi.fn().mockResolvedValue(undefined),
    } as unknown as ResolvedCoreAPI;

    const client = {} as unknown as SDKRpcClient;
    const api = new ClientAPI(client, async () => rpc);

    const request: ChatStreamRequest = {
      modelName: 'fake-model',
      systemPrompt: 's',
      messages: [{ role: 'user', content: [{ type: 'text', text: 'hi' }] }],
      tools: [],
      provider: { type: 'openai', model: 'fake-model', apiKey: 'key' } as ProviderConfig,
    };

    const { streamId } = await api.chatStreamInit({ request, sessionId: 's1', agentId: 'a1' });
    // Wait until the generate call has captured the signal.
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(abortSignals.length).toBe(1);
    expect(abortSignals[0]?.aborted).toBe(false);

    api.chatStreamCancel({ streamId, sessionId: 's1', agentId: 'a1' });

    expect(abortSignals[0]?.aborted).toBe(true);
  });
});
