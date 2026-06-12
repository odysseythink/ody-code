import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { SessionMarkdownExport } from '../../../src/session/export/markdown-export';

describe('SessionMarkdownExport', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'markdown-export-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  function messageRecord(role: string, text: string, time?: number) {
    return {
      type: 'context.append_message' as const,
      time,
      message: {
        role,
        content: [{ type: 'text' as const, text }],
      },
    };
  }

  it('appends a message to a new markdown file', async () => {
    const filePath = join(workDir, 'session.md');
    const exporter = new SessionMarkdownExport({ filePath });

    await exporter.append(messageRecord('user', 'hello', 1718182800000));

    const content = await readFile(filePath, 'utf8');
    expect(content).toContain('role: user');
    expect(content).toContain('hello');
    expect(content).toContain('2024-06-12T09:00:00.000Z');
  });

  it('appends multiple messages without overwriting', async () => {
    const filePath = join(workDir, 'session.md');
    const exporter = new SessionMarkdownExport({ filePath });

    await exporter.append(messageRecord('user', 'first'));
    await exporter.append(messageRecord('assistant', 'second'));

    const content = await readFile(filePath, 'utf8');
    expect(content.indexOf('first')).toBeLessThan(content.indexOf('second'));
    expect(content.match(/role: user/g)).toHaveLength(1);
    expect(content.match(/role: assistant/g)).toHaveLength(1);
  });

  it('serializes concurrent appends', async () => {
    const filePath = join(workDir, 'session.md');
    const exporter = new SessionMarkdownExport({ filePath });

    await Promise.all(
      Array.from({ length: 10 }, (_, i) =>
        exporter.append(messageRecord('user', `msg-${i}`)),
      ),
    );

    const content = await readFile(filePath, 'utf8');
    for (let i = 0; i < 10; i += 1) {
      expect(content).toContain(`msg-${i}`);
    }
  });

  it('increments errorCount and reports onError without throwing', async () => {
    // Make the target path a directory so appendFile fails with EISDIR.
    const filePath = join(workDir, 'blocker');
    await mkdir(filePath);
    const errors: unknown[] = [];
    const exporter = new SessionMarkdownExport({
      filePath,
      onError: (error) => errors.push(error),
    });

    await exporter.append(messageRecord('user', 'hello'));

    expect(exporter.errorCount).toBe(1);
    expect(errors).toHaveLength(1);
  });

  it('renders non-text parts as a JSON code block', async () => {
    const filePath = join(workDir, 'session.md');
    const exporter = new SessionMarkdownExport({ filePath });

    await exporter.append({
      type: 'context.append_message',
      message: {
        role: 'assistant',
        content: [
          { type: 'text', text: 'look at this' },
          { type: 'image', url: 'data:image/png;base64,ABC' },
        ],
      },
    });

    const content = await readFile(filePath, 'utf8');
    expect(content).toContain('look at this');
    expect(content).toContain('```json');
    expect(content).toContain('data:image/png;base64,ABC');
  });
});
