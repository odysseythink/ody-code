import { describe, expect, it } from 'vitest';
import { appendFile, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'pathe';
import {
  scanWire,
  applyRecord,
  createEmptyScanState,
  renderSummary,
  writeSummary,
  DEFAULT_SESSION_MEMORY_CONFIG,
  SUMMARY_START,
  SUMMARY_END,
  type WireScanState,
  type SessionMetadata,
} from '../../src/session/memory/store';
import { encodeWorkDirKey } from '../../src/session/store/workdir-key';
import { SessionMemoryWriterBuiltin } from '../../src/session/hooks/builtin/session-memory-writer';
import { createBuiltinHookRegistry } from '../../src/session/hooks/builtin/registry';

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

describe('renderSummary', () => {
  it('renders the latest user messages and caps all sections', () => {
    const state: WireScanState = {
      offset: 0,
      partialLine: '',
      userMessages: Array.from({ length: 12 }, (_, i) => `task ${i + 1}`),
      toolsUsed: Array.from({ length: 25 }, (_, i) => `Tool${i + 1}`),
      filesModified: Array.from({ length: 35 }, (_, i) => `file${i + 1}.ts`),
      totalUserMessages: 12,
    };
    const meta: SessionMetadata = {
      startedAt: new Date('2026-07-13T14:02:00Z').getTime(),
      project: 'ody-code',
      branch: 'main',
      worktree: '/Users/ranwei/workspace/ody-code',
      sessionId: 'session_abc',
    };
    const rendered = renderSummary(state, meta, DEFAULT_SESSION_MEMORY_CONFIG);
    const tasksSection = section(rendered, '### Tasks', '### Files Modified');
    const filesSection = section(rendered, '### Files Modified', '### Tools Used');
    const toolsSection = section(rendered, '### Tools Used', '### Stats');
    expect(tasksSection.match(/^- /gm) ?? []).toHaveLength(10);
    expect(filesSection.match(/^- /gm) ?? []).toHaveLength(30);
    expect(toolsSection.match(/^- /gm) ?? []).toHaveLength(20);
    expect(rendered).toContain('- task 12');
    expect(rendered).toContain('- task 3');
    expect(rendered).not.toContain('- task 2');
    expect(rendered).toContain('Tools Used');
    expect(rendered).toContain('Files Modified');
    expect(rendered).toContain('Total user messages: 12');
    expect(rendered).toContain('**Project:** ody-code');
    expect(rendered).toContain('**Branch:** main');
    expect(rendered).toContain(SUMMARY_START);
    expect(rendered).toContain(SUMMARY_END);
  });

  it('escapes backticks in user messages', () => {
    const state = createEmptyScanState();
    state.userMessages.push('run `git status`');
    state.totalUserMessages = 1;
    const meta: SessionMetadata = {
      startedAt: Date.now(),
      project: 'p',
      branch: 'b',
      worktree: '/w',
      sessionId: 's',
    };
    const rendered = renderSummary(state, meta);
    expect(rendered).toContain('- run \\`git status\\`');
  });
});

describe('writeSummary', () => {
  it('writes summary.md with mode 0600 on first call', async () => {
    const dir = await makeTmpDir();
    const content = renderSummary(
      { ...createEmptyScanState(), userMessages: ['x'], totalUserMessages: 1 },
      { startedAt: Date.now(), project: 'p', branch: 'b', worktree: '/w', sessionId: 's' },
    );
    await writeSummary(dir, content);
    const file = join(dir, 'summary.md');
    expect(await readFile(file, 'utf8')).toContain('## Auto Summary');
    expect((await stat(file)).mode & 0o777).toBe(0o600);
  });

  it('preserves handwritten content and freezes Date/Started on update', async () => {
    const dir = await makeTmpDir();
    const old = renderSummary(
      { ...createEmptyScanState(), userMessages: ['old'], totalUserMessages: 1 },
      { startedAt: new Date('2026-07-01T10:00:00Z').getTime(), project: 'p', branch: 'b', worktree: '/w', sessionId: 's' },
    );
    const oldDate = /\*\*Date:\*\* (.+)/.exec(old)?.[1];
    const oldStarted = /\*\*Started:\*\* (.+)/.exec(old)?.[1];
    await writeSummary(dir, old);
    const file = join(dir, 'summary.md');
    await appendFile(file, '\n\n## Handwritten Notes\nKeep this.\n');

    await new Promise((r) => setTimeout(r, 10));
    const updated = renderSummary(
      { ...createEmptyScanState(), userMessages: ['new'], totalUserMessages: 2 },
      { startedAt: Date.now(), project: 'p', branch: 'b', worktree: '/w', sessionId: 's' },
    );
    await writeSummary(dir, updated);
    const text = await readFile(file, 'utf8');
    expect(text).toContain('Keep this.');
    expect(text).toContain(`**Date:** ${oldDate}`);
    expect(text).toContain(`**Started:** ${oldStarted}`);
    expect(text).toContain('Total user messages: 2');
  });

  it('uses a function replacer so $& in user text is not corrupted', async () => {
    const dir = await makeTmpDir();
    const content = renderSummary(
      { ...createEmptyScanState(), userMessages: ['cost $& and `tick`'], totalUserMessages: 1 },
      { startedAt: Date.now(), project: 'p', branch: 'b', worktree: '/w', sessionId: 's' },
    );
    await writeSummary(dir, content);
    await writeSummary(dir, content);
    const text = await readFile(join(dir, 'summary.md'), 'utf8');
    expect(text).toContain('cost $& and \\`tick\\`');
    expect(text.split(SUMMARY_START)).toHaveLength(2);
    expect(text.split(SUMMARY_END)).toHaveLength(2);
  });

  it('rebuilds corrupted files that lack markers', async () => {
    const dir = await makeTmpDir();
    await writeFile(join(dir, 'summary.md'), '# Handwritten only\nno markers.\n', 'utf8');
    const content = renderSummary(
      { ...createEmptyScanState(), userMessages: ['x'], totalUserMessages: 1 },
      { startedAt: Date.now(), project: 'p', branch: 'b', worktree: '/w', sessionId: 's' },
    );
    await writeSummary(dir, content);
    const text = await readFile(join(dir, 'summary.md'), 'utf8');
    expect(text).toContain(SUMMARY_START);
    expect(text).toContain('## Auto Summary');
  });
});

async function makeSessionTree(homeDir: string, cwd: string, sessionId: string) {
  const wdKey = encodeWorkDirKey(cwd);
  const sessionDir = join(homeDir, 'sessions', wdKey, sessionId);
  const wireDir = join(sessionDir, 'agents', 'main');
  await mkdir(wireDir, { recursive: true });
  return { sessionDir, wirePath: join(wireDir, 'wire.jsonl') };
}

function metadataRecord(createdAt: number) {
  return { type: 'metadata', protocol_version: '1', created_at: createdAt };
}

describe('SessionMemoryWriterBuiltin', () => {
  it('writes summary.md on Stop and returns allow', async () => {
    const homeDir = join(tmpdir(), `memory-writer-${Date.now()}`);
    const cwd = join(tmpdir(), `cwd-${Date.now()}`);
    const sessionId = `session_${Date.now()}`;
    const { sessionDir, wirePath } = await makeSessionTree(homeDir, cwd, sessionId);
    const createdAt = new Date('2026-07-13T14:02:00Z').getTime();
    await writeFile(
      wirePath,
      [
        metadataRecord(createdAt),
        { type: 'turn.prompt', input: [{ type: 'text', text: 'hello world' }], origin: { kind: 'user' } },
      ]
        .map((r) => JSON.stringify(r))
        .join('\n') + '\n',
      'utf8',
    );

    const writer = new SessionMemoryWriterBuiltin();
    const result = await writer.run(
      { session_id: sessionId },
      { cwd, env: { ODY_CODE_HOME: homeDir }, timeout: 30 },
    );

    expect(result.action).toBe('allow');
    const summary = await readFile(join(sessionDir, 'summary.md'), 'utf8');
    expect(summary).toContain('## Auto Summary');
    expect(summary).toContain('- hello world');
    expect(summary).toContain('Total user messages: 1');
    const started = new Date(createdAt);
    const expectedTime = `${String(started.getHours()).padStart(2, '0')}:${String(started.getMinutes()).padStart(2, '0')}`;
    expect(summary).toContain(`**Started:** ${expectedTime}`);
  });

  it('increments state across multiple runs without re-scanning from offset zero', async () => {
    const homeDir = join(tmpdir(), `memory-writer-inc-${Date.now()}`);
    const cwd = join(tmpdir(), `cwd-inc-${Date.now()}`);
    const sessionId = `session_${Date.now()}`;
    const { sessionDir, wirePath } = await makeSessionTree(homeDir, cwd, sessionId);
    const writer = new SessionMemoryWriterBuiltin();

    await writeFile(
      wirePath,
      [metadataRecord(Date.now()), userPrompt('first')].map((r) => JSON.stringify(r)).join('\n') + '\n',
      'utf8',
    );
    await writer.run({ session_id: sessionId }, { cwd, env: { ODY_CODE_HOME: homeDir }, timeout: 30 });

    await writeFile(wirePath, JSON.stringify(userPrompt('second')) + '\n', { flag: 'a' });
    await writer.run({ session_id: sessionId }, { cwd, env: { ODY_CODE_HOME: homeDir }, timeout: 30 });

    const summary = await readFile(join(sessionDir, 'summary.md'), 'utf8');
    expect(summary).toContain('- first');
    expect(summary).toContain('- second');
    expect(summary).toContain('Total user messages: 2');
  });

  it('skips writing when there are no user messages', async () => {
    const homeDir = join(tmpdir(), `memory-writer-empty-${Date.now()}`);
    const cwd = join(tmpdir(), `cwd-empty-${Date.now()}`);
    const sessionId = `session_${Date.now()}`;
    const { sessionDir, wirePath } = await makeSessionTree(homeDir, cwd, sessionId);
    await writeFile(wirePath, JSON.stringify(metadataRecord(Date.now())) + '\n', 'utf8');

    const writer = new SessionMemoryWriterBuiltin();
    const result = await writer.run(
      { session_id: sessionId },
      { cwd, env: { ODY_CODE_HOME: homeDir }, timeout: 30 },
    );

    expect(result.action).toBe('allow');
    await expect(readFile(join(sessionDir, 'summary.md'), 'utf8')).rejects.toThrow();
  });

  it('always returns allow even when the session directory does not exist', async () => {
    const writer = new SessionMemoryWriterBuiltin();
    const result = await writer.run(
      { session_id: 'session_missing' },
      { cwd: '/nonexistent/cwd', env: {}, timeout: 30 },
    );
    expect(result.action).toBe('allow');
  });

  it('is registered in the builtin hook registry', () => {
    const registry = createBuiltinHookRegistry();
    const builtin = registry.get('session-memory-writer');
    expect(builtin).toBeDefined();
    expect(builtin?.id).toBe('session-memory-writer');
  });
});

function section(rendered: string, from: string, to: string): string {
  return rendered.slice(rendered.indexOf(from), rendered.indexOf(to));
}

async function makeTmpDir(): Promise<string> {  const dir = join(tmpdir(), `session-memory-${Date.now()}-${Math.random().toString(36).slice(2)}`);
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
