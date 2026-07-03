const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;

export interface NormalizedLoopSnapshot {
  readonly turnResult: unknown;
  readonly recordedEvents: unknown;
  readonly liveEvents: unknown;
}

function normalizeValue(value: unknown): unknown {
  if (typeof value === 'string') {
    return value.replace(UUID_RE, '<id>');
  }
  if (Array.isArray(value)) {
    return value.map((item) => normalizeValue(item));
  }
  if (value !== null && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(value)) {
      if (val !== undefined) {
        out[key] = normalizeValue(val);
      }
    }
    return out;
  }
  return value;
}

export function normalizeLoopSnapshot(snapshot: {
  readonly turnResult: unknown;
  readonly recordedEvents: unknown;
  readonly liveEvents: unknown;
}): NormalizedLoopSnapshot {
  return {
    turnResult: normalizeValue(snapshot.turnResult),
    recordedEvents: normalizeValue(snapshot.recordedEvents),
    liveEvents: normalizeValue(snapshot.liveEvents),
  };
}
