import {
  Session,
  type SDKRpcClient,
} from '@odysseythink/ody-code-sdk';
import type {
  CreateSessionOptions,
  ExportSessionInput,
  ExportSessionResult,
  ForkSessionInput,
  GetConfigOptions,
  ListSessionsOptions,
  OdyConfigPatch,
  RenameSessionInput,
  ResumedSessionSummary,
  SessionSummary,
  TelemetryClient,
} from '@odysseythink/ody-code-sdk';
import type { ExperimentalFlagMap, TelemetryContextPatch } from '@odysseythink/agent-core';

import type { OdyHarness } from '#tui/types';

import { RustHostAuthFacade } from './rust-host-auth';

export interface RustHostHarnessOptions {
  readonly client: SDKRpcClient;
  readonly telemetry?: TelemetryClient | undefined;
}

export class RustHostHarness implements OdyHarness {
  readonly homeDir: string;
  readonly configPath: string;
  readonly auth = new RustHostAuthFacade() as OdyHarness['auth'];
  private readonly client: SDKRpcClient;
  private readonly activeSessions = new Map<string, Session>();
  private readonly _track: (event: string, properties?: Record<string, unknown>) => void;

  constructor(options: RustHostHarnessOptions) {
    this.client = options.client;
    this.homeDir = options.client.homeDir;
    this.configPath = options.client.configPath;
    const tel = options.telemetry;
    this._track = tel?.track
      ? (event: string, properties?: Record<string, unknown>) => {
          tel.track(event, properties as any);
        }
      : () => {};
  }

  get interactiveAgentId(): string {
    return this.client.interactiveAgentId;
  }

  set interactiveAgentId(value: string) {
    this.client.interactiveAgentId = value;
  }

  track(event: string, properties?: Record<string, unknown>): void {
    this._track(event, properties);
  }

  setTelemetryContext(patch: TelemetryContextPatch): void {
    // No-op for prototype — Rust host owns telemetry
    void patch;
  }

  async ensureConfigFile(): Promise<void> {
    // Rust host owns config file creation; prototype no-op.
  }

  async getConfig(options?: GetConfigOptions): Promise<import('@odysseythink/ody-code-sdk').OdyConfig> {
    return this.client.getConfig(options);
  }

  async setConfig(patch: OdyConfigPatch): Promise<import('@odysseythink/ody-code-sdk').OdyConfig> {
    return this.client.setConfig(patch);
  }

  async removeProvider(providerId: string): Promise<import('@odysseythink/ody-code-sdk').OdyConfig> {
    return this.client.removeProvider(providerId);
  }

  async getExperimentalFlags(): Promise<ExperimentalFlagMap> {
    return this.client.getExperimentalFlags();
  }

  async createSession(options: CreateSessionOptions): Promise<Session> {
    const { sessionMode, ...coreOptions } = options;
    const summary = await this.client.createSession(coreOptions);
    const session = this.wrapSession(summary);
    if (sessionMode !== undefined && sessionMode !== 'normal') {
      await session.setSessionMode(sessionMode);
    }
    return session;
  }

  async resumeSession(input: { readonly id: string }): Promise<Session> {
    const id = input.id.trim();
    const active = this.activeSessions.get(id);
    if (active !== undefined) return active;
    const summary = await this.client.resumeSession({ id });
    return this.wrapSession(summary);
  }

  async forkSession(input: ForkSessionInput): Promise<Session> {
    const summary = await this.client.forkSession({
      id: input.id,
      forkId: input.forkId,
      title: input.title,
      metadata: input.metadata,
    });
    return this.wrapSession(summary);
  }

  async listSessions(options?: ListSessionsOptions): Promise<readonly SessionSummary[]> {
    return this.client.listSessions(options);
  }

  async renameSession(input: RenameSessionInput): Promise<void> {
    await this.client.renameSession(input);
  }

  async exportSession(input: ExportSessionInput): Promise<ExportSessionResult> {
    return this.client.exportSession(input);
  }

  async closeSession(id: string): Promise<void> {
    await this.activeSessions.get(id)?.close();
  }

  async requestCodeReview(
    input: Record<string, unknown>,
    options?: Record<string, unknown>,
  ): Promise<any> {
    return this.client.requestCodeReview({ ...input, ...options } as any);
  }

  async close(): Promise<void> {
    await Promise.all(Array.from(this.activeSessions.values(), (session) => session.close()));
    this.activeSessions.clear();
  }

  private wrapSession(summary: SessionSummary | ResumedSessionSummary): Session {
    const session = new Session({
      id: summary.id,
      workDir: summary.workDir,
      summary: 'title' in summary ? summary : undefined,
      resumeState: 'sessionMetadata' in summary
        ? { sessionMetadata: summary.sessionMetadata, agents: summary.agents, warning: summary.warning }
        : undefined,
      rpc: this.client,
      onClose: () => {
        this.activeSessions.delete(summary.id);
      },
    });
    this.activeSessions.set(session.id, session);
    return session;
  }
}
