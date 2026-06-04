import { randomUUID } from 'node:crypto';
import { basename, dirname, join } from 'pathe';

import type { Agent } from '..';
import { generateHeroSlug } from '../../utils/hero-slug';

/**
 * Whether the current planning session is a regular implementation `plan`
 * or a `design` (brainstorming / spec exploration). Both share this same
 * read-only-with-one-writable-file machinery; only the prompts, the output
 * directory and the surfacing labels differ.
 */
export type PlanKind = 'plan' | 'design';

export type PlanData = null | {
  id: string;
  content: string;
  path: string;
  kind: PlanKind;
};
export type PlanFilePath = string | null;

export class PlanMode {
  protected _isActive = false;
  protected _planId: null | string = null;
  protected _planFilePath: PlanFilePath = null;
  protected _kind: PlanKind = 'plan';
  private _preModeModelAlias: { value: string | undefined } | null = null;

  constructor(protected readonly agent: Agent) {}

  createPlanId(): string {
    return generateHeroSlug(randomUUID(), new Set());
  }

  async enter(
    id = this.createPlanId(),
    createFile = false,
    emitStatus = true,
    kind: PlanKind = 'plan',
  ): Promise<void> {
    if (this._isActive) {
      throw new Error('Already in plan mode');
    }

    this._isActive = true;
    this._planId = id;
    this._kind = kind;
    this._planFilePath = null;

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
      const planFilePath = this.planFilePathFor(id);
      this._planFilePath = planFilePath;
      await this.ensurePlanDirectory(planFilePath);
      this.agent.records.logRecord({ type: 'plan_mode.enter', id, kind });
      enterRecorded = true;
      if (createFile) {
        await this.writeEmptyPlanFile(planFilePath);
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
        this._planId = null;
        this._planFilePath = null;
        this._kind = 'plan';
      }
      throw error;
    }

    if (emitStatus) this.agent.emitStatusUpdated();
  }

  restoreEnter({ id, kind = 'plan' }: { readonly id: string; readonly kind?: PlanKind }): void {
    this.agent.replayBuilder.push({
      type: 'plan_updated',
      enabled: true,
      kind,
    });

    this._isActive = true;
    this._planId = id;
    this._kind = kind;
    this._planFilePath = this.planFilePathFor(id);
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
    this._isActive = false;
    this._planId = null;
    this._planFilePath = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }

  async clear(): Promise<void> {
    if (!this._planFilePath) return;
    await this.writeEmptyPlanFile(this._planFilePath);
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
    this._isActive = false;
    this._planId = null;
    this._planFilePath = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }

  get isActive() {
    return this._isActive;
  }

  get kind(): PlanKind {
    return this._kind;
  }

  get planFilePath(): PlanFilePath {
    return this._planFilePath;
  }

  /**
   * Whether `path` is part of the current plan's writable fileset. This is the
   * single source of truth the read-only guard ({@link PlanModeGuardDenyPermissionPolicy})
   * uses to decide what Write/Edit may touch while plan mode is active.
   *
   * The set is the main plan file (`<id>.md`) plus its split siblings
   * (`<id>-<subsystem>.md`) in the same directory — and nothing else, so source
   * files outside the plans directory stay denied. A single-file plan (the
   * common case, and all of design mode) only matches the main file exactly.
   */
  isWritablePlanPath(path: string): boolean {
    if (this._planFilePath === null || this._planId === null) return false;
    if (path === this._planFilePath) return true;
    if (dirname(path) !== dirname(this._planFilePath)) return false;
    const base = basename(path);
    if (!base.endsWith('.md')) return false;
    const stem = base.slice(0, -'.md'.length);
    return stem === this._planId || stem.startsWith(`${this._planId}-`);
  }

  async data(): Promise<PlanData> {
    if (!this._planId || !this._planFilePath) return null;
    let content = '';
    try {
      content = await this.agent.kaos.readText(this._planFilePath);
    } catch (error) {
      if (!isMissingFileError(error)) throw error;
    }
    return {
      id: this._planId,
      content,
      path: this._planFilePath,
      kind: this._kind,
    };
  }

  private async writeEmptyPlanFile(path: string): Promise<void> {
    await this.ensurePlanDirectory(path);
    await this.agent.kaos.writeText(path, '');
  }

  private async ensurePlanDirectory(path: string): Promise<void> {
    await this.agent.kaos.mkdir(dirname(path), {
      parents: true,
      existOk: true,
    });
  }

  private planFilePathFor(id: string): string {
    const cwdSubdir = this._kind === 'design' ? 'design' : 'plan';
    const homeSubdir = this._kind === 'design' ? 'designs' : 'plans';
    const plansDir =
      this.agent.homedir === undefined
        ? join(this.agent.config.cwd, cwdSubdir)
        : join(this.agent.homedir, homeSubdir);
    return join(plansDir, `${id}.md`);
  }
}

function isMissingFileError(error: unknown): boolean {
  if (error === null || typeof error !== 'object') return false;
  const code = (error as { readonly code?: unknown }).code;
  return code === 'ENOENT';
}
