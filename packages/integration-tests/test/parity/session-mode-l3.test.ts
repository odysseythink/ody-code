import { describe, it, expect } from 'vitest';
import { runTsSessionModeFixture } from '../../src/parity/session-mode-l3-driver';
import { normalizeSessionModeEvents } from '../../src/parity/normalize-session-mode';
import type { SessionModeFixture } from '../../src/parity/session-mode-fixture';
import planEnterExit from '../../src/parity/fixtures/session-mode/plan-enter-exit.json';
import designEnterExit from '../../src/parity/fixtures/session-mode/design-enter-exit.json';
import productEnterExit from '../../src/parity/fixtures/session-mode/product-enter-exit.json';
import gameDesignEnterExit from '../../src/parity/fixtures/session-mode/game-design-enter-exit.json';
import handoff from '../../src/parity/fixtures/session-mode/handoff.json';
import injectionContent from '../../src/parity/fixtures/session-mode/injection-content.json';

const fixtures = [
  ['plan-enter-exit', planEnterExit],
  ['design-enter-exit', designEnterExit],
  ['product-enter-exit', productEnterExit],
  ['game-design-enter-exit', gameDesignEnterExit],
  ['handoff', handoff],
  ['injection-content', injectionContent],
] as const;

describe('SessionMode L3 — TS self-parity', () => {
  for (const [name, fixture] of fixtures) {
    it(`${name} produces expected events`, async () => {
      const events = await runTsSessionModeFixture(fixture as SessionModeFixture);
      const normalized = normalizeSessionModeEvents(events);
      expect(normalized.length).toBeGreaterThanOrEqual(1);
      // Verify first event type matches
      const firstExpected = (fixture as SessionModeFixture).expectedEvents[0];
      expect(normalized[0]!['type']).toBe(firstExpected?.['type']);
    });
  }
});
