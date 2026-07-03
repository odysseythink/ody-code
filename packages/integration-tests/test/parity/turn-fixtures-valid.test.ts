import { readdir, readFile } from 'node:fs/promises';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { parseTurnFixture } from '../../src/parity/turn-fixture';

const fixturesDir = join(import.meta.dirname, '../../src/parity/fixtures/turn');

describe('turn fixtures are valid', () => {
  it.each([
    'end-turn.json',
    'single-tool-call.json',
    'tool-not-found.json',
    'steer-buffer.json',
    'cancel-mid-step.json',
    'same-step-dedup.json',
    'cross-step-dedup.json',
    'goal-continuation.json',
  ])('%s parses against schema', async (name) => {
    const raw = await readFile(join(fixturesDir, name), 'utf8');
    const parsed = parseTurnFixture(raw);
    expect(parsed.name).toBe(name.replace('.json', ''));
    expect(parsed.actions.length).toBeGreaterThan(0);
  });
});
