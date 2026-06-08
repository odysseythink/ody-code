import { appendFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';

import type { MCPToolResult } from './types';

export interface TraceRecord {
  readonly timestamp: string;
  readonly toolName: string;
  readonly args: Record<string, unknown>;
  readonly resultSummary: {
    readonly isError: boolean;
    readonly contentTypes: string[];
    readonly hasScreenshot: boolean;
    readonly screenshotFiles: string[];
  };
}

const SENSITIVE_KEYS = new Set([
  'password',
  'token',
  'secret',
  'apikey',
  'api_key',
  'auth',
  'cookie',
]);

export class ChromeTraceRecorder {
  private readonly manifestPath: string;
  private readonly screenshotsDir: string;
  private seq = 0;

  constructor(private readonly traceDir: string) {
    this.manifestPath = join(traceDir, 'manifest.jsonl');
    this.screenshotsDir = join(traceDir, 'screenshots');
  }

  async record(
    toolName: string,
    args: Record<string, unknown>,
    result: MCPToolResult,
  ): Promise<void> {
    try {
      await this.ensureDirs();
      const screenshotFiles: string[] = [];
      const contentTypes: string[] = [];
      let hasScreenshot = false;

      for (const block of result.content) {
        contentTypes.push(block.type);
        if (block.type === 'image' && typeof block.data === 'string') {
          hasScreenshot = true;
          const fileName = `${String(++this.seq).padStart(4, '0')}-${toolName}.png`;
          const filePath = join(this.screenshotsDir, fileName);
          await writeFile(filePath, Buffer.from(block.data, 'base64'));
          screenshotFiles.push(fileName);
        }
      }

      const record: TraceRecord = {
        timestamp: new Date().toISOString(),
        toolName,
        args: this.sanitizeArgs(args),
        resultSummary: {
          isError: result.isError,
          contentTypes,
          hasScreenshot,
          screenshotFiles,
        },
      };

      await appendFile(
        this.manifestPath,
        JSON.stringify(record) + '\n',
        'utf-8',
      );
    } catch {
      // Silently drop trace write failures to avoid disrupting the main flow.
    }
  }

  private async ensureDirs(): Promise<void> {
    await mkdir(this.traceDir, { recursive: true });
    await mkdir(this.screenshotsDir, { recursive: true });
  }

  private sanitizeArgs(
    args: Record<string, unknown>,
  ): Record<string, unknown> {
    return this.sanitizeValue(args) as Record<string, unknown>;
  }

  private sanitizeValue(value: unknown): unknown {
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      const sanitized: Record<string, unknown> = {};
      for (const [key, nestedValue] of Object.entries(value)) {
        if (SENSITIVE_KEYS.has(key.toLowerCase())) {
          sanitized[key] = '<redacted>';
        } else {
          sanitized[key] = this.sanitizeValue(nestedValue);
        }
      }
      return sanitized;
    }
    return value;
  }
}
