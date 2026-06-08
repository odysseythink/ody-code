export { browserHostApprovalRule, matchesBrowserHostRule } from '../../support/browser-rule-match';

export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength) + '\n[...truncated]';
}
