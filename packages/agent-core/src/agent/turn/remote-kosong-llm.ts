import { randomUUID } from 'node:crypto';

import { isRetryableGenerateError, type ModelCapability, type ProviderConfig } from '@odysseythink/kosong';

import { ErrorCodes, fromOdyErrorPayload, OdyError } from '@odysseythink/agent-core-shared';
import type {
  LLM,
  LLMChatParams,
  LLMChatResponse,
  ToolCallDelta,
} from '#loop/llm';
import type { SDKAgentRPC } from '#rpc';
import type {
  ChatStreamDeltaPayload,
  ChatStreamEndPayload,
  ChatStreamErrorPayload,
  ChatStreamRequest,
  ChatStreamResult,
  StreamDelta,
} from '#rpc/llm-stream';
import type { CompletionBudgetConfig } from '#utils/completion-budget';

export interface RemoteKosongLLMConfig {
  readonly sdk: SDKAgentRPC;
  readonly modelName: string;
  readonly systemPrompt: string;
  readonly capability?: ModelCapability | undefined;
  readonly completionBudgetConfig?: CompletionBudgetConfig | undefined;
  readonly provider: ProviderConfig;
}

interface StreamHandlers {
  onDelta(delta: StreamDelta): void;
  onEnd(result: ChatStreamResult): void;
  onError(error: Error): void;
}

class RemoteLLMStreamRegistry {
  private readonly streams = new Map<string, StreamHandlers>();

  register(streamId: string, handlers: StreamHandlers): void {
    this.streams.set(streamId, handlers);
  }

  unregister(streamId: string): void {
    this.streams.delete(streamId);
  }

  dispatchDelta({ streamId, delta }: ChatStreamDeltaPayload): void {
    this.streams.get(streamId)?.onDelta(delta);
  }

  dispatchEnd({ streamId, result }: ChatStreamEndPayload): void {
    const handlers = this.streams.get(streamId);
    if (handlers === undefined) return;
    handlers.onEnd(result);
    this.unregister(streamId);
  }

  dispatchError({ streamId, error }: ChatStreamErrorPayload): void {
    const handlers = this.streams.get(streamId);
    if (handlers === undefined) return;
    handlers.onError(fromOdyErrorPayload(error));
    this.unregister(streamId);
  }
}

export const remoteLLMStreamRegistry = new RemoteLLMStreamRegistry();

export class RemoteKosongLLM implements LLM {
  readonly systemPrompt: string;
  readonly modelName: string;
  readonly capability?: ModelCapability | undefined;

  private readonly sdk: SDKAgentRPC;
  private readonly completionBudgetConfig: CompletionBudgetConfig | undefined;
  private readonly provider: ProviderConfig;

  constructor(config: RemoteKosongLLMConfig) {
    this.sdk = config.sdk;
    this.modelName = config.modelName;
    this.systemPrompt = config.systemPrompt;
    this.capability = config.capability;
    this.completionBudgetConfig = config.completionBudgetConfig;
    this.provider = config.provider;
  }

  async chat(params: LLMChatParams): Promise<LLMChatResponse> {
    const request = this.buildRequest(params);
    // The worker/server owns the stream ID so it can register the handler before
    // the host starts emitting deltas, avoiding a race where early deltas are lost.
    const streamId = randomUUID();

    const signal = params.signal;
    signal?.throwIfAborted();

    return new Promise<LLMChatResponse>((resolve, reject) => {
      const onAbort = (): void => {
        this.sdk.chatStreamCancel({ streamId });
        remoteLLMStreamRegistry.unregister(streamId);
        reject(new OdyError(ErrorCodes.INTERNAL, 'Stream cancelled'));
      };
      signal?.addEventListener('abort', onAbort, { once: true });

      const handlers: StreamHandlers = {
        onDelta: (delta) => {
          if (signal?.aborted) return;
          this.forwardDelta(delta, params);
        },
        onEnd: (result) => {
          signal?.removeEventListener('abort', onAbort);
          remoteLLMStreamRegistry.unregister(streamId);
          resolve(this.toLLMChatResponse(result));
        },
        onError: (error) => {
          signal?.removeEventListener('abort', onAbort);
          remoteLLMStreamRegistry.unregister(streamId);
          reject(error);
        },
      };
      remoteLLMStreamRegistry.register(streamId, handlers);

      // Initiate generation on the host only after the local handler is
      // registered, so no deltas are dropped.
      this.sdk.chatStreamInit({ request, streamId }).catch((error) => {
        signal?.removeEventListener('abort', onAbort);
        remoteLLMStreamRegistry.unregister(streamId);
        reject(error);
      });
    });
  }

  isRetryableError(error: unknown): boolean {
    return isRetryableGenerateError(error);
  }

  private buildRequest(params: LLMChatParams): ChatStreamRequest {
    return {
      modelName: this.modelName,
      systemPrompt: this.systemPrompt,
      messages: params.messages,
      // Strip executable-tool runtime state before crossing the RPC boundary;
      // only the model-visible definition is needed on the SDK side.
      tools: params.tools.map(({ name, description, parameters }) => ({ name, description, parameters })),
      capability: this.capability,
      completionBudgetConfig: this.completionBudgetConfig,
      requestLogContext: params.requestLogContext,
      provider: this.provider,
    };
  }

  private toLLMChatResponse(result: ChatStreamResult): LLMChatResponse {
    return {
      toolCalls: [...result.toolCalls],
      providerFinishReason: result.providerFinishReason,
      rawFinishReason: result.rawFinishReason,
      usage: result.usage,
      streamTiming: result.streamTiming,
    };
  }

  private forwardDelta(delta: StreamDelta, params: LLMChatParams): void {
    switch (delta.type) {
      case 'text':
        params.onTextDelta?.(delta.text);
        break;
      case 'think':
        params.onThinkDelta?.(delta.think);
        break;
      case 'tool_call_part': {
        const toolDelta: ToolCallDelta = {
          toolCallId: delta.toolCallId,
          name: delta.name,
          argumentsPart: delta.argumentsPart,
        };
        params.onToolCallDelta?.(toolDelta);
        break;
      }
    }
  }
}
