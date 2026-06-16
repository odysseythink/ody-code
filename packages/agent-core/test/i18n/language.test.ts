import { describe, expect, it } from 'vitest';
import { isSupportedLanguage, normalizeLanguage } from '../../src/i18n';

describe('isSupportedLanguage', () => {
  it('accepts zh', () => expect(isSupportedLanguage('zh')).toBe(true));
  it('accepts en', () => expect(isSupportedLanguage('en')).toBe(true));
  it('rejects fr', () => expect(isSupportedLanguage('fr')).toBe(false));
  it('rejects undefined', () => expect(isSupportedLanguage(undefined)).toBe(false));
  it('rejects "cn"', () => expect(isSupportedLanguage('cn')).toBe(false));
});

describe('normalizeLanguage', () => {
  it('maps ZH-CN to zh', () => expect(normalizeLanguage('ZH-CN')).toBe('zh'));
  it('maps zh-TW to zh', () => expect(normalizeLanguage('zh-TW')).toBe('zh'));
  it('maps fr to en', () => expect(normalizeLanguage('fr')).toBe('en'));
  it('maps empty string to en', () => expect(normalizeLanguage('')).toBe('en'));
});
