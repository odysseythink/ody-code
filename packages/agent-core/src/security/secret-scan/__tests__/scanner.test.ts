import { describe, expect, it } from 'vitest';
import { SecretLeakScanner } from '../scanner';

describe('SecretLeakScanner', () => {
  it('detects an AWS access key ID', () => {
    const scanner = new SecretLeakScanner([
      {
        id: 'aws-access-key-id',
        name: 'AWS Access Key ID',
        pattern: /\b(AKIA[0-9A-Z]{16})\b/g,
      },
    ]);
    const text = 'export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE';
    const matches = scanner.scan(text);
    expect(matches).toHaveLength(1);
    expect(matches[0]).toMatchObject({
      ruleId: 'aws-access-key-id',
      matchedText: 'AKIAIOSFODNN7EXAMPLE',
    });
  });

  it('filters low-entropy matches when entropyMin is set', () => {
    const scanner = new SecretLeakScanner([
      {
        id: 'high-entropy-secret',
        name: 'High Entropy Secret',
        pattern: /\b([A-Za-z0-9]{32})\b/g,
        entropyMin: 4.5,
      },
    ]);
    // 32 个 'a' 的熵为 0，不应命中
    const low = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
    expect(scanner.scan(low)).toHaveLength(0);
    // 随机-looking 32 字符应命中
    const high = 'aBc9xK2mP0vL8nQ4rT6wZ1yF3hJ5gD7e';
    expect(scanner.scan(high)).toHaveLength(1);
  });

  it('truncates input to maxScanBytes and reports truncation', () => {
    const scanner = new SecretLeakScanner(
      [{ id: 'x', name: 'X', pattern: /x/g }],
      { maxScanBytes: 10 },
    );
    const text = 'xxxxxxxxxxxxxxxxxxxx';
    const matches = scanner.scan(text);
    // 只扫描前 10 字节，命中 10 次
    expect(matches).toHaveLength(10);
  });

  it('returns empty array for normal URLs without credentials', () => {
    const scanner = new SecretLeakScanner([
      {
        id: 'generic-api-key',
        name: 'Generic API Key',
        pattern: /(?:api[_-]?key|apikey)\s*[:=]\s*['"]?([A-Za-z0-9_\-/+=]{16,})['"]?/gi,
        entropyMin: 3.5,
      },
    ]);
    expect(scanner.scan('curl https://api.example.com/v1/users')).toHaveLength(0);
  });
});
