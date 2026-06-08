import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'pathe';

import { resolveOdyHome } from '#/config/path';

interface BuiltInFileEntry {
  readonly path: string;
  readonly sha256: string;
}

interface BuiltInServerEntry {
  readonly name: string;
  readonly files: readonly BuiltInFileEntry[];
}

interface BuiltInManifest {
  readonly version: number;
  readonly servers: readonly BuiltInServerEntry[];
}

interface SeaModule {
  isSea(): boolean;
  getAssetKeys(): string[];
  getRawAsset(key: string): ArrayBuffer | string;
}

const nodeRequire = createRequire(import.meta.url);
let seaModule: SeaModule | null | undefined;

function loadSeaModule(): SeaModule | null {
  if (seaModule !== undefined) return seaModule;
  try {
    seaModule = nodeRequire('node:sea') as SeaModule;
  } catch {
    seaModule = null;
  }
  return seaModule;
}

function toBuffer(value: ArrayBuffer | string): Buffer {
  if (typeof value === 'string') return Buffer.from(value);
  return Buffer.from(value);
}

function sha256(bytes: Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function getCacheBase(): string {
  const envCache = process.env['ODY_CODE_CACHE_DIR'];
  if (envCache && envCache.length > 0) return envCache;
  return join(resolveOdyHome(), 'cache');
}

function ensureDir(path: string): void {
  mkdirSync(path, { recursive: true });
}

function isCacheValid(cacheDir: string, files: readonly BuiltInFileEntry[]): boolean {
  for (const file of files) {
    const filePath = join(cacheDir, file.path);
    if (!existsSync(filePath)) return false;
  }
  return true;
}

export function resolveBuiltInFromSea(serverName: string): string | null {
  const sea = loadSeaModule();
  if (sea === null || !sea.isSea()) return null;

  const keys = sea.getAssetKeys();
  if (!keys.includes('built-in/manifest.json')) return null;

  let manifest: BuiltInManifest;
  try {
    const raw = sea.getRawAsset('built-in/manifest.json');
    manifest = JSON.parse(toBuffer(raw).toString('utf-8')) as BuiltInManifest;
  } catch {
    return null;
  }

  if (manifest.version !== 1) return null;

  const server = manifest.servers.find((s) => s.name === serverName);
  if (server === undefined) return null;

  const manifestHash = sha256(Buffer.from(JSON.stringify(server.files)));
  const cacheDir = join(getCacheBase(), 'built-in', serverName, manifestHash);

  if (!isCacheValid(cacheDir, server.files)) {
    ensureDir(cacheDir);
    for (const file of server.files) {
      const assetKey = `built-in/${serverName}/${file.path}`;
      if (!keys.includes(assetKey)) return null;
      const rawAsset = sea.getRawAsset(assetKey);
      const filePath = join(cacheDir, file.path);
      ensureDir(dirname(filePath));
      writeFileSync(filePath, toBuffer(rawAsset));
    }
  }

  return cacheDir;
}
