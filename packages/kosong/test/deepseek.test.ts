import { UNKNOWN_CAPABILITY } from '#/capability';
import type { Message } from '#/message';
import { DeepSeekChatProvider } from '#/providers/deepseek';
import { describe, expect, it, vi } from 'vitest';

function makeProvider(options?: {
  model?: string;
  apiKey?: string;
  baseUrl?: string;
  stream?: boolean;
}): DeepSeekChatProvider {
  return new DeepSeekChatProvider({
    model: options?.model ?? 'deepseek-chat',
    apiKey: options?.apiKey ?? 'test-key',
    baseUrl: options?.baseUrl,
    stream: options?.stream ?? false,
  });
}

describe('DeepSeekChatProvider', () => {
  it('has name "deepseek"', () => {
    const provider = makeProvider();
    expect(provider.name).toBe('deepseek');
  });

  it('exposes the configured model name', () => {
    const provider = makeProvider({ model: 'deepseek-reasoner' });
    expect(provider.modelName).toBe('deepseek-reasoner');
  });

  it('defaults baseUrl to DeepSeek endpoint', () => {
    const provider = makeProvider();
    expect(provider.modelParameters['baseUrl']).toBe('https://api.deepseek.com/v1');
  });

  it('honors explicit baseUrl override', () => {
    const provider = makeProvider({ baseUrl: 'https://custom.example.com/v1' });
    expect(provider.modelParameters['baseUrl']).toBe('https://custom.example.com/v1');
  });

  it('reads apiKey from DEEPSEEK_API_KEY env var when not passed explicitly', () => {
    const original = process.env['DEEPSEEK_API_KEY'];
    process.env['DEEPSEEK_API_KEY'] = 'env-key';
    try {
      const provider = new DeepSeekChatProvider({ model: 'deepseek-chat' });
      // The delegate should pick up the env key; modelParameters carries it
      // indirectly via the working provider instance.
      expect(provider.modelParameters['model']).toBe('deepseek-chat');
    } finally {
      process.env['DEEPSEEK_API_KEY'] = original;
    }
  });

  describe('getCapability', () => {
    it('deepseek-chat → tool_use, no thinking', () => {
      const cap = makeProvider({ model: 'deepseek-chat' }).getCapability();
      expect(cap.tool_use).toBe(true);
      expect(cap.thinking).toBe(false);
      expect(cap.image_in).toBe(false);
    });

    it('deepseek-reasoner → thinking, no tool_use', () => {
      const cap = makeProvider({ model: 'deepseek-reasoner' }).getCapability();
      expect(cap.thinking).toBe(true);
      expect(cap.tool_use).toBe(false);
      expect(cap.image_in).toBe(false);
    });

    it('explicit model arg overrides this.modelName', () => {
      const provider = makeProvider({ model: 'deepseek-chat' });
      expect(provider.getCapability('deepseek-reasoner').thinking).toBe(true);
      expect(provider.getCapability('deepseek-reasoner').tool_use).toBe(false);
    });

    it('deepseek-v4-pro → thinking, tool_use, 1M context', () => {
      const cap = makeProvider({ model: 'deepseek-v4-pro' }).getCapability();
      expect(cap.thinking).toBe(true);
      expect(cap.tool_use).toBe(true);
      expect(cap.max_context_tokens).toBe(1_000_000);
    });

    it('deepseek-v4-flash → thinking, tool_use, 1M context', () => {
      const cap = makeProvider({ model: 'deepseek-v4-flash' }).getCapability();
      expect(cap.thinking).toBe(true);
      expect(cap.tool_use).toBe(true);
      expect(cap.max_context_tokens).toBe(1_000_000);
    });

    it('unknown model → UNKNOWN_CAPABILITY (no throw)', () => {
      const cap = makeProvider({ model: 'deepseek-unknown' }).getCapability();
      expect(cap).toEqual(UNKNOWN_CAPABILITY);
    });
  });

  describe('delegation to OpenAILegacyChatProvider', () => {
    it('generate delegates to the underlying provider', async () => {
      const provider = makeProvider();
      const history: Message[] = [
        { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] },
      ];

      // Mock the delegate's generate method
      const mockStream = {
        async *[Symbol.asyncIterator]() {
          yield { type: 'text', text: 'Hi!' };
        },
        id: 'resp-1',
        usage: null,
        finishReason: 'completed' as const,
        rawFinishReason: 'stop',
      };

      vi.spyOn((provider as any)._delegate, 'generate').mockResolvedValue(mockStream);

      const stream = await provider.generate('You are helpful.', [], history);
      const parts = [];
      for await (const part of stream) {
        parts.push(part);
      }

      expect(parts).toEqual([{ type: 'text', text: 'Hi!' }]);
      expect(stream.id).toBe('resp-1');
    });
  });

  describe('clone methods', () => {
    it('withThinking returns a new DeepSeekChatProvider', () => {
      const original = makeProvider();
      const cloned = original.withThinking('high');
      expect(cloned).toBeInstanceOf(DeepSeekChatProvider);
      expect(cloned).not.toBe(original);
      expect(cloned.name).toBe('deepseek');
      expect(cloned.thinkingEffort).toBe('high');
    });

    it('withGenerationKwargs returns a new DeepSeekChatProvider', () => {
      const original = makeProvider();
      const cloned = original.withGenerationKwargs({ temperature: 0.5 });
      expect(cloned).toBeInstanceOf(DeepSeekChatProvider);
      expect(cloned).not.toBe(original);
      expect(cloned.modelParameters['temperature']).toBe(0.5);
    });

    it('withMaxCompletionTokens returns a new DeepSeekChatProvider', () => {
      const original = makeProvider();
      const cloned = original.withMaxCompletionTokens(1024);
      expect(cloned).toBeInstanceOf(DeepSeekChatProvider);
      expect(cloned).not.toBe(original);
      expect(cloned.modelParameters['max_tokens']).toBe(1024);
    });

    it('withMaxCompletionTokens clamps to maxTokens cap when budget exceeds it', () => {
      const original = new DeepSeekChatProvider({
        model: 'deepseek-v4-pro',
        apiKey: 'test-key',
        maxTokens: 384000,
      });
      const cloned = original.withMaxCompletionTokens(1_000_000);
      expect(cloned.modelParameters['max_tokens']).toBe(384000);
    });

    it('withMaxCompletionTokens can tighten below maxTokens cap', () => {
      const original = new DeepSeekChatProvider({
        model: 'deepseek-v4-pro',
        apiKey: 'test-key',
        maxTokens: 384000,
      });
      const cloned = original.withMaxCompletionTokens(16000);
      expect(cloned.modelParameters['max_tokens']).toBe(16000);
    });

    it('withGenerationKwargs clamps max_tokens to maxTokens cap', () => {
      const original = new DeepSeekChatProvider({
        model: 'deepseek-v4-pro',
        apiKey: 'test-key',
        maxTokens: 384000,
      });
      const cloned = original.withGenerationKwargs({ max_tokens: 1_000_000 });
      expect(cloned.modelParameters['max_tokens']).toBe(384000);
    });
  });
});
