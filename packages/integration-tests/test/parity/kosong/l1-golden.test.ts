import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsKosongGolden, type Fixture } from '../../../src/parity/kosong-golden';

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
  'kosong',
);
const fixtures: Array<{ name: string; expectError?: boolean }> = [
  { name: 'l1-generate-text.json' },
  { name: 'l1-tool-call-single.json' },
  { name: 'l1-tool-call-parallel.json' },
  { name: 'l1-empty-rejection.json', expectError: true },
  { name: 'l1-thinking-only-rejection.json', expectError: true },
];

function loadFixture(name: string): Fixture {
  const raw = readFileSync(join(fixturesDir, name), 'utf8');
  return JSON.parse(raw);
}

function sortKeys(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(sortKeys);
  if (obj !== null && typeof obj === 'object') {
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
      const val = (obj as Record<string, unknown>)[key];
      // Omit undefined values — they don't exist in JSON and Rust serde
      // skips them with skip_serializing_if = "Option::is_none".
      if (val === undefined) continue;
      sorted[key] = sortKeys(val);
    }
    return sorted;
  }
  return obj;
}

describe('kosong L1 golden parity', () => {
  beforeAll(() => {
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`kosong-golden exited ${result.status}: ${result.stderr}`);
    }
    const rust = JSON.parse(result.stdout);

    if (expectError) {
      expect(ts.error).toBeTruthy();
      expect(rust.error).toBeTruthy();
      return;
    }

    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
