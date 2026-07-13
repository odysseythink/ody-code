import { access } from 'node:fs/promises';
import { join } from 'pathe';

const PRETTIER_CONFIG_NAMES = [
  '.prettierrc',
  '.prettierrc.json',
  '.prettierrc.js',
  '.prettierrc.cjs',
  '.prettierrc.mjs',
  '.prettierrc.toml',
  '.prettierrc.yaml',
  '.prettierrc.yml',
  'prettier.config.js',
  'prettier.config.cjs',
  'prettier.config.mjs',
];

async function exists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function hasAny(cwd: string, names: readonly string[]): Promise<boolean> {
  for (const name of names) {
    if (await exists(join(cwd, name))) return true;
  }
  return false;
}

async function binExists(cwd: string, name: string): Promise<boolean> {
  return exists(join(cwd, 'node_modules', '.bin', name));
}

export async function resolveFormatterCommand(cwd: string): Promise<string | undefined> {
  if (await binExists(cwd, 'prettier')) {
    if (await hasAny(cwd, PRETTIER_CONFIG_NAMES)) {
      return './node_modules/.bin/prettier --write';
    }
  }
  if (await binExists(cwd, 'biome')) {
    if (await exists(join(cwd, 'biome.json'))) {
      return './node_modules/.bin/biome format --write';
    }
  }
  return undefined;
}

export async function resolveTypecheckCommand(cwd: string): Promise<string | undefined> {
  if ((await exists(join(cwd, 'tsconfig.json'))) && (await binExists(cwd, 'tsc'))) {
    return './node_modules/.bin/tsc --noEmit';
  }
  return undefined;
}
