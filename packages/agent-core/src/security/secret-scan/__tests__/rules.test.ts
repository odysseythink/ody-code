import { describe, expect, it } from 'vitest';
import { createDefaultScanner } from '../rules';

describe('createDefaultScanner', () => {
  const scanner = createDefaultScanner();

  it('detects real-looking secrets and ignores must-survive inputs', () => {
    const text = `
      export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
      export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
      api_key=EXAMPLE_KEY
      password=1234567890abcdef
      token=aBc9xK2mP0vL8nQ4rT6wZ1yF3hJ5gD7eA
    `;
    const matches = scanner.scan(text);
    const ruleIds = matches.map((m) => m.ruleId);
    expect(ruleIds).toContain('aws-access-key-id');
    expect(ruleIds).toContain('aws-secret-access-key');
    expect(ruleIds).not.toContain('generic-api-key'); // EXAMPLE_KEY 被允许列表跳过
    expect(ruleIds).not.toContain('generic-secret'); // 1234567890abcdef 被允许列表跳过
    expect(ruleIds).toContain('generic-token');
  });

  it('does not flag normal URLs or commands', () => {
    const text = 'curl -s https://api.github.com/repos/odysseythink/ody-code | jq .name';
    expect(scanner.scan(text)).toHaveLength(0);
  });
});
