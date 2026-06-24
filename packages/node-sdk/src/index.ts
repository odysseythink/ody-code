export { KimiHarness } from '#/kimi-harness';
export { Session } from '#/session';
export { KimiAuthFacade } from '#/auth';
export { KimiForCodingProvider } from '#/kimi-code-model-provider';
export type { KimiForCodingProviderOptions } from '#/kimi-code-model-provider';

export {
  applyCatalogProvider,
  catalogBaseUrl,
  catalogModelToAlias,
  catalogProviderModels,
  CatalogFetchError,
  DEFAULT_CATALOG_URL,
  fetchCatalog,
  inferWireType,
  loadBuiltInCatalog,
} from '#/catalog';
export type {
  ApplyCatalogProviderOptions,
  Catalog,
  CatalogModel,
  CatalogProviderEntry,
} from '#/catalog';

export {
  ErrorCodes,
  OdyError,
  type OdyErrorCode,
  type OdyErrorInfo,
  type OdyErrorOptions,
  type OdyErrorPayload,
  ODY_ERROR_INFO,
  fromOdyErrorPayload,
  isOdyError,
  toOdyErrorPayload,
} from '@odysseythink/agent-core';

// Diagnostic logging — public surface only.
// RootLogger / getRootLogger / LoggingConfig stay inside agent-core.
export {
  flushDiagnosticLogs,
  log,
  redact,
  resolveGlobalLogPath,
  resolveOdyHome,
} from '@odysseythink/agent-core';
export type { LogContext, LogLevel, LogPayload, Logger } from '@odysseythink/agent-core';

// Goal completion message builder — single source of truth for the deterministic
// "Goal complete · turns · tokens · time" text (live render + persisted message).
export { buildGoalCompletionMessage } from '@odysseythink/agent-core';

// Experimental feature flags — types only. Resolved values come from
// `KimiHarness.getExperimentalFlags()` over RPC, not from a re-exported runtime value.
export type {
  ExperimentalFlagMap,
  FlagDefinition,
  FlagDefinitionInput,
  FlagId,
  FlagSurface,
} from '@odysseythink/agent-core';

export type {
  KimiAuthLoginResult,
  KimiAuthLogoutResult,
  KimiAuthSubmitFeedbackInput,
} from '#/auth';

export * from '#/events';
export type * from '#/types';

// Core-worker bootstrap — exposed for custom worker thread integration
export type { CoreWorkerBootPayload } from '#/core-worker';

// i18n for office-hours language adaptation
export { t, isSupportedLanguage, normalizeLanguage } from '@odysseythink/agent-core';
export type { SupportedLanguage, MessageKey } from '@odysseythink/agent-core';

// Code review report markdown renderer
export { renderCodeReviewReportToMarkdown } from '@odysseythink/agent-core';
// Code review model resolver
export { resolveCodeReviewModel } from '@odysseythink/agent-core';
export type { ResolveModelOverrides } from '@odysseythink/agent-core';
