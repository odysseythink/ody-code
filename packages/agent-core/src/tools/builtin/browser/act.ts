import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import {
  BrowserActInputSchema,
  type BrowserActInput,
  type BrowserConnectionManager,
} from '../../../browser';

export class BrowserActTool implements BuiltinTool<BrowserActInput> {
  readonly name = 'BrowserAct' as const;
  readonly description =
    'Perform an action on the current page: click, type, scroll, screenshot, or wait.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserActInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserActInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: `Browser act: ${args.action}`,
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserActInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      switch (args.action) {
        case 'click': {
          if (!args.selector) {
            return { output: 'Click action requires a selector', isError: true };
          }
          await page.click(args.selector);
          return { output: `Clicked ${args.selector}`, isError: false };
        }
        case 'type': {
          if (!args.selector || args.value === undefined) {
            return { output: 'Type action requires selector and value', isError: true };
          }
          await page.type(args.selector, args.value);
          return { output: `Typed into ${args.selector}`, isError: false };
        }
        case 'scroll_down': {
          // @ts-expect-error window is a browser global inside page.evaluate
          await page.evaluate(() => window.scrollBy(0, window.innerHeight));
          return { output: 'Scrolled down', isError: false };
        }
        case 'scroll_up': {
          // @ts-expect-error window is a browser global inside page.evaluate
          await page.evaluate(() => window.scrollBy(0, -window.innerHeight));
          return { output: 'Scrolled up', isError: false };
        }
        case 'screenshot': {
          const screenshot = await page.screenshot({ encoding: 'base64', fullPage: false });
          return {
            output: `Screenshot (base64):\n${screenshot}`,
            isError: false,
          };
        }
        case 'wait': {
          if (args.selector) {
            await page.waitForSelector(args.selector, { timeout: 10000 });
            return { output: `Waited for ${args.selector}`, isError: false };
          }
          return { output: 'Wait action requires a selector', isError: true };
        }
        default:
          return { output: `Unknown action: ${args.action}`, isError: true };
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Action failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
