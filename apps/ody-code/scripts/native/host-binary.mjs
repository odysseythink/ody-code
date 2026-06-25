import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const HOST_BINARY_ENV = 'ODY_HOST_BINARY';

export function hostBinaryName(platform = process.platform) {
  return platform === 'win32' ? 'ody-host.exe' : 'ody-host';
}

export function defaultHostBinaryPath(target, platform = process.platform) {
  // Development fallback: repo-relative target/release binary.
  return resolve(
    process.cwd(),
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
