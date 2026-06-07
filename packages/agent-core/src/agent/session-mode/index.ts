import { randomUUID } from 'node:crypto';
import { basename, dirname, join, normalize } from 'pathe';

import type { Agent } from '..';
import {
  extractFirstHeading,
  formatDatePrefix,
  slugifyTitle,
  buildTitlePrompt,
} from './topic-generator';

/**
 * Whether the current planning session is a regular implementation `plan`
 * or a `design` (brainstorming / spec exploration). Both share this same
 * read-only-with-one-writable-file machinery; only the prompts, the output
 * directory and the surfacing labels differ.
 */
export type SessionModeKind = 'plan' | 'design';

export type SessionModeData = null | {
  id: string;
  content: string;
  path: string;
  kind: SessionModeKind;
};
export type SessionModeFilePath = string | null;

export class SessionMode {
  protected _isActive = false;
  protected _sessionModeId: null | string = null;
  protected _sessionModeFilePath: SessionModeFilePath = null;
  protected _kind: SessionModeKind = 'plan';
  private _preModeModelAlias: { value: string | undefined } | null = null;

  constructor(protected readonly agent: Agent) {}

  createSessionModeId(): string {
    return randomUUID();
  }

  updatePreModeModelAlias(alias: string | undefined): void {
    if (this._preModeModelAlias !== null) {
      this._preModeModelAlias = { value: alias };
    }
  }

  async enter(
    id = this.createSessionModeId(),
    _createFile = false, // ignored — no file is created on enter
    emitStatus = true,
    kind: SessionModeKind = 'plan',
  ): Promise<void> {
    if (this._isActive) {
      if (this._kind === kind) {
        return;
      }
      // Switching directly between plan and design: exit current first.
      this.exit();
    }

    this._isActive = true;
    this._sessionModeId = id;
    this._kind = kind;
    this._sessionModeFilePath = null;

    const modeModel = this.agent.kimiConfig?.modeModels?.[kind];
    if (modeModel !== undefined && modeModel !== this.agent.config.modelAlias) {
      try {
        this.agent.modelProvider?.resolveProviderConfig(modeModel);
        this._preModeModelAlias = { value: this.agent.config.modelAlias };
        this.agent.config.update({ modelAlias: modeModel });
      } catch {
        this.agent.log?.warn(`modeModels.${kind} "${modeModel}" not found, keeping current model`);
        this._preModeModelAlias = null;
      }
    }

    try {
      const { dir, isProjectScoped } = await this.resolveSessionModeDirectory(kind);
      if (isProjectScoped) {
        try {
          await this.ensureGitignore(this.agent.config.cwd);
        } catch (error) {
          this.agent.log?.warn('Failed to update .gitignore', { error });
        }
      }

      this.agent.records.logRecord({
        type: 'session_mode.enter',
        id,
        kind,
      });
    } catch (error) {
      if (this._preModeModelAlias !== null) {
        this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
        this._preModeModelAlias = null;
      }
      this._isActive = false;
      this._sessionModeId = null;
      this._sessionModeFilePath = null;
      this._kind = 'plan';
      throw error;
    }

    if (emitStatus) this.agent.emitStatusUpdated();
  }

  restoreEnter({
    id,
    kind = 'plan',
  }: {
    readonly id: string;
    readonly kind?: SessionModeKind;
  }): void {
    this.agent.replayBuilder.push({
      type: 'session_mode_updated',
      enabled: true,
      kind,
    });

    this._isActive = true;
    this._sessionModeId = id;
    this._kind = kind;
    this._sessionModeFilePath = null;
  }

  cancel(id?: string): void {
    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this._preModeModelAlias = null;
    }
    this.agent.records.logRecord({ type: 'session_mode.cancel', id });
    this.agent.replayBuilder.push({
      type: 'session_mode_updated',
      enabled: false,
      kind: this._kind,
    });
    this._isActive = false;
    this._sessionModeId = null;
    this._sessionModeFilePath = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }

  async clear(): Promise<void> {
    if (!this._sessionModeFilePath) return;
    await this.writeEmptySessionModeFile(this._sessionModeFilePath);
  }

  exit(id?: string): void {
    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this._preModeModelAlias = null;
    }
    this.agent.records.logRecord({ type: 'session_mode.exit', id });
    this.agent.replayBuilder.push({
      type: 'session_mode_updated',
      enabled: false,
      kind: this._kind,
    });
    this._isActive = false;
    this._sessionModeId = null;
    this._sessionModeFilePath = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }

  get isActive() {
    return this._isActive;
  }

  get kind(): SessionModeKind {
    return this._kind;
  }

  get sessionModeFilePath(): SessionModeFilePath {
    return this._sessionModeFilePath;
  }

  /**
   * Whether `path` is part of the current SessionMode's writable fileset. This is the
   * single source of truth the read-only guard ({@link PlanModeGuardDenyPermissionPolicy})
   * uses to decide what Write/Edit may touch while SessionMode mode is active.
   *
   * The set is the main SessionMode file plus `.md` files inside a subdirectory
   * named after the main file stem. Normalizes paths to defend against directory
   * traversal (e.g., ../).
   */
  isWritableSessionModePath(path: string): boolean {
    if (this._sessionModeFilePath === null) return false;
    if (path === this._sessionModeFilePath) return true;

    const mainDir = dirname(this._sessionModeFilePath);
    const mainBase = basename(this._sessionModeFilePath);
    const mainStem = mainBase.slice(0, -'.md'.length);

    const splitDir = normalize(join(mainDir, mainStem));
    const normalizedPath = normalize(path);
    if (!normalizedPath.startsWith(splitDir + '/')) return false;
    if (!basename(normalizedPath).endsWith('.md')) return false;
    return true;
  }

  async resolveFilePathFromContent(content: string): Promise<string> {
    if (this._sessionModeFilePath !== null) {
      return this._sessionModeFilePath;
    }

    const { dir } = await this.resolveSessionModeDirectory(this._kind);

    const heading = extractFirstHeading(content);
    let slug: string;
    if (heading) {
      slug = slugifyTitle(heading);
    } else {
      const title = await this.llmSummarizeTitle(content);
      slug = title ? slugifyTitle(title) : 'untitled';
    }

    const datePrefix = formatDatePrefix(new Date());
    const stem = `${datePrefix}-${slug}`;
    const finalStem = await this.findUniqueStemInDir(dir, stem);
    const path = join(dir, `${finalStem}.md`);

    this._sessionModeFilePath = path;
    this.agent.emitStatusUpdated();
    return path;
  }

  private async llmSummarizeTitle(content: string): Promise<string | null> {
    const prompt = buildTitlePrompt(content);
    try {
      const provider = this.agent.config.provider;
      const result = await this.agent.generate(
        provider,
        '',
        [],
        [{ role: 'user', content: [{ type: 'text', text: prompt }], toolCalls: [] }],
        {},
        { signal: AbortSignal.timeout(5000) },
      );
      const title = result.message.content
        .filter((part) => part.type === 'text')
        .map((part) => part.text)
        .join('')
        .trim();
      return title.length > 0 ? title : null;
    } catch (error) {
      this.agent.log?.warn('Failed to summarize title for plan/design file', { error });
      return null;
    }
  }

  async data(): Promise<SessionModeData> {
    if (!this._sessionModeId || !this._sessionModeFilePath) return null;
    let content = '';
    try {
      content = await this.agent.kaos.readText(this._sessionModeFilePath);
    } catch (error) {
      if (!isMissingFileError(error)) throw error;
    }
    return {
      id: this._sessionModeId,
      content,
      path: this._sessionModeFilePath,
      kind: this._kind,
    };
  }

  private async writeEmptySessionModeFile(path: string): Promise<void> {
    await this.ensureSessionModeDirectory(path);
    await this.agent.kaos.writeText(path, '');
  }

  private async ensureSessionModeDirectory(path: string): Promise<void> {
    await this.agent.kaos.mkdir(dirname(path), {
      parents: true,
      existOk: true,
    });
  }

  private async resolveSessionModeDirectory(kind: SessionModeKind): Promise<{ dir: string; isProjectScoped: boolean }> {
    const projectDir = join(this.agent.config.cwd, '.ody-code', kind === 'design' ? 'designs' : 'plans');
    try {
      await this.agent.kaos.mkdir(projectDir, { parents: true, existOk: true });
      return { dir: projectDir, isProjectScoped: true };
    } catch (error) {
      if (isPermissionError(error) && this.agent.homedir !== undefined) {
        const sessionDir = join(this.agent.homedir, kind === 'design' ? 'designs' : 'plans');
        await this.agent.kaos.mkdir(sessionDir, { parents: true, existOk: true });
        return { dir: sessionDir, isProjectScoped: false };
      }
      throw error;
    }
  }

  private async ensureGitignore(cwd: string): Promise<void> {
    const gitignorePath = join(cwd, '.gitignore');
    const entry = '.ody-code/';
    try {
      const content = await this.agent.kaos.readText(gitignorePath);
      if (content.trim().length === 0) {
        await this.agent.kaos.writeText(gitignorePath, entry + '\n');
        return;
      }
      const lines = content.split('\n');
      for (const line of lines) {
        if (line.trim() === entry) {
          return; // already present
        }
      }
      const separator = content.endsWith('\n') ? '' : '\n';
      await this.agent.kaos.writeText(gitignorePath, content + separator + entry + '\n');
    } catch (error) {
      if (isMissingFileError(error)) {
        await this.agent.kaos.writeText(gitignorePath, entry + '\n');
      } else {
        throw error;
      }
    }
  }

  private async findUniqueStemInDir(dir: string, baseStem: string): Promise<string> {
    let stem = baseStem;
    let suffix = 1;
    const MAX_SUFFIX = 1000;
    while (suffix <= MAX_SUFFIX) {
      const candidatePath = join(dir, `${stem}.md`);
      try {
        await this.agent.kaos.stat(candidatePath);
        stem = `${baseStem}-${suffix}`;
        suffix++;
      } catch {
        return stem;
      }
    }
    const micro = Date.now();
    return `${baseStem}-${micro}`;
  }

  async findUniqueStem(baseStem: string): Promise<string> {
    if (!this._sessionModeFilePath) return baseStem;
    return this.findUniqueStemInDir(dirname(this._sessionModeFilePath), baseStem);
  }
}

function isMissingFileError(error: unknown): boolean {
  if (error === null || typeof error !== 'object') return false;
  const code = (error as { readonly code?: unknown }).code;
  return code === 'ENOENT';
}

function isPermissionError(error: unknown): boolean {
  if (error === null || typeof error !== 'object') return false;
  const code = (error as { readonly code?: unknown }).code;
  return code === 'EACCES' || code === 'EPERM';
}
