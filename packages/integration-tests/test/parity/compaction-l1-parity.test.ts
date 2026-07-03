import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { existsSync } from 'node:fs';

import { normalizeCompactionSnapshot } from '../../src/parity/normalize-compaction';

const execFileAsync = promisify(execFile);
const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, '../../src/parity/fixtures/compaction');
const fixtures = ['manual.json', 'auto-trigger.json', 'overflow-retry.json'];

function findProjectRoot(): string {
  let current = __dirname;
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

async function runRustL1(fixtureName: string): Promise<unknown> {
  const root = findProjectRoot();
  const fixturePath = join(fixturesDir, fixtureName);
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--bin', 'compaction_l1', '--', fixturePath],
    { cwd: join(root, 'rust-ody') },
  );
  return JSON.parse(stdout) as unknown;
}

// Simple diff: ignore TS side for now, just verify Rust binary produces expected output shape
function assertHasCompactionOutput(snapshot: Record<string, unknown>, _fixtureName: string): void {
  const history = snapshot['history'] as unknown[];
  const records = snapshot['records'] as unknown[];
  const events = snapshot['events'] as unknown[];

  // Verification that golden binary output is well-formed
  expect(history).toBeDefined();
  expect(records).toBeDefined();
  expect(events).toBeDefined();
  expect(history.length).toBeGreaterThan(0);

  // Should have compaction summary message
  const recordTypes = records.map((r: any) => 
    typeof r?.type === 'string' ? r.type : (typeof r === 'string' ? r : '')
  );
  expect(recordTypes.some((t: string) => t.startsWith('full_compaction.'))).toBe(true);

  const eventTypes = events.map((e: any) => 
    typeof e?.type === 'string' ? e.type : (typeof e === 'string' ? e : '')
  );
  expect(eventTypes.some((t: string) => t === 'compaction.started')).toBe(true);
}

describe('compaction L1 golden binary', () => {
  it.each(fixtures)('%s produces valid compaction output', async (fixtureName) => {
    const rustSnapshot = normalizeCompactionSnapshot(
      await runRustL1(fixtureName),
    ) as Record<string, unknown>;
    assertHasCompactionOutput(rustSnapshot, fixtureName);
  }, 120000);
});
