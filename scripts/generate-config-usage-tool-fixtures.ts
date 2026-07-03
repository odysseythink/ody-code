import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.resolve(__dirname, '../rust-ody/crates/agent-rs/tests/fixtures');
mkdirSync(outDir, { recursive: true });

const configFixture = {
  cwd: '/fixture/cwd',
  provider: {
    type: 'kimi',
    model: 'kimi-k2',
  },
  modelAlias: 'kimi-k2',
  modelCapabilities: {
    image_in: false,
    video_in: false,
    audio_in: false,
    thinking: true,
    tool_use: true,
    max_context_tokens: 256000,
    max_output_tokens: 16384,
  },
  profileName: 'fixture',
  thinkingLevel: 'high',
  systemPrompt: 'fixture system prompt',
};

const usageFixture = {
  byModel: {
    'kimi-k2': {
      inputOther: 13,
      output: 7,
      inputCacheRead: 2,
      inputCacheCreation: 1,
    },
  },
  total: {
    inputOther: 13,
    output: 7,
    inputCacheRead: 2,
    inputCacheCreation: 1,
  },
  currentTurn: {
    inputOther: 3,
    output: 2,
    inputCacheRead: 0,
    inputCacheCreation: 0,
  },
};

const toolsFixture = [
  { name: 'Bash', description: 'Execute a shell command.', active: false, source: 'builtin' },
  { name: 'Edit', description: 'Apply a targeted edit to a text file.', active: false, source: 'builtin' },
  { name: 'Glob', description: 'Find files matching a glob pattern.', active: false, source: 'builtin' },
  { name: 'Grep', description: 'Search file contents with a regex.', active: true, source: 'builtin' },
  { name: 'Read', description: 'Read a text file from the local filesystem.', active: true, source: 'builtin' },
  { name: 'Write', description: 'Write or overwrite a text file.', active: false, source: 'builtin' },
  { name: 'custom_user_tool', description: 'A user-registered tool for fixture generation.', active: true, source: 'user' },
];

writeFileSync(path.join(outDir, 'config-ts.json'), JSON.stringify(configFixture, null, 2));
writeFileSync(path.join(outDir, 'usage-ts.json'), JSON.stringify(usageFixture, null, 2));
writeFileSync(path.join(outDir, 'tools-ts.json'), JSON.stringify(toolsFixture, null, 2));

console.log('TS fixtures written to', outDir);
