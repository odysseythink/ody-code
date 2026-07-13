import { createHash } from 'node:crypto';
import { appendFile, mkdir } from 'node:fs/promises';
import { dirname } from 'pathe';

export interface SecretScanFindingRecordInput {
  readonly ts: number;
  readonly sessionId: string;
  readonly toolName: string;
  readonly ruleId: string;
  readonly ruleName: string;
  readonly matchedText: string;
  readonly action: 'warn' | 'block';
}

export interface SecretScanFindingRecord {
  readonly ts: number;
  readonly sessionId: string;
  readonly toolName: string;
  readonly ruleId: string;
  readonly ruleName: string;
  readonly matchedHash: string;
  readonly matchedPrefix: string;
  readonly action: 'warn' | 'block';
}

export function hashMatchedText(text: string): string {
  return createHash('sha256').update(text).digest('hex');
}

export class SecretScanAuditLog {
  constructor(private readonly logPath: string) {}

  async write(input: SecretScanFindingRecordInput): Promise<void> {
    const record: SecretScanFindingRecord = {
      ts: input.ts,
      sessionId: input.sessionId,
      toolName: input.toolName,
      ruleId: input.ruleId,
      ruleName: input.ruleName,
      matchedHash: hashMatchedText(input.matchedText),
      matchedPrefix: input.matchedText.slice(0, 4),
      action: input.action,
    };
    await mkdir(dirname(this.logPath), { recursive: true });
    await appendFile(this.logPath, `${JSON.stringify(record)}\n`, 'utf8');
  }
}
