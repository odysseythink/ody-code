import { describe, expect, it, vi } from 'vitest';
import { assertNoLegacyFields } from '#tui/utils/assert-no-legacy-fields';

describe('assertNoLegacyFields', () => {
  it('rejects planMode', () => {
    expect(() => assertNoLegacyFields({ planMode: true }, 'test'))
      .toThrow("Legacy field 'planMode' detected in test");
  });
  it('rejects designMode', () => {
    expect(() => assertNoLegacyFields({ designMode: true }, 'test'))
      .toThrow("Legacy field 'designMode' detected in test");
  });
  it('allows sessionMode', () => {
    expect(() => assertNoLegacyFields({ sessionMode: 'plan' }, 'test')).not.toThrow();
  });
  it('allows empty object', () => {
    expect(() => assertNoLegacyFields({}, 'test')).not.toThrow();
  });
  it('sends telemetry on rejection', () => {
    const track = vi.fn();
    expect(() => assertNoLegacyFields({ planMode: true }, 'test', track))
      .toThrow();
    expect(track).toHaveBeenCalledWith('legacy_field_detected', {
      legacyField: 'planMode',
      context: 'test',
      object: { planMode: true },
    });
  });
});
