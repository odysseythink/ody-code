import { describe, expect, it } from 'vitest';
import { AppendBuilderProfileInputSchema } from '../../../../src/tools/builtin/product/append-profile';

/**
 * The `signals` description is read by the model when it decides which founder
 * signals to record. It must stay in lockstep with the product prompt
 * contract's verification-strength vocabulary — otherwise the model could
 * re-introduce the lumped `demand_evidence` signal the diagnostic is meant to
 * kill. These assertions pin that vocabulary so drift is caught.
 */
describe('AppendBuilderProfile signals description', () => {
  const description = AppendBuilderProfileInputSchema.shape.signals.description ?? '';

  it('names the three verification-strength demand signals', () => {
    expect(description).toContain('demand_transacted');
    expect(description).toContain('demand_observed');
    expect(description).toContain('demand_stated');
  });

  it('does not reference the removed lumped demand signal', () => {
    expect(description).not.toContain('demand_evidence');
  });
});
