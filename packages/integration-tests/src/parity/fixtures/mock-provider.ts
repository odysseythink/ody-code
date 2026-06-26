import type { ChatProvider, FinishReason, GenerateOptions, Message, ModelCapability, StreamedMessage, StreamedMessagePart, ThinkingEffort, Tool, TokenUsage } from '@odysseythink/kosong';
import { UNKNOWN_CAPABILITY } from '@odysseythink/kosong';

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

  constructor(
    private readonly partsOrResponses: StreamedMessagePart[] | StreamedMessagePart[][],
    private readonly options: MockChatProviderOptions = {},
  ) {
    this.modelName = options.modelName ?? 'mock';
  }

  private currentParts(): StreamedMessagePart[] {
    const first = (this.partsOrResponses as StreamedMessagePart[][])[0];
    if (Array.isArray(first)) {
      const responses = this.partsOrResponses as StreamedMessagePart[][];
      const parts = responses[this.callIndex % responses.length];
      this.callIndex++;
      return parts ?? [];
    }
    return this.partsOrResponses as StreamedMessagePart[];
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
    return new MockChatProvider([...(this.partsOrResponses as StreamedMessagePart[][])], this.options);
  }
}
