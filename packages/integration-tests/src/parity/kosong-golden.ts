import type {
  ChatProvider,
  FinishReason,
  Message,
  StreamedMessage,
  StreamedMessagePart,
  ThinkingEffort,
  Tool,
} from '@odysseythink/kosong';
import { generate } from '@odysseythink/kosong';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: {
    auth?: { apiKey?: string; headers?: Record<string, string> };
  };
  providerStep: {
    id?: string;
    parts: unknown[];
    usage?: {
      inputOther: number;
      output: number;
      inputCacheRead: number;
      inputCacheCreation: number;
    };
    finishReason?: string;
    rawFinishReason?: string;
  };
  expectError?: string | null;
}

class MockProvider implements ChatProvider {
  readonly name = 'mock';
  readonly modelName = 'm1';
  readonly thinkingEffort = null;

  constructor(private readonly step: Fixture['providerStep']) {}

  async generate(
    _systemPrompt: string,
    _tools: Tool[],
    _history: Message[],
    _options?: {
      signal?: AbortSignal;
      auth?: { apiKey?: string; headers?: Record<string, string> };
    },
  ): Promise<StreamedMessage> {
    const step = this.step;
    return {
      id: step.id ?? null,
      usage: step.usage ?? null,
      finishReason: (step.finishReason ?? null) as FinishReason | null,
      rawFinishReason: step.rawFinishReason ?? null,
      async *[Symbol.asyncIterator]() {
        for (const part of step.parts) {
          yield part as StreamedMessagePart;
        }
      },
    } as StreamedMessage;
  }

  withThinking(_effort: ThinkingEffort): ChatProvider {
    return this;
  }
}

export async function runTsKosongGolden(fixture: Fixture): Promise<{
  assistantMessage: unknown | null;
  error: string | null;
}> {
  const provider = new MockProvider(fixture.providerStep);
  try {
    const result = await generate(
      provider,
      fixture.systemPrompt ?? '',
      fixture.tools ?? [],
      fixture.history,
      undefined,
      {
        auth: fixture.options?.auth,
      },
    );
    return { assistantMessage: result.message, error: null };
  } catch (e) {
    return { assistantMessage: null, error: String(e) };
  }
}
