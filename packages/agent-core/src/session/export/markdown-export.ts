/**
 * Real-time Markdown session export.
 *
 * Every `context.append_message` record is appended to a single Markdown
 * transcript under a file lock. The writer is best-effort: failures are
 * caught, counted, and reported via the optional `onError` callback so that
 * a full disk or transient lock problem cannot crash the turn.
 */

import { appendFile, mkdir } from 'node:fs/promises';
import { dirname } from 'pathe';

import type { ContentPart } from '@odysseythink/kosong';

import type { AgentRecordOf } from '#/agent/records';
import { withFileLock } from '#/utils/file-lock';
import type { ContextMessage } from '../../agent/context';

export interface SessionMarkdownExportOptions {
  /** Absolute path to the Markdown transcript file. */
  readonly filePath: string;
  /** Optional error sink for append failures. */
  readonly onError?: ((error: unknown) => void) | undefined;
}

/**
 * Append-only Markdown transcript writer.
 */
export class SessionMarkdownExport {
  private _errorCount = 0;

  constructor(private readonly options: SessionMarkdownExportOptions) {}

  get path(): string {
    return this.options.filePath;
  }

  /** Number of append attempts that failed since construction. */
  get errorCount(): number {
    return this._errorCount;
  }

  /**
   * Render and append a single message to the transcript.
   *
   * Errors are caught internally and bump {@link errorCount}; this method
   * never throws.
   */
  async append(record: AgentRecordOf<'context.append_message'>): Promise<void> {
    const block = renderMessage(record.message, record.time);
    try {
      await withFileLock(this.options.filePath, async () => {
        await mkdir(dirname(this.options.filePath), { recursive: true });
        await appendFile(this.options.filePath, `${block}\n\n`, 'utf8');
      });
    } catch (error) {
      this._errorCount += 1;
      this.options.onError?.(error);
    }
  }
}

function renderMessage(message: ContextMessage, time?: number): string {
  const headerLines = [`---`, `role: ${message.role}`];
  if (time !== undefined) {
    headerLines.push(`time: ${new Date(time).toISOString()}`);
  }
  headerLines.push(`---`);

  const body = renderContent(message.content);
  if (body.length > 0) {
    return `${headerLines.join('\n')}\n${body}`;
  }
  return headerLines.join('\n');
}

function renderContent(content: readonly ContentPart[]): string {
  const textParts: string[] = [];
  const nonTextParts: unknown[] = [];

  for (const part of content) {
    if (part.type === 'text') {
      textParts.push(part.text);
    } else if (part.type === 'think') {
      textParts.push(`<think>\n${part.think}\n</think>`);
    } else {
      nonTextParts.push(part);
    }
  }

  const lines: string[] = [];
  if (textParts.length > 0) {
    lines.push(textParts.join('\n\n'));
  }
  if (nonTextParts.length > 0) {
    lines.push('```json');
    lines.push(JSON.stringify(nonTextParts, null, 2));
    lines.push('```');
  }
  return lines.join('\n');
}
