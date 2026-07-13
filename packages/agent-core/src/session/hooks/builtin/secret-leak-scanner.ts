import { homedir } from 'node:os';
import { join } from 'pathe';
import type { SecretScanConfig } from '@odysseythink/agent-core-shared';

import { SecretScanAuditLog } from '#security/secret-scan/audit';
import { createDefaultScanner } from '#security/secret-scan/rules';
import { encodeWorkDirKey } from '#session/store/workdir-key';

import type { BuiltinHook, HookResult } from '../types';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export class SecretLeakScannerBuiltin implements BuiltinHook {
  readonly id = 'secret-leak-scanner';
  private readonly scanner;
  private readonly blockOnMatch;
  private readonly auditLog;

  constructor(config?: SecretScanConfig, auditLogPath?: string) {
    this.scanner = createDefaultScanner({
      maxScanBytes: config?.maxScanBytes,
      entropyThreshold: config?.entropyThreshold,
      allowList: config?.allowList,
    });
    this.blockOnMatch = config?.blockOnMatch ?? false;
    this.auditLog = auditLogPath !== undefined ? new SecretScanAuditLog(auditLogPath) : undefined;
  }

  async run(
    input: Record<string, unknown>,
    ctx: {
      readonly cwd: string | undefined;
      readonly env: Readonly<Record<string, string | undefined>>;
      readonly signal?: AbortSignal;
      readonly timeout: number;
    },
  ): Promise<HookResult> {
    const toolNameValue = input['toolName'] ?? input['tool_name'];
    const toolName = typeof toolNameValue === 'string' ? toolNameValue : '';
    const toolInputValue = input['toolInput'] ?? input['tool_input'];
    const toolInput = isRecord(toolInputValue) ? toolInputValue : {};
    const text = extractScanText(toolName, toolInput);
    if (text.length === 0) {
      return { action: 'allow' };
    }

    const matches = this.scanner.scan(text);
    if (matches.length === 0) {
      return { action: 'allow' };
    }

    const uniqueRules = Array.from(new Set(matches.map((m) => m.ruleId)));
    const message = `Potential secrets detected by rules: ${uniqueRules.join(', ')}`;

    const auditLog = this.auditLog ?? resolveAuditLog(input, ctx);
    if (auditLog !== undefined) {
      const sessionIdValue = input['session_id'];
      const sessionId = typeof sessionIdValue === 'string' ? sessionIdValue : '';
      const ts = Date.now();
      for (const match of matches) {
        await auditLog.write({
          ts,
          sessionId,
          toolName,
          ruleId: match.ruleId,
          ruleName: match.ruleName,
          matchedText: match.matchedText,
          action: this.blockOnMatch ? 'block' : 'warn',
        });
      }
    }

    if (this.blockOnMatch) {
      return { action: 'block', reason: message };
    }
    return { action: 'allow', reason: message };
  }
}

function extractScanText(toolName: string, toolInput: Record<string, unknown>): string {
  if (toolName === 'Bash') {
    const command = toolInput['command'];
    return typeof command === 'string' ? command : '';
  }
  return collectStrings(toolInput).join('\n');
}

function collectStrings(value: unknown): string[] {
  if (typeof value === 'string') return [value];
  if (Array.isArray(value)) return value.flatMap(collectStrings);
  if (isRecord(value)) return Object.values(value).flatMap(collectStrings);
  return [];
}

function resolveAuditLog(
  input: Record<string, unknown>,
  ctx: {
    readonly cwd: string | undefined;
    readonly env: Readonly<Record<string, string | undefined>>;
  },
): SecretScanAuditLog | undefined {
  const sessionId = input['session_id'];
  if (typeof sessionId !== 'string' || sessionId.length === 0) return undefined;
  const cwd = ctx.cwd ?? '.';
  const homeDir = ctx.env['ODY_CODE_HOME'] ?? join(homedir(), '.ody-code');
  const wdKey = encodeWorkDirKey(cwd);
  const logPath = join(homeDir, 'sessions', wdKey, sessionId, 'secret-scan.jsonl');
  return new SecretScanAuditLog(logPath);
}
