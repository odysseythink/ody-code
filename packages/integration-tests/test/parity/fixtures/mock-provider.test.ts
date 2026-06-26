import { describe, expect, it } from 'vitest';
import { MockChatProvider } from '../../../src/parity/fixtures/mock-provider';

describe('MockChatProvider', () => {
  it('returns the same parts for a single-response constructor', async () => {
    const provider = new MockChatProvider([{ type: 'text', text: 'hi' }]);
    const stream = await provider.generate('', [], []);
    const parts: unknown[] = [];
    for await (const part of stream) {
      parts.push(part);
    }
    expect(parts).toHaveLength(1);
    expect((parts[0] as { type: string; text: string }).text).toBe('hi');
  });

  it('cycles through multiple responses', async () => {
    const provider = new MockChatProvider([
      [{ type: 'text', text: 'first' }],
      [{ type: 'text', text: 'second' }],
    ]);
    const first = await collectText(await provider.generate('', [], []));
    const second = await collectText(await provider.generate('', [], []));
    const third = await collectText(await provider.generate('', [], []));
    expect(first).toBe('first');
    expect(second).toBe('second');
    expect(third).toBe('first');
  });

  it('withThinking preserves single-response shape', async () => {
    const provider = new MockChatProvider([{ type: 'text', text: 'hi' }]);
    const thinking = provider.withThinking('low');
    const text = await collectText(await thinking.generate('', [], []));
    expect(text).toBe('hi');
  });

  it('withThinking preserves multi-response shape', async () => {
    const provider = new MockChatProvider([
      [{ type: 'text', text: 'a' }],
      [{ type: 'text', text: 'b' }],
    ]);
    const thinking = provider.withThinking('low');
    const first = await collectText(await thinking.generate('', [], []));
    const second = await collectText(await thinking.generate('', [], []));
    expect(first).toBe('a');
    expect(second).toBe('b');
  });
});

async function collectText(stream: AsyncIterable<unknown>): Promise<string> {
  const parts: unknown[] = [];
  for await (const part of stream) {
    parts.push(part);
  }
  return parts
    .map(
      (p) =>
        (p as { delta?: string; text?: string }).delta ??
        (p as { text?: string }).text ??
        '',
    )
    .join('');
}
