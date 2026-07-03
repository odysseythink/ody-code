import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from 'vitest';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(
  __dirname,
  '../../../../../rust-ody/crates/agent-rs/tests/fixtures/config-rust.json',
);

test('rust config fixture matches TS expectations', () => {
  const data = JSON.parse(readFileSync(fixture, 'utf8'));
  expect(data.cwd).toBe('/fixture/cwd');
  expect(data.modelAlias).toBe('kimi-k2');
  expect(data.profileName).toBe('fixture');
  expect(data.thinkingLevel).toBe('high');
  expect(data.systemPrompt).toBe('fixture system prompt');
  expect(data.modelCapabilities).toEqual({
    image_in: false,
    video_in: false,
    audio_in: false,
    thinking: true,
    tool_use: true,
    max_context_tokens: 256000,
    max_output_tokens: 16384,
  });
  expect(data.provider.type).toBe('kimi');
  expect(data.provider.model).toBe('kimi-k2');
});
