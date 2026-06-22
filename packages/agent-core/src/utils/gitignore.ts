import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';

const GITIGNORE_ENTRY = '.ody-code/';

export async function ensureGitignore(
  cwd: string,
  kaos: Pick<Kaos, 'readText' | 'writeText'>,
): Promise<void> {
  const gitignorePath = join(cwd, '.gitignore');
  try {
    const content = await kaos.readText(gitignorePath);
    if (content.trim().length === 0) {
      await kaos.writeText(gitignorePath, GITIGNORE_ENTRY + '\n');
      return;
    }
    const lines = content.split('\n');
    for (const line of lines) {
      if (line.trim() === GITIGNORE_ENTRY) {
        return;
      }
    }
    const separator = content.endsWith('\n') ? '' : '\n';
    await kaos.writeText(gitignorePath, content + separator + GITIGNORE_ENTRY + '\n');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      await kaos.writeText(gitignorePath, GITIGNORE_ENTRY + '\n');
    } else {
      throw error;
    }
  }
}
