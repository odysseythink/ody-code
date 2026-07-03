import { describe, expect, it } from 'vitest';
import type { ScenarioSnapshot } from '../../src/parity/types';
import { normalize, normalizeTurnEvents } from '../../src/parity/normalize';

function snapshot(input: Partial<ScenarioSnapshot> = {}): ScenarioSnapshot {
  return {
    responses: [],
    events: [],
    ...input,
  };
}

describe('normalize', () => {
  it('replaces UUIDs with <id>', () => {
    const result = normalize(
      snapshot({ responses: ['session-550e8400-e29b-41d4-a716-446655440000-end'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['session-<id>-end']);
  });

  it('replaces uppercase UUIDs case-insensitively', () => {
    const result = normalize(
      snapshot({ responses: ['uuid:550E8400-E29B-41D4-A716-446655440000'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['uuid:<id>']);
  });

  it('rejects 31-character pseudo-UUIDs', () => {
    const input = 'short-550e8400-e29b-41d4-a716-44665544000';
    const result = normalize(snapshot({ responses: [input] }), { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' });
    expect(result.responses).toEqual([input]);
  });

  it('replaces homeDir and tmpDir with placeholders', () => {
    const result = normalize(
      snapshot({ responses: ['/tmp/home/config.toml', '/tmp/tmp/log.txt'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['<HOME>/config.toml', '<TMP>/log.txt']);
  });

  it('replaces Windows-style paths and preserves drive letter boundary', () => {
    const result = normalize(
      snapshot({ responses: ['C:\\tmp\\home\\config.toml', 'C:\\tmp\\tmp\\log.txt'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['C:<HOME>/config.toml', 'C:<TMP>/log.txt']);
  });

  it('does not replace homeDir when embedded in a path component', () => {
    const result = normalize(
      snapshot({ responses: ['/not_tmp/home/config.toml', 'file_in_/tmp/home/txt'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['/not_tmp/home/config.toml', 'file_in_/tmp/home/txt']);
  });

  it('ignores single-character dirs to avoid over-matching', () => {
    const result = normalize(snapshot({ responses: ['/'] }), { homeDir: '/', tmpDir: '/tmp/tmp' });
    expect(result.responses).toEqual(['/']);
  });

  it('replaces timestamp-ish long numbers', () => {
    const result = normalize(
      snapshot({ responses: [{ duration: 1719782400000 }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ duration: 0 }]);
  });

  it('replaces fixedIds placeholders', () => {
    const result = normalize(
      snapshot({ responses: ['seed-abc123'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp', fixedIds: new Map([['abc123', '<id:0>']]) },
    );
    expect(result.responses).toEqual(['seed-<id:0>']);
  });

  it('fixedIds take precedence over UUID replacement', () => {
    const uuid = '550e8400-e29b-41d4-a716-446655440000';
    const result = normalize(
      snapshot({ responses: [`session-${uuid}`] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp', fixedIds: new Map([[uuid, '<id:session>']]) },
    );
    expect(result.responses).toEqual(['session-<id:session>']);
  });

  it('fixedIds take precedence over timestamp replacement for non-UUID ids', () => {
    const stableTs = '1719782400000';
    const result = normalize(
      snapshot({ responses: [{ duration: `elapsed:${stableTs}ms` }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp', fixedIds: new Map([[stableTs, '<id:ts>']]) },
    );
    expect(result.responses).toEqual([{ duration: 'elapsed:<id:ts>ms' }]);
  });

  it('zeroes timestamp-ish number fields regardless of magnitude', () => {
    const result = normalize(
      snapshot({ responses: [{ duration: 1719782400 }, { duration: 1719782400000 }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ duration: 0 }, { duration: 0 }]);
  });

  it('replaces timestamp-ish substrings of 10+ digits', () => {
    const result = normalize(
      snapshot({ responses: [{ duration: 'elapsed:1719782400ms' }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ duration: 'elapsed:<ts>ms' }]);
  });

  it('keeps ordinary text with short numbers (must-survive)', () => {
    const result = normalize(
      snapshot({ responses: ['hello 12345 world', 'count:12345'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['hello 12345 world', 'count:12345']);
  });

  it('replaces exactly 10 digits but preserves 9 digits in timestamp-ish fields', () => {
    const result = normalize(
      snapshot({ responses: [{ duration: 'nine:123456789 ten:1234567890' }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ duration: 'nine:123456789 ten:<ts>' }]);
  });

  it('normalizes path separators in path-like fields', () => {
    const result = normalize(
      snapshot({ responses: [{ path: 'C:\\Users\\x\\file.txt' }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ path: 'C:/Users/x/file.txt' }]);
  });

  it('strips stack and absolute paths from error objects', () => {
    const result = normalize(
      snapshot({
        responses: [{
          code: 'E_TEST',
          kind: 'test',
          message: 'failed at /tmp/home/main.ts',
          stack: 'at /tmp/home/main.ts:1:1',
        }],
      }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{
      code: 'E_TEST',
      kind: 'test',
      message: 'failed at <HOME>/main.ts',
    }]);
  });

  it('leaves error objects without a message field unchanged', () => {
    const result = normalize(
      snapshot({ responses: [{ code: 'E_TEST', kind: 'test' }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ code: 'E_TEST', kind: 'test' }]);
  });

  it('strips stack even when message is absent', () => {
    const result = normalize(
      snapshot({ responses: [{ code: 'E_TEST', stack: 'at /tmp/home/main.ts:1:1' }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ code: 'E_TEST' }]);
  });

  it('merges consecutive assistant.delta events for the same turn', () => {
    const result = normalize(
      snapshot({
        events: [
          { type: 'turn.started', turnId: 1, origin: { kind: 'user' } },
          { type: 'assistant.delta', turnId: 1, delta: 'Hel' },
          { type: 'assistant.delta', turnId: 1, delta: 'lo' },
          { type: 'turn.ended', turnId: 1, reason: 'completed' },
        ] as any,
      }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.events).toHaveLength(3);
    expect((result.events[1] as any).delta).toBe('Hello');
    expect(result.meta).toEqual({ joinedDeltaCount: 1 });
  });

  it('does not merge assistant.delta events across different turns', () => {
    const result = normalize(
      snapshot({
        events: [
          { type: 'assistant.delta', turnId: 1, delta: 'first' },
          { type: 'assistant.delta', turnId: 2, delta: 'second' },
        ] as any,
      }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.events).toHaveLength(2);
    expect((result.events[0] as any).delta).toBe('first');
    expect((result.events[1] as any).delta).toBe('second');
    expect(result.meta).toBeUndefined();
  });

  it('maps Rust tool.call to TS tool.call.started', () => {
    const rust = [
      { type: 'tool.call', turnId: 1, toolCallId: 'c1', toolName: 'Read', args: { path: 'x' } },
    ];
    const out = normalizeTurnEvents(rust as any);
    expect(out[0]).toMatchObject({
      type: 'tool.call.started',
      turnId: 1,
      toolCallId: 'c1',
      name: 'Read',
      args: { path: 'x' },
    });
  });
});
