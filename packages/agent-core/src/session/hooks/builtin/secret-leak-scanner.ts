import type { SecretScanConfig } from '@odysseythink/agent-core-shared';

import { createDefaultScanner } from '#security/secret-scan/rules';

import type { BuiltinHook, HookResult } from '../types';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export class SecretLeakScannerBuiltin implements BuiltinHook {
  readonly id = 'secret-leak-scanner';
  private readonly scanner;
  private readonly blockOnMatch;

  constructor(config?: SecretScanConfig) {
    this.scanner = createDefaultScanner({
      maxScanBytes: config?.maxScanBytes,
      entropyThreshold: config?.entropyThreshold,
      allowList: config?.allowList,
    });
    this.blockOnMatch = config?.blockOnMatch ?? false;
  }

  async run(
    input: Record<string, unknown>,
    _ctx: {
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
