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

  constructor(
    private readonly parts: StreamedMessagePart[],
    private readonly options: MockChatProviderOptions = {},
  ) {
    this.modelName = options.modelName ?? 'mock';
  }

  async generate(
    _systemPrompt: string,
    _tools: Tool[],
    _history: Message[],
    _options?: GenerateOptions,
  ): Promise<StreamedMessage> {
    const id = this.options.id ?? 'mock';
    const finishReason = this.options.finishReason ?? 'completed';
    const rawFinishReason = this.options.rawFinishReason ?? 'stop';
    const usage = this.options.usage ?? null;
    const parts = this.parts;
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
    return new MockChatProvider([...this.parts], this.options);
  }
}
