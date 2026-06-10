import { randomUUID } from 'node:crypto';
import { basename, dirname, join, normalize } from 'pathe';

import type { Agent } from '..';
import {
  extractFirstHeading,
  extractTopicFromMessage,
  formatDatePrefix,
  slugifyTitle,
  stripDatePrefix,
  stripLocators,
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
  private _lastCompletedDesignFilePath: string | null = null;

  constructor(protected readonly agent: Agent) {}

  createSessionModeId(): string {
    return randomUUID();
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
    if (modeModel !== undefined) {
      try {
        this.agent.modelProvider?.resolveProviderConfig(modeModel);
        this._preModeModelAlias = { value: this.agent.config.modelAlias };
        if (modeModel !== this.agent.config.modelAlias) {
          this.agent.config.update({ modelAlias: modeModel });
        }
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
    path,
  }: {
    readonly id: string;
    readonly kind?: SessionModeKind;
    readonly path?: string;
  }): void {
    this.agent.replayBuilder.push({
      type: 'session_mode_updated',
      enabled: true,
      kind,
    });

    this._isActive = true;
    this._sessionModeId = id;
    this._kind = kind;
    this._sessionModeFilePath = path && path.length > 0 ? path : null;

    // Resume boots directly into a session mode without the live enter() path, so
    // _preModeModelAlias was never captured. Without it, a later exit/cancel back
    // to normal mode cannot restore the normal-mode model and stays stuck on this
    // mode's model. Seed it with the normal-mode model (defaultModel), mirroring
    // what enter() captures when entering a mode from normal.
    const normalModel = this.agent.kimiConfig?.defaultModel;
    this._preModeModelAlias = normalModel !== undefined ? { value: normalModel } : null;
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
    // The path may be only RESERVED (eager resolution at entry) with no file on
    // disk yet. Clearing a plan/design the model has not written should stay a
    // no-op — do not materialise an empty file for work that was never started.
    try {
      await this.agent.kaos.stat(this._sessionModeFilePath);
    } catch {
      return;
    }
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
    if (this._kind === 'design' && this._sessionModeFilePath !== null) {
      this._lastCompletedDesignFilePath = this._sessionModeFilePath;
    }
    this._isActive = false;
    this._sessionModeId = null;
    this._sessionModeFilePath = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }

  /**
   * Lock the plan-mode output file to an explicit SOURCE file's name, used by the
   * `/writing-plan <file>` command. The plan is written to `<plans>/<source-basename>.md`
   * (same name as the source, deduplicated on disk), bypassing topic-based naming AND
   * the design→plan filename handoff ({@link _lastCompletedDesignFilePath}, which is
   * cleared here so it can never override the explicit name).
   *
   * Throws if the source file does not exist. Caller must already be in plan mode.
   */
  async setWritingPlanSource(sourceFilePath: string): Promise<void> {
    await this.validatePlanSource(sourceFilePath);
    const { dir } = await this.resolveSessionModeDirectory('plan');
    const base = basename(sourceFilePath);
    const sourceStem = base.endsWith('.md') ? base.slice(0, -'.md'.length) : base;
    const stem = await this.findUniqueStemInDir(dir, sourceStem);
    // Sever any pending design handoff so it cannot win over the explicit name.
    this._lastCompletedDesignFilePath = null;
    // NOTE: the locked path is not recorded, so a resume BEFORE the first write
    // falls back to topic-based naming — same narrow limitation as the design→plan
    // handoff. Once the model writes, the correctly-named file exists on disk.
    this._sessionModeFilePath = join(dir, `${stem}.md`);
    this.agent.emitStatusUpdated();
  }

  /**
   * Validate that `sourceFilePath` is an existing regular file (not a directory).
   * Throws otherwise. Callable before {@link enter} so a bad path never mutates
   * session mode. Used by {@link setWritingPlanSource} and the `enterPlan` handler.
   */
  async validatePlanSource(sourceFilePath: string): Promise<void> {
    let st;
    try {
      st = await this.agent.kaos.stat(sourceFilePath);
    } catch {
      throw new Error(`文件不存在: ${sourceFilePath}`);
    }
    // POSIX mode: (stMode & S_IFMT) === S_IFDIR → it's a directory.
    if ((st.stMode & 0o170000) === 0o040000) {
      throw new Error(`不是文件（是目录）: ${sourceFilePath}`);
    }
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

  /** Kebab topic from the latest real user message, or null when none is usable. */
  private topicSlugFromHistory(): string | null {
    const history = this.agent.context?.history;
    if (history === undefined) return null;
    const lastUserMessage = history.findLast(
      (msg) => msg.role === 'user' && msg.origin?.kind === 'user',
    );
    if (lastUserMessage === undefined) return null;
    const text = lastUserMessage.content
      .filter((part) => part.type === 'text')
      .map((part) => part.text)
      .join('')
      .trim();
    if (text.length === 0) return null;
    // Strip path/URL noise so the topic reflects intent ("合并两份设计"), not the
    // file paths the user pasted ("合并两份设计-users-ranwei-ody-code").
    return extractTopicFromMessage(stripLocators(text));
  }

  async resolveFilePathFromContent(content: string): Promise<string> {
    if (this._sessionModeFilePath !== null) {
      return this._sessionModeFilePath;
    }

    const { dir } = await this.resolveSessionModeDirectory(this._kind);

    // Lazy fallback: if design stem is known, use it instead of deriving from content.
    if (this._kind === 'plan' && this._lastCompletedDesignFilePath !== null) {
      const designBase = basename(this._lastCompletedDesignFilePath);
      const designStem = designBase.endsWith('.md') ? designBase.slice(0, -'.md'.length) : designBase;
      this._lastCompletedDesignFilePath = null;
      const stem = await this.findUniqueStemInDir(dir, designStem);
      const path = join(dir, `${stem}.md`);
      this._sessionModeFilePath = path;
      this.agent.emitStatusUpdated();
      this.agent.records.logRecord({
        type: 'session_mode.enter',
        id: this._sessionModeId!,
        kind: this._kind,
        path,
      });
      return path;
    }

    // Topic source priority: the user's prompt (now present even if the mode was
    // entered before it), then the document's H1 heading, then an LLM summary,
    // finally "untitled" as a last resort.
    let slug = this.topicSlugFromHistory();
    if (!slug) {
      const heading = extractFirstHeading(content);
      if (heading) {
        slug = slugifyTitle(heading);
      } else {
        const title = await this.llmSummarizeTitle(content);
        slug = title ? slugifyTitle(title) : 'untitled';
      }
    }

    slug = stripDatePrefix(slug);
    const datePrefix = formatDatePrefix(new Date());
    const stem = `${datePrefix}-${slug && slug.length > 0 ? slug : 'untitled'}`;
    const finalStem = await this.findUniqueStemInDir(dir, stem);
    const path = join(dir, `${finalStem}.md`);

    this._sessionModeFilePath = path;
    this.agent.emitStatusUpdated();
    this.agent.records.logRecord({
      type: 'session_mode.enter',
      id: this._sessionModeId!,
      kind: this._kind,
      path,
    });
    return path;
  }

  /**
   * Resolve the session-mode file path from the model's first Write request.
   * Extracts a slug from the requested path basename, normalizes it with
   * a date prefix, deduplicates, and commits the path to the wire record.
   *
   * When the basename yields an unusable slug after sanitization (e.g. the
   * model requests `---.md`), falls back to content-based resolution via
   * {@link resolveFilePathFromContent}.
   */
  async resolveFilePathFromModelRequest(
    requestedPath: string,
    content: string,
  ): Promise<string> {
    if (this._sessionModeFilePath !== null) {
      return this._sessionModeFilePath;
    }

    const { dir } = await this.resolveSessionModeDirectory(this._kind);

    // Extract slug from the model's requested path basename.  basename()
    // strips any directory structure the model may have invented, so the
    // host remains in control of where the file actually lands.
    const base = basename(requestedPath);
    let slug = base.endsWith('.md') ? base.slice(0, -'.md'.length) : base;
    slug = slugifyTitle(slug);
    // The model may already include a `YYYY-MM-DD-` prefix in its requested
    // basename; strip it so the host's own date prefix isn't doubled.
    slug = stripDatePrefix(slug);

    if (slug.length < 2) {
      // The model invented a path whose basename yields no usable slug.
      // Fall through to the existing content-based resolution (heading,
      // LLM summary, "untitled") which will set up the path + logRecord
      // internally.
      return this.resolveFilePathFromContent(content);
    }

    const datePrefix = formatDatePrefix(new Date());
    const stem = `${datePrefix}-${slug}`;
    const finalStem = await this.findUniqueStemInDir(dir, stem);
    const path = join(dir, `${finalStem}.md`);

    this._sessionModeFilePath = path;
    this.agent.emitStatusUpdated();

    this.agent.records.logRecord({
      type: 'session_mode.enter',
      id: this._sessionModeId!,
      kind: this._kind,
      path,
    });

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
