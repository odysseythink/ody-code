export function normalizeCompactionSnapshot(snapshot: unknown): unknown {
  const s = JSON.stringify(snapshot)
    .replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, '<id>')
    .replace(/\d{13}/g, '<ts>');
  return JSON.parse(s);
}
