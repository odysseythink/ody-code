import { describe, expect, it } from 'vitest';

import { buildFollowupMessage, renderFinding } from '../../../src/tui/commands/design-review';
import type { DesignReviewData } from '@odysseythink/ody-code-sdk';

function makeResult(overrides: Partial<DesignReviewData> = {}): DesignReviewData {
  return {
    path: '/session/plans/my-plan.md',
    auditLevel: 'Standard',
    reviewerAlias: 'glm_1/glm-5.1',
    ok: true,
    findings: [],
    ...overrides,
  };
}

describe('buildFollowupMessage', () => {
  it('always embeds the reviewed file path so the model does not need to search for it', () => {
    const result = makeResult({ path: '/absolute/path/to/design.md' });
    const msg = buildFollowupMessage(result, 'design');
    expect(msg).toContain('/absolute/path/to/design.md');
    expect(msg).toContain('Apply every fix NOT marked [ESCALATE] to THAT file directly');
    expect(msg).toContain('do not search for or recreate');
  });

  it('includes the path even when there are no escalated findings', () => {
    const result = makeResult({
      path: '/plans/no-escalate.md',
      findings: [{ severity: 'low', title: 'Nit', detail: 'naming', escalate: false }],
    });
    const msg = buildFollowupMessage(result, 'plan');
    expect(msg).toContain('/plans/no-escalate.md');
    expect(msg).toContain('None of the findings require my sign-off.');
  });

  it('keeps the escalation and disproven-skip semantics intact', () => {
    const result = makeResult({
      path: '/plans/escalate.md',
      findings: [{ severity: 'high', confidence: 'certain', title: 'Bug', detail: 'bad', escalate: true }],
    });
    const msg = buildFollowupMessage(result, 'plan');
    expect(msg).toContain('[ESCALATE]');
    expect(msg).toContain('verify the claim against the code first');
    expect(msg).toContain('disproven false-positive');
    expect(msg).toContain('AskUserQuestion');
    expect(msg).toContain('/plans/escalate.md');
  });
});

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
