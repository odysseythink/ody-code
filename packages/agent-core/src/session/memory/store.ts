import { open, stat } from 'node:fs/promises';

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

const ANSI_PATTERN = /\u001b\[[0-9;]*m/g;

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
  return text.replace(/`/g, '\\`');
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
  if (!isRecord(rec) || typeof rec.type !== 'string') return;

  switch (rec.type) {
    case 'turn.prompt':
    case 'turn.steer': {
      const origin = rec.origin;
      if (!isRecord(origin) || origin.kind !== 'user') return;
      const input = rec.input;
      if (!Array.isArray(input)) return;
      const text = input
        .filter((part) => isRecord(part) && part.type === 'text' && typeof part.text === 'string')
        .map((part) => (part as { text: string }).text)
        .join(' ');
      const cleaned = stripAnsi(text).trim();
      if (cleaned.length === 0) return;
      state.userMessages.push(cleaned.slice(0, cfg.userMessageChars));
      state.totalUserMessages += 1;
      return;
    }
    case 'context.append_message': {
      const message = rec.message;
      if (!isRecord(message) || message.role !== 'assistant') return;
      const toolCalls = message.toolCalls;
      if (!Array.isArray(toolCalls)) return;
      for (const tc of toolCalls) {
        if (!isRecord(tc)) continue;
        const name = typeof tc.name === 'string' ? tc.name : '';
        if (name.length === 0) continue;
        pushUnique(state.toolsUsed, name);
        if (name === 'Edit' || name === 'Write' || name === 'NotebookEdit') {
          const argsText = tc.arguments;
          if (typeof argsText !== 'string') continue;
          let args: unknown;
          try {
            args = JSON.parse(argsText);
          } catch {
            continue;
          }
          if (isRecord(args) && typeof args.file_path === 'string') {
            pushUnique(state.filesModified, args.file_path);
          }
        }
      }
      return;
    }
    default:
      return;
  }
}
