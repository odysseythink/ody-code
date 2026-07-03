import { generate } from '@odysseythink/kosong';
import type { Message, Tool } from '@odysseythink/kosong';
import { OpenAILegacyChatProvider } from '@odysseythink/kosong/providers/openai-legacy';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: { apiKey?: string; headers?: Record<string, string> } };
  providerOptions: { model: string; apiKey?: string; baseUrl?: string; stream?: boolean; reasoningKey?: string };
  response: { status: number; stream?: boolean; body?: string; error?: { message: string; code?: string } };
  expectError?: boolean;
}

export interface GoldenResult { assistantMessage: unknown | null; error: string | null; }

type ClientFactory = NonNullable<ConstructorParameters<typeof OpenAILegacyChatProvider>[0]['clientFactory']>;
type OpenAIClient = ReturnType<ClientFactory>;

export async function runTsKosongOpenAIGolden(fixture: Fixture): Promise<GoldenResult> {
  const provider = new OpenAILegacyChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
    stream: fixture.providerOptions.stream ?? true,
    reasoningKey: fixture.providerOptions.reasoningKey,
    clientFactory: () => createMockClient(fixture.response) as unknown as OpenAIClient,
  });
  try {
    const result = await generate(provider, fixture.systemPrompt ?? '', fixture.tools ?? [], fixture.history, undefined, fixture.options);
    return { assistantMessage: result.message, error: null };
  } catch (e) { return { assistantMessage: null, error: e instanceof Error ? e.message : String(e) }; }
}

function createMockClient(response: Fixture['response']) {
  return {
    chat: { completions: { create: async (_params: unknown, _options?: unknown) => {
      if (response.error) return JSON.parse('{}');
      if (response.stream) return parseSSE(response.body ?? '');
      return JSON.parse(response.body ?? '{}');
    }}}
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
