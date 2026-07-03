import { existsSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { LocalKaos } from '@odysseythink/kaos';
import {
  resolveRustGoldenBinary,
  runRustGolden,
  runTsGolden,
} from '../../../src/parity/kaos-golden';

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
let binaryPath: string;
let kaos: LocalKaos;

beforeAll(async () => {
  binaryPath = resolveRustGoldenBinary(rootDir);
  // Always rebuild to ensure the binary is up to date.
  execSync('cargo build -p kaos-rs --bin kaos-golden', {
    cwd: join(rootDir, 'rust-ody'),
    stdio: 'inherit',
  });
  kaos = await LocalKaos.create();
});

const baseFixtures = [
  'l1-paths.json',
  'l1-glob-patterns.json',
  'l1-file-io.json',
  'l1-directory-ops.json',
];
const fixtures =
  process.platform === 'win32'
    ? baseFixtures
    : [...baseFixtures, 'l1-process-ops.json'];

async function loadFixture(name: string) {
  const raw = await import('node:fs/promises').then((m) =>
    m.readFile(
      join(
        rootDir,
        'packages',
        'integration-tests',
        'src',
        'parity',
        'fixtures',
        'kaos',
        name,
      ),
      'utf8',
    ),
  );
  return JSON.parse(raw);
}

describe('kaos L1 golden parity', () => {
  it.each(fixtures)('%s TS matches Rust', async (name) => {
    const fixture = await loadFixture(name);
    const ts = await runTsGolden(kaos, fixture);
    const fixturePath = join(
      rootDir,
      'packages',
      'integration-tests',
      'src',
      'parity',
      'fixtures',
      'kaos',
      name,
    );
    const rust = runRustGolden(fixturePath, binaryPath);
    // Normalize both outputs: sort keys deeply and re-serialize, then compare.
    const sortKeys = (obj: unknown): unknown => {
      if (Array.isArray(obj)) return obj.map(sortKeys);
      if (obj !== null && typeof obj === 'object') {
        const sorted: Record<string, unknown> = {};
        for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
          sorted[key] = sortKeys((obj as Record<string, unknown>)[key]);
        }
        return sorted;
      }
      return obj;
    };
    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
