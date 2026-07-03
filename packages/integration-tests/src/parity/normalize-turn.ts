const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;

const TURN_EVENT_TYPES = new Set([
  'turn.started',
  'turn.step.started',
  'turn.step.completed',
  'turn.ended',
  'tool.call.started',
  'tool.result',
]);

const TURN_RECORD_TYPES = new Set(['turn.prompt', 'turn.steer', 'turn.cancel']);

const TELEMETRY_EVENT_NAMES = new Set(['turn_started', 'tool_call']);

const TELEMETRY_PROPS = new Set([
  'tool_name',
  'outcome',
  'duration_ms',
  'error_type',
]);

const USAGE_NUMERIC_FIELDS = new Set([
  'inputCacheCreation',
  'inputCacheRead',
  'inputOther',
  'output',
  'tokensUsed',
  'turnsUsed',
  'contextTokens',
  'maxContextTokens',
  'contextUsage',
]);

const DURATION_FIELDS = new Set([
  'duration_ms',
  'llmFirstTokenLatencyMs',
  'llmStreamDurationMs',
]);

const TIME_FIELDS = new Set([
  'time',
  'createdAt',
  'updatedAt',
  'wallClockResumedAt',
]);

const ID_FIELDS = new Set([
  'stepId',
  'stepUuid',
  'uuid',
  'toolCallId',
  'goalId',
]);

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function normalizeString(value: string): string {
  return value.replace(UUID_RE, '<id>');
}

function normalizeValue(key: string | undefined, value: unknown): unknown {
  if (key !== undefined && TIME_FIELDS.has(key)) {
    return '<time>';
  }
  if (key !== undefined && DURATION_FIELDS.has(key)) {
    return 0;
  }
  if (key !== undefined && USAGE_NUMERIC_FIELDS.has(key) && typeof value === 'number') {
    return 0;
  }
  if (key !== undefined && ID_FIELDS.has(key) && typeof value === 'string') {
    return normalizeString(value);
  }
  if (typeof value === 'string') {
    return normalizeString(value);
  }
  if (Array.isArray(value)) {
    return value.map((item) => normalizeValue(undefined, item));
  }
  if (isObject(value)) {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      if (v !== undefined) {
        out[k] = normalizeValue(k, v);
      }
    }
    return out;
  }
  return value;
}

function canonicalizeTurnId(value: unknown, side: 'ts' | 'rust'): unknown {
  if (typeof value !== 'number') return value;
  return side === 'rust' ? value - 1 : value;
}

function canonicalizeEvents(events: unknown, side: 'ts' | 'rust'): unknown {
  if (!Array.isArray(events)) return events;
  const out: unknown[] = [];
  for (const raw of events) {
    if (!isObject(raw)) continue;
    const type = raw['type'];
    if (typeof type !== 'string') continue;
    if (side === 'ts' && (type === 'agent.status.updated' || type === 'tool.call.delta')) {
      continue;
    }
    if (!TURN_EVENT_TYPES.has(type)) continue;

    const ev: Record<string, unknown> = { type };
    const add = (key: string, rawKey?: string) => {
      const k = rawKey ?? key;
      let v = raw[k];
      if (v === undefined) return;
      if (key === 'turnId') {
        v = canonicalizeTurnId(v, side);
      }
      ev[key] = v;
    };

    if (type === 'turn.started') {
      add('turnId');
      add('origin');
    } else if (type === 'turn.step.started') {
      add('turnId');
      add('step');
      add('stepId');
    } else if (type === 'turn.step.completed') {
      add('turnId');
      add('step');
      add('stepId');
      add('finishReason');
      add('usage');
      ev['llmFirstTokenLatencyMs'] = 0;
      ev['llmStreamDurationMs'] = 0;
    } else if (type === 'turn.ended') {
      add('turnId');
      add('reason');
    } else if (type === 'tool.call.started') {
      add('turnId');
      add('toolCallId');
      add('name');
      add('args');
    } else if (type === 'tool.result') {
      add('turnId');
      add('toolCallId');
      add('output');
      add('isError');
    }
    out.push(ev);
  }
  return out;
}

function canonicalizeRecords(records: unknown): unknown {
  if (!Array.isArray(records)) return records;
  return records
    .filter(isObject)
    .filter((r) => TURN_RECORD_TYPES.has(r['type'] as string))
    .map((r) => {
      const out: Record<string, unknown> = { type: r['type'] };
      if (r['input'] !== undefined) out['input'] = r['input'];
      if (r['origin'] !== undefined) out['origin'] = r['origin'];
      if (r['time'] !== undefined) out['time'] = r['time'];
      return out;
    });
}

function canonicalizeTelemetry(telemetry: unknown): unknown {
  if (!Array.isArray(telemetry)) return telemetry;
  return telemetry
    .filter(isObject)
    .filter((t) => TELEMETRY_EVENT_NAMES.has(t['event'] as string))
    .map((t) => {
      const props = isObject(t['properties']) ? t['properties'] : {};
      const filtered: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(props)) {
        if (TELEMETRY_PROPS.has(k)) {
          filtered[k] = k === 'duration_ms' ? 0 : v;
        }
      }
      return { event: t['event'], properties: filtered };
    });
}

function canonicalizeTurns(
  turns: unknown,
  events: unknown,
  side: 'ts' | 'rust',
): unknown {
  if (!Array.isArray(turns)) return turns;
  const eventList = Array.isArray(events) ? events.filter(isObject) : [];
  return turns.map((t) => {
    if (!isObject(t)) return t;
    let turnId: unknown;
    let reason: unknown;
    let stopReason: unknown;
    let blockedByUserPromptHook = false;
    if (side === 'rust') {
      turnId = canonicalizeTurnId(t['turn_id'], side);
      reason = t['reason'];
      stopReason = t['stop_reason'];
      blockedByUserPromptHook = (t['blocked_by_user_prompt_hook'] as boolean | undefined) ?? false;
    } else {
      turnId = canonicalizeTurnId(t['turnId'], side);
      reason = t['reason'];
      const stepCompleted = eventList
        .filter(
          (e) =>
            e['type'] === 'turn.step.completed' && e['turnId'] === turnId,
        )
        .pop();
      stopReason = stepCompleted?.['finishReason'];
    }
    return {
      turnId,
      reason,
      stopReason,
      blockedByUserPromptHook,
    };
  });
}

function isTextPart(value: unknown): value is { type: 'text'; text: string } {
  return isObject(value) && value['type'] === 'text' && typeof value['text'] === 'string';
}

function canonicalizeContextInputs(
  contextInputs: unknown,
  records: unknown,
  side: 'ts' | 'rust',
): unknown {
  if (side === 'ts') return contextInputs;
  if (!Array.isArray(records)) return contextInputs;
  return records
    .filter(isObject)
    .filter(
      (r) =>
        (r['type'] === 'turn.prompt' || r['type'] === 'turn.steer') && Array.isArray(r['input']),
    )
    .map((r) => {
      const text = (r['input'] as unknown[])
        .filter(isTextPart)
        .map((p) => p['text'])
        .join('');
      const origin = isObject(r['origin']) ? r['origin'] : {};
      const originKind = (origin['kind'] as string | undefined) ?? 'unknown';
      return { text, originKind };
    });
}

export function normalizeTurnSnapshot(
  snapshot: {
    readonly name: string;
    readonly turns: unknown;
    readonly events: unknown;
    readonly records: unknown;
    readonly contextInputs: unknown;
    readonly telemetry: unknown;
    readonly goalState?: unknown;
  },
  side: 'ts' | 'rust' = 'ts',
) {
  const canonicalEvents = canonicalizeEvents(snapshot.events, side);
  const canonicalRecords = canonicalizeRecords(snapshot.records);
  const canonicalTelemetry = canonicalizeTelemetry(snapshot.telemetry);
  const canonicalContextInputs = canonicalizeContextInputs(
    snapshot.contextInputs,
    canonicalRecords,
    side,
  );
  const canonicalTurns = canonicalizeTurns(snapshot.turns, canonicalEvents, side);

  return {
    name: snapshot.name,
    turns: normalizeValue(undefined, canonicalTurns),
    events: normalizeValue(undefined, canonicalEvents),
    records: normalizeValue(undefined, canonicalRecords),
    contextInputs: normalizeValue(undefined, canonicalContextInputs),
    telemetry: normalizeValue(undefined, canonicalTelemetry),
    goalState: undefined,
  };
}
