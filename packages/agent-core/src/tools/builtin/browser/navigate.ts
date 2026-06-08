import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { BrowserNavigateInputSchema, type BrowserNavigateInput, type BrowserConnectionManager } from '../../../browser';
import { browserHostApprovalRule, matchesBrowserHostRule } from './_utils';

export class BrowserNavigateTool implements BuiltinTool<BrowserNavigateInput> {
  readonly name = 'BrowserNavigate' as const;
  readonly description = 'Navigate the browser to a specific URL.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserNavigateInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserNavigateInput): ToolExecution {
    const host = new URL(args.url).host;
    return {
      accesses: ToolAccesses.none(),
      description: `Navigating to ${args.url}`,
      approvalRule: browserHostApprovalRule(host),
      matchesRule: (ruleArgs) => matchesBrowserHostRule(ruleArgs, host),
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserNavigateInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.goto(args.url, { waitUntil: 'networkidle2', timeout: 30000 });
      return { output: `Navigated to ${args.url}`, isError: false };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to navigate: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
