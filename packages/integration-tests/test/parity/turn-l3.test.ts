import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { runTurnL3Fixture } from '../../src/parity/turn-l3-driver';

const fixturesDir = join(import.meta.dirname, '../../src/parity/fixtures/turn');

describe('turn L3 TS runner', () => {
  it.each([
    'end-turn.json',
    'single-tool-call.json',
    'tool-not-found.json',
    'steer-buffer.json',
    'cancel-mid-step.json',
    'same-step-dedup.json',
    'cross-step-dedup.json',
    'goal-continuation.json',
  ])('%s produces a snapshot', async (name) => {
    const snapshot = await runTurnL3Fixture(join(fixturesDir, name));
    expect(snapshot.name).toBe(name.replace('.json', ''));
    expect(snapshot.turns.length).toBeGreaterThan(0);
    expect(snapshot.events.some((e) => (e as { type: string }).type === 'turn.started')).toBe(true);
    expect(snapshot.events.some((e) => (e as { type: string }).type === 'turn.ended')).toBe(true);
  }, 60000);
});
