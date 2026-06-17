# Design: Extend Login/Logout with LLM Provider Type Parameter

## 1. Scope

### In Scope

- CLI: `./ody --login <provider-type>` interactive login for supported API-key providers [C:USER]
- CLI: `./ody --logout <provider-type>` interactive logout filtered by provider type [C:USER]
- TUI: `/login <provider-type>` interactive login [C:USER]
- TUI: `/logout <provider-type>` logout with provider-type filtering [C:USER]
- Custom provider name input with validation (letters, digits, underscores; must start with a letter) [C:USER]
- API key validation via provider's `/models` endpoint [C:USER]
- Default base URL per provider with optional user override [C:USER]
- Model selection from fetched list [C:USER]
- Cascade delete associated model aliases on logout [C:USER]
- Telemetry extension: `provider_type` field on login/logout events [C:USER]
- Supported provider types: `deepseek`, `openai`, `kimi`, `openai_responses`, `anthropic` [C:USER]

### Out of Scope (Deferred)

- Google GenAI / VertexAI login — these providers do not expose a standard `/models` endpoint suitable for API-key validation at login time [C:DEFERRED]
- OAuth-type provider extension (managed:ody-code remains unchanged) [C:DEFERRED]
- Non-interactive CLI login via arguments or environment variables [C:DEFERRED]
- Encrypted API key storage [C:DEFERRED]
- Environment variable reference in config.toml (`api_key = "${VAR}"`) [C:DEFERRED]

---

## 2. Architecture & Data Flow

```
User ──► CLI --login deepseek
  │
  ▼
[apps/ody-code/src/cli/commands.ts] ──► parse --login <type>
  │
  ▼
[apps/ody-code/src/main.ts] ──► validateOptions → check TTY
  │                                └── if print mode: exit(1)
  ▼
[apps/ody-code/src/cli/run-shell.ts] ──► launch TUI with authIntent
  │
  ▼
[TUI Shell] ──► on startup detect authIntent → dispatch /login deepseek
  │
  ▼
[apps/ody-code/src/tui/commands/auth.ts]
  │   handleLoginCommand(host, 'deepseek')
  │
  ├──► promptCustomProviderName(host) ──► "deepseek_main"
  ├──► promptApiKey(host) ──► "sk-..."
  ├──► promptCustomBaseUrl(host, default) ──► "https://api.deepseek.com/v1" (or override)
  ├──► fetchProviderModels(def, apiKey, baseUrl) ──► ProviderModelInfo[]
  ├──► promptModelSelection(host, models) ──► { model: ProviderModelInfo, thinking: boolean }
  ├──► applyProviderLoginConfig(config, opts) ──► writes providers + models
  ├──► host.harness.setConfig(patch) ──► persists to ~/.ody-code/config.toml
  └──► host.authFlow.refreshConfigAfterLogin()

User ──► TUI /logout deepseek
  │
  ▼
[apps/ody-code/src/tui/commands/auth.ts]
  │   handleLogoutCommand(host, 'deepseek')
  │
  ├──► gatherProvidersByType(config, 'deepseek') ──► ['deepseek_main', 'deepseek_backup']
  ├──► (if empty) fallback to all providers [C:USER]
  ├──► promptLogoutProviderSelection(host, options) ──► "deepseek_main"
  ├──► removeProviderConfig(config, 'deepseek_main') ──► cascade delete aliases
  ├──► host.harness.setConfig(patch)
  └──► (if target === currentProvider) refreshConfigAfterLogout() + clearActiveSessionAfterLogout()
```

---

## 3. Data Structures & Interfaces

### 3.1 Provider Login Definition

**File:** `packages/oauth/src/provider-login.ts` [C:INFERRED]

```ts
export interface ProviderLoginDefinition {
  /** Wire type used in config.toml `type = "..."`. Must match kosong ProviderConfig['type']. */
  readonly type: string;
  /** Human-readable label shown in prompts. */
  readonly displayName: string;
  /** Default base URL when user does not override. */
  readonly defaultBaseUrl: string;
  /** Strategy for fetching the model list at login time. */
  readonly modelListStrategy: 'openai-compatible' | 'anthropic-sdk' | 'none';
}

export const SUPPORTED_PROVIDER_LOGINS: readonly ProviderLoginDefinition[] = [
  { type: 'deepseek', displayName: 'DeepSeek', defaultBaseUrl: 'https://api.deepseek.com/v1', modelListStrategy: 'openai-compatible' },
  { type: 'openai', displayName: 'OpenAI', defaultBaseUrl: 'https://api.openai.com/v1', modelListStrategy: 'openai-compatible' },
  { type: 'kimi', displayName: 'Kimi (Open Platform)', defaultBaseUrl: 'https://api.moonshot.cn/v1', modelListStrategy: 'openai-compatible' },
  { type: 'openai_responses', displayName: 'OpenAI (Responses API)', defaultBaseUrl: 'https://api.openai.com/v1', modelListStrategy: 'openai-compatible' },
  { type: 'anthropic', displayName: 'Anthropic', defaultBaseUrl: 'https://api.anthropic.com', modelListStrategy: 'anthropic-sdk' },
];

/** Lookup a supported provider login definition by wire type. */
export function getProviderLoginDefinition(type: string): ProviderLoginDefinition | undefined;

/** Check if a wire type is supported for interactive login. */
export function isSupportedProviderLoginType(type: string): boolean;
```

### 3.2 Model Info

**File:** `packages/oauth/src/provider-login.ts` [C:INFERRED]

```ts
export interface ProviderModelInfo {
  readonly id: string;
  readonly displayName?: string;
  readonly contextLength: number;
  readonly supportsToolUse: boolean;
  readonly supportsReasoning: boolean;
  readonly supportsImageIn: boolean;
  readonly supportsVideoIn: boolean;
}
```

### 3.3 Config Schema Extension

**File:** `packages/agent-core/src/config/schema.ts` [C:INFERRED]

```ts
// Line 6: extend ProviderTypeSchema to include 'deepseek'
export const ProviderTypeSchema = z.enum([
  'anthropic',
  'openai',
  'kimi',
  'google-genai',
  'openai_responses',
  'vertexai',
  'deepseek',        // ← [C:INFERRED] was missing despite kosong support
]);
```

### 3.4 Login Application Result

**File:** `packages/oauth/src/provider-login.ts` [C:INFERRED]

```ts
export interface ApplyProviderLoginResult {
  readonly defaultModel: string;
  readonly defaultThinking: boolean;
}

export function applyProviderLoginConfig(
  config: ManagedOdyConfigShape,
  options: {
    readonly providerName: string;
    readonly definition: ProviderLoginDefinition;
    readonly baseUrl: string;
    readonly apiKey: string;
    readonly models: readonly ProviderModelInfo[];
    readonly selectedModel: ProviderModelInfo;
    readonly thinking: boolean;
  },
): ApplyProviderLoginResult;
```

---

## 4. Pseudocode for Non-Trivial Algorithms

### 4.1 Fetch Provider Models (OpenAI-Compatible)

```ts
async function fetchOpenAICompatibleModels(
  baseUrl: string,
  apiKey: string,
  fetchImpl: typeof fetch,
  signal?: AbortSignal,
): Promise<ProviderModelInfo[]> {
  const res = await fetchImpl(`${baseUrl.replace(/\/+$/, '')}/models`, {
    headers: { Authorization: `Bearer ${apiKey}`, Accept: 'application/json' },
    signal,
  });
  if (!res.ok) {
    throw new OpenPlatformApiError(
      await readApiErrorMessage(res, `Failed to list models (HTTP ${res.status}).`),
      res.status,
    );
  }
  const payload: unknown = await res.json();
  if (!isRecord(payload) || !Array.isArray(payload['data'])) {
    throw new Error(`Unexpected models response from ${baseUrl}.`);
  }
  return payload['data']
    .map((item) => {
      if (!isRecord(item) || typeof item['id'] !== 'string') return undefined;
      const contextLength = Number(item['context_length']);
      return {
        id: item['id'],
        displayName: typeof item['display_name'] === 'string' ? item['display_name'] : undefined,
        contextLength: Number.isInteger(contextLength) && contextLength > 0 ? contextLength : 64000,
        supportsToolUse: true,
        supportsReasoning: false,
        supportsImageIn: false,
        supportsVideoIn: false,
      };
    })
    .filter((m): m is ProviderModelInfo => m !== undefined);
}
```

### 4.2 Fetch Provider Models (Anthropic SDK)

```ts
async function fetchAnthropicModels(
  baseUrl: string,
  apiKey: string,
  fetchImpl: typeof fetch,
  signal?: AbortSignal,
): Promise<ProviderModelInfo[]> {
  const res = await fetchImpl(`${baseUrl.replace(/\/+$/, '')}/v1/models`, {
    headers: { 'x-api-key': apiKey, 'anthropic-version': '2023-06-01', Accept: 'application/json' },
    signal,
  });
  if (!res.ok) {
    throw new OpenPlatformApiError(
      await readApiErrorMessage(res, `Failed to list models (HTTP ${res.status}).`),
      res.status,
    );
  }
  const payload: unknown = await res.json();
  if (!isRecord(payload) || !Array.isArray(payload['data'])) {
    throw new Error(`Unexpected models response from Anthropic.`);
  }
  return payload['data']
    .map((item) => {
      if (!isRecord(item) || typeof item['id'] !== 'string') return undefined;
      // Anthropic models do not expose context_length in /v1/models;
      // fall back to capability-registry or a safe default.
      return {
        id: item['id'],
        displayName: typeof item['display_name'] === 'string' ? item['display_name'] : undefined,
        contextLength: 200000,  // Claude family default [C:INFERRED]
        supportsToolUse: true,
        supportsReasoning: item['id'].toLowerCase().includes('claude'),
        supportsImageIn: item['id'].toLowerCase().includes('claude'),
        supportsVideoIn: false,
      };
    })
    .filter((m): m is ProviderModelInfo => m !== undefined);
}
```

### 4.3 Apply Provider Login Config

```ts
function applyProviderLoginConfig(
  config: ManagedOdyConfigShape,
  options: {
    readonly providerName: string;
    readonly definition: ProviderLoginDefinition;
    readonly baseUrl: string;
    readonly apiKey: string;
    readonly models: readonly ProviderModelInfo[];
    readonly selectedModel: ProviderModelInfo;
    readonly thinking: boolean;
  },
): ApplyProviderLoginResult {
  const providerKey = options.providerName;
  const modelKey = `${providerKey}/${options.selectedModel.id}`;

  config.providers[providerKey] = {
    type: options.definition.type as ProviderType,
    baseUrl: options.baseUrl,
    apiKey: options.apiKey,
  };

  const existingModels = config.models ?? {};

  // Remove any existing aliases for this provider name (cover case)
  for (const [key, model] of Object.entries(existingModels)) {
    if (isRecord(model) && model['provider'] === providerKey) {
      delete existingModels[key];
    }
  }

  // Create aliases for all fetched models
  for (const model of options.models) {
    const aliasKey = `${providerKey}/${model.id}`;
    const caps: string[] = [];
    if (model.supportsToolUse) caps.push('tool_use');
    if (model.supportsReasoning) caps.push('thinking');
    if (model.supportsImageIn) caps.push('image_in');
    if (model.supportsVideoIn) caps.push('video_in');

    existingModels[aliasKey] = {
      provider: providerKey,
      model: model.id,
      maxContextSize: model.contextLength,
      maxOutputSize: 8192,  // safe default [C:INFERRED]
      capabilities: caps.length > 0 ? caps : undefined,
      displayName: model.displayName,
    };
  }

  config.models = existingModels;

  // Set as default ONLY when no default is currently set [C:USER]
  const hasDefault = config.defaultModel !== undefined && config.defaultModel.trim().length > 0;
  if (!hasDefault) {
    config.defaultModel = modelKey;
    config.defaultProvider = providerKey;
    config.defaultThinking = options.thinking;
  }

  return { defaultModel: modelKey, defaultThinking: options.thinking };
}
```

### 4.4 Remove Provider Config (Cascade Delete)

```ts
function removeProviderConfig(
  config: ManagedOdyConfigShape,
  providerId: string,
): void {
  delete config.providers[providerId];

  let removedDefault = false;
  const existingModels = config.models ?? {};
  for (const [key, model] of Object.entries(existingModels)) {
    if (!isRecord(model) || model['provider'] !== providerId) continue;
    delete existingModels[key];
    if (config.defaultModel === key) removedDefault = true;
  }
  config.models = existingModels;

  if (removedDefault) {
    config.defaultModel = undefined;
  }
  if (config.defaultProvider === providerId) {
    config.defaultProvider = undefined;
  }
}
```

### 4.5 Validate Provider Name

```ts
const PROVIDER_NAME_RE = /^[a-zA-Z][a-zA-Z0-9_]*$/;
const RESERVED_NAMES = new Set(['managed:ody-code']);

function validateProviderName(
  name: string,
  existingProviders: Record<string, unknown>,
): string | undefined {
  if (name.length === 0) return 'Provider name cannot be empty.';
  if (!PROVIDER_NAME_RE.test(name)) {
    return 'Provider name must start with a letter and contain only letters, numbers, and underscores.';
  }
  if (RESERVED_NAMES.has(name)) return `Provider name "${name}" is reserved.`;
  if (existingProviders[name] !== undefined) return `Provider name "${name}" already exists.`;
  return undefined;
}
```

---

## 5. Call-Site Integration

### 5.1 CLI Commands — Add `--login` and `--logout` options

**File:** `apps/ody-code/src/cli/commands.ts`  
**Line range:** ~34–78 (after existing `.option('--design', ...)`)

```ts
.addOption(
  new Option(
    '-L, --login <provider-type>',
    'Interactive login for a supported LLM provider (deepseek, openai, anthropic, kimi, openai_responses).',
  ),
)
.addOption(
  new Option(
    '-O, --logout <provider-type>',
    'Interactive logout for providers of the given type.',
  ),
)
```

**File:** `apps/ody-code/src/cli/options.ts`  
**Line range:** ~4–15 (extend `CLIOptions`)

```ts
export interface CLIOptions {
  session: string | undefined;
  continue: boolean;
  yolo: boolean;
  auto: boolean;
  plan: boolean;
  design?: boolean;
  model: string | undefined;
  outputFormat: PromptOutputFormat | undefined;
  prompt: string | undefined;
  skillsDirs: string[];
  loginProvider: string | undefined;      // ← NEW
  logoutProvider: string | undefined;     // ← NEW
}
```

**File:** `apps/ody-code/src/cli/commands.ts`  
**Line range:** ~99–118 (extend `opts` construction)

```ts
const opts: CLIOptions = {
  session: sessionValue,
  continue: raw['continue'] as boolean,
  yolo: yoloValue,
  auto: autoValue,
  plan: raw['plan'] as boolean,
  design: raw['design'] as boolean,
  model: raw['model'] as string | undefined,
  outputFormat: raw['outputFormat'] as CLIOptions['outputFormat'],
  prompt: raw['prompt'] as string | undefined,
  skillsDirs: raw['skillsDir'] as string[],
  loginProvider: raw['login'] as string | undefined,     // ← NEW
  logoutProvider: raw['logout'] as string | undefined,    // ← NEW
};
```

### 5.2 Main Entry — Handle `--login` / `--logout`

**File:** `apps/ody-code/src/main.ts`  
**Line range:** ~41–67 (inside `handleMainCommand`)

```ts
export async function handleMainCommand(opts: CLIOptions, version: string): Promise<void> {
  let validated: ReturnType<typeof validateOptions>;
  try {
    validated = validateOptions(opts);
  } catch (error) {
    // ... existing error handling ...
  }

  // NEW: --login / --logout require shell mode
  if (opts.loginProvider !== undefined || opts.logoutProvider !== undefined) {
    if (validated.uiMode === 'print') {
      process.stderr.write('error: --login and --logout require interactive shell mode.\n');
      process.exit(1);
    }
    await runShell(validated.options, version, {
      authIntent: opts.loginProvider !== undefined
        ? { kind: 'login', providerType: opts.loginProvider }
        : { kind: 'logout', providerType: opts.logoutProvider },
    });
    return;
  }

  if (validated.uiMode === 'print') {
    await runPrompt(validated.options, version);
    return;
  }
  await runShell(validated.options, version);
}
```

### 5.3 TUI Slash Command — Extend `/login` and `/logout` signatures

**File:** `apps/ody-code/src/tui/commands/auth.ts`  
**Line range:** ~29–41 (rewrite `handleLoginCommand`)

```ts
export async function handleLoginCommand(
  host: SlashCommandHost,
  providerTypeArg?: string,
): Promise<void> {
  const providerType = providerTypeArg?.trim().toLowerCase();

  if (providerType === undefined || providerType.length === 0) {
    // No argument: preserve existing behavior (platform picker for OAuth + Open Platforms)
    await handleLegacyLoginCommand(host);
    return;
  }

  if (!isSupportedProviderLoginType(providerType)) {
    host.showError(
      `Unsupported provider type: "${providerType}". ` +
      `Supported: ${SUPPORTED_PROVIDER_LOGINS.map((p) => p.type).join(', ')}.`
    );
    return;
  }

  const definition = getProviderLoginDefinition(providerType)!;
  await handleProviderLogin(host, definition);
}
```

**File:** `apps/ody-code/src/tui/commands/auth.ts`  
**Line range:** ~177–237 (rewrite `handleLogoutCommand`)

```ts
export async function handleLogoutCommand(
  host: SlashCommandHost,
  providerTypeArg?: string,
): Promise<void> {
  const oauthStatus = await host.harness.auth.status(DEFAULT_OAUTH_PROVIDER_NAME);
  const hasOAuthToken = oauthStatus.providers.some(
    (p) => p.providerName === DEFAULT_OAUTH_PROVIDER_NAME && p.hasToken,
  );
  const config = await host.harness.getConfig();
  const hasManagedRemnant =
    hasOAuthToken || config.providers[DEFAULT_OAUTH_PROVIDER_NAME] !== undefined;

  const providerType = providerTypeArg?.trim().toLowerCase();

  let apiKeyProviderIds: string[];
  if (providerType !== undefined && providerType.length > 0) {
    // Filter by type; fallback to all if none match [C:USER]
    apiKeyProviderIds = Object.keys(config.providers ?? {})
      .filter((id) => id !== DEFAULT_OAUTH_PROVIDER_NAME)
      .filter((id) => config.providers[id]?.type === providerType);
    if (apiKeyProviderIds.length === 0) {
      apiKeyProviderIds = Object.keys(config.providers ?? {})
        .filter((id) => id !== DEFAULT_OAUTH_PROVIDER_NAME);
    }
  } else {
    // No argument: show all (legacy behavior)
    apiKeyProviderIds = Object.keys(config.providers ?? {})
      .filter((id) => id !== DEFAULT_OAUTH_PROVIDER_NAME)
      .toSorted();
  }

  const options: ChoiceOption[] = [];
  if (hasManagedRemnant) {
    options.push({
      value: DEFAULT_OAUTH_PROVIDER_NAME,
      label: PRODUCT_NAME,
      description: 'OAuth login',
    });
  }
  for (const id of apiKeyProviderIds) {
    const baseUrl = config.providers[id]?.baseUrl;
    const pType = config.providers[id]?.type;
    options.push({
      value: id,
      label: id,
      description: `${pType ?? 'unknown'} · ${typeof baseUrl === 'string' && baseUrl.length > 0 ? baseUrl : 'no base_url'}`,
    });
  }

  if (options.length === 0) {
    host.showStatus('Nothing to logout.');
    return;
  }

  const currentModel = host.state.appState.model.trim();
  const currentProvider = host.state.availableModels[currentModel]?.provider;

  const target = await promptLogoutProviderSelection(host, options, currentProvider);
  if (target === undefined) return;

  if (target === DEFAULT_OAUTH_PROVIDER_NAME) {
    await host.harness.auth.logout(DEFAULT_OAUTH_PROVIDER_NAME);
  } else {
    await host.harness.removeProvider(target);
  }

  if (target === currentProvider) {
    await host.authFlow.refreshConfigAfterLogout();
    await host.authFlow.clearActiveSessionAfterLogout();
  } else {
    const updated = await host.harness.getConfig({ reload: true });
    host.setAppState({
      availableModels: updated.models ?? {},
      availableProviders: updated.providers ?? {},
    });
  }

  host.track('logout', { provider: target, provider_type: config.providers[target]?.type });
  const label = target === DEFAULT_OAUTH_PROVIDER_NAME ? PRODUCT_NAME : target;
  host.showStatus(`Logged out from ${label}.`);
}
```

### 5.4 Telemetry Extension

**File:** `apps/ody-code/src/tui/commands/auth.ts`

Login track (line ~71–74 area):
```ts
host.track('login', {
  provider: providerName,
  provider_type: definition.type,   // ← NEW
  method: 'api_key',
});
```

Logout track (line ~234 area):
```ts
host.track('logout', {
  provider: target,
  provider_type: config.providers[target]?.type,   // ← NEW
});
```

---

## 6. Error & Degradation

| Error Class | Immediate Handling | Degradation Path | Recovery |
|---|---|---|---|
| Unsupported provider type (`--login xyz`) | CLI: stderr + exit 1; TUI: `showError` | User re-enters correct type | Manual retry |
| Provider name already exists | Prompt: "Already exists. Overwrite? (y/N)" | User chooses overwrite (removes old + re-adds) or cancel | Manual retry or rename |
| Provider name format invalid | Real-time validation; red hint with rule | User re-enters | Manual retry |
| API key validation 401 | `showError` + hint "Please check your API key" | User re-enters key | Manual retry |
| API key validation network error | `showError` with network detail | User checks network / baseUrl | Manual retry |
| `/models` returns empty list | `showError` "No models available for this provider" | User checks key permissions or baseUrl | Manual retry |
| Logout: no providers match given type | Fallback to showing all providers [C:USER] | User selects from full list | Continue operation |
| Logout: target is active provider | Clear model + session [C:UPSTREAM] | TUI shows "Logged out"; user must `/login` again | Re-login |
| Config write failure | `showError` + throw | Disk space / permission issue | Manual fix + retry |

---

## 7. Test Plan

### Unit Tests — `packages/oauth`

| Test | Assertions |
|---|---|
| `getProviderLoginDefinition('deepseek')` | returns non-undefined; `displayName === 'DeepSeek'`; `defaultBaseUrl === 'https://api.deepseek.com/v1'` |
| `isSupportedProviderLoginType('google-genai')` | returns `false` |
| `fetchProviderModels` (openai-compatible mock, 200) | returns array with correct `id`, `contextLength`, `supportsToolUse` |
| `fetchProviderModels` (openai-compatible mock, 401) | throws `OpenPlatformApiError` with `status === 401` |
| `applyProviderLoginConfig` (empty defaultModel) | `config.providers` contains new entry; `config.models` has aliases; `config.defaultModel` is set |
| `applyProviderLoginConfig` (existing defaultModel) | `config.defaultModel` is **not** overwritten |
| `removeProviderConfig` | provider deleted; associated aliases deleted; `defaultModel` cleared if matched |
| `validateProviderName('deepseek_1', {})` | returns `undefined` (valid) |
| `validateProviderName('1deepseek', {})` | returns error string (starts with digit) |
| `validateProviderName('deep seek', {})` | returns error string (contains space) |
| `validateProviderName('deepseek!', {})` | returns error string (special char) |
| `validateProviderName('managed:ody-code', {})` | returns error string (reserved) |

### Integration Tests — `apps/ody-code`

| Test | Assertions |
|---|---|
| CLI `--login deepseek` with TTY | Shell launches; `authIntent` triggers provider login flow |
| CLI `--login deepseek` without TTY | Exits code 1 with error message |
| TUI `/login deepseek` full flow | Prompts name → key → baseUrl → models → selection → writes config → `showStatus` success |
| TUI `/login deepseek` with existing name | Prompts overwrite confirmation |
| TUI `/logout deepseek` with matches | Shows only deepseek-type providers; selection removes target + aliases |
| TUI `/logout deepseek` no matches | Falls back to showing all providers [C:USER] |
| TUI `/logout` no arg | Shows all providers (legacy behavior) |
| TUI logout active provider | Calls `refreshConfigAfterLogout` + `clearActiveSessionAfterLogout` |

### Done Criteria

```bash
# All affected packages must pass type-check and tests
pnpm --filter packages/oauth test
pnpm --filter packages/agent-core test
pnpm --filter apps/ody-code test
# Build must succeed
pnpm build
```

---

## 8. Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Anthropic `/v1/models` endpoint unavailable or returns incompatible format | Medium | High | Separate `anthropic-sdk` strategy; if fetch fails, degrade to "Cannot fetch model list" with manual model ID input fallback |
| R2 | Provider `/models` response lacks `context_length` | Medium | Medium | Use `capability-registry` lookup or safe default (64k) when field is missing |
| R3 | Config schema `ProviderTypeSchema` missing `deepseek` causes validation failure on `setConfig` | High | High | Update `ProviderTypeSchema` in `packages/agent-core/src/config/schema.ts` in the same PR |
| R4 | Multiple providers of same type cause alias key collision | Low | Low | Alias keys use `${providerName}/${modelId}` format, which is naturally namespaced by providerName |
| R5 | CLI `--login` invoked in `--prompt` (print) mode | Low | Medium | Explicit check in `handleMainCommand`; exit(1) with clear error |
| R6 | Provider name collides with reserved name (`managed:ody-code`) | Low | Low | Validation rejects reserved names before write |
| R7 | `baseUrl` override breaks model fetch (e.g. trailing slash, wrong path) | Medium | Low | Normalize `baseUrl` with `.replace(/\/+$/, '')` before appending `/models`; validate URL format |

---

## 9. Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if Wrong | How to Verify |
|---|---|---|---|---|
| A1 | Kosong `ProviderConfig` union already includes `deepseek` (it does, per `packages/kosong/src/providers/index.ts`) | High | Config write fails validation | Read `packages/kosong/src/providers/index.ts` — verified ✓ |
| A2 | `packages/agent-core/src/config/schema.ts` `ProviderTypeSchema` is missing `deepseek` | High | `setConfig` throws on deepseek provider | Read `packages/agent-core/src/config/schema.ts` — verified ✓ |
| A3 | `host.harness.removeProvider()` already cascade-deletes model aliases (it does, per `packages/oauth/src/open-platform.ts` `removeOpenPlatformConfig`) | High | Aliases become orphaned on logout | Read `packages/oauth/src/open-platform.ts` — verified ✓ |
| A4 | Anthropic `/v1/models` endpoint exists and returns `{ data: [{ id, ... }] }` | Medium | Login for Anthropic fails | Manual curl test against `api.anthropic.com/v1/models` |
| A5 | All OpenAI-compatible providers (deepseek, openai, kimi, openai_responses) support `GET /models` with `Authorization: Bearer <key>` | Medium | Model fetch fails for some providers | Manual test each provider's /models endpoint |
| A6 | `runShell` accepts an optional `authIntent` startup parameter (or can be added without breaking existing callers) | Medium | CLI `--login` cannot trigger TUI flow | Read `apps/ody-code/src/cli/run-shell.ts` signature |
| A7 | `promptApiKey`, `promptModelSelectionForOpenPlatform`, `promptLogoutProviderSelection` can be reused or extended for the new flow | High | Need to duplicate prompt logic | Read `apps/ody-code/src/tui/commands/prompts.ts` — verified ✓ |
| A8 | `ManagedOdyConfigShape` type from `@odysseythink/kimi-code-oauth` is compatible with the generic provider config shape | High | Type errors in `applyProviderLoginConfig` | Read `packages/oauth/src/managed-kimi-code.ts` |
