import { mkdtempSync, writeFileSync, mkdirSync, chmodSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import {
  resolveFormatterCommand,
  resolveTypecheckCommand,
} from '../../../src/session/hooks/builtin/resolve-formatter';

function stubDir(): string {
  return mkdtempSync(join(tmpdir(), 'kimi-hooks-fmt-'));
}

function bin(dir: string, name: string, body: string): void {
  const d = join(dir, 'node_modules', '.bin');
  mkdirSync(d, { recursive: true });
  const f = join(d, name);
  writeFileSync(f, `#!/bin/sh\n${body}`, 'utf-8');
  chmodSync(f, 0o755);
}

describe('resolveFormatterCommand', () => {
  it('returns prettier command when config and binary exist', async () => {
    const dir = stubDir();
    writeFileSync(join(dir, '.prettierrc'), '{}');
    bin(dir, 'prettier', 'exit 0');
    expect(await resolveFormatterCommand(dir)).toContain('prettier --write');
  });

  it('returns biome command when biome.json and binary exist', async () => {
    const dir = stubDir();
    writeFileSync(join(dir, 'biome.json'), '{}');
    bin(dir, 'biome', 'exit 0');
    expect(await resolveFormatterCommand(dir)).toContain('biome format --write');
  });

  it('returns undefined when formatter binary is missing', async () => {
    const dir = stubDir();
    writeFileSync(join(dir, '.prettierrc'), '{}');
    expect(await resolveFormatterCommand(dir)).toBeUndefined();
  });
});

describe('resolveTypecheckCommand', () => {
  it('returns tsc command when tsconfig and binary exist', async () => {
    const dir = stubDir();
    writeFileSync(join(dir, 'tsconfig.json'), '{}');
    bin(dir, 'tsc', 'exit 0');
    expect(await resolveTypecheckCommand(dir)).toContain('tsc --noEmit');
  });

  it('returns undefined when tsconfig is missing', async () => {
    const dir = stubDir();
    bin(dir, 'tsc', 'exit 0');
    expect(await resolveTypecheckCommand(dir)).toBeUndefined();
  });

  it('returns undefined when tsc binary is missing', async () => {
    const dir = stubDir();
    writeFileSync(join(dir, 'tsconfig.json'), '{}');
    expect(await resolveTypecheckCommand(dir)).toBeUndefined();
  });
});
