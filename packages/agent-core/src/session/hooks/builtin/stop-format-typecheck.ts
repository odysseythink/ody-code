import { runHook } from '../runner';
import type { BuiltinHook, HookResult } from '../types';
import type { EditAccumulatorBuiltin } from './edit-accumulator';
import {
  resolveFormatterCommand,
  resolveTypecheckCommand,
} from './resolve-formatter';

export class StopFormatTypecheckBuiltin implements BuiltinHook {
  readonly id = 'stop-format-typecheck';

  constructor(private readonly accumulator: EditAccumulatorBuiltin) {}

  async run(
    _input: Record<string, unknown>,
    ctx: {
      readonly cwd: string | undefined;
      readonly env: Readonly<Record<string, string | undefined>>;
      readonly signal?: AbortSignal;
      readonly timeout: number;
    },
  ): Promise<HookResult> {
    const files = this.accumulator.readAndClear();
    const cwd = ctx.cwd ?? '.';

    if (files.length === 0) {
      return { action: 'allow', stdout: 'no files to check' };
    }

    const fileList = files.map((f) => JSON.stringify(f)).join(' ');

    const formatter = await resolveFormatterCommand(cwd);
    if (formatter !== undefined) {
      const result = await runHook(`${formatter} ${fileList}`, {}, {
        timeout: ctx.timeout,
        cwd,
        signal: ctx.signal,
      });
      if (result.action === 'block' || (result.exitCode ?? 0) !== 0) {
        return {
          action: 'block',
          reason: result.reason ?? result.stderr ?? 'formatter failed',
          stderr: result.stderr,
          exitCode: result.exitCode,
        };
      }
    }

    const typecheck = await resolveTypecheckCommand(cwd);
    if (typecheck !== undefined) {
      const result = await runHook(typecheck, {}, {
        timeout: ctx.timeout,
        cwd,
        signal: ctx.signal,
      });
      if (result.action === 'block' || (result.exitCode ?? 0) !== 0) {
        return {
          action: 'block',
          reason: result.reason ?? result.stderr ?? 'typecheck failed',
          stderr: result.stderr,
          exitCode: result.exitCode,
        };
      }
    }

    return { action: 'allow', stdout: `checked ${files.length} file(s)` };
  }
}
