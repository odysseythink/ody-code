import { UNKNOWN_CAPABILITY } from '#/capability';
import type { Message, StreamedMessagePart } from '#/message';
import { GLMChatProvider } from '#/providers/glm';
import { describe, expect, it, vi } from 'vitest';

function makeProvider(options?: {
  model?: string;
  apiKey?: string;
  baseUrl?: string;
  stream?: boolean;
}): GLMChatProvider {
  return new GLMChatProvider({
    model: options?.model ?? 'glm-4.7',
    apiKey: options?.apiKey ?? 'test-key',
    baseUrl: options?.baseUrl,
    stream: options?.stream ?? false,
  });
}

describe('GLMChatProvider', () => {
  it('has name "glm"', () => {
    const provider = makeProvider();
    expect(provider.name).toBe('glm');
  });

  it('exposes the configured model name', () => {
    const provider = makeProvider({ model: 'glm-4.5' });
    expect(provider.modelName).toBe('glm-4.5');
  });

  it('defaults baseUrl to Z.AI endpoint', () => {
    const provider = makeProvider();
    expect(provider.modelParameters['baseUrl']).toBe('https://api.z.ai/api/paas/v4/');
  });

  it('honors explicit baseUrl override', () => {
    const provider = makeProvider({ baseUrl: 'https://open.bigmodel.cn/api/paas/v4/' });
    expect(provider.modelParameters['baseUrl']).toBe('https://open.bigmodel.cn/api/paas/v4/');
  });

  it('reads apiKey from GLM_API_KEY env var when not passed explicitly', () => {
    const original = process.env['GLM_API_KEY'];
    process.env['GLM_API_KEY'] = 'env-key';
    try {
      const provider = new GLMChatProvider({ model: 'glm-4.7' });
      expect(provider.modelParameters['model']).toBe('glm-4.7');
    } finally {
      process.env['GLM_API_KEY'] = original;
    }
  });

  describe('getCapability', () => {
    it('returns UNKNOWN_CAPABILITY for all models (no hard-coded catalog)', () => {
      const provider = makeProvider({ model: 'glm-4.7' });
      expect(provider.getCapability()).toBe(UNKNOWN_CAPABILITY);
    });

    it('explicit model arg still returns UNKNOWN_CAPABILITY', () => {
      const provider = makeProvider({ model: 'glm-4.5' });
      expect(provider.getCapability('glm-4.7')).toBe(UNKNOWN_CAPABILITY);
    });
  });

  describe('clone methods', () => {
    it('withThinking returns a new GLMChatProvider', () => {
      const original = makeProvider();
      const cloned = original.withThinking('high');
      expect(cloned).toBeInstanceOf(GLMChatProvider);
      expect(cloned).not.toBe(original);
      expect(cloned.name).toBe('glm');
      expect(cloned.thinkingEffort).toBe('high');
    });

    it('withGenerationKwargs returns a new GLMChatProvider', () => {
      const original = makeProvider();
      const cloned = original.withGenerationKwargs({ temperature: 0.5 });
      expect(cloned).toBeInstanceOf(GLMChatProvider);
      expect(cloned).not.toBe(original);
      expect(cloned.modelParameters['temperature']).toBe(0.5);
    });

    it('withMaxCompletionTokens returns a new GLMChatProvider', () => {
      const original = makeProvider();
      const cloned = original.withMaxCompletionTokens(1024);
      expect(cloned).toBeInstanceOf(GLMChatProvider);
      expect(cloned).not.toBe(original);
      expect(cloned.modelParameters['max_tokens']).toBe(1024);
    });

    it('withMaxCompletionTokens clamps to maxTokens cap when budget exceeds it', () => {
      const original = new GLMChatProvider({
        model: 'glm-4.5',
        apiKey: 'test-key',
        maxTokens: 96000,
      });
      const cloned = original.withMaxCompletionTokens(200000);
      expect(cloned.modelParameters['max_tokens']).toBe(96000);
    });

    it('withMaxCompletionTokens can tighten below maxTokens cap', () => {
      const original = new GLMChatProvider({
        model: 'glm-4.5',
        apiKey: 'test-key',
        maxTokens: 96000,
      });
      const cloned = original.withMaxCompletionTokens(8000);
      expect(cloned.modelParameters['max_tokens']).toBe(8000);
    });
  });

  describe('message conversion', () => {
    it('filters out empty-string text parts but preserves non-empty parts', async () => {
      const provider = makeProvider();
      const history: Message[] = [
        {
          role: 'assistant',
          content: [
            { type: 'text', text: '' },
            { type: 'text', text: 'hello' },
          ],
          toolCalls: [],
        },
      ];

      const createSpy = vi.fn().mockResolvedValue({
        id: 'resp-1',
        choices: [{ message: { content: 'ok' }, finish_reason: 'stop' }],
        usage: null,
      });

      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      await provider.generate('', [], history);

      const callArgs = (createSpy.mock.calls[0]!)[0] as Record<string, unknown>;
      const msgs = callArgs['messages'] as Array<Record<string, unknown>>;
      expect(msgs).toHaveLength(1);
      expect((msgs[0]!)['content']).toBe('hello');
    });

    it('preserves whitespace-only text parts', async () => {
      const provider = makeProvider();
      const history: Message[] = [
        {
          role: 'assistant',
          content: [{ type: 'text', text: ' ' }],
          toolCalls: [],
        },
      ];

      const createSpy = vi.fn().mockResolvedValue({
        id: 'resp-1',
        choices: [{ message: { content: 'ok' }, finish_reason: 'stop' }],
        usage: null,
      });
      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      await provider.generate('', [], history);

      const callArgs = (createSpy.mock.calls[0]!)[0] as Record<string, unknown>;
      const msgs = callArgs['messages'] as Array<Record<string, unknown>>;
      expect((msgs[0]!)['content']).toBe(' ');
    });

    it('drops a message whose only text part is empty string', async () => {
      const provider = makeProvider();
      const history: Message[] = [
        {
          role: 'assistant',
          content: [{ type: 'text', text: '' }],
          toolCalls: [],
        },
      ];

      const createSpy = vi.fn().mockResolvedValue({
        id: 'resp-1',
        choices: [{ message: { content: 'ok' }, finish_reason: 'stop' }],
        usage: null,
      });
      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      await provider.generate('', [], history);

      const callArgs = (createSpy.mock.calls[0]!)[0] as Record<string, unknown>;
      const msgs = callArgs['messages'] as Array<Record<string, unknown>>;
      expect((msgs[0]!)['content']).toBeUndefined();
    });

    it('throws for non-text multimodal content parts', () => {
      const provider = makeProvider();
      const history: Message[] = [
        {
          role: 'user',
          content: [{ type: 'image_url', imageUrl: { url: 'https://example.com/img.png' } }],
          toolCalls: [],
        },
      ];

      return expect(provider.generate('', [], history)).rejects.toThrow(
        'GLM provider does not support image_url content parts.',
      );
    });
  });

  describe('thinking parameter injection', () => {
    it('injects thinking: { type: "disabled" } when withThinking("off")', async () => {
      const provider = makeProvider().withThinking('off');
      const history: Message[] = [
        { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] },
      ];

      const createSpy = vi.fn().mockResolvedValue({
        id: 'resp-1',
        choices: [{ message: { content: 'ok' }, finish_reason: 'stop' }],
        usage: null,
      });
      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      await provider.generate('', [], history);

      const callArgs = (createSpy.mock.calls[0]!)[0] as Record<string, unknown>;
      expect(callArgs['thinking']).toEqual({ type: 'disabled' });
    });

    it('omits thinking param when thinkingEffort is default (null)', async () => {
      const provider = makeProvider();
      const history: Message[] = [
        { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] },
      ];

      const createSpy = vi.fn().mockResolvedValue({
        id: 'resp-1',
        choices: [{ message: { content: 'ok' }, finish_reason: 'stop' }],
        usage: null,
      });
      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      await provider.generate('', [], history);

      const callArgs = (createSpy.mock.calls[0]!)[0] as Record<string, unknown>;
      expect(callArgs['thinking']).toBeUndefined();
    });

    it('omits thinking param when withThinking("medium")', async () => {
      const provider = makeProvider().withThinking('medium');
      const history: Message[] = [
        { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] },
      ];

      const createSpy = vi.fn().mockResolvedValue({
        id: 'resp-1',
        choices: [{ message: { content: 'ok' }, finish_reason: 'stop' }],
        usage: null,
      });
      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      await provider.generate('', [], history);

      const callArgs = (createSpy.mock.calls[0]!)[0] as Record<string, unknown>;
      expect(callArgs['thinking']).toBeUndefined();
    });
  });

  describe('stream response parsing', () => {
    it('extracts reasoning_content from stream chunks', async () => {
      const provider = makeProvider({ stream: true });
      const history: Message[] = [
        { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] },
      ];

      async function* mockStream() {
        yield {
          id: 'stream-1',
          choices: [
            {
              delta: { reasoning_content: 'Let me think...' },
              finish_reason: null,
            },
          ],
        };
        yield {
          id: 'stream-1',
          choices: [{ delta: { content: 'Hi!' }, finish_reason: 'stop' }],
        };
      }

      const createSpy = vi.fn().mockReturnValue(mockStream());
      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      const stream = await provider.generate('', [], history);
      const parts: StreamedMessagePart[] = [];
      for await (const part of stream) {
        parts.push(part);
      }

      expect(parts).toEqual([
        { type: 'think', think: 'Let me think...' },
        { type: 'text', text: 'Hi!' },
      ]);
      expect(stream.id).toBe('stream-1');
      expect(stream.finishReason).toBe('completed');
    });

    it('extracts text and tool_calls from stream chunks', async () => {
      const provider = makeProvider({ stream: true });
      const history: Message[] = [
        { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] },
      ];

      async function* mockStream() {
        yield {
          id: 'stream-2',
          choices: [
            {
              delta: {
                tool_calls: [
                  { index: 0, id: 'call-1', function: { name: 'getWeather', arguments: '' } },
                ],
              },
              finish_reason: null,
            },
          ],
        };
        yield {
          id: 'stream-2',
          choices: [
            {
              delta: {
                tool_calls: [
                  { index: 0, function: { arguments: '{"city": "Beijing"}' } },
                ],
              },
              finish_reason: 'tool_calls',
            },
          ],
        };
      }

      const createSpy = vi.fn().mockReturnValue(mockStream());
      const mockClient = { chat: { completions: { create: createSpy } } } as unknown as import('openai').default;
      vi.spyOn(provider as any, '_createClient').mockReturnValue(mockClient);

      const stream = await provider.generate('', [], history);
      const parts: StreamedMessagePart[] = [];
      for await (const part of stream) {
        parts.push(part);
      }

      expect(parts).toHaveLength(2);
      expect(parts[0]).toMatchObject({
        type: 'function',
        id: 'call-1',
        name: 'getWeather',
      });
      expect(parts[1]).toMatchObject({
        type: 'tool_call_part',
        argumentsPart: '{"city": "Beijing"}',
        index: 0,
      });
      expect(stream.finishReason).toBe('tool_calls');
    });
  });

  describe('generate end-to-end', () => {
    it('delegates through the OpenAI SDK client and yields parts', async () => {
      const provider = makeProvider();
      const history: Message[] = [
        { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] },
      ];

      vi.spyOn(provider as any, '_createClient').mockReturnValue({
        chat: {
          completions: {
            create: vi.fn().mockResolvedValue({
              id: 'resp-1',
              choices: [{ message: { content: 'Hi!' }, finish_reason: 'stop' }],
              usage: null,
            }),
          },
        },
      } as unknown as import('openai').default);

      const stream = await provider.generate('You are helpful.', [], history);
      const parts = [];
      for await (const part of stream) {
        parts.push(part);
      }

      expect(parts).toEqual([{ type: 'text', text: 'Hi!' }]);
      expect(stream.id).toBe('resp-1');
    });
  });
});
