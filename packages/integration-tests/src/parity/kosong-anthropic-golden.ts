import Anthropic from '@anthropic-ai/sdk';
import type { MessageStreamEvent, MessageParam, Tool } from '@anthropic-ai/sdk/resources/messages/messages.js';
import { AnthropicChatProvider } from '@odysseythink/kosong/providers/anthropic';
import { generate } from '@odysseythink/kosong';
import type { Message, Tool as KosongTool } from '@odysseythink/kosong';

export interface AnthropicFixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: {
    stream?: boolean;
    auth?: { apiKey?: string; headers?: Record<string, string> };
  };
  providerStep: {
    events?: unknown[];
    response?: {
      id: string;
      stop_reason?: string | null;
      usage: {
        input_tokens: number;
        output_tokens: number;
        cache_read_input_tokens?: number;
        cache_creation_input_tokens?: number;
      };
      content: unknown[];
    };
  };
  expectError?: string | null;
}

function createMockClient(fixture: AnthropicFixture): Anthropic {
  const stream = fixture.options?.stream ?? true;
  return {
    messages: {
      create: async (_params: unknown, _options: unknown) => {
        if (stream) {
          const events = fixture.providerStep.events ?? [];
          return (async function* () {
            for (const event of events) {
              yield event as MessageStreamEvent;
            }
          })();
        }
        return fixture.providerStep.response as Anthropic.Messages.Message;
      },
    },
  } as unknown as Anthropic;
}

export async function runTsAnthropicGolden(fixture: AnthropicFixture): Promise<{
  generateResult: {
    id: string | null;
    message: unknown;
    usage: unknown;
    finishReason: string | null;
    rawFinishReason: string | null;
  } | null;
  error: string | null;
}> {
  const provider = new AnthropicChatProvider({
    model: 'claude-opus-4-7',
    apiKey: 'sk-golden',
    baseUrl: 'http://localhost:0',
    stream: fixture.options?.stream ?? true,
    defaultMaxTokens: 1024,
    betaFeatures: [],
    clientFactory: () => createMockClient(fixture),
  });

  try {
    const result = await generate(
      provider,
      fixture.systemPrompt ?? '',
      ((fixture.tools ?? []) as unknown) as KosongTool[],
      fixture.history,
      undefined,
      { auth: fixture.options?.auth },
    );
    return {
      generateResult: {
        id: result.id,
        message: result.message,
        usage: result.usage,
        finishReason: result.finishReason,
        rawFinishReason: result.rawFinishReason,
      },
      error: null,
    };
  } catch (e) {
    return { generateResult: null, error: String(e) };
  }
}
