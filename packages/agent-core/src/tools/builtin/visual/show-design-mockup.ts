/**
 * ShowDesignMockupTool — the design-mode "visual companion".
 *
 * During a brainstorming / design session the LLM can render a self-contained
 * HTML mockup (a layout, a side-by-side comparison, a diagram) and ask the host
 * to open it in the user's browser. The HTML is written next to the current
 * design file (under a `.mockups/` directory) and opened via the `openExternal`
 * reverse-RPC. Hosts without a desktop browser leave `openExternal` unhandled,
 * in which case this tool reports that the mockup was written but not opened.
 *
 * This tool is only registered when the host advertises `openExternal`
 * (see `ToolManager.initializeBuiltinTools`).
 */

import type { Agent } from '#/agent';
import { mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './show-design-mockup.md';

// ── Input schema ─────────────────────────────────────────────────────

export interface ShowDesignMockupInput {
  html: string;
  title?: string;
}

export const ShowDesignMockupInputSchema: z.ZodType<ShowDesignMockupInput> = z
  .object({
    html: z
      .string()
      .min(1)
      .describe(
        'A complete, self-contained HTML document (inline CSS only — no external assets) to render as the mockup.',
      ),
    title: z
      .string()
      .default('Design mockup')
      .describe('Short title for this mockup; used for the generated file name.'),
  })
  .strict();

// ── Implementation ───────────────────────────────────────────────────

export class ShowDesignMockupTool implements BuiltinTool<ShowDesignMockupInput> {
  readonly name = 'ShowDesignMockup' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ShowDesignMockupInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: ShowDesignMockupInput): ToolExecution {
    const title = args.title ?? 'Design mockup';
    return {
      description: `Showing design mockup: ${title}`,
      approvalRule: this.name,
      execute: () => this.execution(args, title),
    };
  }

  private async execution(args: ShowDesignMockupInput, title: string): Promise<ExecutableToolResult> {
    const openExternal = this.agent.rpc?.openExternal;
    if (openExternal === undefined) {
      return {
        isError: true,
        output:
          'Visual companion is not available in this host (no openExternal). Describe the mockup in text instead.',
      };
    }

    let file: string;
    try {
      file = await this.writeMockup(args.html, title);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return { isError: true, output: `Failed to write mockup file: ${message}` };
    }

    const url = pathToFileURL(file).href;
    const result = await openExternal({ url, title });
    if (!result.opened) {
      const suffix = result.error !== undefined ? `: ${result.error}` : '';
      return {
        isError: false,
        output: `Wrote mockup to ${file} but the host did not open it${suffix}. Share the path with the user or describe the mockup in text.`,
      };
    }

    return {
      isError: false,
      output: `Opened mockup in the user's browser: ${file}`,
    };
  }

  private async writeMockup(html: string, title: string): Promise<string> {
    const designPath = this.agent.planMode.advancedSessionModeFilePath;
    const baseDir =
      designPath !== null && designPath.length > 0
        ? join(dirname(designPath), '.mockups')
        : join(tmpdir(), 'ody-design-mockups');
    await mkdir(baseDir, { recursive: true });
    const slug = slugify(title);
    const file = join(baseDir, `${String(Date.now())}-${slug}.html`);
    await writeFile(file, html, 'utf8');
    return file;
  }
}

function slugify(title: string): string {
  const slug = title
    .toLowerCase()
    .replaceAll(/[^a-z0-9]+/g, '-')
    .replaceAll(/^-+|-+$/g, '')
    .slice(0, 40);
  return slug.length > 0 ? slug : 'mockup';
}
