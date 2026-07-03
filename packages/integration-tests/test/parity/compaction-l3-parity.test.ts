import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { existsSync } from 'node:fs';

const execFileAsync = promisify(execFile);
const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, '../../src/parity/fixtures/turn');
const compactionFixtures = ['overflow-compaction.json', 'compaction-events.json'];

function findProjectRoot(): string {
  let current = __dirname;
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

async function runRustL3(fixtureName: string): Promise<Record<string, unknown>> {
  const root = findProjectRoot();
  const fixturePath = join(fixturesDir, fixtureName);
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--bin', 'turn_l3', '--', fixturePath],
    { cwd: join(root, 'rust-ody') },
  );
  return JSON.parse(stdout) as Record<string, unknown>;
}

describe('compaction L3 golden binary', () => {
  it.each(compactionFixtures)('%s has compaction fields', async (fixtureName) => {
    const rust = await runRustL3(fixtureName);

    // Verify structure has compaction fields (Rust serde outputs snake_case)
    expect(rust).toHaveProperty('compaction_events');
    expect(rust).toHaveProperty('compaction_records');
    expect(Array.isArray(rust['compaction_events'])).toBe(true);
    expect(Array.isArray(rust['compaction_records'])).toBe(true);
  }, 120000);
});
