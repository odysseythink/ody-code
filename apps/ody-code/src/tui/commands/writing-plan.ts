import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/ody-tui';
import { formatErrorMessage } from '../utils/event-payload';
import type { SlashCommandHost } from './dispatch';

// ---------------------------------------------------------------------------
// /writing-plan <file> — plan-mode-only: convert a source file into a plan
// ---------------------------------------------------------------------------

/**
 * Lock the plan file to an explicit source file's name and kick off the
 * conversion. Requires a file argument; errors when missing or the file does
 * not exist. Once used, plan naming no longer depends on the design→plan
 * filename handoff — the plan file is named after the provided source file.
 */
export async function handleWritingPlanCommand(host: SlashCommandHost, args: string): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const filePath = args.trim();
  if (filePath.length === 0) {
    host.showError('用法: /writing-plan <文件路径>');
    return;
  }

  try {
    // Enters plan mode and locks the plan filename to the source file; the agent
    // rejects here if the source file does not exist.
    await session.writingPlan({ filePath });
    host.setAppState({ sessionMode: 'plan' });
    const plan = await session.getPlan().catch(() => null);
    const planPath = plan?.path ?? null;
    host.showNotice('writing-plan', `执行计划将写入: ${planPath ?? '(待定)'}`);
    // Start the conversion turn. Reference the concrete destination path when
    // known so the model writes to the locked plan file, not an invented one.
    const target = planPath !== null ? `计划文件 ${planPath}` : '当前 plan 模式的计划文件';
    host.sendNormalUserInput(
      `请阅读文件 ${filePath} 的内容，并将其转换为一份完整、可执行的执行计划，写入${target}。`,
    );
  } catch (error) {
    host.showError(`writing-plan 失败: ${formatErrorMessage(error)}`);
  }
}
