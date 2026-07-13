import { describe, expect, it } from 'vitest';
import { appendFile, mkdir, writeFile } from 'node:fs/promises';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'pathe';
import {
  scanWire,
  applyRecord,
  createEmptyScanState,
  DEFAULT_SESSION_MEMORY_CONFIG,
} from '../../src/session/memory/store';

function userPrompt(text: string) {
  return {
    type: 'turn.prompt',
    input: [{ type: 'text', text }],
    origin: { kind: 'user' },
  };
}

function assistantToolCall(name: string, args?: Record<string, unknown>) {
  return {
    type: 'context.append_message',
    message: {
      role: 'assistant',
      toolCalls:
        args === undefined
          ? [{ type: 'function', id: '1', name }]
          : [{ type: 'function', id: '1', name, arguments: JSON.stringify(args) }],
    },
  };
}

describe('scanWire / applyRecord', () => {
  it('extracts user messages, tools, and file edits from mixed records', async () => {
    const state = createEmptyScanState();
    const records = [
      { type: 'metadata', protocol_version: '1', created_at: Date.now() },
      userPrompt('hello'),
      userPrompt('run tests'),
      {
        type: 'turn.prompt',
        input: [{ type: 'text', text: 'skill' }],
        origin: { kind: 'skill_activation', activationId: '1', skillName: 'x', trigger: 'user-slash' },
      },
      assistantToolCall('Edit', { file_path: 'src/a.ts' }),
      assistantToolCall('Write', { file_path: 'src/b.ts' }),
      assistantToolCall('Bash'),
    ];
    await scanWireFromRecords(records, state);
    expect(state.userMessages).toEqual(['hello', 'run tests']);
    expect(state.totalUserMessages).toBe(2);
    expect(state.toolsUsed).toEqual(['Edit', 'Write', 'Bash']);
    expect(state.filesModified).toEqual(['src/a.ts', 'src/b.ts']);
  });

  it('rejects non-user prompt origins', () => {
    const origins = [
      { kind: 'injection', variant: 'memory_summary' },
      { kind: 'hook_result', event: 'Stop' },
      { kind: 'cron_job', jobId: '1', cron: '* * * * *', recurring: true, coalescedCount: 1, stale: false },
      { kind: 'system_trigger', name: 'init' },
    ];
    for (const origin of origins) {
      const state = createEmptyScanState();
      applyRecord(
        { type: 'turn.prompt', input: [{ type: 'text', text: 'x' }], origin },
        state,
        DEFAULT_SESSION_MEMORY_CONFIG,
      );
      expect(state.totalUserMessages).toBe(0);
    }
  });

  it('truncates user messages to the configured length', async () => {
    const state = createEmptyScanState();
    const longText = 'a'.repeat(250);
    await scanWireFromRecords([userPrompt(longText)], state);
    expect(state.userMessages[0]).toHaveLength(200);
  });

  it('scans incrementally without duplicates', async () => {
    const state = createEmptyScanState();
    const dir = await makeTmpDir();
    const wirePath = join(dir, 'wire.jsonl');
    await writeRecords(wirePath, [userPrompt('one'), userPrompt('two')]);
    await scanWire(wirePath, state);
    const offsetAfterFirst = state.offset;
    await appendFile(wirePath, JSON.stringify(userPrompt('three')) + '\n', 'utf8');
    await scanWire(wirePath, state);
    expect(state.offset).toBeGreaterThan(offsetAfterFirst);
    expect(state.userMessages).toEqual(['one', 'two', 'three']);
  });

  it('buffers a partial final line across scans', async () => {
    const state = createEmptyScanState();
    const dir = await makeTmpDir();
    const wirePath = join(dir, 'wire.jsonl');
    const record = JSON.stringify(userPrompt('partial'));
    await writeFile(wirePath, record.slice(0, record.length - 5), 'utf8');
    await scanWire(wirePath, state);
    expect(state.userMessages).toEqual([]);
    expect(state.partialLine.length).toBeGreaterThan(0);
    await appendFile(wirePath, record.slice(record.length - 5) + '\n', 'utf8');
    await scanWire(wirePath, state);
    expect(state.userMessages).toEqual(['partial']);
  });

  it('skips bad JSON lines and reports the count', async () => {
    const state = createEmptyScanState();
    const dir = await makeTmpDir();
    const wirePath = join(dir, 'wire.jsonl');
    await writeFile(wirePath, 'bad\n' + JSON.stringify(userPrompt('ok')) + '\nmore bad\n', 'utf8');
    const { badLines } = await scanWire(wirePath, state);
    expect(badLines).toBe(2);
    expect(state.userMessages).toEqual(['ok']);
  });

  it('counts tool name but skips file path when Edit arguments are not valid JSON', () => {
    const state = createEmptyScanState();
    applyRecord(
      {
        type: 'context.append_message',
        message: {
          role: 'assistant',
          toolCalls: [{ type: 'function', id: '1', name: 'Edit', arguments: 'not json' }],
        },
      },
      state,
      DEFAULT_SESSION_MEMORY_CONFIG,
    );
    expect(state.toolsUsed).toEqual(['Edit']);
    expect(state.filesModified).toEqual([]);
  });
});

async function makeTmpDir(): Promise<string> {
  const dir = join(tmpdir(), `session-memory-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  await mkdir(dir, { recursive: true });
  return dir;
}

async function writeRecords(wirePath: string, records: unknown[]): Promise<void> {
  await mkdir(dirname(wirePath), { recursive: true });
  await writeFile(
    wirePath,
    records.map((r) => JSON.stringify(r)).join('\n') + '\n',
    'utf8',
  );
}

function scanWireFromRecords(records: unknown[], state: ReturnType<typeof createEmptyScanState>) {
  const text = records.map((r) => JSON.stringify(r)).join('\n') + '\n';
  const wirePath = join(tmpdir(), `scan-${Date.now()}-${Math.random().toString(36).slice(2)}.jsonl`);
  writeFileSync(wirePath, text, 'utf8');
  return scanWire(wirePath, state);
}
