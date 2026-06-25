import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { appRoot } from './paths.mjs';

const HOST_BINARY_ENV = 'ODY_HOST_BINARY';

const repoRoot = resolve(appRoot, '..', '..');

export function hostBinaryName(platform = process.platform) {
  return platform === 'win32' ? 'ody-host.exe' : 'ody-host';
}

export function defaultHostBinaryPath(target, platform = process.platform) {
  // Development fallback: repo-relative target/release binary.
  return resolve(
    repoRoot,
    'rust-ody',
    'target',
    target.endsWith('-debug') ? 'debug' : 'release',
    hostBinaryName(platform),
  );
}

export function resolveHostBinaryPath(target, platform = process.platform) {
  const envPath = process.env[HOST_BINARY_ENV];
  if (envPath !== undefined) return resolve(envPath);
  return defaultHostBinaryPath(target, platform);
}

export function hostBinaryAssetKey(target) {
  return `host/${target}/ody-host`;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const target = `${process.platform}-${process.arch}`;
  console.log(resolveHostBinaryPath(target));
}
