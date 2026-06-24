import { mkdtemp, readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { describe, expect, it } from 'vitest';
import { ChromeTraceRecorder } from '#/trace-recorder';
import type { MCPToolResult } from '#/types';

describe('ChromeTraceRecorder', () => {
  it('writes manifest.jsonl with tool call record', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'trace-test-'));
    const recorder = new ChromeTraceRecorder(dir);
    const result: MCPToolResult = {
      content: [{ type: 'text', text: 'navigated' }],
      isError: false,
    };
    await recorder.record('navigate', { url: 'https://example.com' }, result);

    const manifest = await readFile(join(dir, 'manifest.jsonl'), 'utf-8');
    const record = JSON.parse(manifest.trim());
    expect(record.toolName).toBe('navigate');
    expect(record.args.url).toBe('https://example.com');
    expect(record.resultSummary.isError).toBe(false);
    expect(record.resultSummary.contentTypes).toEqual(['text']);
  });

  it('extracts screenshot images into screenshots/ directory', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'trace-test-'));
    const recorder = new ChromeTraceRecorder(dir);
    // 1×1 transparent PNG base64
    const base64Png =
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
    const result: MCPToolResult = {
      content: [{ type: 'image', data: base64Png, mimeType: 'image/png' }],
      isError: false,
    };
    await recorder.record('take_screenshot', {}, result);

    const screenshots = await readdir(join(dir, 'screenshots'));
    expect(screenshots.length).toBe(1);
    expect(screenshots[0]).toMatch(/^0001-take_screenshot\.png$/);
  });

  it('redacts sensitive args before recording', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'trace-test-'));
    const recorder = new ChromeTraceRecorder(dir);
    const result: MCPToolResult = { content: [], isError: false };
    await recorder.record(
      'fill',
      { password: 'secret123', username: 'alice', apiKey: 'xyz' },
      result,
    );

    const manifest = await readFile(join(dir, 'manifest.jsonl'), 'utf-8');
    const record = JSON.parse(manifest.trim());
    expect(record.args.password).toBe('<redacted>');
    expect(record.args.apiKey).toBe('<redacted>');
    expect(record.args.username).toBe('alice');
  });

  it('redacts nested sensitive args', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'trace-test-'));
    const recorder = new ChromeTraceRecorder(dir);
    const result: MCPToolResult = { content: [], isError: false };
    await recorder.record(
      'fill',
      { credentials: { password: 'secret123', username: 'alice' }, url: 'https://example.com' },
      result,
    );

    const manifest = await readFile(join(dir, 'manifest.jsonl'), 'utf-8');
    const record = JSON.parse(manifest.trim());
    expect(record.args.credentials.password).toBe('<redacted>');
    expect(record.args.credentials.username).toBe('alice');
    expect(record.args.url).toBe('https://example.com');
  });

  it('survives write failures without throwing', async () => {
    const recorder = new ChromeTraceRecorder('/dev/null/invalid-path');
    const result: MCPToolResult = { content: [], isError: false };
    await expect(
      recorder.record('navigate', {}, result),
    ).resolves.not.toThrow();
  });
});
