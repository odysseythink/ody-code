import { randomUUID } from 'node:crypto';
import { basename, dirname, join } from 'pathe';

import type { Agent } from '..';
import {
  extractFirstHeading,
  formatDatePrefix,
  slugifyTitle,
} from './topic-generator';
import { generateHeroSlug } from '../../utils/hero-slug';

/**
 * Whether the current planning session is a regular implementation `plan`
 * or a `design` (brainstorming / spec exploration). Both share this same
 * read-only-with-one-writable-file machinery; only the prompts, the output
 * directory and the surfacing labels differ.
 */
export type AdvancedSessionModeKind = 'plan' | 'design';

export type AdvancedSessionModeData = null | {
  id: string;
  content: string;
  path: string;
  kind: AdvancedSessionModeKind;
};
export type AdvancedSessionModeFilePath = string | null;

export class AdvancedSessionMode {
  protected _isActive = false;
  protected _sessionModeId: null | string = null;
  protected _SessionModeFilePath: AdvancedSessionModeFilePath = null;
  protected _kind: AdvancedSessionModeKind = 'plan';
  protected _fileStem: string | null = null;
  protected _lastDesignFileStem: string | null = null;
  protected _manualTopicSlug: string | null = null;
  private _preModeModelAlias: { value: string | undefined } | null = null;

  constructor(protected readonly agent: Agent) {}

  createAdvancedSessionModeId(): string {
    return generateHeroSlug(randomUUID(), new Set());
  }

  updatePreModeModelAlias(alias: string | undefined): void {
    if (this._preModeModelAlias !== null) {
      this._preModeModelAlias = { value: alias };
    }
  }

  async enter(
    id = this.createAdvancedSessionModeId(),
    createFile = false,
    emitStatus = true,
    kind: AdvancedSessionModeKind = 'plan',
    fileStem?: string,
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
    this._SessionModeFilePath = null;

    let effectiveStem = fileStem;
    if (!effectiveStem) {
      if (kind === 'plan' && this._lastDesignFileStem) {
        const designSlug = extractSlugFromDatedStem(this._lastDesignFileStem);
        effectiveStem = `${formatDatePrefix(new Date())}-${designSlug}`;
      }
    }
    if (fileStem) {
      const slug = slugifyTitle(fileStem);
      if (slug) this._manualTopicSlug = slug;
    }
    this._fileStem = effectiveStem ?? id;

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

    let enterRecorded = false;
    try {
      const advancedSessionModeFilePath = this.advancedSessionModeFilePathFor(this._fileStem);
      this._SessionModeFilePath = advancedSessionModeFilePath;
      await this.ensureAdvancedSessionModeDirectory(advancedSessionModeFilePath);
      this.agent.records.logRecord({
        type: 'plan_mode.enter',
        id,
        kind,
        ...(this._fileStem !== id ? { fileStem: this._fileStem } : {}),
      });
      enterRecorded = true;
      if (createFile) {
        await this.writeEmptyAdvancedSessionModeFile(advancedSessionModeFilePath);
      }
    } catch (error) {
      if (enterRecorded) {
        this.cancel(id);
      } else {
        if (this._preModeModelAlias !== null) {
          this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
          this._preModeModelAlias = null;
        }
        this._isActive = false;
        this._sessionModeId = null;
        this._SessionModeFilePath = null;
        this._fileStem = null;
        this._kind = 'plan';
      }
      throw error;
    }

    if (emitStatus) this.agent.emitStatusUpdated();
  }

  restoreEnter({
    id,
    kind = 'plan',
    fileStem,
  }: {
    readonly id: string;
    readonly kind?: AdvancedSessionModeKind;
    readonly fileStem?: string;
  }): void {
    this.agent.replayBuilder.push({
      type: 'plan_updated',
      enabled: true,
      kind,
    });

    this._isActive = true;
    this._sessionModeId = id;
    this._kind = kind;
    this._fileStem = fileStem ?? id;
    this._SessionModeFilePath = this.advancedSessionModeFilePathFor(this._fileStem);
  }

  cancel(id?: string): void {
    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this._preModeModelAlias = null;
    }
    this.agent.records.logRecord({ type: 'plan_mode.cancel', id });
    this.agent.replayBuilder.push({
      type: 'plan_updated',
      enabled: false,
      kind: this._kind,
    });
    this._manualTopicSlug = null;
    this._isActive = false;
    this._sessionModeId = null;
    this._SessionModeFilePath = null;
    this._fileStem = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }

  async clear(): Promise<void> {
    if (!this._SessionModeFilePath) return;
    await this.writeEmptyAdvancedSessionModeFile(this._SessionModeFilePath);
  }

  exit(id?: string): void {
    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this._preModeModelAlias = null;
    }
    this.agent.records.logRecord({ type: 'plan_mode.exit', id });
    this.agent.replayBuilder.push({
      type: 'plan_updated',
      enabled: false,
      kind: this._kind,
    });
    if (this._kind === 'design' && this._fileStem) {
      this._lastDesignFileStem = this._fileStem;
    }
    this._manualTopicSlug = null;
    this._isActive = false;
    this._sessionModeId = null;
    this._SessionModeFilePath = null;
    this._fileStem = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }

  get isActive() {
    return this._isActive;
  }

  get kind(): AdvancedSessionModeKind {
    return this._kind;
  }

  get fileStem(): string | null {
    return this._fileStem;
  }

  get advancedSessionModeFilePath(): AdvancedSessionModeFilePath {
    return this._SessionModeFilePath;
  }

  /**
   * Whether `path` is part of the current AdvancedSessionMode's writable fileset. This is the
   * single source of truth the read-only guard ({@link PlanModeGuardDenyPermissionPolicy})
   * uses to decide what Write/Edit may touch while AdvancedSessionMode mode is active.
   *
   * The set is the main AdvancedSessionMode file (`<id>.md`) plus its split siblings
   * (`<id>-<subsystem>.md`) in the same directory — and nothing else, so source
   * files outside the AdvancedSessionModes directory stay denied. A single-file AdvancedSessionMode (the
   * common case, and all of design mode) only matches the main file exactly.
   */
  isWritableAdvancedSessionModePath(path: string): boolean {
    if (this._SessionModeFilePath === null || this._sessionModeId === null) return false;
    if (path === this._SessionModeFilePath) return true;
    if (dirname(path) !== dirname(this._SessionModeFilePath)) return false;
    const base = basename(path);
    if (!base.endsWith('.md')) return false;
    const stem = base.slice(0, -'.md'.length);
    return stem === this._sessionModeId || stem.startsWith(`${this._sessionModeId}-`);
  }

  async data(): Promise<AdvancedSessionModeData> {
    if (!this._sessionModeId || !this._SessionModeFilePath) return null;
    let content = '';
    try {
      content = await this.agent.kaos.readText(this._SessionModeFilePath);
    } catch (error) {
      if (!isMissingFileError(error)) throw error;
    }
    return {
      id: this._sessionModeId,
      content,
      path: this._SessionModeFilePath,
      kind: this._kind,
    };
  }

  private async writeEmptyAdvancedSessionModeFile(path: string): Promise<void> {
    await this.ensureAdvancedSessionModeDirectory(path);
    await this.agent.kaos.writeText(path, '');
  }

  private async ensureAdvancedSessionModeDirectory(path: string): Promise<void> {
    await this.agent.kaos.mkdir(dirname(path), {
      parents: true,
      existOk: true,
    });
  }

  private advancedSessionModeFilePathFor(stem: string): string {
    const cwdSubdir = this._kind === 'design' ? 'design' : 'plan';
    const homeSubdir = this._kind === 'design' ? 'designs' : 'plans';
    const plansDir =
      this.agent.homedir === undefined
        ? join(this.agent.config.cwd, cwdSubdir)
        : join(this.agent.homedir, homeSubdir);
    return join(plansDir, `${stem}.md`);
  }

  async finalizeFileName(): Promise<string | null> {
    if (!this._SessionModeFilePath || !this._fileStem) return this._SessionModeFilePath;

    let content: string;
    try {
      content = await this.agent.kaos.readText(this._SessionModeFilePath);
    } catch {
      return this._SessionModeFilePath;
    }

    if (content.trim().length === 0) {
      return this._SessionModeFilePath;
    }

    const heading = extractFirstHeading(content);
    const today = formatDatePrefix(new Date());
    const slug = heading
      ? slugifyTitle(heading)
      : (this._manualTopicSlug ||
         (this._fileStem && this._fileStem !== this._sessionModeId
           ? extractSlugFromDatedStem(this._fileStem)
           : null) ||
         this._sessionModeId ||
         'untitled');

    let finalStem = `${today}-${slug}`;
    finalStem = await this.findUniqueStem(finalStem);

    if (finalStem === this._fileStem) {
      return this._SessionModeFilePath;
    }

    const finalPath = this.advancedSessionModeFilePathFor(finalStem);

    try {
      await this.agent.kaos.writeText(finalPath, content);
    } catch (error) {
      this.agent.log?.warn('Failed to write finalized plan/design file', { error });
      return this._SessionModeFilePath;
    }

    this._SessionModeFilePath = finalPath;
    this._fileStem = finalStem;
    this.agent.emitStatusUpdated();
    return finalPath;
  }

  async findUniqueStem(baseStem: string): Promise<string> {
    let stem = baseStem;
    let suffix = 1;
    while (true) {
      const candidatePath = this.advancedSessionModeFilePathFor(stem);
      try {
        await this.agent.kaos.stat(candidatePath);
        stem = `${baseStem}-${suffix}`;
        suffix++;
      } catch {
        return stem;
      }
    }
  }
}

function extractSlugFromDatedStem(stem: string): string {
  const m = stem.match(/^\d{4}-\d{2}-\d{2}-(.+)$/);
  return m ? (m[1] ?? stem) : stem;
}

function isMissingFileError(error: unknown): boolean {
  if (error === null || typeof error !== 'object') return false;
  const code = (error as { readonly code?: unknown }).code;
  return code === 'ENOENT';
}
