import type { AgentType } from '#/agent';
import type { BackgroundTaskInfo } from '#/agent/background';
import type { AgentConfigData, AgentConfigUpdateData } from '#/agent/config';
import type { AgentContextData, ContextMessage } from '#/agent/context';
import type {
  PermissionApprovalResultRecord,
  PermissionData,
  PermissionMode,
} from '#/agent/permission';
import type { SessionModeData, SessionModeKind } from '#/agent/session-mode';
import type { ToolInfo } from '#/agent/tool';
import type { SessionSummary } from '#/rpc/core-api';
import type { UsageStatus } from '#/rpc/events';
import type { SessionMeta } from '#/session';

export type AgentReplayRecord =
  | { type: 'message'; message: ContextMessage; mode?: string }
  | { type: 'session_mode_updated'; enabled: boolean; kind?: SessionModeKind }
  | { type: 'config_updated'; config: AgentConfigUpdateData }
  | { type: 'permission_updated'; mode: PermissionMode }
  | { type: 'approval_result'; record: PermissionApprovalResultRecord };

export interface ResumedAgentState {
  readonly type: AgentType;
  readonly config: AgentConfigData;
  readonly context: AgentContextData;
  readonly replay: readonly AgentReplayRecord[];
  readonly replays?: Readonly<Record<string, readonly AgentReplayRecord[]>>;
  readonly permission: PermissionData;
  readonly sessionMode: SessionModeData;
  readonly usage: UsageStatus;
  readonly tools: readonly ToolInfo[];
  readonly toolStore?: Readonly<Record<string, unknown>>;
  readonly background: readonly BackgroundTaskInfo[];
}

export interface ResumeSessionResult extends SessionSummary {
  readonly sessionMetadata: SessionMeta;
  readonly agents: Readonly<Record<string, ResumedAgentState>>;
  readonly warning?: string | undefined;
}
