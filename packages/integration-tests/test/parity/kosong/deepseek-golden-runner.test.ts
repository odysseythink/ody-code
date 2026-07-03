import { describe, it, expect } from 'vitest';
import { runTsKosongDeepSeekGolden, type Fixture } from '../../../src/parity/kosong-deepseek-golden';

const TEXT_STREAM_BODY =
  'data: {"id":"1","choices":[{"index":0,"delta":{"role":"assistant","content":"Hi"}}]}\n\n' +
  'data: {"id":"2","choices":[{"index":0,"delta":{"content":" there"}}]}\n\n' +
  'data: {"id":"3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\n' +
  'data: [DONE]\n\n';

const FIXTURE: Fixture = {
  systemPrompt: '',
  history: [{ role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] }],
  providerOptions: { model: 'deepseek-chat', stream: true },
  response: { status: 200, stream: true, body: TEXT_STREAM_BODY },
};

describe('kosong-deepseek-golden runner', () => {
  it('merges streamed text', async () => {
    const result = await runTsKosongDeepSeekGolden(FIXTURE);
    expect(result.error).toBeNull();
    expect(result.assistantMessage).toMatchObject({ role: 'assistant', content: [{ type: 'text', text: 'Hi there' }], toolCalls: [] });
  });
});
