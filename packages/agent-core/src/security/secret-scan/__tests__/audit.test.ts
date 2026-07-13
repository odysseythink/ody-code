import { describe, expect, it } from 'vitest';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import { SecretScanAuditLog, hashMatchedText } from '../audit';

describe('SecretScanAuditLog', () => {
  it('writes redacted findings as JSONL', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'secret-scan-audit-'));
    const logPath = join(dir, 'secret-scan.jsonl');
    const log = new SecretScanAuditLog(logPath);
    await log.write({
      ts: 1700000000000,
      sessionId: 'session-1',
      toolName: 'Bash',
      ruleId: 'aws-access-key-id',
      ruleName: 'AWS Access Key ID',
      matchedText: 'AKIAIOSFODNN7EXAMPLE',
      action: 'warn',
    });
    const content = await readFile(logPath, 'utf8');
    const record = JSON.parse(content.trim());
    expect(record.ts).toBe(1700000000000);
    expect(record.sessionId).toBe('session-1');
    expect(record.toolName).toBe('Bash');
    expect(record.ruleId).toBe('aws-access-key-id');
    expect(record.matchedHash).toBe(hashMatchedText('AKIAIOSFODNN7EXAMPLE'));
    expect(record.matchedPrefix).toBe('AKIA');
    expect(record.matchedText).toBeUndefined();
    expect(record.action).toBe('warn');
    await rm(dir, { recursive: true, force: true });
  });

  it('creates parent directories lazily', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'secret-scan-audit-'));
    const nested = join(dir, 'a', 'b', 'secret-scan.jsonl');
    const log = new SecretScanAuditLog(nested);
    await log.write({
      ts: 1,
      sessionId: 's',
      toolName: 'Bash',
      ruleId: 'x',
      ruleName: 'X',
      matchedText: 'secret',
      action: 'block',
    });
    const content = await readFile(nested, 'utf8');
    expect(content.trim().length).toBeGreaterThan(0);
    await rm(dir, { recursive: true, force: true });
  });
});
