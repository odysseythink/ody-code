import { describe, expect, it } from 'vitest';

import type {
  ChatStreamRequest,
  ChatStreamResult,
  StreamDelta,
} from '../../src/rpc/llm-stream';

describe('llm stream contract', () => {
  it('ChatStreamRequest round-trips through JSON', () => {
    const request: ChatStreamRequest = {
      modelName: 'kimi-k2',
      systemPrompt: 'You are a helpful assistant.',
      messages: [{ role: 'user', content: 'hello' }] as any,
      tools: [],
      capability: { thinking: true, image_in: false, video_in: false, audio_in: false, tool_use: false, max_context_tokens: 0, max_output_tokens: 0 },
      requestLogContext: { turnId: 't1', step: 1 },
      provider: { type: 'openai', model: 'fake-model', apiKey: 'fake-key' },
    };
    const json = JSON.stringify(request);
    expect(JSON.parse(json)).toEqual(request);
  });

  it('ChatStreamResult round-trips through JSON', () => {
    const delta: StreamDelta = { type: 'text', text: 'hi' };
    const result: ChatStreamResult = {
      toolCalls: [],
      providerFinishReason: 'completed',
      rawFinishReason: 'stop',
      usage: { promptTokens: 1, completionTokens: 1, totalTokens: 2 } as any,
      streamTiming: { firstTokenLatencyMs: 10, streamDurationMs: 20 },
    };
    expect(JSON.parse(JSON.stringify(delta))).toEqual(delta);
    expect(JSON.parse(JSON.stringify(result))).toEqual(result);
  });
});
