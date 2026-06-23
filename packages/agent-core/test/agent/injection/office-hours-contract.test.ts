import { describe, expect, it } from 'vitest';

function escapeRegex(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
import {
  officeHoursEntryReminder,
  officeHoursFullReminder,
  officeHoursSparseReminder,
  officeHoursReentryReminder,
  officeHoursExitReminder,
} from '#/agent/injection/office-hours-contract';

describe('office-hours-contract', () => {
  const path = '/project/.ody-code/office-hours/2026-06-16-my-startup.md';

  const LANGUAGE_PREFIX = '**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.';

  it('entry reminder starts with Language instruction', () => {
    const msg = officeHoursEntryReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });

  it('full reminder starts with Language instruction', () => {
    const msg = officeHoursFullReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });

  it('sparse reminder starts with Language instruction', () => {
    const msg = officeHoursSparseReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });

  it('reentry reminder starts with Language instruction', () => {
    const msg = officeHoursReentryReminder(path);
    expect(msg).toMatch(new RegExp('^' + escapeRegex(LANGUAGE_PREFIX)));
  });

  describe('officeHoursEntryReminder', () => {
    it('includes the design file path', () => {
      const msg = officeHoursEntryReminder(path);
      expect(msg).toContain(path);
    });

    it('includes office hours activation notice', () => {
      const msg = officeHoursEntryReminder(path);
      expect(msg).toContain('Office hours');
    });

    it('forbids writing code', () => {
      const msg = officeHoursEntryReminder(path);
      expect(msg).toContain('Do NOT write code');
    });
  });

  describe('officeHoursFullReminder', () => {
    it('includes all phases', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('Phase 1');
      expect(msg).toContain('Phase 2');
      expect(msg).toContain('Phase 3');
      expect(msg).toContain('Phase 4');
      expect(msg).toContain('Phase 5');
      expect(msg).toContain('Phase 6');
    });

    it('includes AskUserQuestion discipline', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('AskUserQuestion');
    });

    it('includes design doc template section', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('Design Doc');
    });

    it('defines the demand-provenance tag vocabulary', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('[V:TRANSACTED]');
      expect(msg).toContain('[V:OBSERVED]');
      expect(msg).toContain('[V:STATED]');
    });

    it('splits demand signals by verification strength', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('demand_transacted');
      expect(msg).toContain('demand_observed');
      expect(msg).toContain('demand_stated');
      // the old lumped signal must be gone
      expect(msg).not.toContain('demand_evidence');
    });

    it('walks the demand dependency chain (pain before frequency before payment)', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('DEPENDENCY CHAIN');
      expect(msg).toContain('Status Quo');
    });

    it('requires AppendBuilderProfile before exit', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('MUST call AppendBuilderProfile');
      expect(msg).toContain('before moving to Phase 5 or calling ExitOfficeHoursMode');
    });

    it('runs the Phase 2.25 critical-thinking pass on demand claims', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('Phase 2.25');
      // state the conclusion back, then pin ambiguous words
      expect(msg).toContain('State the conclusion back');
      expect(msg).toContain('Pin the ambiguous words');
      // assumptions resolve to cheapest falsification, not debate
      expect(msg).toContain('cheapest test that could prove this FALSE');
      // startup-only, skipped in builder mode
      expect(msg).toContain('Skip this phase entirely in builder mode');
    });

    it('confirms the subject before detail questions (both tracks)', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('CONFIRM THE SUBJECT');
      expect(msg).toContain('is a GUESS until confirmed');
    });

    it('locks the startup/builder track early and routes phases', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('LOCK THE TRACK');
      // builder track skips startup-only diagnostics and forced distribution
      expect(msg).toContain('SKIP Phase 2A and Phase 2.25');
      expect(msg).toContain('do NOT invent an acquisition channel');
    });

    it('enforces question economy (infer over ask)', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('Question economy');
    });

    it('gives the premise challenge teeth (cheapest falsification, not rubber-stamp)', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('cheapest test that could prove it FALSE');
      expect(msg).toContain('is NOT validated');
    });

    it('tells the model how to emit the doc and not to enter other modes', () => {
      const msg = officeHoursFullReminder(path);
      expect(msg).toContain('.ody-code/products/');
      expect(msg).toContain('do NOT call EnterDesignMode or EnterPlanMode');
    });
  });

  describe('officeHoursSparseReminder', () => {
    it('is shorter than full reminder', () => {
      const sparse = officeHoursSparseReminder(path);
      const full = officeHoursFullReminder(path);
      expect(sparse.length).toBeLessThan(full.length);
    });

    it('includes ONE question at a time', () => {
      expect(officeHoursSparseReminder(path)).toContain('ONE question');
    });
  });

  describe('officeHoursExitReminder', () => {
    it('signals session completion', () => {
      expect(officeHoursExitReminder(path)).toContain('complete');
    });
  });

  describe('officeHoursReentryReminder', () => {
    it('acknowledges existing content', () => {
      expect(officeHoursReentryReminder(path)).toContain('existing');
    });
  });
});
