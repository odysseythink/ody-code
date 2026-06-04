import { UNKNOWN_CAPABILITY, type ModelCapability } from '#/capability';
import type { Message } from '#/message';
import type { ChatProvider, GenerateOptions, StreamedMessage, ThinkingEffort } from '#/provider';
import type { Tool } from '#/tool';
import {
  OpenAILegacyChatProvider,
  type OpenAILegacyGenerationKwargs,
  type OpenAILegacyOptions,
} from './openai-legacy';

export interface DeepSeekOptions {
  apiKey?: string | undefined;
  baseUrl?: string | undefined;
  model: string;
  stream?: boolean | undefined;
  maxTokens?: number | undefined;
  reasoningKey?: string | undefined;
  httpClient?: unknown;
  defaultHeaders?: Record<string, string>;
  toolMessageConversion?: OpenAILegacyOptions['toolMessageConversion'];
  clientFactory?: OpenAILegacyOptions['clientFactory'];
}

export interface DeepSeekGenerationKwargs extends OpenAILegacyGenerationKwargs {}

const DEEPSEEK_REASONER_CAPABILITY: ModelCapability = Object.freeze({
  image_in: false,
  video_in: false,
  audio_in: false,
  thinking: true,
  tool_use: false,
  max_context_tokens: 0,
});

const DEEPSEEK_CHAT_CAPABILITY: ModelCapability = Object.freeze({
  image_in: false,
  video_in: false,
  audio_in: false,
  thinking: false,
  tool_use: true,
  max_context_tokens: 0,
});

export class DeepSeekChatProvider implements ChatProvider {
  readonly name: string = 'deepseek';

  private _delegate: OpenAILegacyChatProvider;

  constructor(options: DeepSeekOptions) {
    const apiKey = options.apiKey ?? process.env['DEEPSEEK_API_KEY'];
    // Pass an explicit empty string when no API key is available so the
    // underlying OpenAILegacyChatProvider does not fall back to OPENAI_API_KEY.
    const resolvedApiKey = apiKey === undefined || apiKey.length === 0 ? '' : apiKey;

    this._delegate = new OpenAILegacyChatProvider({
      ...options,
      apiKey: resolvedApiKey,
      baseUrl: options.baseUrl ?? 'https://api.deepseek.com/v1',
    });
  }

  get modelName(): string {
    return this._delegate.modelName;
  }

  get thinkingEffort(): ThinkingEffort | null {
    return this._delegate.thinkingEffort;
  }

  get modelParameters(): Record<string, unknown> {
    return {
      ...this._delegate.modelParameters,
      provider: 'deepseek',
    };
  }

  getCapability(model?: string): ModelCapability {
    const normalized = (model ?? this._delegate.modelName).toLowerCase();
    if (normalized.startsWith('deepseek-reasoner')) {
      return DEEPSEEK_REASONER_CAPABILITY;
    }
    if (normalized.startsWith('deepseek-chat')) {
      return DEEPSEEK_CHAT_CAPABILITY;
    }
    return UNKNOWN_CAPABILITY;
  }

  async generate(
    systemPrompt: string,
    tools: Tool[],
    history: Message[],
    options?: GenerateOptions,
  ): Promise<StreamedMessage> {
    return this._delegate.generate(systemPrompt, tools, history, options);
  }

  withThinking(effort: ThinkingEffort): DeepSeekChatProvider {
    const clone = this._clone();
    clone._delegate = this._delegate.withThinking(effort);
    return clone;
  }

  withGenerationKwargs(kwargs: DeepSeekGenerationKwargs): DeepSeekChatProvider {
    const clone = this._clone();
    clone._delegate = this._delegate.withGenerationKwargs(kwargs);
    return clone;
  }

  withMaxCompletionTokens(maxCompletionTokens: number): DeepSeekChatProvider {
    return this.withGenerationKwargs({ max_tokens: maxCompletionTokens });
  }

  private _clone(): DeepSeekChatProvider {
    const clone = Object.assign(
      Object.create(Object.getPrototypeOf(this) as object) as DeepSeekChatProvider,
      this,
    );
    return clone;
  }
}
