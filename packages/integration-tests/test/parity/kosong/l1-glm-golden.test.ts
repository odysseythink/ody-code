import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { runTsKosongGLMGolden, type Fixture } from '../../../src/parity/kosong-glm-golden';

function findProjectRoot(): string {
  let current = dirname(import.meta.filename);
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
  'kosong-glm',
);
const fixtures: Array<{ name: string; expectError: boolean }> = [
  { name: 'l1-glm-text.json', expectError: false },
  { name: 'l1-glm-thinking.json', expectError: false },
  { name: 'l1-glm-tool-call-single.json', expectError: false },
  { name: 'l1-glm-error.json', expectError: true },
];

function loadFixture(name: string): Fixture {
  const raw = readFileSync(join(fixturesDir, name), 'utf8');
  return JSON.parse(raw);
}

function sortKeys(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(sortKeys);
  if (obj !== null && typeof obj === 'object') {
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(obj as Record<string, unknown>).toSorted()) {
      const val = (obj as Record<string, unknown>)[key];
      if (val === undefined) continue;
      sorted[key] = sortKeys(val);
    }
    return sorted;
  }
  return obj;
}

describe('kosong-glm L1 golden parity', () => {
  beforeAll(() => {
    const binaryPath =
      process.env['ODY_GLM_GOLDEN_BINARY_PATH'] ?? join(rootDir, 'rust-ody', 'target', 'debug', 'glm_golden');
    if (existsSync(binaryPath)) return;
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'glm_golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath =
    process.env['ODY_GLM_GOLDEN_BINARY_PATH'] ?? join(rootDir, 'rust-ody', 'target', 'debug', 'glm_golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongGLMGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });

    if (expectError) {
      expect(ts.error).toBeTruthy();
      if (result.status === 0) {
        const rust = JSON.parse(result.stdout);
        expect(rust.error).toBeTruthy();
      }
      return;
    }

    if (result.status !== 0) throw new Error(`glm_golden exited ${result.status}: ${result.stderr}`);
    const rust = JSON.parse(result.stdout);

    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
