import { resolve } from 'pathe';

import type { BuiltinHook, HookResult } from '../types';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export class EditAccumulatorBuiltin implements BuiltinHook {
  readonly id = 'edit-accumulator';
  private readonly paths = new Set<string>();

  async run(
    input: Record<string, unknown>,
    ctx: {
      readonly cwd: string | undefined;
      readonly env: Readonly<Record<string, string | undefined>>;
      readonly signal?: AbortSignal;
      readonly timeout: number;
    },
  ): Promise<HookResult> {
    const toolName = typeof input.toolName === 'string' ? input.toolName : '';
    const toolInput = isRecord(input.toolInput) ? input.toolInput : {};
    const path = typeof toolInput.path === 'string' ? toolInput.path : undefined;

    if (path !== undefined && (toolName === 'Edit' || toolName === 'Write')) {
      this.paths.add(resolve(ctx.cwd ?? '.', path));
    }

    return { action: 'allow', stdout: '' };
  }

  readAndClear(): string[] {
    const out = Array.from(this.paths);
    this.paths.clear();
    return out;
  }
}
