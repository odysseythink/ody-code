import type { AgentEvent } from '@odysseythink/agent-core';
import type { SDKRpcClient } from '@odysseythink/ody-code-sdk';

export type BackendKind = 'ts' | 'rust';

export interface ParityBackend {
  readonly kind: BackendKind;
  readonly client: SDKRpcClient;
  readonly homeDir: string;
  close(): Promise<void>;
}

export interface Scenario {
  readonly name: string;
  readonly run: (backend: ParityBackend) => Promise<ScenarioSnapshot>;
}

export interface ScenarioSnapshot {
  readonly responses: readonly unknown[];
  readonly events: readonly AgentEvent[];
  readonly records?: readonly unknown[];
  readonly fsTree?: unknown;
}

export interface NormalizedSnapshot {
  readonly responses: readonly unknown[];
  readonly events: readonly AgentEvent[];
  readonly records?: readonly unknown[];
  readonly fsTree?: unknown;
  readonly meta?: NormalizedMeta;
}

export interface NormalizedMeta {
  readonly joinedDeltaCount: number;
}

export interface FieldDiff {
  readonly path: string;
  readonly tsValue: unknown;
  readonly rustValue: unknown;
}

export interface ParityDiff {
  readonly scenarioName: string;
  readonly ts: NormalizedSnapshot;
  readonly rust: NormalizedSnapshot;
  readonly diffs: readonly FieldDiff[];
}

export interface NormalizerOptions {
  readonly homeDir: string;
  readonly tmpDir: string;
  readonly fixedIds?: ReadonlyMap<string, string> | undefined;
  readonly ignoreEventTypes?: ReadonlySet<string> | undefined;
}
