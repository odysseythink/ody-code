import type { AgentEvent } from '@odysseythink/agent-core';
import type { NormalizedSnapshot, NormalizerOptions, ScenarioSnapshot } from './types';
import { normalizeTurnEvents } from './normalize-turn-events';

const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
const LONG_NUMBER_RE = /\d{10,}/g;

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

const STAT_METADATA_KEYS = new Set([
  'stIno', 'stDev', 'stNlink', 'stUid', 'stGid', 'stAtime', 'stMtime', 'stCtime',
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

function isStatMetadata(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return STAT_METADATA_KEYS.has(key);
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function normpath(value: string): string {
  return value.replace(/\\/g, '/');
}

function replaceDir(value: string, dir: string, placeholder: string): string {
  if (dir.length < 2) return value;
  // Normalize separators so POSIX homeDir can match Windows-style paths and vice versa.
  const normalizedDir = normpath(dir);
  if (normalizedDir.length < 2) return value;
  // Match the dir only when it is not embedded inside a path component.
  // Allowed preceding characters: start of string or any char that cannot be part of a file/path name.
  // Path separators (/ or \) are explicitly allowed as prefixes so /tmp/home matches /tmp/home/file.
  const pattern = normalizedDir.split('/').map(escapeRegExp).join('[\\\\/]');
  const prefix = '(^|(?<![\\w.-]))';
  const suffix = '(?![\\w.-])';
  const re = new RegExp(`${prefix}${pattern}${suffix}`, 'g');
  return value.replace(re, `$1${placeholder}`);
}

function normalizeString(value: string, options: NormalizerOptions, path: string): string {
  let s = value;
  // fixedIds must run before UUID replacement so that a UUID-shaped fixed id
  // is replaced by its stable placeholder (e.g. <id:0>) rather than <id>.
  if (options.fixedIds !== undefined) {
    for (const [id, placeholder] of options.fixedIds) {
      s = s.split(id).join(placeholder);
    }
  }

  s = replaceDir(s, options.homeDir, '<HOME>');
  s = replaceDir(s, options.tmpDir, '<TMP>');

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
  // Normalize path separators for explicit path fields and for any string that
  // already contained a home/tmp placeholder (which implies it is path-like).
  if (isPathLike(path) || s.includes('<HOME>') || s.includes('<TMP>')) {
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

function normalizeToolDefinitions(value: unknown[]): unknown[] {
  if (value.length === 0) return value;
  if (
    typeof value[0] === 'object' &&
    value[0] !== null &&
    ('source' in (value[0] as object) || 'active' in (value[0] as object))
  ) {
    const sorted = [...value].sort((a, b) => {
      const na = String((a as Record<string, unknown>)['name'] ?? '');
      const nb = String((b as Record<string, unknown>)['name'] ?? '');
      return na.localeCompare(nb);
    });
    return sorted.map((t) => {
      const item = t as Record<string, unknown>;
      const out: Record<string, unknown> = {};
      if (typeof item['name'] === 'string') out['name'] = item['name'];
      if (typeof item['active'] === 'boolean') out['active'] = item['active'];
      if (typeof item['source'] === 'string') out['source'] = item['source'];
      return out;
    });
  }
  return value;
}

function walk(value: unknown, options: NormalizerOptions, path: string): unknown {
  if (typeof value === 'string') {
    return normalizeString(value, options, path);
  }
  if (typeof value === 'number') {
    if (isTimestampish(path) || isStatMetadata(path)) return 0;
    return value;
  }
  if (Array.isArray(value)) {
    const mapped = value.map((item, i) => walk(item, options, `${path}[${i}]`));
    return normalizeToolDefinitions(mapped);
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
  const ignoreEventTypes = options.ignoreEventTypes;
  let events = walk(snapshot.events, options, '$.events') as AgentEvent[];
  if (ignoreEventTypes !== undefined) {
    events = events.filter((event) => !ignoreEventTypes.has(event.type));
  }
  // Normalize turn/tool event shapes between Rust and TS
  events = normalizeTurnEvents(events);
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

export { normalizeTurnEvents } from './normalize-turn-events';
