import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { BrowserClickInputSchema, type BrowserClickInput, type BrowserConnectionManager } from '../../../browser';

export class BrowserClickTool implements BuiltinTool<BrowserClickInput> {
  readonly name = 'BrowserClick' as const;
  readonly description = 'Click an element on the current page by CSS selector.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserClickInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserClickInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: `Click ${args.selector}`,
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserClickInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.click(args.selector);
      return { output: `Clicked ${args.selector}`, isError: false };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to click: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
