import { existsSync } from 'node:fs';
import { join } from 'pathe';

const CANDIDATES: Array<() => string | undefined> = [
  () => process.env['ODY_HOST_BINARY_PATH'],
  () => join(process.cwd(), 'rust-ody', 'target', 'release', 'ody-host'),
  () => join(process.cwd(), 'rust-ody', 'target', 'debug', 'ody-host'),
];

export function resolveRustBinaryPath(): string {
  for (const candidate of CANDIDATES) {
    const path = candidate();
    if (path !== undefined && existsSync(path)) {
      return path;
    }
  }
  throw new Error(
    'Rust host binary not found. Set ODY_HOST_BINARY_PATH or build with `pnpm run build:host`.',
  );
}
