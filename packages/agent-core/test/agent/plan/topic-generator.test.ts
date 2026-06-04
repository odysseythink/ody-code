import { describe, expect, it, vi } from 'vitest';
import {
  TopicGenerator,
  buildTopicPrompt,
  cleanupTopic,
  formatUtcTimestamp,
} from '../../../src/agent/plan/topic-generator';
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
    expect(cleanupTopic('用户仪表盘')).toBeNull();
    expect(cleanupTopic('hello 世界 world')).toBe('hello-world');
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

  it('returns null and tracks on LLM error', async () => {
    const agent = makeAgent({
      generate: vi.fn().mockRejectedValue(new Error('Timeout')),
      history: [
        { role: 'user', content: [{ type: 'text', text: 'some request' }], origin: { kind: 'user' } },
      ],
    });
    const generator = new TopicGenerator(agent);
    const topic = await generator.generate();
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'Error',
    });
  });

  it('returns null when LLM returns empty text', async () => {
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
    expect(topic).toBeNull();
    expect(agent.telemetry.track).toHaveBeenCalledWith('topic_generation_failed', {
      reason: 'empty_result',
    });
  });

  it('returns null when cleaned topic contains sensitive words', async () => {
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
    expect(topic).toBeNull();
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
