import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsKosongUtilsGolden } from '../../../src/parity/kosong-utils-golden';

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
  'kosong-utils',
);
const fixtures = [
  'tool-call-id.json',
  'request-auth.json',
  'capability-registry.json',
  'catalog.json',
];

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

describe('kosong utils L1 golden parity', () => {
  beforeAll(() => {
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-utils-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = join(
    rootDir,
    'rust-ody',
    'target',
    'debug',
    'kosong-utils-golden',
  );

  it.each(fixtures)('%s TS matches Rust', async (name) => {
    const fixturePath = join(fixturesDir, name);
    const ts = await runTsKosongUtilsGolden(fixturePath);
    const result = spawnSync(binaryPath, [fixturePath], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(
        `kosong-utils-golden exited ${result.status}: ${result.stderr}`,
      );
    }
    const rust = JSON.parse(result.stdout);
    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
