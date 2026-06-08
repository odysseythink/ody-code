import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { ToolResultBuilder } from '../../support/result-builder';
import {
  BrowserExtractInputSchema,
  type BrowserExtractInput,
  type BrowserConnectionManager,
} from '../../../browser';
import { browserHostApprovalRule, matchesBrowserHostRule } from './_utils';

export class BrowserExtractTool implements BuiltinTool<BrowserExtractInput> {
  readonly name = 'BrowserExtract' as const;
  readonly description =
    'Extract structured data from the current page or a specified URL using CSS selectors.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserExtractInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserExtractInput): ToolExecution {
    const host = args.url ? new URL(args.url).host : undefined;
    return {
      accesses: ToolAccesses.none(),
      description: host ? `Extract from ${args.url}` : 'Extract from current page',
      approvalRule: host ? browserHostApprovalRule(host) : this.name,
      matchesRule: host ? (ruleArgs) => matchesBrowserHostRule(ruleArgs, host) : undefined,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserExtractInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      if (args.url) {
        await page.goto(args.url, { waitUntil: 'networkidle2', timeout: 30000 });
      }

      const data = await page.evaluate((schema: Record<string, string>) => {
        const result: Record<string, string> = {};
        for (const [key, selector] of Object.entries(schema)) {
          // @ts-expect-error document is a browser global inside page.evaluate
          const el = document.querySelector(selector);
          result[key] = el ? (el.textContent ?? '') : '';
        }
        return result;
      }, args.schema);

      const url = page.url();
      const title = await page.title();

      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(JSON.stringify({ success: true, url, title, data }, null, 2));
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Extraction failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
