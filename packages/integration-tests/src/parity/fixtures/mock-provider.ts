import type { ChatProvider, FinishReason, GenerateOptions, Message, ModelCapability, StreamedMessage, StreamedMessagePart, ThinkingEffort, Tool, TokenUsage } from '@odysseythink/kosong';
import { UNKNOWN_CAPABILITY } from '@odysseythink/kosong';

function normalizeResponses(
  partsOrResponses: StreamedMessagePart[] | StreamedMessagePart[][],
): StreamedMessagePart[][] {
  if (partsOrResponses.length === 0) {
    return [[]];
  }
  const first = partsOrResponses[0];
  if (Array.isArray(first)) {
    return (partsOrResponses as StreamedMessagePart[][]).map((parts) => [...parts]);
  }
  return [[...(partsOrResponses as StreamedMessagePart[])]];
}

export interface MockChatProviderOptions {
  id?: string;
  modelName?: string;
  finishReason?: FinishReason | null;
  rawFinishReason?: string | null;
  usage?: TokenUsage;
}

export class MockChatProvider implements ChatProvider {
  readonly name = 'mock';
  readonly modelName: string;
  readonly thinkingEffort: ThinkingEffort | null = null;
  private callIndex = 0;
  private readonly responses: StreamedMessagePart[][];

  constructor(
    partsOrResponses: StreamedMessagePart[] | StreamedMessagePart[][],
    private readonly options: MockChatProviderOptions = {},
  ) {
    this.modelName = options.modelName ?? 'mock';
    this.responses = normalizeResponses(partsOrResponses);
  }

  private currentParts(): StreamedMessagePart[] {
    const parts = this.responses[this.callIndex % this.responses.length];
    this.callIndex++;
    return parts ?? [];
  }

  async generate(
    _systemPrompt: string,
    _tools: Tool[],
    _history: Message[],
    _options?: GenerateOptions,
  ): Promise<StreamedMessage> {
    const parts = this.currentParts();
    const id = this.options.id ?? 'mock';
    const finishReason = this.options.finishReason ?? 'completed';
    const rawFinishReason = this.options.rawFinishReason ?? 'stop';
    const usage = this.options.usage ?? null;
    return {
      id,
      usage,
      finishReason,
      rawFinishReason,
      async *[Symbol.asyncIterator]() {
        for (const part of parts) {
          yield part;
        }
      },
    };
  }

  getCapability(_model?: string): ModelCapability {
    return UNKNOWN_CAPABILITY;
  }

  withThinking(_effort: ThinkingEffort): MockChatProvider {
    return new MockChatProvider(this.responses.map((parts) => [...parts]), this.options);
  }
}
