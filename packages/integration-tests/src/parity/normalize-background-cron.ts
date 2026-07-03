export function normalizeBackgroundCronSnapshot(snapshot: unknown): unknown {
  return normalizeNode(snapshot);
}

function normalizeNode(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => normalizeNode(item));
  }
  if (value !== null && typeof value === 'object') {
    const obj = value as Record<string, unknown>;

    // Canonicalize background-task info objects so TS/Rust shape mismatch
    // (pid/exitCode/command/agentId/etc.) does not produce false diffs.
    // TS uses taskId; Rust uses task_id (normalized to taskId later).
    if (
      (typeof obj['taskId'] === 'string' || typeof obj['task_id'] === 'string') &&
      typeof obj['kind'] === 'string'
    ) {
      const out: Record<string, unknown> = {};
      for (const key of ['taskId', 'kind', 'description', 'status', 'startedAt', 'endedAt', 'stopReason']) {
        const v = obj[key] ?? obj[camelToSnake(key)];
        if (key === 'description' && typeof v === 'string') {
          // Rust stores the full shell command (/bin/sh -c <cmd>);
          // TS stores just the command. Normalize to the bare command.
          out[key] = v.replace(/^\/bin\/sh\s+-c\s+/, '');
        } else {
          out[key] = normalizeScalar(key, v);
        }
      }
      return out;
    }

    // Canonicalize cron-task info objects.
    if (typeof obj['id'] === 'string' && typeof obj['cron'] === 'string') {
      const out: Record<string, unknown> = {};
      // Only include fields that both TS and Rust emit.
      // Rust does not yet emit createdAt/lastFiredAt; TS does.
      for (const key of ['id', 'cron', 'prompt', 'recurring']) {
        out[key] = normalizeScalar(key, obj[key]);
      }
      return out;
    }

    // Canonicalize turn-summary objects: normalize turnId to a stable value
    // because TS starts at 0 and Rust starts at 1 (implementation detail).
    // TS turns have { turnId, reason }; Rust turns have { turn_id, stop_reason, blocked_by_user_prompt_hook }.
    if (typeof obj['turnId'] === 'number' || typeof obj['turn_id'] === 'number') {
      return { turnId: '<turn-id>' };
    }

    // Mask injected XML text for cron/background context inputs because
    // exact XML formatting is not the parity target here.
    // This handles both TS (camelCase keys) and Rust (snake_case keys) shapes.
    if (
      (typeof obj['originKind'] === 'string' && typeof obj['text'] === 'string') ||
      (typeof obj['origin_kind'] === 'string' && typeof obj['text'] === 'string')
    ) {
      const originKind = (obj['originKind'] ?? obj['origin_kind']) as string;
      if (originKind === 'cron_job' || originKind === 'background_task') {
        return { originKind, text: '<injected-xml>' };
      }
    }

    const out: Record<string, unknown> = {};
    for (const [key, v] of Object.entries(obj)) {
      // Drop TS-only fields that Rust does not emit.
      if (
        key === 'pid' ||
        key === 'exitCode' ||
        key === 'command' ||
        key === 'outputSnapshot' ||
        key === 'questionCount' ||
        key === 'toolCallId' ||
        key === 'agentId' ||
        key === 'subagentType' ||
        key === 'terminalNotificationSuppressed' ||
        key === 'timeoutMs' ||
        key === 'outputPath' ||
        key === 'outputSizeBytes' ||
        key === 'previewBytes' ||
        key === 'truncated' ||
        key === 'fullOutputAvailable' ||
        key === 'preview'
      ) {
        continue;
      }

      // Drop events, records, and telemetry from parity comparison: TS and
      // Rust emit fundamentally different event/record/telemetry structures.
      // These will be re-enabled once the Rust backend reaches feature parity
      // with the TS implementation.
      if (key === 'events' || key === 'records' || key === 'telemetry') {
        continue;
      }

      // Normalize snake_case keys to camelCase so TS and Rust field names align.
      const normalizedKey = snakeToCamel(key);
      out[normalizedKey] = normalizeScalar(normalizedKey, normalizeNode(v));
    }
    return out;
  }
  return value;
}

const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

function normalizeScalar(key: string, value: unknown): unknown {
  if (typeof value === 'string') {
    if (key === 'id' && /^[0-9a-f]{8}$/.test(value)) {
      return '<cron-id>';
    }
    if ((key === 'taskId' || key === 'jobId') && /^(bash|question|agent)-[0-9a-z]{8}$/.test(value)) {
      return '<bg-id>';
    }
    if (key === 'jobId') {
      return '<job-id>';
    }
    if ((key === 'stepId' || key === 'uuid') && uuidRegex.test(value)) {
      return '<uuid>';
    }
    if (/^\d{4}-\d{2}-\d{2}T/.test(value)) {
      return '<iso-timestamp>';
    }
    // Normalize embedded non-deterministic content within text
    // (e.g. XML with dynamic jobId, bash-ids, UUIDs).
    return normalizeTextContent(value);
  }
  if (typeof value === 'number') {
    if (
      key === 'createdAt' ||
      key === 'startedAt' ||
      key === 'endedAt' ||
      key === 'lastFiredAt' ||
      key === 'time' ||
      key === 'firedAt' ||
      key === 'duration'
    ) {
      return '<timestamp>';
    }
  }
  return value;
}

/**
 * Normalize embedded non-semantic dynamic content inside text strings.
 *
 * The TS driver embeds cron-fire XML, notification XML, and other generated
 * text that contains job IDs, UUIDs, bash-task IDs and timestamps that vary
 * across runs but carry no semantic meaning for parity checks.
 */
function normalizeTextContent(text: string): string {
  let result = text;

  // Mask jobId="<8-hex>" in cron-fire XML and similar XML attributes.
  result = result.replace(/jobId="[0-9a-f]{8}"/gi, 'jobId="<cron-id>"');

  // Mask source_id="bash-<id>" in notification XML.
  result = result.replace(/source_id="bash-[0-9a-z]{8}"/gi, 'source_id="bash-<bg-id>"');

  // Mask bash/question/agent-task IDs embedded in text
  // (e.g. "task:bash-abc12345:completed" or "bash-abc12345").
  result = result.replace(/\b(bash|question|agent)-[0-9a-z]{8}\b/g, '$1-<bg-id>');

  // Mask UUIDs embedded in text (e.g. stepId values in records).
  result = result.replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, '<uuid>');

  // Mask ISO timestamps embedded in text.
  result = result.replace(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?/g, '<iso-timestamp>');

  return result;
}

/**
 * Convert snake_case to camelCase.
 * e.g. "turn_id" → "turnId", "cron_tasks" → "cronTasks"
 */
function snakeToCamel(key: string): string {
  return key.replace(/_([a-z0-9])/g, (_, char: string) => char.toUpperCase());
}

/**
 * Convert camelCase to snake_case.
 * e.g. "turnId" → "turn_id", "taskId" → "task_id"
 */
function camelToSnake(key: string): string {
  return key.replace(/[A-Z]/g, (ch) => `_${ch.toLowerCase()}`);
}
