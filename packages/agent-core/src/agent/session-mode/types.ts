export const SESSION_MODE_KINDS = ['plan', 'design', 'product', 'game-design'] as const;
export type SessionModeKind = typeof SESSION_MODE_KINDS[number];

export const RUNTIME_MODES = [...SESSION_MODE_KINDS, 'normal'] as const;
export type RuntimeMode = typeof RUNTIME_MODES[number];

export function isSessionModeKind(value: string): value is SessionModeKind {
  return (SESSION_MODE_KINDS as readonly string[]).includes(value);
}

export function isRuntimeMode(value: string): value is RuntimeMode {
  return (RUNTIME_MODES as readonly string[]).includes(value);
}

export function normalizeRuntimeMode(
  value: string,
  warn: (message: string) => void = console.warn,
): RuntimeMode {
  if (isRuntimeMode(value)) return value;
  warn(`Unknown runtime mode "${value}", falling back to "normal"`);
  return 'normal';
}
