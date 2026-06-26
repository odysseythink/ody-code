import { describe, expect, it } from 'vitest';
import type { ScenarioSnapshot } from '../../src/parity/types';
import { normalize } from '../../src/parity/normalize';

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

  it('keeps ordinary text with short numbers (must-survive)', () => {
    const result = normalize(
      snapshot({ responses: ['hello 12345 world', 'count:12345'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['hello 12345 world', 'count:12345']);
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
});
