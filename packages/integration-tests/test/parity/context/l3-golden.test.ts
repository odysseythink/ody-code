import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsContextGolden } from '../../../src/parity/context-golden';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const fixturesDir = join(
  rootDir,
  'packages',
  'integration-tests',
  'src',
  'parity',
  'fixtures',
  'context',
);
const fixtures = [{ name: 'l3-memory.json' }];

function sortKeys(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(sortKeys);
  if (obj !== null && typeof obj === 'object') {
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
      const val = (obj as Record<string, unknown>)[key];
      if (val === undefined) continue;
      sorted[key] = sortKeys(val);
    }
    return sorted;
  }
  return obj;
}

describe('context L3 golden parity', () => {
  beforeAll(() => {
    spawnSync('cargo', ['build', '-p', 'agent-rs', '--bin', 'context-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'context-golden');

  it.each(fixtures)('$name TS matches Rust', ({ name }: { name: string }) => {
    const fixturePath = join(fixturesDir, name);
    const raw = readFileSync(fixturePath, 'utf8');
    const ts = runTsContextGolden(JSON.parse(raw));
    const result = spawnSync(binaryPath, [fixturePath], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`context-golden exited ${result.status}: ${result.stderr}`);
    }
    const rust = JSON.parse(result.stdout);
    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
