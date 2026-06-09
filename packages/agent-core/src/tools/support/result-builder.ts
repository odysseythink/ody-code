import type { ExecutableToolResult } from '../../loop/types';

export interface ToolResultBuilderOptions {
  maxLineLength?: number | null;
}

export class ToolResultBuilder {
  private readonly maxLineLength: number | null;
  private readonly chunks: string[] = [];
  nChars = 0;

  constructor(options?: ToolResultBuilderOptions) {
    this.maxLineLength = options?.maxLineLength === undefined ? 500 : options.maxLineLength;
  }

  write(text: string): void {
    if (this.maxLineLength !== null) {
      const limit = this.maxLineLength;
      this.chunks.push(
        text
          .split('\n')
          .map((line) => (line.length > limit ? `${line.slice(0, limit)}…` : line))
          .join('\n'),
      );
    } else {
      this.chunks.push(text);
    }
    this.nChars += text.length;
  }

  ok(message?: string, options?: { brief?: string }): ExecutableToolResult {
    return {
      output: this.buildOutput(),
      isError: false,
      message: options?.brief ?? message,
    };
  }

  error(message: string, options?: { brief?: string }): ExecutableToolResult {
    return {
      output: this.buildOutput(),
      isError: true,
      message: options?.brief ?? message,
    };
  }

  private buildOutput(): string {
    return this.chunks.join('');
  }
}
