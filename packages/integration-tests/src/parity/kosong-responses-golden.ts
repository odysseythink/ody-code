import {
  generate,
  isContentPart,
  isToolCall,
  type ContentPart,
  type Message,
  type StreamedMessagePart,
  type Tool,
  type ToolCall,
} from '@odysseythink/kosong';
import {
  OpenAIResponsesChatProvider,
  OpenAIResponsesStreamedMessage,
} from '@odysseythink/kosong/providers/openai-responses';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: { apiKey?: string; headers?: Record<string, string> } };
  providerOptions: { model: string; apiKey?: string; baseUrl?: string };
  response: {
    status: number;
    stream?: boolean;
    body?: string;
    error?: { message: string; code?: string };
  };
  expectError?: boolean;
}

export interface GoldenResult {
  assistantMessage: unknown | null;
  error: string | null;
}

type ClientFactory = NonNullable<
  ConstructorParameters<typeof OpenAIResponsesChatProvider>[0]['clientFactory']
>;
type OpenAIClient = ReturnType<ClientFactory>;

export async function runTsKosongResponsesGolden(
  fixture: Fixture,
): Promise<GoldenResult> {
  if (fixture.response.error) {
    return runThroughProvider(fixture);
  }
  if (fixture.response.stream) {
    return runThroughProvider(fixture);
  }
  return runDirectParser(fixture);
}

async function runThroughProvider(fixture: Fixture): Promise<GoldenResult> {
  const provider = new OpenAIResponsesChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
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
  } catch (e) {
    return {
      assistantMessage: null,
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

async function runDirectParser(fixture: Fixture): Promise<GoldenResult> {
  try {
    const response = JSON.parse(fixture.response.body ?? '{}');
    const stream = new OpenAIResponsesStreamedMessage(response, false);
    const message = await partsToMessage(stream);
    return { assistantMessage: message, error: null };
  } catch (e) {
    return {
      assistantMessage: null,
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

async function partsToMessage(
  stream: AsyncIterable<StreamedMessagePart>,
): Promise<Message> {
  const content: ContentPart[] = [];
  const toolCalls: ToolCall[] = [];
  for await (const part of stream) {
    if (isContentPart(part)) {
      content.push(part);
    } else if (isToolCall(part)) {
      const { _streamIndex, ...tc } = part as ToolCall & {
        _streamIndex?: number | string;
      };
      toolCalls.push(tc as ToolCall);
    }
  }
  return {
    role: 'assistant',
    content,
    toolCalls,
  };
}

function createMockClient(response: Fixture['response']) {
  return {
    responses: {
      create: async (_params: unknown, _options?: unknown) => {
        if (response.error) return JSON.parse('{}');
        if (response.stream) return parseResponsesStream(response.body ?? '');
        return JSON.parse(response.body ?? '{}');
      },
    },
  };
}

async function* parseResponsesStream(body: string): AsyncIterable<unknown> {
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed.length === 0) continue;
    yield JSON.parse(trimmed);
  }
}
