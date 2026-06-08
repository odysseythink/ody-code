import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { ToolResultBuilder } from '../../support/result-builder';
import { BrowserSnapshotInputSchema, type BrowserSnapshotInput, type BrowserConnectionManager } from '../../../browser';

export class BrowserSnapshotTool implements BuiltinTool<BrowserSnapshotInput> {
  readonly name = 'BrowserSnapshot' as const;
  readonly description = 'Get the text content of the current browser page, optionally scoped to a CSS selector.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserSnapshotInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserSnapshotInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: args.selector ? `Snapshot of ${args.selector}` : 'Snapshot of current page',
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserSnapshotInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      let text: string;
      if (args.selector) {
        const element = await page.$(args.selector);
        if (!element) {
          return { output: `Element not found: ${args.selector}`, isError: true };
        }
        text = await element.evaluate((el: unknown) => (el as { textContent?: string | null }).textContent ?? '');
      } else {
        // @ts-expect-error document is a browser global inside page.evaluate
        text = await page.evaluate(() => document.body.innerText);
      }
      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(text);
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to get snapshot: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
