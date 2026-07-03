import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsKosongOpenAIGolden, type Fixture } from '../../../src/parity/kosong-openai-golden';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const fixturesDir = join(rootDir, 'packages', 'integration-tests', 'src', 'parity', 'fixtures', 'kosong-openai');
const fixtures: Array<{ name: string; expectError: boolean }> = [
  { name: 'l1-openai-text.json', expectError: false },
  { name: 'l1-openai-thinking.json', expectError: false },
  { name: 'l1-openai-tool-call-single.json', expectError: false },
  { name: 'l1-openai-tool-call-parallel.json', expectError: false },
  { name: 'l1-openai-truncated.json', expectError: false },
  { name: 'l1-openai-usage.json', expectError: false },
  { name: 'l1-openai-error.json', expectError: true },
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

describe('kosong-openai L1 golden parity', () => {
  beforeAll(() => {
    const binaryPath = process.env['ODY_OPENAI_GOLDEN_BINARY_PATH'] ?? join(rootDir, 'rust-ody', 'target', 'debug', 'openai_golden');
    if (existsSync(binaryPath)) return;
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'openai_golden'], { cwd: join(rootDir, 'rust-ody'), stdio: 'inherit' });
  });

  const binaryPath = process.env['ODY_OPENAI_GOLDEN_BINARY_PATH'] ?? join(rootDir, 'rust-ody', 'target', 'debug', 'openai_golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongOpenAIGolden(fixture);
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

    if (result.status !== 0) throw new Error(`openai_golden exited ${result.status}: ${result.stderr}`);
    const rust = JSON.parse(result.stdout);

    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
