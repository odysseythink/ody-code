import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsAnthropicGolden, type AnthropicFixture } from '../../../src/parity/kosong-anthropic-golden';

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
  'kosong-anthropic',
);

const fixtures: Array<{ name: string; expectError?: boolean }> = [
  { name: 'l1-stream-text.json' },
  { name: 'l1-stream-thinking.json' },
  { name: 'l1-stream-tool-call.json' },
  { name: 'l1-stream-parallel-tool-calls.json' },
  { name: 'l1-nonstream-text-tool.json' },
];

function loadFixture(name: string): AnthropicFixture {
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

describe('kosong Anthropic L1 golden parity', () => {
  beforeAll(() => {
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-anthropic-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-anthropic-golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsAnthropicGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`kosong-anthropic-golden exited ${result.status}: ${result.stderr}`);
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
