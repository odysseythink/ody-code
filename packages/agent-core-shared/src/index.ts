/// <reference path="./types.d.ts" />

export {
  ErrorCodes,
  ODY_ERROR_INFO,
  type OdyErrorCode,
  type OdyErrorInfo,
  OdyError,
  type OdyErrorOptions,
  fromOdyErrorPayload,
  isOdyError,
  makeErrorPayload,
  toOdyErrorPayload,
  type OdyErrorPayload,
} from './errors';

export type {
  Logger,
  LogLevel,
  LogContext,
  LogPayload,
  LogEntry,
  LoggingConfig,
  SessionLogHandle,
  SessionAttachInput,
  RootLogger,
} from './logging';

export { levelEnabled, LOG_LEVEL_RANK } from './logging';

export {
  abortError,
  abortable,
  UserCancellationError,
  userCancellationReason,
  isUserCancellation,
  createDeadlineAbortSignal,
  type DeadlineAbortSignal,
  linkAbortSignal,
} from './abort';

export { getCoreVersion } from './version';

export type {
  ExecutableTool,
  ExecutableToolContext,
  ExecutableToolResult,
  ExecutableToolSuccessResult,
  ExecutableToolErrorResult,
  RunnableToolExecution,
  ToolExecution,
  ToolUpdate,
} from './tool-execution';

export { toInputJsonSchema } from './input-schema';

export { MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE } from './mcp-events';
export type { McpOAuthAuthorizationUrlUpdateData } from './mcp-events';

export {
  parsePattern,
  isValidPermissionPattern,
  type ParsedPattern,
} from './permission-pattern';
export { HOOK_EVENT_TYPES, type HookEventType } from './hook-events';

export {
  ProviderTypeSchema,
  type ProviderType,
  OAuthRefSchema,
  type OAuthRef,
  ProviderConfigSchema,
  type ProviderConfig,
  ModelAliasSchema,
  type ModelAlias,
  ThinkingConfigSchema,
  type ThinkingConfig,
  PermissionModeSchema,
  PermissionRuleDecisionSchema,
  PermissionRuleScopeSchema,
  PermissionRuleSchema,
  PermissionConfigSchema,
  type PermissionConfig,
  LoopControlSchema,
  type LoopControl,
  BackgroundConfigSchema,
  type BackgroundConfig,
  HookDefSchema,
  HOOK_PROFILES,
  type HookProfile,
  type HookDefConfig,
  MoonshotServiceConfigSchema,
  type MoonshotServiceConfig,
  WebSearchProviderNameSchema,
  type WebSearchProviderName,
  WebSearchProviderConfigSchema,
  type WebSearchProviderConfig,
  WebSearchConfigSchema,
  type WebSearchConfig,
  DuckDuckGoOptionsSchema,
  SerpApiOptionsSchema,
  SearchApiOptionsSchema,
  SerperOptionsSchema,
  BingOptionsSchema,
  BaiduOptionsSchema,
  SerplyOptionsSchema,
  SearXNGOptionsSchema,
  TavilyOptionsSchema,
  ExaOptionsSchema,
  PerplexityOptionsSchema,
  MoonshotOptionsSchema,
  ServicesConfigSchema,
  type ServicesConfig,
  McpServerStdioConfigSchema,
  type McpServerStdioConfig,
  McpServerHttpConfigSchema,
  type McpServerHttpConfig,
  McpServerConfigSchema,
  type McpServerConfig,
  BrowserConfigSchema,
  type BrowserConfig,
  E2EConfigSchema,
  type E2EConfig,
  TestReviewConfigSchema,
  type TestReviewConfig,
  MicroagentBudgetConfigSchema,
  type MicroagentBudgetConfig,
  OdyConfigSchema,
  type OdyConfig,
  OdyConfigPatchSchema,
  type OdyConfigPatch,
  getDefaultConfig,
  validateConfig,
  formatConfigValidationError,
} from './config';

export {
  FLAG_DEFINITIONS,
  flags,
  FlagResolver,
  MASTER_ENV,
  type FlagId,
  type FlagSurface,
  type FlagDefinitionInput,
  type FlagDefinition,
  type ExperimentalFlagMap,
} from './flags';

export {
  loadWasmModule,
  wrapWithFallback,
  type WasmFlagId,
  type WasmExports,
  type WasmModuleConfig,
  type LoadContext,
} from './wasm-loader';
export {
  writeString,
  readCString,
  callWasmStringFunction,
  callWasmU32Function,
  type StringAllocation,
} from './wasm-string';

export * from './product-state';
