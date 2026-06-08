const GLOB_LITERAL_SPECIAL = /[\\*?[\]{}()!+@|]/g;

export function browserHostApprovalRule(host: string): string {
  return `Browser*(${host.replace(GLOB_LITERAL_SPECIAL, '\\$&')})`;
}

export function matchesBrowserHostRule(ruleArgs: string, host: string): boolean {
  return ruleArgs === host;
}

export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength) + '\n[...truncated]';
}
