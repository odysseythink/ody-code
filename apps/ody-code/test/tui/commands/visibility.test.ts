import { isCommandVisibleInMode } from '#tui/commands/index';
import { describe, expect, it } from 'vitest';

describe('isCommandVisibleInMode', () => {
  it('returns true when hiddenInModes is undefined', () => {
    expect(isCommandVisibleInMode({}, 'normal')).toBe(true);
    expect(isCommandVisibleInMode({}, 'plan')).toBe(true);
    expect(isCommandVisibleInMode({}, 'design')).toBe(true);
  });

  it('returns true when hiddenInModes is empty', () => {
    expect(isCommandVisibleInMode({ hiddenInModes: [] }, 'normal')).toBe(true);
    expect(isCommandVisibleInMode({ hiddenInModes: [] }, 'plan')).toBe(true);
  });

  it('hides command when mode is in hiddenInModes', () => {
    expect(isCommandVisibleInMode({ hiddenInModes: ['design'] }, 'design')).toBe(false);
    expect(isCommandVisibleInMode({ hiddenInModes: ['plan'] }, 'plan')).toBe(false);
    expect(isCommandVisibleInMode({ hiddenInModes: ['normal'] }, 'normal')).toBe(false);
  });

  it('shows command when mode is NOT in hiddenInModes', () => {
    expect(isCommandVisibleInMode({ hiddenInModes: ['design'] }, 'plan')).toBe(true);
    expect(isCommandVisibleInMode({ hiddenInModes: ['design'] }, 'normal')).toBe(true);
    expect(isCommandVisibleInMode({ hiddenInModes: ['plan'] }, 'design')).toBe(true);
  });

  it('handles multiple hidden modes', () => {
    const hiddenInModes = ['plan', 'normal'] as const;
    expect(isCommandVisibleInMode({ hiddenInModes }, 'design')).toBe(true);
    expect(isCommandVisibleInMode({ hiddenInModes }, 'plan')).toBe(false);
    expect(isCommandVisibleInMode({ hiddenInModes }, 'normal')).toBe(false);
  });

  it('hides commands in office-hours mode', () => {
    expect(isCommandVisibleInMode({ hiddenInModes: ['office-hours'] }, 'office-hours')).toBe(false);
    expect(isCommandVisibleInMode({ hiddenInModes: ['office-hours'] }, 'normal')).toBe(true);
  });
});
