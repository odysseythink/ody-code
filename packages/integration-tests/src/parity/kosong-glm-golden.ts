import { generate } from '@odysseythink/kosong';
import type { Message, Tool } from '@odysseythink/kosong';
import { GLMChatProvider } from '@odysseythink/kosong/providers/glm';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: { apiKey?: string; headers?: Record<string, string> } };
  providerOptions: {
    model: string;
    apiKey?: string;
    baseUrl?: string;
    stream?: boolean;
    maxTokens?: number;
  };
  response: {
    status: number;
    stream?: boolean;
    body?: string;
    error?: { message: string; code?: string };
  };
  expectError?: boolean;
}

export interface GoldenResult {
  assistantMessage: unknown;
  error: string | null;
}

type ClientFactory = NonNullable<
  ConstructorParameters<typeof GLMChatProvider>[0]['clientFactory']
>;
type OpenAIClient = ReturnType<ClientFactory>;

export async function runTsKosongGLMGolden(fixture: Fixture): Promise<GoldenResult> {
  const provider = new GLMChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
    stream: fixture.providerOptions.stream ?? true,
    maxTokens: fixture.providerOptions.maxTokens,
    clientFactory: () => createMockClient(fixture.response) as unknown as OpenAIClient,
  });
  try {
    const result = await generate(
      provider,
      fixture.systemPrompt ?? '',
      fixture.tools ?? [],
      fixture.history,
      undefined,
      fixture.options,
    );
    return { assistantMessage: result.message, error: null };
  } catch (error) {
    return { assistantMessage: null, error: error instanceof Error ? error.message : String(error) };
  }
}

function createMockClient(response: Fixture['response']) {
  return {
    chat: {
      completions: {
        create: async (_params: unknown, _options?: unknown) => {
          if (response.error) return JSON.parse('{}');
          if (response.stream) return parseSSE(response.body ?? '');
          return JSON.parse(response.body ?? '{}');
        },
      },
    },
  };
}

async function* parseSSE(body: string): AsyncIterable<unknown> {
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('data: ')) continue;
    const data = trimmed.slice(6);
    if (data === '[DONE]') break;
    yield JSON.parse(data);
  }
}
