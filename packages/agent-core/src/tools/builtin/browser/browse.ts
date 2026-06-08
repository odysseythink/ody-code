import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { ToolResultBuilder } from '../../support/result-builder';
import {
  BrowserBrowseInputSchema,
  type BrowserBrowseInput,
  type BrowserConnectionManager,
} from '../../../browser';
import { browserHostApprovalRule, matchesBrowserHostRule, truncateText } from './_utils';

export class BrowserBrowseTool implements BuiltinTool<BrowserBrowseInput> {
  readonly name = 'BrowserBrowse' as const;
  readonly description =
    'Navigate to a URL, wait for the page to load, and return the page title, URL, ' +
    'text content (truncated to 8000 chars), and a viewport screenshot.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserBrowseInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserBrowseInput): ToolExecution {
    const host = new URL(args.url).host;
    return {
      accesses: ToolAccesses.none(),
      description: `Browsing ${args.url}`,
      approvalRule: browserHostApprovalRule(host),
      matchesRule: (ruleArgs) => matchesBrowserHostRule(ruleArgs, host),
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserBrowseInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.goto(args.url, { waitUntil: 'networkidle2', timeout: 30000 });

      if (args.waitFor !== undefined) {
        if (typeof args.waitFor === 'number') {
          await new Promise((resolve) => setTimeout(resolve, args.waitFor as number));
        } else {
          await page.waitForSelector(args.waitFor, { timeout: 10000 });
        }
      }

      const title = await page.title();
      const url = page.url();

      let extractedData: unknown | undefined;
      let content: string | undefined;

      if (args.extract) {
        extractedData = await page.evaluate((schema: Record<string, string>) => {
          const result: Record<string, string> = {};
          for (const [key, selector] of Object.entries(schema)) {
            // @ts-expect-error document is a browser global inside page.evaluate
            const el = document.querySelector(selector);
            result[key] = el ? (el.textContent ?? '') : '';
          }
          return result;
        }, args.extract);
      } else {
        // @ts-expect-error document is a browser global inside page.evaluate
        const rawText = await page.evaluate(() => document.body.innerText);
        content = truncateText(rawText, 8000);
      }

      const screenshot = await page.screenshot({
        encoding: 'base64',
        fullPage: false,
      });

      const result: {
        success: true;
        url: string;
        title: string;
        content?: string;
        data?: unknown;
        screenshot?: string;
      } = {
        success: true,
        url,
        title,
      };

      if (content !== undefined) result.content = content;
      if (extractedData !== undefined) result.data = extractedData;
      if (typeof screenshot === 'string') result.screenshot = screenshot;

      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(JSON.stringify(result, null, 2));
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(
        JSON.stringify({ success: false, url: args.url, title: '', error: msg }, null, 2),
      );
      return builder.error('Browser browse failed');
    } finally {
      handle.releasePage(page);
    }
  }
}
