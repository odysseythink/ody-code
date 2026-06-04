import { readApiErrorMessage } from './api-error';
import { isRecord } from './utils';
import { OpenPlatformApiError } from './open-platform';
import type { ManagedKimiConfigShape } from './managed-kimi-code';

export interface ProviderLoginDefinition {
  readonly type: string;
  readonly displayName: string;
  readonly defaultBaseUrl: string;
  readonly modelListStrategy: 'openai-compatible' | 'anthropic-sdk' | 'none';
}

export const SUPPORTED_PROVIDER_LOGINS: readonly ProviderLoginDefinition[] = [
  { type: 'deepseek', displayName: 'DeepSeek', defaultBaseUrl: 'https://api.deepseek.com/v1', modelListStrategy: 'openai-compatible' },
  { type: 'openai', displayName: 'OpenAI', defaultBaseUrl: 'https://api.openai.com/v1', modelListStrategy: 'openai-compatible' },
  { type: 'kimi', displayName: 'Kimi (Open Platform)', defaultBaseUrl: 'https://api.moonshot.cn/v1', modelListStrategy: 'openai-compatible' },
  { type: 'openai_responses', displayName: 'OpenAI (Responses API)', defaultBaseUrl: 'https://api.openai.com/v1', modelListStrategy: 'openai-compatible' },
  { type: 'anthropic', displayName: 'Anthropic', defaultBaseUrl: 'https://api.anthropic.com', modelListStrategy: 'anthropic-sdk' },
];

export function getProviderLoginDefinition(type: string): ProviderLoginDefinition | undefined {
  return SUPPORTED_PROVIDER_LOGINS.find((p) => p.type === type);
}

export function isSupportedProviderLoginType(type: string): boolean {
  return getProviderLoginDefinition(type) !== undefined;
}

export interface ProviderModelInfo {
  readonly id: string;
  readonly displayName?: string;
  readonly contextLength: number;
  readonly supportsToolUse: boolean;
  readonly supportsReasoning: boolean;
  readonly supportsImageIn: boolean;
  readonly supportsVideoIn: boolean;
}

export interface ApplyProviderLoginResult {
  readonly defaultModel: string;
  readonly defaultThinking: boolean;
}

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
      const displayName = typeof item['display_name'] === 'string' ? item['display_name'] : undefined;
      return {
        id: item['id'],
        contextLength: Number.isInteger(contextLength) && contextLength > 0 ? contextLength : 64000,
        supportsToolUse: true,
        supportsReasoning: false,
        supportsImageIn: false,
        supportsVideoIn: false,
        ...(displayName !== undefined ? { displayName } : {}),
      };
    })
    .filter((m): m is ProviderModelInfo => m !== undefined);
}

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
      const displayName = typeof item['display_name'] === 'string' ? item['display_name'] : undefined;
      return {
        id: item['id'],
        contextLength: 200000,
        supportsToolUse: true,
        supportsReasoning: item['id'].toLowerCase().includes('claude'),
        supportsImageIn: item['id'].toLowerCase().includes('claude'),
        supportsVideoIn: false,
        ...(displayName !== undefined ? { displayName } : {}),
      };
    })
    .filter((m): m is ProviderModelInfo => m !== undefined);
}

export async function fetchProviderModels(
  definition: ProviderLoginDefinition,
  apiKey: string,
  fetchImpl: typeof fetch = fetch,
  signal?: AbortSignal,
): Promise<ProviderModelInfo[]> {
  switch (definition.modelListStrategy) {
    case 'openai-compatible':
      return fetchOpenAICompatibleModels(definition.defaultBaseUrl, apiKey, fetchImpl, signal);
    case 'anthropic-sdk':
      return fetchAnthropicModels(definition.defaultBaseUrl, apiKey, fetchImpl, signal);
    case 'none':
      return [];
    default:
      throw new Error(`Unknown model list strategy: ${definition.modelListStrategy}`);
  }
}

export function applyProviderLoginConfig(
  config: ManagedKimiConfigShape,
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
    type: options.definition.type,
    baseUrl: options.baseUrl,
    apiKey: options.apiKey,
  };

  const existingModels = config.models ?? {};

  for (const [key, model] of Object.entries(existingModels)) {
    if (isRecord(model) && model['provider'] === providerKey) {
      delete existingModels[key];
    }
  }

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
      maxOutputSize: 8192,
      capabilities: caps.length > 0 ? caps : undefined,
      displayName: model.displayName,
    };
  }

  config.models = existingModels;

  const hasDefault = config.defaultModel !== undefined && config.defaultModel.trim().length > 0;
  if (!hasDefault) {
    config.defaultModel = modelKey;
    config.defaultThinking = options.thinking;
  }

  return { defaultModel: modelKey, defaultThinking: options.thinking };
}

export function removeProviderConfig(
  config: ManagedKimiConfigShape,
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
}

const PROVIDER_NAME_RE = /^[a-zA-Z][a-zA-Z0-9_]*$/;
const RESERVED_NAMES = new Set(['managed:ody-code']);

export function validateProviderName(
  name: string,
  existingProviders: Record<string, unknown>,
): string | undefined {
  if (name.length === 0) return 'Provider name cannot be empty.';
  if (RESERVED_NAMES.has(name)) return `Provider name "${name}" is reserved.`;
  if (!PROVIDER_NAME_RE.test(name)) {
    return 'Provider name must start with a letter and contain only letters, numbers, and underscores.';
  }
  if (existingProviders[name] !== undefined) return `Provider name "${name}" already exists.`;
  return undefined;
}
