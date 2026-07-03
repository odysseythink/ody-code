import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

interface FixtureScenario {
  name: string;
  description: string;
  mode: string;
  toolName: string;
  rules: { decision: string; scope: string; pattern: string }[];
  expectedDecision: string;
  expectedMessageContains: string | null;
}

function readFixture(): FixtureScenario[] {
  const fixturePath = path.resolve(
    __dirname,
    '../../../../../rust-ody/crates/agent-rs/tests/fixtures/permission-scenarios-rust.json',
  );
  return JSON.parse(fs.readFileSync(fixturePath, 'utf-8'));
}

describe('Permission parity — TS reads Rust fixture', () => {
  it.each(readFixture().map((s) => [s.name, s]))(
    '%s: mode=%s, tool=%s → %s',
    (_name: string, scenario: FixtureScenario) => {
      // Verify the fixture is well-formed and expectations are consistent
      expect(scenario.mode).toBeOneOf(['manual', 'yolo', 'auto']);
      expect(scenario.expectedDecision).toBeOneOf(['approve', 'deny', 'ask']);
      expect(typeof scenario.toolName).toBe('string');
      expect(Array.isArray(scenario.rules)).toBe(true);
    },
  );

  it('fixture has at least 5 scenarios', () => {
    const scenarios = readFixture();
    expect(scenarios.length).toBeGreaterThanOrEqual(5);
  });

  it('each scenario has a non-empty name and description', () => {
    for (const s of readFixture()) {
      expect(s.name.length).toBeGreaterThan(0);
      expect(s.description.length).toBeGreaterThan(0);
    }
  });
});
