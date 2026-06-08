import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import { ToolResultBuilder } from '../../support/result-builder';
import { BrowserEvaluateInputSchema, type BrowserEvaluateInput, type BrowserConnectionManager } from '../../../browser';

export class BrowserEvaluateTool implements BuiltinTool<BrowserEvaluateInput> {
  readonly name = 'BrowserEvaluate' as const;
  readonly description = 'Evaluate a JavaScript snippet in the context of the current page.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserEvaluateInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserEvaluateInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: 'Evaluate JavaScript in page',
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserEvaluateInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      const result = await page.evaluate((script: string) => {
        // Indirect eval to avoid bundler scope-hoisting issues while still
        // evaluating the script in the browser page context.
        return (0, eval)(script);
      }, args.script);
      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(typeof result === 'string' ? result : JSON.stringify(result, null, 2));
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Evaluation failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
