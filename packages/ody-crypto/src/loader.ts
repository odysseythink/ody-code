import { createRequire } from 'node:module';

import type { OdyCrypto } from './types';
import { tsFallback } from './fallback';

const SUPPORTED_TARGETS = [
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64',
  'linux-x64',
  'win32-arm64',
  'win32-x64',
];

function currentTarget(): string {
  return `${process.platform}-${process.arch}`;
}

function debug(message: string, ...args: unknown[]): void {
  console.debug(`ody-crypto: ${message}`, ...args);
}

export function loadNative(): OdyCrypto | null {
  const target = currentTarget();
  if (!SUPPORTED_TARGETS.includes(target)) {
    debug('unsupported target, using TS fallback', target);
    return null;
  }
  const pkg = `@odysseythink/ody-crypto-${target}`;
  try {
    const req = createRequire(import.meta.url);
    return req(pkg) as OdyCrypto;
  } catch (err) {
    debug('native load failed, using TS fallback', target, (err as Error).message);
    return null;
  }
}

export function getOdyCrypto(): OdyCrypto {
  return loadNative() ?? tsFallback;
}
