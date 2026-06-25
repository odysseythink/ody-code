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

function capturedStreamId(sdk: SDKAgentRPC): string {
  return (sdk.chatStreamInit as unknown as ReturnType<typeof vi.fn>).mock.calls[0]![0].streamId as string;
}

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
    const streamId = capturedStreamId(sdk);

    remoteLLMStreamRegistry.dispatchDelta({ streamId, delta: { type: 'text', text: 'hello' } });
    remoteLLMStreamRegistry.dispatchDelta({ streamId, delta: { type: 'think', think: '<think>' } });
    remoteLLMStreamRegistry.dispatchEnd({
      streamId,
      result: { toolCalls: [], usage: { inputOther: 0, output: 2, inputCacheRead: 0, inputCacheCreation: 0 } } as any,
    });

    const response = await chatPromise;
    expect(response.usage.output).toBe(2);
    expect(onTextDelta).toHaveBeenCalledWith('hello');
    expect(onThinkDelta).toHaveBeenCalledWith('<think>');
    expect(sdk.chatStreamCancel).not.toHaveBeenCalled();
    expect(sdk.chatStreamInit).toHaveBeenCalledWith(
      expect.objectContaining({
        streamId: expect.stringMatching(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i),
        request: expect.objectContaining({
          modelName: 'm',
          systemPrompt: 's',
          tools: [],
        }),
      }),
    );
  });

  it('forwards tool_call_part deltas to onToolCallDelta', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's1-tool' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const onToolCallDelta = vi.fn();
    const chatPromise = llm.chat({
      messages: [],
      tools: [],
      signal: new AbortController().signal,
      onToolCallDelta,
    });

    await new Promise((resolve) => setTimeout(resolve, 0));
    const streamId = capturedStreamId(sdk);

    remoteLLMStreamRegistry.dispatchDelta({
      streamId,
      delta: { type: 'tool_call_part', toolCallId: 'tc1', name: 'fn', argumentsPart: '{"a":1}' },
    });
    remoteLLMStreamRegistry.dispatchEnd({
      streamId,
      result: { toolCalls: [], usage: { inputOther: 0, output: 0, inputCacheRead: 0, inputCacheCreation: 0 } } as any,
    });

    await chatPromise;
    expect(onToolCallDelta).toHaveBeenCalledWith({
      toolCallId: 'tc1',
      name: 'fn',
      argumentsPart: '{"a":1}',
    });
  });

  it('rejects immediately when the signal is already aborted', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's0' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const controller = new AbortController();
    controller.abort();

    await expect(llm.chat({ messages: [], tools: [], signal: controller.signal })).rejects.toThrow();
    expect(sdk.chatStreamInit).not.toHaveBeenCalled();
  });

  it('rejects when chatStreamInit fails', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => {
        throw new OdyError(ErrorCodes.PROVIDER_API_ERROR, 'init failed');
      }),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const chatPromise = llm.chat({ messages: [], tools: [], signal: new AbortController().signal });

    await expect(chatPromise).rejects.toMatchObject({ code: 'provider.api_error', message: 'init failed' });
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
    const streamId = capturedStreamId(sdk);
    controller.abort();

    await expect(chatPromise).rejects.toThrow();
    expect(sdk.chatStreamCancel).toHaveBeenCalledWith({ streamId });
    expect((remoteLLMStreamRegistry as unknown as { streams: Map<string, unknown> }).streams.has(streamId)).toBe(false);
  });

  it('rejects when the registry reports an error', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's3' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const chatPromise = llm.chat({ messages: [], tools: [], signal: new AbortController().signal });

    await new Promise((resolve) => setTimeout(resolve, 0));
    const streamId = capturedStreamId(sdk);

    remoteLLMStreamRegistry.dispatchError({
      streamId,
      error: toOdyErrorPayload(new OdyError(ErrorCodes.PROVIDER_API_ERROR, 'boom')),
    });

    await expect(chatPromise).rejects.toMatchObject({ code: 'provider.api_error' });
  });

  it('strips executable runtime state from tools before RPC serialization', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's4' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's', provider: fakeProvider });
    const executableTool = {
      name: 'testTool',
      description: 'A test tool',
      parameters: { type: 'object', properties: {}, required: [] } as const,
      execute: vi.fn(),
      agentRef: { id: 'agent1' },
    };

    const chatPromise = llm.chat({
      messages: [],
      tools: [executableTool as unknown as Parameters<typeof llm.chat>[0]['tools'][number]],
      signal: new AbortController().signal,
    });

    await new Promise((resolve) => setTimeout(resolve, 0));
    const streamId = capturedStreamId(sdk);
    remoteLLMStreamRegistry.dispatchEnd({
      streamId,
      result: { toolCalls: [], usage: { inputOther: 0, output: 0, inputCacheRead: 0, inputCacheCreation: 0 } } as any,
    });

    await chatPromise;

    const initCall = (sdk.chatStreamInit as unknown as ReturnType<typeof vi.fn>).mock.calls[0]![0] as {
      streamId: string;
      request: { tools: unknown[] };
    };
    expect(initCall.request.tools).toHaveLength(1);
    expect(initCall.request.tools[0]).toEqual({
      name: 'testTool',
      description: 'A test tool',
      parameters: { type: 'object', properties: {}, required: [] },
    });
  });
});
