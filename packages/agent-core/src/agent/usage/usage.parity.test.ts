import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from 'vitest';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(
  __dirname,
  '../../../../../rust-ody/crates/agent-rs/tests/fixtures/usage-rust.json',
);

test('rust usage fixture matches TS expectations', () => {
  const data = JSON.parse(readFileSync(fixture, 'utf8'));
  expect(data.byModel['kimi-k2']).toEqual({
    inputOther: 13,
    output: 7,
    inputCacheRead: 2,
    inputCacheCreation: 1,
  });
  expect(data.total).toEqual({
    inputOther: 13,
    output: 7,
    inputCacheRead: 2,
    inputCacheCreation: 1,
  });
  expect(data.currentTurn).toEqual({
    inputOther: 3,
    output: 2,
    inputCacheRead: 0,
    inputCacheCreation: 0,
  });
});
