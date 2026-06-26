import { existsSync } from 'node:fs';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) {
      return current;
    }
    current = dirname(current);
  }
  return process.cwd();
}

export function resolveRustBinaryPath(searchRoot?: string): string {
  const root = searchRoot ?? findProjectRoot();
  const candidates: Array<() => string | undefined> = [
    () => process.env['ODY_HOST_BINARY_PATH'],
    () => join(root, 'rust-ody', 'target', 'release', 'ody-host'),
    () => join(root, 'rust-ody', 'target', 'debug', 'ody-host'),
  ];

  for (const candidate of candidates) {
    const path = candidate();
    if (path !== undefined && existsSync(path)) {
      return path;
    }
  }
  throw new Error(
    'Rust host binary not found. Set ODY_HOST_BINARY_PATH or build with `pnpm run build:host`.',
  );
}
