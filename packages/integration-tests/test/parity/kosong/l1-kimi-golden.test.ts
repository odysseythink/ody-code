import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsKosongKimiGolden, type Fixture } from '../../../src/parity/kosong-kimi-golden';

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
  'kosong-kimi',
);
const fixtures: Array<{ name: string; expectError: boolean }> = [
  { name: 'l1-kimi-text.json', expectError: false },
  { name: 'l1-kimi-thinking.json', expectError: false },
  { name: 'l1-kimi-tool-call-single.json', expectError: false },
  { name: 'l1-kimi-usage.json', expectError: false },
  { name: 'l1-kimi-error.json', expectError: true },
  { name: 'l1-kimi-tool-schema.json', expectError: false },
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
      if (val === undefined) continue;
      sorted[key] = sortKeys(val);
    }
    return sorted;
  }
  return obj;
}

describe('kosong-kimi L1 golden parity', () => {
  beforeAll(() => {
    const binaryPath =
      process.env['ODY_KIMI_GOLDEN_BINARY_PATH'] ??
      join(rootDir, 'rust-ody', 'target', 'debug', 'kimi_golden');
    if (existsSync(binaryPath)) return;
    spawnSync(
      'cargo',
      ['build', '-p', 'kosong-rs', '--bin', 'kimi_golden'],
      { cwd: join(rootDir, 'rust-ody'), stdio: 'inherit' },
    );
  });

  const binaryPath =
    process.env['ODY_KIMI_GOLDEN_BINARY_PATH'] ??
    join(rootDir, 'rust-ody', 'target', 'debug', 'kimi_golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongKimiGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });

    if (expectError) {
      // Error fixtures: both sides should produce a truthy error.
      // Rust binary may exit non-zero for error cases — that's acceptable.
      expect(ts.error).toBeTruthy();
      if (result.status === 0) {
        const rust = JSON.parse(result.stdout);
        expect(rust.error).toBeTruthy();
      }
      return;
    }

    if (result.status !== 0) throw new Error(`kimi_golden exited ${result.status}: ${result.stderr}`);
    const rust = JSON.parse(result.stdout);

    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
