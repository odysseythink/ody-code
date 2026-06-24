import { describe, expect, it, vi } from 'vitest';

import {
  RemoteKosongLLM,
  remoteLLMStreamRegistry,
} from '../../../src/agent/turn/remote-kosong-llm';
import { ErrorCodes, OdyError } from '../../../src/errors';
import { toOdyErrorPayload } from '@odysseythink/agent-core-shared';
import type { LLMChatParams } from '../../../src/loop/llm';
import type { SDKAgentRPC } from '../../../src/rpc';

const fakeProvider = { type: 'openai', model: 'fake-model', apiKey: 'fake-key' } as const;

describe('RemoteKosongLLM', () => {
  it('forwards deltas and resolves with the streamed result', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's1' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const onTextDelta = vi.fn();
    const onThinkDelta = vi.fn();
    const params: LLMChatParams = {
      messages: [],
      tools: [],
      signal: new AbortController().signal,
      onTextDelta,
      onThinkDelta,
    };

    const chatPromise = llm.chat(params);

    // Yield to the microtask queue so the await inside chat() settles
    // and the stream handler is registered before we dispatch.
    await new Promise((resolve) => setTimeout(resolve, 0));

    remoteLLMStreamRegistry.dispatchDelta({ streamId: 's1', delta: { type: 'text', text: 'hello' } });
    remoteLLMStreamRegistry.dispatchDelta({ streamId: 's1', delta: { type: 'think', think: '<think>' } });
    remoteLLMStreamRegistry.dispatchEnd({
      streamId: 's1',
      result: { toolCalls: [], usage: { inputOther: 0, output: 2, inputCacheRead: 0, inputCacheCreation: 0 } } as any,
    });

    const response = await chatPromise;
    expect(response.usage.output).toBe(2);
    expect(onTextDelta).toHaveBeenCalledWith('hello');
    expect(onThinkDelta).toHaveBeenCalledWith('<think>');
    expect(sdk.chatStreamCancel).not.toHaveBeenCalled();
  });

  it('cancels the stream when the signal aborts', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's2' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const controller = new AbortController();
    const chatPromise = llm.chat({ messages: [], tools: [], signal: controller.signal });

    await new Promise((resolve) => setTimeout(resolve, 0));
    controller.abort();

    await expect(chatPromise).rejects.toThrow();
    expect(sdk.chatStreamCancel).toHaveBeenCalledWith({ streamId: 's2' });
  });

  it('rejects when the registry reports an error', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's3' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const chatPromise = llm.chat({ messages: [], tools: [], signal: new AbortController().signal });

    await new Promise((resolve) => setTimeout(resolve, 0));

    remoteLLMStreamRegistry.dispatchError({
      streamId: 's3',
      error: toOdyErrorPayload(new OdyError(ErrorCodes.PROVIDER_API_ERROR, 'boom')),
    });

    await expect(chatPromise).rejects.toMatchObject({ code: 'provider.api_error' });
  });
});
