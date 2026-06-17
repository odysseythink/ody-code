import { randomUUID } from 'node:crypto';
import { basename, dirname, join, normalize } from 'pathe';

import type { Agent } from '..';
import type { DesignSessionCheckpoint } from '../../session/checkpoint/checkpoint';
import type { ResolvedRuntimeProvider } from '../../session/provider-manager';
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
export type SessionModeKind = 'plan' | 'design' | 'office-hours';

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
  private _pendingHandoffForPlan: {
    path: string;
    filename: string;
    selectedLabel?: string;
  } | null = null;
  private _pendingHandoffForNormal: {
    content: string;
    path: string;
    selectedLabel?: string;
  } | null = null;
  private _designSessions: DesignSessionCheckpoint[] = [];

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
    const enterModelAlias = this.agent.config.modelAlias;
    this.agent.log?.debug('sessionMode.enter start', {
      kind,
      fromModelAlias: enterModelAlias,
      isActive: this._isActive,
      currentKind: this._kind,
    });
    if (this._isActive) {
      if (this._kind === kind) {
        this.agent.log?.debug('sessionMode.enter already in kind', { kind });
        return;
      }
      // Switching directly between plan and design: exit current first.
      this.exit();
    }

    // The model to restore when leaving modes entirely. Read AFTER the exit()
    // above: a direct plan↔design switch restores the normal model there, so the
    // entry-time alias (captured before exit) would be the PREVIOUS mode's model
    // and would leak back into normal on the final exit. See regression test
    // "restores the normal model after a direct plan→design→normal switch".
    const restoreTargetAlias = this.agent.config.modelAlias;

    this._isActive = true;
    this._sessionModeId = id;
    this._kind = kind;
    this._sessionModeFilePath = null;

    if (kind === 'design') {
      this.startDesignSession(id);
    }

    if (kind === 'plan' || kind === 'design') {
      const modeModel = this.agent.kimiConfig?.modeModels?.[kind];
      if (modeModel !== undefined) {
        let resolved: ResolvedRuntimeProvider | undefined;
        let usable = false;
        try {
          resolved = this.agent.modelProvider?.resolveProviderConfig(modeModel);
          usable = resolved === undefined || this.modelAliasHasUsableAuth(modeModel, resolved);
        } catch {
          this.agent.log?.warn(`modeModels.${kind} "${modeModel}" not found, keeping current model`);
          this._preModeModelAlias = null;
        }
        if (usable) {
          this._preModeModelAlias = { value: restoreTargetAlias };
          if (modeModel !== this.agent.config.modelAlias) {
            this.agent.log?.debug('sessionMode.enter switching model', {
              kind,
              fromModelAlias: restoreTargetAlias,
              toModelAlias: modeModel,
            });
            this.agent.config.update({ modelAlias: modeModel });
            this.agent.refreshLlm();
          }
        } else if (resolved !== undefined) {
          this.agent.log?.warn(
            `modeModels.${kind} "${modeModel}" has no configured API key or OAuth login; keeping current model`,
          );
          this._preModeModelAlias = null;
        }
      }
    }

    this.agent.log?.debug('sessionMode.enter end', {
      kind,
      modelAlias: this.agent.config.modelAlias,
      preModeModelAlias: this._preModeModelAlias?.value,
    });

    try {
      const { isProjectScoped } = await this.resolveSessionModeDirectory(kind);
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
      // Switch the active context partition AFTER the WAL record so that
      // during replay the session_mode.enter record precedes any context
      // records that belong to this partition.
      this.agent.setContextMode(kind);
    } catch (error) {
      this.agent.setContextMode('normal');
      if (this._preModeModelAlias !== null) {
        this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
        this.agent.refreshLlm();
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

    // Route subsequent replay records to this mode's context partition.
    this.agent.setContextMode(kind);
  }

  cancel(id?: string): void {
    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this.agent.refreshLlm();
      this._preModeModelAlias = null;
    }
    if (this._kind === 'design') {
      this.closeCurrentDesignSession();
    }
    this.agent.records.logRecord({ type: 'session_mode.cancel', id });
    // Return to the normal context partition AFTER the WAL record so replay
    // routes subsequent context records correctly.
    this.agent.setContextMode('normal');
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
    const exitModelAlias = this.agent.config.modelAlias;
    const restoreModelAlias = this._preModeModelAlias?.value;
    this.agent.log?.debug('sessionMode.exit start', {
      kind: this._kind,
      currentModelAlias: exitModelAlias,
      restoreModelAlias,
    });
    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this.agent.refreshLlm();
      this._preModeModelAlias = null;
    }
    if (this._kind === 'design') {
      this.closeCurrentDesignSession(this._sessionModeFilePath ?? undefined);
    }
    this.agent.records.logRecord({ type: 'session_mode.exit', id });
    this.agent.log?.debug('sessionMode.exit end', {
      kind: this._kind,
      modelAlias: this.agent.config.modelAlias,
    });
    // Return to the normal context partition AFTER the WAL record so replay
    // routes subsequent context records correctly.
    this.agent.setContextMode('normal');
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
    const path = join(dir, `${stem}.md`);
    this._sessionModeFilePath = path;
    this.agent.emitStatusUpdated();
    // Record the locked path so resuming (or forking) a session before the first
    // write still restores the same plan file instead of falling back to topic-based
    // naming and potentially losing the explicit source.
    this.agent.records.logRecord({
      type: 'session_mode.enter',
      id: this._sessionModeId!,
      kind: this._kind,
      path,
    });
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

  /** Consume and return the pending design→plan handoff artifact (if any). */
  consumePendingHandoffForPlan(): {
    path: string;
    filename: string;
    selectedLabel?: string;
  } | null {
    const p = this._pendingHandoffForPlan;
    this._pendingHandoffForPlan = null;
    return p;
  }

  /** Consume and return the pending plan→normal handoff artifact (if any). */
  consumePendingHandoffForNormal(): {
    content: string;
    path: string;
    selectedLabel?: string;
  } | null {
    const p = this._pendingHandoffForNormal;
    this._pendingHandoffForNormal = null;
    return p;
  }

  /**
   * Exit the current mode and chain into `target`, carrying the current artifact
   * into the target partition via the injection system's next-turn reminder.
   *
   * design → plan: exits design, enters plan, stores artifact for DesignModeInjector.
   * plan → normal: exits plan, stores artifact for PlanModeInjector.
   *
   * `cancel()` still bypasses this and does a plain exit with no handoff.
   */
  async handoffTo(
    target: 'plan' | 'normal',
    opts?: { selectedLabel?: string },
  ): Promise<void> {
    const data = await this.data();
    this.agent.log?.debug('sessionMode.handoffTo start', {
      target,
      fromKind: this._kind,
      fromModelAlias: this.agent.config.modelAlias,
    });

    if (target === 'plan') {
      const selectedLabel = opts?.selectedLabel;
      const artifact =
        data !== null && data.path.length > 0
          ? {
              path: data.path,
              filename: basename(data.path),
              selectedLabel:
                selectedLabel !== undefined && selectedLabel.length > 0 ? selectedLabel : undefined,
            }
          : null;
      this._pendingHandoffForPlan = artifact;
      this.exit();
      try {
        await this.enter(this.createSessionModeId(), false, true, 'plan');
      } catch (error) {
        this._pendingHandoffForPlan = null; // prevent ghost injection on next turn
        throw error;
      }
    } else {
      const artifact =
        data !== null && data.content.trim().length > 0
          ? { content: data.content, path: data.path }
          : null;
      // The plan→normal tool result stays in the plan partition (deferred context
      // switch), so the selected approach is carried into normal via the injection.
      const selectedLabel = opts?.selectedLabel;
      this._pendingHandoffForNormal =
        artifact === null
          ? null
          : selectedLabel !== undefined && selectedLabel.length > 0
            ? { ...artifact, selectedLabel }
            : artifact;
      this.exit();
    }
    this.agent.log?.debug('sessionMode.handoffTo end', {
      target,
      toKind: this._kind,
      toModelAlias: this.agent.config.modelAlias,
    });
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

  get designSessions(): readonly DesignSessionCheckpoint[] {
    return this._designSessions;
  }

  /** Replace the tracked design sessions, used during resume from a checkpoint. */
  restoreDesignSessions(sessions: readonly DesignSessionCheckpoint[]): void {
    this._designSessions = sessions.slice();
  }

  private startDesignSession(id: string): void {
    this._designSessions.push({
      designSessionID: id,
      startedAtMsg: this.currentMessageCount(),
    });
  }

  private closeCurrentDesignSession(approvedPath?: string): void {
    const session = this._designSessions[this._designSessions.length - 1];
    if (session === undefined || session.exitedAtMsg !== undefined) return;
    const count = this.currentMessageCount();
    // After a context clear (live or during replay), history.length may be 0 even
    // though the session started at a positive index.  Writing exitedAtMsg < startedAtMsg
    // would permanently corrupt the checkpoint — the idempotency guard above would
    // prevent any later overwrite.  Skip the write and leave exitedAtMsg undefined,
    // which the integrity check treats as "still active" (no validation).
    if (count < session.startedAtMsg) return;
    session.exitedAtMsg = count;
    if (approvedPath !== undefined && approvedPath.length > 0) {
      session.approvedPath = approvedPath;
    }
  }

  private currentMessageCount(): number {
    return this.agent.context?.history?.length ?? 0;
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
    const isUserOrigin = (msg: { role: string; origin?: { kind: string } }) =>
      msg.role === 'user' && msg.origin?.kind === 'user';

    // Check the current mode's partition first (user may have prompted after entering
    // this mode), then fall back to normal (user prompted before entering).
    const currentHistory = this.agent.context?.history as readonly { role: string; origin?: { kind: string }; content: readonly { type: string; text?: string }[] }[] | undefined;
    const normalHistory = this.agent.contexts?.normal?.history as readonly { role: string; origin?: { kind: string }; content: readonly { type: string; text?: string }[] }[] | undefined;
    const lastUserMessage =
      currentHistory?.findLast(isUserOrigin) ??
      normalHistory?.findLast(isUserOrigin);
    if (lastUserMessage === undefined) return null;
    const text = lastUserMessage.content
      .filter((part) => part.type === 'text')
      .map((part) => part.text ?? '')
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
    const subdir = kind === 'office-hours' ? 'office-hours' : kind === 'design' ? 'designs' : 'plans';
    const projectDir = join(this.agent.config.cwd, '.ody-code', subdir);
    try {
      await this.agent.kaos.mkdir(projectDir, { parents: true, existOk: true });
      return { dir: projectDir, isProjectScoped: true };
    } catch (error) {
      if (isPermissionError(error) && this.agent.homedir !== undefined) {
        const sessionDir = join(this.agent.homedir, subdir);
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

  /**
   * Whether a model alias can actually be used for generation. A model is usable
   * when its provider has either a resolved API key (config value or environment
   * variable) or a configured OAuth login. This prevents mode transitions from
   * silently switching to a model that will fail on its first LLM call with a
   * cryptic "apiKey is required" provider error.
   */
  private modelAliasHasUsableAuth(
    modelAlias: string,
    resolved: ResolvedRuntimeProvider,
  ): boolean {
    // OAuth path: resolveAuth returns a wrapper whenever the raw provider config
    // has an `oauth` entry. The wrapper fetches the access token per request.
    const withAuth = this.agent.modelProvider?.resolveAuth?.(modelAlias, {
      log: this.agent.log,
    });
    if (withAuth !== undefined) return true;

    // API-key path: the resolved KosongProviderConfig already folds in config
    // values and environment variables via provider-manager's providerApiKey().
    const apiKey = (resolved.provider as { apiKey?: string }).apiKey;
    return apiKey !== undefined && apiKey.length > 0;
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
