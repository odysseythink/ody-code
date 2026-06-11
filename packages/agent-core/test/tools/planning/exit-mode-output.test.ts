import { describe, expect, it } from 'vitest';

import {
  declaredOptionLabel,
  isViaApproval,
  selectedApproachPrefix,
  selectedLabelOf,
} from '../../../src/tools/builtin/planning/exit-mode-output';

describe('exit-mode shared output helpers', () => {
  describe('isViaApproval', () => {
    it('is true only when metadata explicitly flags viaApproval', () => {
      expect(isViaApproval({ viaApproval: true })).toBe(true);
      expect(isViaApproval({ viaApproval: false })).toBe(false);
      expect(isViaApproval({ selectedLabel: 'A' })).toBe(false);
      expect(isViaApproval(undefined)).toBe(false);
      expect(isViaApproval(null)).toBe(false);
      expect(isViaApproval('viaApproval')).toBe(false);
    });
  });

  describe('selectedLabelOf', () => {
    it('returns the string label or undefined for non-string/missing values', () => {
      expect(selectedLabelOf({ selectedLabel: 'Approach A' })).toBe('Approach A');
      expect(selectedLabelOf({ selectedLabel: 42 })).toBeUndefined();
      expect(selectedLabelOf({})).toBeUndefined();
      expect(selectedLabelOf(undefined)).toBeUndefined();
    });
  });

  describe('declaredOptionLabel', () => {
    const options = [{ label: 'Approach A' }, { label: 'Approach B' }];

    it('passes through a label that matches a declared option', () => {
      expect(declaredOptionLabel(options, 'Approach B')).toBe('Approach B');
    });

    it('drops a label that is not among the options (e.g. reserved "Approve")', () => {
      expect(declaredOptionLabel(options, 'Approach C')).toBeUndefined();
      expect(declaredOptionLabel(options, 'Approve')).toBeUndefined();
    });

    it('returns undefined when there are no options or no label', () => {
      expect(declaredOptionLabel(undefined, 'Approach A')).toBeUndefined();
      expect(declaredOptionLabel(options, undefined)).toBeUndefined();
    });
  });

  describe('selectedApproachPrefix', () => {
    it('renders the directive for a present label and nothing otherwise', () => {
      expect(selectedApproachPrefix('Approach A')).toContain('Selected approach: Approach A');
      expect(selectedApproachPrefix('Approach A')).toContain('Execute ONLY the selected approach');
      expect(selectedApproachPrefix(undefined)).toBe('');
      expect(selectedApproachPrefix('')).toBe('');
    });
  });
});
