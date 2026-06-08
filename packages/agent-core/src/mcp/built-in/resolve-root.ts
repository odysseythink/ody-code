import { existsSync } from 'node:fs';
import { dirname, join } from 'pathe';

import { resolveBuiltInFromSea } from './sea-builtins';

export class BuiltInRootNotFoundError extends Error {
  constructor(public readonly serverName: string) {
    super(`Built-in server "${serverName}" not found`);
  }
}

export function resolveBuiltInRoot(serverName: string, candidates?: readonly string[]): string {
  const resolvedCandidates = candidates ?? [
    join(dirname(process.execPath), 'built-in', serverName),
    join(__dirname, '..', 'built-in', serverName),
    join(__dirname, '..', '..', 'built-in', serverName),
    join(__dirname, '..', '..', '..', 'built-in', serverName),
  ];
  for (const candidate of resolvedCandidates) {
    if (existsSync(join(candidate, 'package.json')) || existsSync(join(candidate, 'index.js'))) {
      return candidate;
    }
  }

  const seaPath = resolveBuiltInFromSea(serverName);
  if (seaPath !== null) {
    return seaPath;
  }

  throw new BuiltInRootNotFoundError(serverName);
}
