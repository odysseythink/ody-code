import type { Agent } from '..';

export const DEFAULT_SENSITIVE_WORDS = [
  'key',
  'token',
  'password',
  'secret',
  'credential',
] as const;

export interface TopicGeneratorOptions {
  readonly maxLength?: number;
  readonly sensitiveWords?: readonly string[];
}

export function buildTopicPrompt(userMessageText: string): string {
  return `You are a concise topic extractor. Based on the user's message below, generate a short English topic phrase (2-5 words) in kebab-case (lowercase, hyphen-separated).

Rules:
- Ignore API keys, passwords, tokens, secrets, credentials, or any sensitive information.
- Focus on the functional topic or feature being discussed.
- If the message is ambiguous, return "general".
- Output ONLY the kebab-case topic, nothing else.

User message: """${userMessageText}"""`;
}

export function cleanupTopic(
  raw: string,
  maxLength = 50,
  sensitiveWords?: readonly string[],
): string | null {
  const words = sensitiveWords ?? DEFAULT_SENSITIVE_WORDS;

  let topic = raw.trim().toLowerCase();
  topic = topic.replace(/[^\p{L}\p{N}]+/gu, '-');
  topic = topic.replace(/^-+|-+$/g, '');
  topic = topic.replace(/-+/g, '-');

  if (words.some((w) => topic.includes(w))) {
    return null;
  }

  if (topic.length > maxLength) {
    topic = topic.slice(0, maxLength);
    topic = topic.replace(/-+$/, '');
  }

  if (topic.length < 2) {
    return null;
  }

  return topic;
}

export function formatUtcTimestamp(date: Date): string {
  const iso = date.toISOString();
  return (
    iso.slice(0, 4) +
    iso.slice(5, 7) +
    iso.slice(8, 10) +
    '-' +
    iso.slice(11, 13) +
    iso.slice(14, 16) +
    iso.slice(17, 19)
  );
}

export class TopicGenerator {
  constructor(
    private readonly agent: Agent,
    private readonly options: TopicGeneratorOptions = {},
  ) {}

  async generate(): Promise<string | null> {
    const history = this.agent.context.history;
    const lastUserMessage = history.findLast(
      (msg) => msg.role === 'user' && msg.origin?.kind === 'user',
    );

    if (lastUserMessage === undefined) {
      this.agent.telemetry.track('topic_generation_failed', { reason: 'no_user_message' });
      return null;
    }

    const userMessageText = lastUserMessage.content
      .filter((part) => part.type === 'text')
      .map((part) => part.text)
      .join('')
      .trim();

    if (userMessageText.length === 0) {
      this.agent.telemetry.track('topic_generation_failed', { reason: 'empty_user_message' });
      return null;
    }

    let rawTopic: string;
    try {
      const provider = this.agent.config.provider;
      const result = await this.agent.generate(
        provider,
        buildTopicPrompt(userMessageText),
        [],
        [{ role: 'user', content: [{ type: 'text', text: userMessageText }], toolCalls: [] }],
        {},
        { signal: AbortSignal.timeout(3000) },
      );
      rawTopic = result.message.content
        .filter((part) => part.type === 'text')
        .map((part) => part.text)
        .join('')
        .trim();
    } catch (error) {
      const reason = error instanceof Error ? error.name : 'unknown_error';
      this.agent.telemetry.track('topic_generation_failed', { reason });
      return null;
    }

    if (rawTopic.length === 0) {
      this.agent.telemetry.track('topic_generation_failed', { reason: 'empty_result' });
      return null;
    }

    const topic = cleanupTopic(rawTopic, this.options.maxLength, this.options.sensitiveWords);
    if (topic === null) {
      this.agent.telemetry.track('topic_generation_failed', {
        reason: 'sensitive_content_or_invalid',
      });
      return null;
    }

    return topic;
  }
}
