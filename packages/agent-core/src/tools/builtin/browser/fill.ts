import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { BrowserFillInputSchema, type BrowserFillInput, type BrowserConnectionManager } from '../../../browser';

export class BrowserFillTool implements BuiltinTool<BrowserFillInput> {
  readonly name = 'BrowserFill' as const;
  readonly description = 'Fill an input element on the current page by CSS selector.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserFillInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserFillInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: `Fill ${args.selector}`,
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserFillInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.type(args.selector, args.value);
      return { output: `Filled ${args.selector}`, isError: false };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to fill: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
