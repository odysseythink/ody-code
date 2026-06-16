import type { MessageKey, SupportedLanguage } from './types';
import { translations } from './translations';

export { translations } from './translations';
export type { MessageKey, SupportedLanguage } from './types';

export function t(
  key: MessageKey,
  lang: SupportedLanguage | undefined,
  fallback?: string,
): string {
  if (lang !== undefined && translations[lang] !== undefined && translations[lang][key] !== undefined) {
    return translations[lang][key];
  }
  const enText = translations['en'][key];
  if (enText !== undefined) return enText;
  if (fallback !== undefined) return fallback;
  return key;
}

export function isSupportedLanguage(value: unknown): value is SupportedLanguage {
  return value === 'en' || value === 'zh';
}

export function normalizeLanguage(value: string): SupportedLanguage {
  const normalized = value.toLowerCase().split('-')[0] ?? '';
  if (['zh', 'zh_cn', 'zh_tw', 'zh_hk'].includes(normalized)) return 'zh';
  return 'en';
}
