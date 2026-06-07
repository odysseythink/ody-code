import { describe, expect, it, vi } from 'vitest';
import {
  TopicGenerator,
  buildTopicPrompt,
  cleanupTopic,
  extractFirstHeading,
  extractTopicFromMessage,
  formatDatePrefix,
  formatUtcTimestamp,
  slugifyTitle,
  stripMarkdownFormatting,
  stripLocators,
} from '../../../src/agent/session-mode/topic-generator';
import type { Agent } from '../../../src/agent';

function makeAgent(overrides: {
  generate?: () => Promise<{
    message: { content: Array<{ type: string; text: string }> };
  }>;
  history?: Array<{
    role: string;
    content: Array<{ type: string; text: string }>;
    origin?: { kind: string; variant?: string };
  }>;
} = {}): Agent {
  return {
    context: {
      history: overrides.history ?? [],
    },
    config: {
      get provider() {
        return { name: 'mock', modelName: 'mock-model' };
      },
    },
    generate:
      overrides.generate ??
      vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'user-dashboard' }] },
      }),
    telemetry: { track: vi.fn() },
  } as unknown as Agent;
}

describe('cleanupTopic', () => {
  it('returns kebab-case slug for valid input', () => {
    expect(cleanupTopic('User Dashboard')).toBe('user-dashboard');
    expect(cleanupTopic('User Profile')).toBe('user-profile');
  });

  it('filters built-in sensitive words', () => {
    expect(cleanupTopic('my-password-manager')).toBeNull();
    expect(cleanupTopic('api-key-handler')).toBeNull();
    expect(cleanupTopic('secret-token-store')).toBeNull();
    expect(cleanupTopic('credential-auth-flow')).toBeNull();
  });

  it('truncates to max length and strips trailing hyphens', () => {
    const long = 'a'.repeat(100);
    expect(cleanupTopic(long)).toHaveLength(50);
    expect(cleanupTopic(long)).toBe('a'.repeat(50));
  });

  it('returns null for too-short input', () => {
    expect(cleanupTopic('a')).toBeNull();
    expect(cleanupTopic('')).toBeNull();
  });

  it('handles non-ASCII input', () => {
    expect(cleanupTopic('用户仪表盘')).toBe('用户仪表盘');
    expect(cleanupTopic('hello 世界 world')).toBe('hello-世界-world');
  });

  it('collapses multiple hyphens and trims edges', () => {
    expect(cleanupTopic('hello---world')).toBe('hello-world');
    expect(cleanupTopic('-hello-')).toBe('hello');
  });
});

describe('formatUtcTimestamp', () => {
  it('formats a UTC date as YYYYMMDD-HHMMSS', () => {
    const date = new Date('2025-06-04T14:30:52.000Z');
    expect(formatUtcTimestamp(date)).toBe('20250604-143052');
  });
});

describe('buildTopicPrompt', () => {
  it('includes the user message and kebab-case instruction', () => {
    const prompt = buildTopicPrompt('Build a dashboard');
    expect(prompt).toContain('Build a dashboard');
    expect(prompt).toContain('kebab-case');
    expect(prompt).toContain('Ignore API keys');
  });
});

describe('TopicGenerator', () => {
  it('generates topic from the last real user message', async () => {
    const agent = makeAgent({
      history: [
        { role: 'assistant', content: [{ type: 'text', text: 'Hello' }] },
        {
          role: 'user',
          content: [{ type: 'text', text: 'Build a user dashboard' }],
          origin: { kind: 'user' },
        },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('user-dashboard');
    expect(agent.generate).toHaveBeenCalledTimes(1);
  });

  it('ignores injection and system messages', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'real-request' }] },
      }),
      history: [
        {
          role: 'user',
          content: [{ type: 'text', text: 'injected reminder' }],
          origin: { kind: 'injection', variant: 'plan_mode' },
        },
        {
          role: 'user',
          content: [{ type: 'text', text: 'real request' }],
          origin: { kind: 'user' },
        },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('real-request');
  });

  it('returns null when no user message exists', async () => {
    const agent = makeAgent({ history: [] });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'no_user_message',
    });
  });

  it('returns null when user message is empty', async () => {
    const agent = makeAgent({
      history: [
        { role: 'user', content: [{ type: 'text', text: '' }], origin: { kind: 'user' } },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'empty_user_message',
    });
  });

  it('falls back to message extraction and tracks on LLM error', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockRejectedValue(new Error('Timeout')),
      history: [
        { role: 'user', content: [{ type: 'text', text: 'some request' }], origin: { kind: 'user' } },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('some-request');
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'Error',
    });
  });

  it('falls back to message extraction when LLM fails', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockRejectedValue(new Error('Timeout')),
      history: [
        {
          role: 'user',
          content: [{ type: 'text', text: 'Build a user dashboard for analytics' }],
          origin: { kind: 'user' },
        },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('build-a-user-dashboard-for');
  });

  it('falls back to message extraction when LLM returns empty text', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: '   ' }] },
      }),
      history: [
        { role: 'user', content: [{ type: 'text', text: 'some request' }], origin: { kind: 'user' } },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('some-request');
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'empty_result',
    });
  });

  it('falls back to message extraction when cleaned topic contains sensitive words', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'password-reset' }] },
      }),
      history: [
        { role: 'user', content: [{ type: 'text', text: 'some request' }], origin: { kind: 'user' } },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBe('some-request');
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'sensitive_content_or_invalid',
    });
  });

  it('respects custom maxLength', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'very-long-topic-name-here' }] },
      }),
      history: [
        { role: 'user', content: [{ type: 'text', text: 'some request' }], origin: { kind: 'user' } },
      ],
    });
    const generator = new TopicGenerator(agent, { maxLength: 10 });
    const topic = await generator.generate();
    expect(topic).toBe('very-long');
  });
});

describe('extractTopicFromMessage', () => {
  it('extracts first few words from English message', () => {
    expect(extractTopicFromMessage('Build a user dashboard for analytics')).toBe('build-a-user-dashboard-for');
  });
  it('extracts Chinese words', () => {
    expect(extractTopicFromMessage('新增对GLM模型的支持')).toBe('新增对glm模型的支持');
  });
  it('returns null for empty text', () => {
    expect(extractTopicFromMessage('')).toBeNull();
  });
  it('returns null for too-short text', () => {
    expect(extractTopicFromMessage('a')).toBeNull();
  });
  it('filters sensitive words', () => {
    expect(extractTopicFromMessage('my password reset')).toBeNull();
  });
  it('respects maxWords limit', () => {
    expect(extractTopicFromMessage('one two three four five six', 3)).toBe('one-two-three');
  });
});


describe('stripLocators', () => {
  it('drops absolute paths glued after a CJK colon, keeping the prose', () => {
    const msg =
      '合并两份设计：/Users/ranwei/.ody-code/sessions/wd_x/agents/main/designs/jay-garrick-flash-monet.md\n/Users/ranwei/workspace/go_work/2026-06-06-frontend-backend-integration-design.md';
    expect(stripLocators(msg)).toBe('合并两份设计：');
  });

  it('drops http(s) URLs', () => {
    expect(stripLocators('see https://example.com/docs and fix it')).toBe('see and fix it');
  });

  it('drops relative paths with a file extension', () => {
    expect(stripLocators('refactor src/agent/index.ts please')).toBe('refactor please');
  });

  it('drops ./ ../ ~/ prefixed paths', () => {
    expect(stripLocators('compare ./a/b.md with ~/notes/c.md and ../d/e.md')).toBe('compare with and');
  });

  it('leaves ordinary slashed words (read/write, and/or) intact', () => {
    expect(stripLocators('support read/write and/or stream modes')).toBe(
      'support read/write and/or stream modes',
    );
  });

  it('leaves prose with no locators untouched', () => {
    expect(stripLocators('add a billing module')).toBe('add a billing module');
  });

  it('returns empty string when the message is only a path', () => {
    expect(stripLocators('/Users/ranwei/workspace/x.md')).toBe('');
  });
});

describe('heading & slug helpers', () => {
  it('extractFirstHeading finds first H1', () => {
    expect(extractFirstHeading('# Hello World')).toBe('Hello World');
    expect(extractFirstHeading('## No H1\n# Yes')).toBe('Yes');
    expect(extractFirstHeading('no heading')).toBeNull();
    expect(extractFirstHeading('```\n# not a heading\n```\n# Real')).toBe('Real');
    expect(extractFirstHeading('# **Bold** Title')).toBe('Bold Title');
    expect(extractFirstHeading('# *Italic* _Plan_')).toBe('Italic Plan');
    expect(extractFirstHeading('#   ')).toBeNull();
    expect(extractFirstHeading('#aNoSpace')).toBeNull();
  });

  it('stripMarkdownFormatting removes inline syntax', () => {
    expect(stripMarkdownFormatting('**Bold** Title')).toBe('Bold Title');
    expect(stripMarkdownFormatting('Title-With-Hyphens')).toBe('Title-With-Hyphens');
    expect(stripMarkdownFormatting('中文标题')).toBe('中文标题');
    // [text](url) must yield only the link text, not text+url concatenated.
    expect(stripMarkdownFormatting('[RFC-42](https://example.com)')).toBe('RFC-42');
    expect(stripMarkdownFormatting('[Visit site](http://x.com) now')).toBe('Visit site now');
  });

  it('slugifyTitle produces kebab-case slugs', () => {
    expect(slugifyTitle('Implement GLM Provider!')).toBe('implement-glm-provider');
    expect(slugifyTitle('中文标题')).toBe('中文标题');
    expect(slugifyTitle('a'.repeat(100))).toBe('a'.repeat(50));
    // All-separator input returns "". Call sites must guard against empty result.
    expect(slugifyTitle('---')).toBe('');
    expect(slugifyTitle('')).toBe('');
  });

  it('extractFirstHeading returns null when heading is all inline markup (cleaned is empty)', () => {
    // After stripMarkdownFormatting the cleaned string may be empty even when raw was non-empty.
    expect(extractFirstHeading('# ```')).toBeNull();
  });

  it('formatDatePrefix returns local YYYY-MM-DD, not UTC date', () => {
    // Use a fixed date object where local and UTC would differ at timezone boundaries.
    // getFullYear/Month/Date extract local-calendar components regardless of timezone.
    const d = new Date(2024, 5, 5); // local 2024-06-05, month is 0-indexed
    expect(formatDatePrefix(d)).toBe('2024-06-05');
  });
});
