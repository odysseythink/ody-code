const GLOB_LITERAL_SPECIAL = /[\\*?[\]{}()!+@|]/g;

export function browserHostApprovalRule(host: string): string {
  return `Browser*(${host.replace(GLOB_LITERAL_SPECIAL, '\\$&')})`;
}

export function matchesBrowserHostRule(ruleArgs: string, host: string): boolean {
  return ruleArgs === host;
}
