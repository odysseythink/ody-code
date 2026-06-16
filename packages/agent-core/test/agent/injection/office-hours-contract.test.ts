import { describe, expect, it } from 'vitest';
import {
  officeHoursEntryReminder,
  officeHoursFullReminder,
  officeHoursSparseReminder,
  officeHoursReentryReminder,
  officeHoursExitReminder,
} from '#/agent/injection/office-hours-contract';

describe('office-hours-contract', () => {
  const path = '/project/.ody-code/office-hours/2026-06-16-my-startup.md';

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
