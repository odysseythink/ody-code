export function normalizeSessionModeEvents(
  events: Array<Record<string, unknown>>,
): Array<Record<string, unknown>> {
  return events.map(event => {
    const normalized = { ...event };
    delete normalized['time'];
    if (typeof normalized['path'] === 'string') {
      normalized['path'] = (normalized['path'] as string).replace(/\/tmp\/[^/]+/, '<TMP>');
    }
    return normalized;
  });
}
