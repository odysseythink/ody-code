import { existsSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

import type { FixtureFile } from '../../../src/parity/tools-rs-golden';
import {
  normalizeGoldenPaths,
  resolveRustGoldenBinary,
  runRustGolden,
  runTsGolden,
} from '../../../src/parity/tools-rs-golden';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) {
      return current;
    }
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const binaryPath = resolveRustGoldenBinary(rootDir);

beforeAll(() => {
  execSync('cargo build -p tools-rs --bin tools-golden', {
    cwd: join(rootDir, 'rust-ody'),
    stdio: 'inherit',
  });
});

const fixtures = [
  'path-policy.json',
  'rule-match.json',
  'schema-validation.json',
  'tool-accesses.json',
  'result-builder.json',
  'file-type.json',
  'rg-locator.json',
  'list-directory.json',
  'core-tools.json',
  'background-cron-tools.json',
  'collaboration-tools.json',
  'goal-state-tools.json',
  'quality-specialized-tools.json',
];

async function loadFixture(name: string): Promise<unknown> {
  const { readFile } = await import('node:fs/promises');
  const raw = await readFile(
    join(rootDir, 'packages', 'integration-tests', 'src', 'parity', 'fixtures', 'tools-rs', name),
    'utf8',
  );
  return JSON.parse(raw);
}

function sortKeys(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(sortKeys);
  if (obj !== null && typeof obj === 'object') {
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
      sorted[key] = sortKeys((obj as Record<string, unknown>)[key]);
    }
    return sorted;
  }
  return obj;
}

describe('tools-rs L1 golden parity', () => {
  it.each(fixtures)('%s TS matches Rust', async (name) => {
    const fixture = await loadFixture(name);
    const ts = normalizeGoldenPaths(await runTsGolden(fixture as FixtureFile));
    const fixturePath = join(
      rootDir,
      'packages',
      'integration-tests',
      'src',
      'parity',
      'fixtures',
      'tools-rs',
      name,
    );
    const rust = normalizeGoldenPaths(runRustGolden(fixturePath, binaryPath));
    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  }, 120000);
});
