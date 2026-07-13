import { describe, expect, it } from 'vitest';
import { t, isSupportedLanguage, normalizeLanguage } from '../../src/i18n';

describe('t', () => {
  it('returns Chinese string for zh language', () => {
    expect(t('product.entered', 'zh')).toBe('Office Hours 模式已激活。');
  });

  it('returns English string for en language', () => {
    expect(t('product.entered', 'en')).toBe('Office hours mode is now active.');
  });

  it('falls back to English when lang is undefined', () => {
    expect(t('product.entered', undefined)).toBe('Office hours mode is now active.');
  });

  it('falls back to English for unsupported language (cast)', () => {
    expect(t('product.entered', 'fr' as any)).toBe('Office hours mode is now active.');
  });

  it('falls back to key string when key is missing in both languages', () => {
    expect(t('nonexistent.key' as any, 'zh')).toBe('nonexistent.key');
  });

  it('returns Chinese text with placeholder for learningRecorded', () => {
    expect(t('product.learningRecorded', 'zh')).toContain('{key}');
  });
});
