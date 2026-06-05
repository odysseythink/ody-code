import { describe, expect, it } from 'vitest';

import { renderFinding } from '../../../src/tui/commands/design-review';

describe('renderFinding', () => {
  it('shows ~spec tag after the title for speculative findings', () => {
    const output = renderFinding(
      { severity: 'med', confidence: 'speculative', title: 'Timing gap', detail: 'may fire late', escalate: false },
      1,
    );
    expect(output).toContain('~spec');
    expect(output).toContain('Timing gap ~spec');
  });

  it('does not show ~spec for certain findings', () => {
    const output = renderFinding(
      { severity: 'high', confidence: 'certain', title: 'Null deref', detail: 'crashes on empty', escalate: true },
      1,
    );
    expect(output).not.toContain('~spec');
  });

  it('does not show ~spec when confidence is absent', () => {
    const output = renderFinding(
      { severity: 'low', title: 'Nit', detail: 'naming', escalate: false },
      1,
    );
    expect(output).not.toContain('~spec');
  });

  it('places ~spec before the location parenthetical', () => {
    const output = renderFinding(
      {
        severity: 'med',
        confidence: 'speculative',
        title: 'Ordering issue',
        detail: 'might reorder',
        location: 'L42',
        escalate: false,
      },
      2,
    );
    const specIdx = output.indexOf('~spec');
    const locIdx = output.indexOf('(L42)');
    expect(specIdx).toBeGreaterThan(-1);
    expect(locIdx).toBeGreaterThan(specIdx);
  });

  it('shows [ESCALATE] tag for escalated findings', () => {
    const output = renderFinding(
      { severity: 'high', confidence: 'certain', title: 'Bug', detail: 'bad', escalate: true },
      3,
    );
    expect(output).toContain('[ESCALATE]');
    expect(output).not.toContain('~spec');
  });
});
