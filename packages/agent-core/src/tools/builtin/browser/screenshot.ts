import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { BrowserScreenshotInputSchema, type BrowserScreenshotInput, type BrowserConnectionManager } from '../../../browser';

export class BrowserScreenshotTool implements BuiltinTool<BrowserScreenshotInput> {
  readonly name = 'BrowserScreenshot' as const;
  readonly description = 'Take a screenshot of the current page.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserScreenshotInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserScreenshotInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: args.fullPage ? 'Full-page screenshot' : 'Viewport screenshot',
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserScreenshotInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      const screenshot = await page.screenshot({
        encoding: 'base64',
        fullPage: args.fullPage ?? false,
      });
      return {
        output: `Screenshot (base64):\n${screenshot}`,
        isError: false,
      };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Screenshot failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
