import type {
  CatalogModelEntry,
  CatalogProviderEntry,
  Message,
  ProviderType,
} from '@odysseythink/kosong';
import {
  catalogBaseUrl,
  catalogModelToCapability,
  catalogProviderModels,
  inferWireType,
} from '@odysseythink/kosong';
import {
  getAnthropicModelCapability,
  getGoogleGenAIModelCapability,
  getOpenAILegacyModelCapability,
  getOpenAIResponsesModelCapability,
  usesOpenAIResponsesDeveloperRole,
} from '@odysseythink/kosong/providers/capability-registry';
import {
  mergeRequestHeaders,
  requireProviderApiKey,
} from '@odysseythink/kosong/providers/request-auth';
import {
  normalizeToolCallIdsForProvider,
  sanitizeOpenAIResponsesCallId,
  sanitizeToolCallId,
  type ToolCallIdPolicy,
} from '@odysseythink/kosong/providers/tool-call-id';

export interface Fixture {
  operations: Array<{
    operation: string;
    cases: Array<{ name: string; input: Record<string, unknown> }>;
  }>;
}

export interface GoldenResult {
  name: string;
  output?: unknown;
  error?: string;
}

export interface GoldenOperation {
  operation: string;
  results: GoldenResult[];
}

export interface GoldenOutput {
  operations: GoldenOperation[];
}

const TOOL_CALL_ID_MAX_LENGTH = 64;

function toolCallIdPolicyForProvider(provider: ProviderType): ToolCallIdPolicy {
  if (provider === 'openai_responses') {
    return {
      normalize: (id: string) =>
        sanitizeOpenAIResponsesCallId(id, TOOL_CALL_ID_MAX_LENGTH),
      maxLength: TOOL_CALL_ID_MAX_LENGTH,
    };
  }
  return {
    normalize: (id: string) => sanitizeToolCallId(id, TOOL_CALL_ID_MAX_LENGTH),
    maxLength: TOOL_CALL_ID_MAX_LENGTH,
  };
}

export async function runTsKosongUtilsGolden(
  fixturePath: string,
): Promise<GoldenOutput> {
  const { readFileSync } = await import('node:fs');
  const raw = readFileSync(fixturePath, 'utf8');
  const fixture: Fixture = JSON.parse(raw);

  const operations: GoldenOperation[] = [];
  for (const op of fixture.operations) {
    const results: GoldenResult[] = [];
    for (const c of op.cases) {
      try {
        const output = runCase(op.operation, c.input);
        results.push({ name: c.name, output });
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        results.push({ name: c.name, error: msg });
      }
    }
    operations.push({ operation: op.operation, results });
  }
  return { operations };
}

function runCase(
  operation: string,
  input: Record<string, unknown>,
): unknown {
  switch (operation) {
    case 'sanitizeToolCallId': {
      const id = String(input['id']);
      const maxLengthVal = input['maxLength'];
      const maxLength =
        maxLengthVal === undefined ? undefined : Number(maxLengthVal);
      return sanitizeToolCallId(id, maxLength);
    }
    case 'sanitizeOpenAIResponsesCallId': {
      const id = String(input['id']);
      const maxLengthVal = input['maxLength'];
      const maxLength =
        maxLengthVal === undefined ? undefined : Number(maxLengthVal);
      return sanitizeOpenAIResponsesCallId(id, maxLength);
    }
    case 'normalizeToolCallIdsForProvider': {
      const messages = input['messages'] as Message[];
      const provider = input['provider'] as ProviderType;
      return normalizeToolCallIdsForProvider(
        messages,
        toolCallIdPolicyForProvider(provider),
      );
    }
    case 'requireProviderApiKey': {
      const providerName = String(input['providerName']);
      const auth = input['auth'] as { apiKey?: string } | undefined;
      const defaultApiKeyVal = input['defaultApiKey'];
      const defaultApiKey =
        defaultApiKeyVal === undefined ? undefined : String(defaultApiKeyVal);
      return requireProviderApiKey(providerName, auth, defaultApiKey);
    }
    case 'mergeRequestHeaders': {
      const defaultHeaders = input['defaultHeaders'] as
        | Record<string, string>
        | undefined;
      const requestHeaders = input['requestHeaders'] as
        | Record<string, string>
        | undefined;
      return mergeRequestHeaders(defaultHeaders, requestHeaders) ?? null;
    }
    case 'getOpenAILegacyModelCapability': {
      return getOpenAILegacyModelCapability(String(input['modelName']));
    }
    case 'getOpenAIResponsesModelCapability': {
      return getOpenAIResponsesModelCapability(String(input['modelName']));
    }
    case 'getAnthropicModelCapability': {
      return getAnthropicModelCapability(String(input['modelName']));
    }
    case 'getGoogleGenAIModelCapability': {
      return getGoogleGenAIModelCapability(String(input['modelName']));
    }
    case 'usesOpenAIResponsesDeveloperRole': {
      return usesOpenAIResponsesDeveloperRole(String(input['modelName']));
    }
    case 'inferWireType': {
      return inferWireType(input['entry'] as CatalogProviderEntry) ?? null;
    }
    case 'catalogBaseUrl': {
      return (
        catalogBaseUrl(
          input['entry'] as CatalogProviderEntry,
          input['wire'] as ProviderType,
        ) ?? null
      );
    }
    case 'catalogModelToCapability': {
      return (
        catalogModelToCapability(input['model'] as CatalogModelEntry) ?? null
      );
    }
    case 'catalogProviderModels': {
      return catalogProviderModels(input['entry'] as CatalogProviderEntry);
    }
    default:
      throw new Error(`unknown operation: ${operation}`);
  }
}
