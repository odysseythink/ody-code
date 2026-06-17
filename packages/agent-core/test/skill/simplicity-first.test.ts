import { describe, expect, it } from 'vitest';
import { filterSimplicityLevels, parseSimplicityLevel } from '../../src/skill/builtin/simplicity-first';
import type { SimplicityLevel } from '../../src/skill/builtin/simplicity-first';

// ============================================================
// filterSimplicityLevels
// ============================================================

describe('filterSimplicityLevels', () => {
  it('returns body unchanged when no level blocks present', () => {
    const body = '# Rules\n\nUse stdlib.';
    expect(filterSimplicityLevels(body, 'full')).toBe('# Rules\n\nUse stdlib.');
  });

  it('keeps content inside a block matching the current level', () => {
    const body = 'before<!-- FULL[ -->inside<!-- ]FULL -->after';
    const result = filterSimplicityLevels(body, 'full');
    expect(result).toBe('beforeinsideafter');
    expect(result).not.toContain('<!--');
  });

  it('removes content inside a block not matching the current level', () => {
    const body = 'before<!-- LITE[ -->inside<!-- ]LITE -->after';
    const result = filterSimplicityLevels(body, 'full');
    expect(result).toBe('beforeafter');
  });

  it('keeps unannotated content and filters level-specific blocks', () => {
    const body = '<!-- FULL[ -->A<!-- ]FULL --><!-- ULTRA[ -->B<!-- ]ULTRA -->';
    const result = filterSimplicityLevels(body, 'full');
    expect(result).toBe('A');
  });

  it('preserves non-level content between blocks', () => {
    const body = 'pre<!-- FULL[ -->A<!-- ]FULL -->mid<!-- ULTRA[ -->B<!-- ]ULTRA -->post';
    const result = filterSimplicityLevels(body, 'full');
    expect(result).toBe('preAmidpost');
  });

  it('handles case-insensitive tags', () => {
    const body = '<!-- lite[ -->X<!-- ]LITE -->';
    expect(filterSimplicityLevels(body, 'lite')).toBe('X');
  });

  it('consumes unclosed block entirely (to end-of-body)', () => {
    const body = 'pre<!-- LITE[ -->no close';
    expect(filterSimplicityLevels(body, 'full')).toBe('pre');
  });

  it('preserves orphan close tags as text (no matching open)', () => {
    const body = '<!-- ]FULL --> as text';
    expect(filterSimplicityLevels(body, 'full')).toBe('<!-- ]FULL --> as text');
  });

  it('preserves mismatched close tag as text (LITE close when FULL open)', () => {
    const body = '<!-- FULL[ -->X<!-- ]LITE -->';
    // The LITE close does not match the open FULL, so it's literal text.
    // The FULL block remains open and consumes the rest.
    expect(filterSimplicityLevels(body, 'full')).toBe('X<!-- ]LITE -->');
  });

  it('handles empty body', () => {
    expect(filterSimplicityLevels('', 'full')).toBe('');
  });

  it('preserves inner different-level tags as text (no nesting support)', () => {
    const body = '<!-- FULL[ -->A<!-- LITE[ -->inner<!-- ]LITE -->B<!-- ]FULL -->';
    // Inner LITE tags are inside FULL — treated as literal text, not filtered
    expect(filterSimplicityLevels(body, 'full')).toBe('A<!-- LITE[ -->inner<!-- ]LITE -->B');
  });

  it('tolerates whitespace around level name in open tag', () => {
    const body = '<!--    FULL   [ -->X<!-- ] FULL -->';
    expect(filterSimplicityLevels(body, 'full')).toBe('X');
  });

  // --- Must-survive adversarial inputs ---
  it('does not match level names mid-sentence', () => {
    const body = 'The word "full" should survive and "lite" too.';
    expect(filterSimplicityLevels(body, 'full')).toBe(body);
  });

  it('does not match level names in regular HTML comments', () => {
    const body = '<!-- This is a normal comment about full mode -->\n<!-- Another comment -->';
    expect(filterSimplicityLevels(body, 'full')).toBe(body);
  });

  it('rejects open tag without closing angle bracket', () => {
    const body = '<!-- FULL[ X';
    expect(filterSimplicityLevels(body, 'full')).toBe(body);
  });
});

// ============================================================
// parseSimplicityLevel
// ============================================================

describe('parseSimplicityLevel', () => {
  it('defaults to full on empty string', () => {
    expect(parseSimplicityLevel('')).toBe('full');
  });

  it('defaults to full on whitespace-only', () => {
    expect(parseSimplicityLevel('   ')).toBe('full');
  });

  it('parses "lite"', () => {
    expect(parseSimplicityLevel('lite')).toBe('lite');
  });

  it('parses "full"', () => {
    expect(parseSimplicityLevel('full')).toBe('full');
  });

  it('parses "ultra"', () => {
    expect(parseSimplicityLevel('ultra')).toBe('ultra');
  });

  it('handles case-insensitive input', () => {
    expect(parseSimplicityLevel('ULTRA')).toBe('ultra');
    expect(parseSimplicityLevel('Lite')).toBe('lite');
  });

  it('trims leading/trailing spaces', () => {
    expect(parseSimplicityLevel('  ultra  ')).toBe('ultra');
  });

  it('throws OdyError with REQUEST_INVALID for unknown level', () => {
    expect(() => parseSimplicityLevel('extreme')).toThrow('Invalid simplicity level');
    try {
      parseSimplicityLevel('extreme');
    } catch (e: any) {
      expect(e.code).toBe('request.invalid');
    }
  });
});
