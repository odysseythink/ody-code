import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/ody-tui';
import type { SlashCommandHost } from './dispatch';

interface SlashArgs {
  readonly base?: string;
  readonly head?: string;
  readonly pr?: string;
  readonly model?: string;
  readonly description?: string;
  readonly requirements?: string;
}

function parseArgs(args: string): SlashArgs {
  const tokens = args.trim().split(/\s+/).filter((t) => t.length > 0);
  const result: Record<string, unknown> = {};
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;
    if (
      token === '--base' || token === '--head' || token === '--pr' || token === '--model' ||
      token === '--description' || token === '--requirements'
    ) {
      result[token.replace(/^--/, '')] = tokens[i + 1];
      i += 1;
    } else if (result['base'] === undefined) {
      result['base'] = token;
    } else if (result['head'] === undefined) {
      result['head'] = token;
    }
  }
  return result as unknown as SlashArgs;
}

/**
 * `/request-code-review` mirrors `/receive-code-review`: it activates the
 * `requesting-code-review` skill and asks the agent to run the review. The skill
 * routes to the RequestCodeReview tool, which spawns a read-only reviewer
 * subagent on the configured reviewer model. (The deterministic diff-only engine
 * remains available via the CLI `request-code-review` subcommand for scripting.)
 */
export async function handleRequestCodeReviewCommand(
  host: SlashCommandHost,
  args: string,
): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  try {
    await session.activateSkill('requesting-code-review');
  } catch (error) {
    host.showError(
      `Failed to load requesting-code-review skill: ${error instanceof Error ? error.message : String(error)}`,
    );
    return;
  }

  const parsed = parseArgs(args);
  const hints: string[] = [];
  if (parsed.pr !== undefined) hints.push(`pr=${parsed.pr}`);
  if (parsed.base !== undefined) hints.push(`base=${parsed.base}`);
  if (parsed.head !== undefined) hints.push(`head=${parsed.head}`);
  if (parsed.model !== undefined) hints.push(`model=${parsed.model}`);
  if (parsed.description !== undefined) hints.push(`description=${JSON.stringify(parsed.description)}`);
  if (parsed.requirements !== undefined) hints.push(`requirements=${JSON.stringify(parsed.requirements)}`);
  const argLine = hints.length > 0 ? ` Pass these arguments to the tool: ${hints.join(', ')}.` : '';

  host.sendNormalUserInput(
    `Review the current changes by calling the RequestCodeReview tool, then summarize the findings and address the Critical and Important ones.${argLine}`,
  );
}
