import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from 'vitest';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(
  __dirname,
  '../../../../../rust-ody/crates/agent-rs/tests/fixtures/tools-rust.json',
);

test('rust tools fixture matches TS expectations', () => {
  const infos = JSON.parse(readFileSync(fixture, 'utf8'));
  const active = infos
    .filter((i: { active: boolean }) => i.active)
    .map((i: { name: string }) => i.name);
  expect(active).toContain('Read');
  expect(active).toContain('Grep');
  expect(active).toContain('custom_user_tool');

  const custom = infos.find((i: { name: string }) => i.name === 'custom_user_tool');
  expect(custom.source).toBe('user');
});
