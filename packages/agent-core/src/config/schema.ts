import { HOOK_EVENT_TYPES } from '../session/hooks/types';
import { parsePattern } from '#/agent/permission/matches-rule';
import { ErrorCodes, OdyError } from '#/errors';
import { z } from 'zod';

export const ProviderTypeSchema = z.enum([
  'anthropic',
  'openai',
  'kimi',
  'google-genai',
  'openai_responses',
  'vertexai',
  'deepseek',
  'glm',
]);

export type ProviderType = z.infer<typeof ProviderTypeSchema>;

export const OAuthRefSchema = z.object({
  storage: z.enum(['file', 'keyring']),
  key: z.string().min(1),
});

export type OAuthRef = z.infer<typeof OAuthRefSchema>;

const StringRecordSchema = z.record(z.string(), z.string());

export const ProviderConfigSchema = z.object({
  type: ProviderTypeSchema,
  apiKey: z.string().optional(),
  baseUrl: z.string().optional(),
  defaultModel: z.string().optional(),
  oauth: OAuthRefSchema.optional(),
  env: StringRecordSchema.optional(),
  customHeaders: StringRecordSchema.optional(),
  source: z.record(z.string(), z.unknown()).optional(),
});

export type ProviderConfig = z.infer<typeof ProviderConfigSchema>;

export const ModelAliasSchema = z.object({
  provider: z.string(),
  model: z.string(),
  maxContextSize: z.number().int().min(1),
  maxOutputSize: z.number().int().min(1).optional(),
  capabilities: z.array(z.string()).optional(),
  displayName: z.string().optional(),
  reasoningKey: z.string().optional(),
  // Explicitly declare adaptive-thinking support, overriding the kosong
  // model-name version inference. Needed for custom-named Anthropic endpoints
  // whose model name does not encode a parseable Claude version.
  adaptiveThinking: z.boolean().optional(),
});

export type ModelAlias = z.infer<typeof ModelAliasSchema>;

export const ThinkingConfigSchema = z.object({
  mode: z.enum(['auto', 'on', 'off']).optional(),
  effort: z.string().optional(),
});

export type ThinkingConfig = z.infer<typeof ThinkingConfigSchema>;

export const PermissionModeSchema = z.enum(['yolo', 'manual', 'auto']);

export const PermissionRuleDecisionSchema = z.enum(['allow', 'deny', 'ask']);
export const PermissionRuleScopeSchema = z.enum([
  'turn-override',
  'session-runtime',
  'project',
  'user',
]);

export const PermissionRuleSchema = z.object({
  decision: PermissionRuleDecisionSchema,
  scope: PermissionRuleScopeSchema.default('user'),
  pattern: z.string().min(1).refine(isValidPermissionPattern, {
    message: 'Invalid permission rule pattern',
  }),
  reason: z.string().optional(),
});

export const PermissionConfigSchema = z.object({
  rules: z.array(PermissionRuleSchema).optional(),
});

export type PermissionConfig = z.infer<typeof PermissionConfigSchema>;

export const LoopControlSchema = z.object({
  maxStepsPerTurn: z.number().int().min(0).optional(),
  maxRetriesPerStep: z.number().int().min(0).optional(),
  maxRalphIterations: z.number().int().min(-1).optional(),
  reservedContextSize: z.number().int().min(0).optional(),
  compactionTriggerRatio: z.number().min(0.5).max(0.99).optional(),
  splitPlanCompactionRatio: z.number().min(0).max(0.95).optional(),
  normalTaskCompactionRatio: z.number().min(0).max(0.95).optional(),
});

export type LoopControl = z.infer<typeof LoopControlSchema>;

export const BackgroundConfigSchema = z.object({
  maxRunningTasks: z.number().int().min(1).optional(),
  keepAliveOnExit: z.boolean().optional(),
  killGracePeriodMs: z.number().int().min(0).optional(),
  agentTaskTimeoutS: z.number().int().min(1).optional(),
  printWaitCeilingS: z.number().int().min(1).optional(),
});

export type BackgroundConfig = z.infer<typeof BackgroundConfigSchema>;

export const HookDefSchema = z
  .object({
    event: z.enum(HOOK_EVENT_TYPES),
    matcher: z.string().optional(),
    command: z.string().min(1),
    timeout: z.number().int().min(1).max(600).optional(),
  })
  .strict();

export type HookDefConfig = z.infer<typeof HookDefSchema>;

export const MoonshotServiceConfigSchema = z.object({
  baseUrl: z.string().optional(),
  apiKey: z.string().optional(),
  oauth: OAuthRefSchema.optional(),
  customHeaders: StringRecordSchema.optional(),
});

export type MoonshotServiceConfig = z.infer<typeof MoonshotServiceConfigSchema>;

export const WebSearchProviderNameSchema = z.enum([
  'duckduckgo',
  'serpapi',
  'searchapi',
  'serper',
  'bing',
  'baidu',
  'serply',
  'searxng',
  'tavily',
  'exa',
  'perplexity',
  'moonshot',
]);
export type WebSearchProviderName = z.infer<typeof WebSearchProviderNameSchema>;

export const DuckDuckGoOptionsSchema = z.object({
  proxyUrl: z.string().url().optional(),
});

export const SerpApiOptionsSchema = z.object({
  engine: z.string().optional(),
});

export const SearchApiOptionsSchema = z.object({
  engine: z.string().optional(),
});

export const SerperOptionsSchema = z.object({});

export const BingOptionsSchema = z.object({
  market: z.string().optional(),
});

export const BaiduOptionsSchema = z.object({
  topK: z.number().int().min(1).max(50).optional(),
});

export const SerplyOptionsSchema = z.object({
  language: z.string().optional(),
  hl: z.string().optional(),
  gl: z.string().optional(),
  device: z.enum(['desktop', 'mobile']).optional(),
});

export const SearXNGOptionsSchema = z.object({
  baseUrl: z.string().url(),
});

export const TavilyOptionsSchema = z.object({
  searchDepth: z.enum(['basic', 'advanced']).optional(),
});

export const ExaOptionsSchema = z.object({
  type: z.enum(['auto', 'fast', 'deep']).optional(),
  livecrawl: z.enum(['fallback', 'preferred']).optional(),
});

export const PerplexityOptionsSchema = z.object({
  maxResults: z.number().int().min(1).max(20).optional(),
  maxTokensPerPage: z.number().int().optional(),
});

export const MoonshotOptionsSchema = z.object({
  baseUrl: z.string().optional(),
  apiKey: z.string().optional(),
  oauth: OAuthRefSchema.optional(),
  customHeaders: StringRecordSchema.optional(),
});

export const WebSearchProviderConfigSchema = z.object({
  provider: WebSearchProviderNameSchema,
  apiKey: z.string().optional(),
  timeoutMs: z.number().int().min(1000).max(120000).optional(),
  options: z.record(z.string(), z.unknown()).optional(),
});
export type WebSearchProviderConfig = z.infer<typeof WebSearchProviderConfigSchema>;

export const WebSearchConfigSchema = z.object({
  primary: WebSearchProviderConfigSchema,
  secondary: WebSearchProviderConfigSchema.optional(),
});
export type WebSearchConfig = z.infer<typeof WebSearchConfigSchema>;

export const ServicesConfigSchema = z.object({
  moonshotSearch: MoonshotServiceConfigSchema.optional(),
  moonshotFetch: MoonshotServiceConfigSchema.optional(),
  webSearch: WebSearchConfigSchema.optional(),
});

export type ServicesConfig = z.infer<typeof ServicesConfigSchema>;

const McpServerCommonFields = {
  enabled: z.boolean().optional(),
  startupTimeoutMs: z.number().int().min(1).optional(),
  toolTimeoutMs: z.number().int().min(1).optional(),
  enabledTools: z.array(z.string()).optional(),
  disabledTools: z.array(z.string()).optional(),
} as const;

export const McpServerStdioConfigSchema = z.object({
  transport: z.literal('stdio'),
  command: z.string().min(1),
  args: z.array(z.string()).optional(),
  env: StringRecordSchema.optional(),
  cwd: z.string().optional(),
  // Reserved for future kaos-backed stdio launchers. `undefined` and `'local'`
  // both mean direct child_process spawn for now.
  executor: z.enum(['local', 'kaos']).optional(),
  ...McpServerCommonFields,
});

export type McpServerStdioConfig = z.infer<typeof McpServerStdioConfigSchema>;

export const McpServerHttpConfigSchema = z.object({
  transport: z.literal('http'),
  url: z.string().url(),
  headers: StringRecordSchema.optional(),
  // Indirect secret reference: the bearer token is looked up from
  // `process.env[bearerTokenEnvVar]` at connection time, never committed.
  bearerTokenEnvVar: z.string().min(1).optional(),
  ...McpServerCommonFields,
});

export type McpServerHttpConfig = z.infer<typeof McpServerHttpConfigSchema>;

const McpServerConfigDiscriminatedSchema = z.discriminatedUnion('transport', [
  McpServerStdioConfigSchema,
  McpServerHttpConfigSchema,
]);

export const McpServerConfigSchema = z.preprocess((raw) => {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return raw;
  const obj = raw as Record<string, unknown>;
  if ('transport' in obj) return obj;
  if (typeof obj['command'] === 'string') return { ...obj, transport: 'stdio' };
  if (typeof obj['url'] === 'string') return { ...obj, transport: 'http' };
  return obj;
}, McpServerConfigDiscriminatedSchema);

export type McpServerConfig = z.infer<typeof McpServerConfigSchema>;

export const BrowserConfigSchema = z.object({
  enabled: z.boolean().optional(),
  chromePort: z.number().int().min(1).max(65535).optional(),
  traceEnabled: z.boolean().optional(),
  traceRetentionDays: z.number().int().min(1).optional(),

});

export type BrowserConfig = z.infer<typeof BrowserConfigSchema>;

export const E2EConfigSchema = z.object({
  enabled: z.boolean().default(true),
  strategy: z.enum(['always', 'smart', 'critical-only']).default('smart'),
  criticalTools: z.array(z.string()).default(['ExitPlanModeTool']),
  failurePolicy: z.enum(['block', 'warn', 'ignore']).default('warn'),
  maxConcurrency: z.number().int().min(1).default(4),
  testTimeout: z.number().int().min(1000).default(30000),
  reportDir: z.string().default('.ody-code/test-reports'),
  generatedTestDir: z.string().default('.ody-code/test-generated/e2e'),
  recursiveAnalysisEnabled: z.boolean().default(true),
  maxRecursiveDepth: z.number().int().min(1).default(3),
  cacheEnabled: z.boolean().default(true),
  cacheDir: z.string().default('.ody-code/e2e-cache'),
  cacheTtlDays: z.number().int().min(1).default(7),
  cacheMaxEntries: z.number().int().min(1).default(20),
});

export type E2EConfig = z.infer<typeof E2EConfigSchema>;

/**
 * Independent adversarial review of the TEST CODE the implementation model wrote
 * (judge ≠ athlete). When enabled, completing a test-related task injects a
 * reminder to call the ReviewTests tool, which runs a second model over the
 * changed tests + their implementation. Enabled by default; set `enabled = false`
 * to opt out. When no `mode_models.test_review` alias is configured, the review
 * runs on the model the current mode is already using.
 */
export const TestReviewConfigSchema = z.object({
  enabled: z.boolean().default(true),
});

export type TestReviewConfig = z.infer<typeof TestReviewConfigSchema>;

export const MicroagentBudgetConfigSchema = z.object({
  maxTokens: z.number().int().min(0).optional(),
});

export type MicroagentBudgetConfig = z.infer<typeof MicroagentBudgetConfigSchema>;

export const OdyConfigSchema = z.object({
  providers: z.record(z.string(), ProviderConfigSchema).default({}),
  defaultProvider: z.string().optional(),
  defaultModel: z.string().optional(),
  models: z.record(z.string(), ModelAliasSchema).optional(),
  thinking: ThinkingConfigSchema.optional(),
  sessionMode: z.enum(['plan', 'design']).optional(),
  yolo: z.boolean().optional(),
  defaultThinking: z.boolean().optional(),
  defaultPermissionMode: PermissionModeSchema.optional(),
  defaultSessionMode: z.enum(['plan', 'design']).optional(),
  permission: PermissionConfigSchema.optional(),
  hooks: z.array(HookDefSchema).optional(),
  services: ServicesConfigSchema.optional(),
  mergeAllAvailableSkills: z.boolean().optional(),
  extraSkillDirs: z.array(z.string()).optional(),
  loopControl: LoopControlSchema.optional(),
  background: BackgroundConfigSchema.optional(),
  telemetry: z.boolean().optional(),
  modeModels: z.object({
    plan: z.string().optional(),
    design: z.string().optional(),
    review: z.string().optional(),
    testReview: z.string().optional(),
    codeReview: z.string().optional(),
    codeReviewRequest: z.string().optional(),
    codeReviewReceive: z.string().optional(),
  }).optional(),
  browser: BrowserConfigSchema.optional(),
  e2e: E2EConfigSchema.optional(),
  testReview: TestReviewConfigSchema.optional(),
  microagentBudget: MicroagentBudgetConfigSchema.optional(),
  raw: z.record(z.string(), z.unknown()).optional(),
});

export type OdyConfig = z.infer<typeof OdyConfigSchema>;

const ProviderConfigPatchSchema = ProviderConfigSchema.partial();
const ModelAliasPatchSchema = ModelAliasSchema.partial();
const ThinkingConfigPatchSchema = ThinkingConfigSchema.partial();
const PermissionConfigPatchSchema = PermissionConfigSchema.partial();
const LoopControlPatchSchema = LoopControlSchema.partial();
const BackgroundConfigPatchSchema = BackgroundConfigSchema.partial();
const MoonshotServiceConfigPatchSchema = MoonshotServiceConfigSchema.partial();
const WebSearchProviderConfigPatchSchema = WebSearchProviderConfigSchema.partial();
const WebSearchConfigPatchSchema = z.object({
  primary: WebSearchProviderConfigPatchSchema.optional(),
  secondary: WebSearchProviderConfigPatchSchema.optional(),
});
const ServicesConfigPatchSchema = z.object({
  moonshotSearch: MoonshotServiceConfigPatchSchema.optional(),
  moonshotFetch: MoonshotServiceConfigPatchSchema.optional(),
  webSearch: WebSearchConfigPatchSchema.optional(),
});

export const OdyConfigPatchSchema = z
  .object({
    providers: z.record(z.string(), ProviderConfigPatchSchema).optional(),
    defaultProvider: z.string().optional(),
    defaultModel: z.string().optional(),
    models: z.record(z.string(), ModelAliasPatchSchema).optional(),
    thinking: ThinkingConfigPatchSchema.optional(),
    sessionMode: z.enum(['plan', 'design']).optional(),
    yolo: z.boolean().optional(),
    defaultThinking: z.boolean().optional(),
    defaultPermissionMode: PermissionModeSchema.optional(),
    defaultSessionMode: z.enum(['plan', 'design']).optional(),
    permission: PermissionConfigPatchSchema.optional(),
    hooks: z.array(HookDefSchema).optional(),
    services: ServicesConfigPatchSchema.optional(),
    mergeAllAvailableSkills: z.boolean().optional(),
    extraSkillDirs: z.array(z.string()).optional(),
    loopControl: LoopControlPatchSchema.optional(),
    background: BackgroundConfigPatchSchema.optional(),
    telemetry: z.boolean().optional(),
    modeModels: z.object({
      plan: z.string().optional(),
      design: z.string().optional(),
      review: z.string().optional(),
      codeReview: z.string().optional(),
      codeReviewRequest: z.string().optional(),
      codeReviewReceive: z.string().optional(),
    }).optional(),
    browser: BrowserConfigSchema.optional(),
    microagentBudget: MicroagentBudgetConfigSchema.optional(),
  })
  .strict();

export type OdyConfigPatch = z.infer<typeof OdyConfigPatchSchema>;

export function getDefaultConfig(): OdyConfig {
  return {
    providers: {},
  };
}

export function validateConfig(config: unknown): OdyConfig {
  try {
    return OdyConfigSchema.parse(config);
  } catch (error) {
    throw new OdyError(ErrorCodes.CONFIG_INVALID, `Invalid configuration: ${formatConfigValidationError(error)}`, {
      cause: error,
    });
  }
}

export function formatConfigValidationError(error: unknown): string {
  const missingModelContextSize = missingModelContextSizeMessage(error);
  if (missingModelContextSize !== undefined) return missingModelContextSize;
  return error instanceof Error ? error.message : String(error);
}

function missingModelContextSizeMessage(error: unknown): string | undefined {
  if (!(error instanceof z.ZodError)) return undefined;
  for (const issue of error.issues) {
    const [section, modelName, field] = issue.path;
    if (section === 'models' && typeof modelName === 'string' && field === 'maxContextSize') {
      return `Model "${modelName}" must define a positive max_context_size in config.toml.`;
    }
  }
  return undefined;
}

function isValidPermissionPattern(pattern: string): boolean {
  try {
    parsePattern(pattern);
    return true;
  } catch {
    return false;
  }
}
