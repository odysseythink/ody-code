import type { AgentEvent } from '@odysseythink/agent-core';
import type { NormalizedMeta, NormalizedSnapshot, NormalizerOptions, ScenarioSnapshot } from './types';

const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
const LONG_NUMBER_RE = /\d{13,}/g;

const TIMESTAMPISH_KEYS = new Set([
  'timestamp', 'time', 'startedAt', 'endedAt', 'duration', 'latency', 'hrtime',
  'llmFirstTokenLatencyMs', 'llmStreamDurationMs', 'createdAt', 'updatedAt',
]);

const PID_KEYS = new Set(['pid', 'processId', 'process_id']);
const PORT_KEYS = new Set(['port', 'tcpPort', 'listenPort']);
const PATH_KEYS = new Set([
  'path', 'file', 'dir', 'cwd', 'workDir', 'homeDir', 'tmpDir', 'socketPath',
  'sourceFilePath', 'outputPath', 'configPath',
]);

function isTimestampish(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return TIMESTAMPISH_KEYS.has(key);
}

function isPidLike(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return PID_KEYS.has(key);
}

function isPortLike(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return PORT_KEYS.has(key);
}

function isPathLike(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return PATH_KEYS.has(key);
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function replaceDir(value: string, dir: string, placeholder: string): string {
  if (dir.length < 2) return value;
  const re = new RegExp(`(^|[^\\w/-])${escapeRegExp(dir)}`, 'g');
  return value.replace(re, `$1${placeholder}`);
}

function normalizeString(value: string, options: NormalizerOptions, path: string): string {
  let s = value;
  s = replaceDir(s, options.homeDir, '<HOME>');
  s = replaceDir(s, options.tmpDir, '<TMP>');

  if (options.fixedIds !== undefined) {
    for (const [id, placeholder] of options.fixedIds) {
      s = s.split(id).join(placeholder);
    }
  }

  s = s.replace(UUID_RE, '<id>');

  if (isTimestampish(path)) {
    s = s.replace(LONG_NUMBER_RE, '<ts>');
  }
  if (isPidLike(path)) {
    s = s.replace(/\b\d{4,5}\b/g, '<pid>');
  }
  if (isPortLike(path)) {
    s = s.replace(/\b\d{1,5}\b/g, '<port>');
  }
  if (isPathLike(path)) {
    s = s.replace(/\\/g, '/');
  }
  return s;
}

function isErrorObject(value: unknown): value is { code: unknown; kind?: unknown; message?: string; stack?: string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    (value as Record<string, unknown>)['code'] !== undefined
  );
}

function normalizeError(
  value: { code: unknown; kind?: unknown; message?: string; stack?: string },
  options: NormalizerOptions,
  path: string,
): unknown {
  const out: Record<string, unknown> = { code: value['code'] };
  if ('kind' in value) out['kind'] = value['kind'];
  if (typeof value['message'] === 'string') {
    out['message'] = normalizeString(value['message'], options, `${path}.message`);
  }
  return out;
}

function joinAssistantDeltas(events: AgentEvent[]): { events: AgentEvent[]; joinedCount: number } {
  const result: AgentEvent[] = [];
  let joinedCount = 0;
  for (const event of events) {
    const prev = result[result.length - 1];
    if (
      event.type === 'assistant.delta' &&
      prev !== undefined &&
      prev.type === 'assistant.delta'
    ) {
      const prevDelta = prev as { turnId: number; delta: string };
      const nextDelta = event as { turnId: number; delta: string };
      if (prevDelta.turnId === nextDelta.turnId) {
        prevDelta.delta += nextDelta.delta;
        joinedCount++;
        continue;
      }
    }
    result.push(event);
  }
  return { events: result, joinedCount };
}

function walk(value: unknown, options: NormalizerOptions, path: string): unknown {
  if (typeof value === 'string') {
    return normalizeString(value, options, path);
  }
  if (typeof value === 'number') {
    if (isTimestampish(path)) return 0;
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item, i) => walk(item, options, `${path}[${i}]`));
  }
  if (isErrorObject(value)) {
    return normalizeError(value, options, path);
  }
  if (typeof value === 'object' && value !== null) {
    const out: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(value)) {
      out[key] = walk(val, options, `${path}.${key}`);
    }
    return out;
  }
  return value;
}

export function normalize(
  snapshot: ScenarioSnapshot,
  options: NormalizerOptions,
): NormalizedSnapshot {
  let events = walk(snapshot.events, options, '$.events') as AgentEvent[];
  const { events: joinedEvents, joinedCount } = joinAssistantDeltas(events);
  events = joinedEvents;

  const normalized: NormalizedSnapshot = {
    responses: walk(snapshot.responses, options, '$.responses') as unknown[],
    events,
    records: snapshot.records !== undefined
      ? (walk(snapshot.records, options, '$.records') as unknown[])
      : undefined,
    fsTree: snapshot.fsTree !== undefined
      ? walk(snapshot.fsTree, options, '$.fsTree')
      : undefined,
    meta: joinedCount > 0 ? { joinedDeltaCount: joinedCount } : undefined,
  };
  return normalized;
}
