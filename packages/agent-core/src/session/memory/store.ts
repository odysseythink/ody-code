import { execFile } from 'node:child_process';
import { chmod, mkdir, open, readFile, rename, stat, writeFile } from 'node:fs/promises';
import { basename, join } from 'pathe';

export interface WireScanState {
  offset: number;
  partialLine: string;
  userMessages: string[];
  toolsUsed: string[];
  filesModified: string[];
  totalUserMessages: number;
}

export interface SessionMetadata {
  startedAt: number;
  project: string;
  branch: string;
  worktree: string;
  sessionId: string;
}

export interface SessionMemoryConfig {
  maxUserMessages: number;
  maxTools: number;
  maxFiles: number;
  userMessageChars: number;
}

export const SUMMARY_START = '<!-- ODY:SUMMARY:START -->';
export const SUMMARY_END = '<!-- ODY:SUMMARY:END -->';

export const DEFAULT_SESSION_MEMORY_CONFIG: SessionMemoryConfig = {
  maxUserMessages: 10,
  maxTools: 20,
  maxFiles: 30,
  userMessageChars: 200,
};

export function createEmptyScanState(): WireScanState {
  return {
    offset: 0,
    partialLine: '',
    userMessages: [],
    toolsUsed: [],
    filesModified: [],
    totalUserMessages: 0,
  };
}

const ANSI_PATTERN = /\u001B\[[0-9;]*m/g;

function stripAnsi(text: string): string {
  return text.replace(ANSI_PATTERN, '');
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function pushUnique<T>(list: T[], value: T): void {
  if (!list.includes(value)) list.push(value);
}

export async function scanWire(
  wirePath: string,
  state: WireScanState,
  cfg: SessionMemoryConfig = DEFAULT_SESSION_MEMORY_CONFIG,
): Promise<{ badLines: number }> {
  let size: number;
  try {
    size = (await stat(wirePath)).size;
  } catch {
    return { badLines: 0 };
  }

  if (size < state.offset) {
    state.offset = 0;
    state.partialLine = '';
  }
  if (size === state.offset) return { badLines: 0 };

  const fh = await open(wirePath, 'r');
  try {
    const buffer = Buffer.allocUnsafe(size - state.offset);
    await fh.read(buffer, 0, buffer.length, state.offset);
    const text = state.partialLine + buffer.toString('utf8');
    const lines = text.split('\n');
    if (text.endsWith('\n')) {
      state.partialLine = '';
    } else {
      state.partialLine = lines.pop() ?? '';
    }
    state.offset = size;

    let badLines = 0;
    for (const line of lines) {
      if (line.length === 0) continue;
      let rec: unknown;
      try {
        rec = JSON.parse(line);
      } catch {
        badLines++;
        continue;
      }
      applyRecord(rec, state, cfg);
    }
    return { badLines };
  } finally {
    await fh.close();
  }
}

export function renderSummary(
  state: WireScanState,
  meta: SessionMetadata,
  cfg: SessionMemoryConfig = DEFAULT_SESSION_MEMORY_CONFIG,
): string {
  const date = formatDate(meta.startedAt);
  const started = formatTime(meta.startedAt);
  const updated = formatTime(Date.now());
  const tasks = state.userMessages.slice(-cfg.maxUserMessages);
  const tools = state.toolsUsed.slice(0, cfg.maxTools);
  const files = state.filesModified.slice(0, cfg.maxFiles);

  const lines: string[] = [
    `# Session Summary: ${date}`,
    `**Date:** ${date}`,
    `**Started:** ${started}`,
    `**Last Updated:** ${updated}`,
    `**Project:** ${meta.project}`,
    `**Branch:** ${meta.branch}`,
    `**Worktree:** ${meta.worktree}`,
    `**Session:** ${meta.sessionId}`,
    '',
    '---',
    '',
    SUMMARY_START,
    '## Auto Summary',
    '',
    '### Tasks',
    ...tasks.map((t) => `- ${escapeBackticks(t)}`),
    '',
    '### Files Modified',
    ...files.map((f) => `- ${f}`),
    '',
    '### Tools Used',
    ...tools.map((t) => `- ${t}`),
    '',
    '### Stats',
    `- Total user messages: ${state.totalUserMessages}`,
    SUMMARY_END,
  ];
  return lines.join('\n') + '\n';
}

function escapeBackticks(text: string): string {
  return text.replaceAll('`', '\\`');
}

function formatDate(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

export function applyRecord(
  rec: unknown,
  state: WireScanState,
  cfg: SessionMemoryConfig,
): void {
  if (!isRecord(rec) || typeof rec['type'] !== 'string') return;

  switch (rec['type']) {
    case 'turn.prompt':
    case 'turn.steer': {
      const origin = rec['origin'];
      if (!isRecord(origin) || origin['kind'] !== 'user') return;
      const input = rec['input'];
      if (!Array.isArray(input)) return;
      const text = input
        .filter(
          (part) => isRecord(part) && part['type'] === 'text' && typeof part['text'] === 'string',
        )
        .map((part) => (part as { text: string }).text)
        .join(' ');
      const cleaned = stripAnsi(text).trim();
      if (cleaned.length === 0) return;
      state.userMessages.push(cleaned.slice(0, cfg.userMessageChars));
      state.totalUserMessages += 1;
      return;
    }
    case 'context.append_message': {
      const message = rec['message'];
      if (!isRecord(message) || message['role'] !== 'assistant') return;
      const toolCalls = message['toolCalls'];
      if (!Array.isArray(toolCalls)) return;
      for (const tc of toolCalls) {
        if (!isRecord(tc)) continue;
        const name = typeof tc['name'] === 'string' ? tc['name'] : '';
        if (name.length === 0) continue;
        pushUnique(state.toolsUsed, name);
        if (name === 'Edit' || name === 'Write' || name === 'NotebookEdit') {
          const argsText = tc['arguments'];
          if (typeof argsText !== 'string') continue;
          let args: unknown;
          try {
            args = JSON.parse(argsText);
          } catch {
            continue;
          }
          if (isRecord(args) && typeof args['file_path'] === 'string') {
            pushUnique(state.filesModified, args['file_path']);
          }
        }
      }
      return;
    }
    default:
      return;
  }
}

export async function writeSummary(sessionDir: string, content: string): Promise<boolean> {
  const target = join(sessionDir, 'summary.md');
  await mkdir(sessionDir, { recursive: true, mode: 0o700 });

  let final = content;
  try {
    const old = await readFile(target, 'utf8');
    if (old.includes(SUMMARY_START) && old.includes(SUMMARY_END)) {
      final = rebuildSummary(old, content);
    }
  } catch {
    // Missing or unreadable file: use the freshly rendered content.
  }

  const tmp = `${target}.tmp-${process.pid}`;
  await writeFile(tmp, final, { mode: 0o600 });
  await rename(tmp, target);
  await chmod(target, 0o600);
  return true;
}

function rebuildSummary(old: string, newContent: string): string {
  const oldDate = extractHeaderField(old, 'Date');
  const oldStarted = extractHeaderField(old, 'Started');

  const newHeaderEnd = newContent.indexOf('\n---\n');
  if (newHeaderEnd < 0) return newContent;
  let header = newContent.slice(0, newHeaderEnd);
  if (oldDate !== undefined) header = setHeaderField(header, 'Date', oldDate);
  if (oldStarted !== undefined) header = setHeaderField(header, 'Started', oldStarted);

  const newBlock = extractBlock(newContent);
  if (newBlock === null) return newContent;

  const oldBodyStart = old.indexOf('---\n');
  if (oldBodyStart < 0) return newContent;
  const oldBody = old.slice(oldBodyStart + 4);
  const replacedBody = oldBody.replace(
    new RegExp(`${escapeRegex(SUMMARY_START)}[\\s\\S]*?${escapeRegex(SUMMARY_END)}\\n?`),
    () => newBlock,
  );
  return `${header.trimEnd()}\n\n---\n${replacedBody.replace(/^\n+/, '')}`;
}

function extractBlock(content: string): string | null {
  const match = content.match(
    new RegExp(`${escapeRegex(SUMMARY_START)}[\\s\\S]*?${escapeRegex(SUMMARY_END)}\\n?`),
  );
  return match?.[0] ?? null;
}

function extractHeaderField(header: string, name: string): string | undefined {
  const match = new RegExp(`^\\*\\*${name}:\\*\\* (.+)$`, 'm').exec(header);
  return match?.[1];
}

function setHeaderField(header: string, name: string, value: string): string {
  return header.replace(new RegExp(`^(\\*\\*${name}:\\*\\* ).+$`, 'm'), `$1${value}`);
}

function escapeRegex(text: string): string {
  return text.replaceAll(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

const GIT_TIMEOUT_MS = 5_000;

export async function collectMetadata(
  cwd: string,
  sessionId: string,
  startedAt: number,
): Promise<SessionMetadata> {
  const [project, branch] = await Promise.all([
    runGit(cwd, ['rev-parse', '--show-toplevel']).then((out) =>
      out !== null ? basename(out) : 'unknown',
    ),
    runGit(cwd, ['rev-parse', '--abbrev-ref', 'HEAD']).then((out) => out ?? 'unknown'),
  ]);
  return {
    startedAt,
    project,
    branch,
    worktree: cwd,
    sessionId,
  };
}

function runGit(cwd: string, args: readonly string[]): Promise<string | null> {
  return new Promise((resolve) => {
    execFile(
      'git',
      ['-C', cwd, ...args],
      { encoding: 'utf8', signal: AbortSignal.timeout(GIT_TIMEOUT_MS) },
      (error, stdout) => {
        resolve(error ? null : stdout.trim());
      },
    );
  });
}
