import { mkdtempSync } from 'node:fs';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import { afterEach, describe, expect, it } from 'vitest';

import { ErrorCodes, OdyError } from '../../src/errors';
import {
  OdyConfigPatchSchema,
  OdyConfigSchema,
  configToTomlData,
  ensureConfigFile,
  mergeConfigPatch,
  parseConfigString,
  parseBooleanEnv,
  readConfigFile,
  resolveConfigPath,
  resolveConfigValue,
  resolveOdyHome,
  validateConfig,
  writeConfigFile,
} from '../../src/config';

const tempDirs: string[] = [];

afterEach(async () => {
  for (const dir of tempDirs.splice(0)) {
    await rm(dir, { recursive: true, force: true });
  }
});

function makeTempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'kimi-core-config-'));
  tempDirs.push(dir);
  return dir;
}

function expectOdyErrorCode(fn: () => unknown, code: string): void {
  try {
    fn();
  } catch (error) {
    expect(error).toBeInstanceOf(OdyError);
    expect((error as OdyError).code).toBe(code);
    return;
  }
  throw new Error('expected function to throw');
}

const COMPLETE_TOML = `
default_model = "kimi-code/kimi-for-coding"
default_thinking = true
default_permission_mode = "auto"
default_session_mode = 'plan'
merge_all_available_skills = true
extra_skill_dirs = ["~/team-skills", ".agents/team-skills"]
telemetry = false
theme = "dark"

[providers."managed:ody-code"]
type = "kimi"
base_url = "https://api.kimi.com/coding/v1"
api_key = "sk-file"
custom_headers = { "X-Test" = "1" }

[providers."managed:ody-code".env]
GOOGLE_CLOUD_PROJECT = "project-1"

[models."kimi-code/kimi-for-coding"]
provider = "managed:ody-code"
model = "kimi-for-coding"
max_context_size = 262144
capabilities = ["image_in", "thinking", "video_in"]
display_name = "Kimi for Coding"

[thinking]
mode = "auto"
effort = "medium"

[permission]
mode = "manual"

[[permission.rules]]
decision = "deny"
scope = "user"
pattern = "Bash(rm *)"
reason = "no rm"

[[permission.allow]]
tool = "Read"
match = "src/**"
reason = "read src"

[loop_control]
max_steps_per_run = 42
max_retries_per_step = 3
reserved_context_size = 50000
compaction_trigger_ratio = 0.85

[background]
max_running_tasks = 4
keep_alive_on_exit = false
kill_grace_period_ms = 2000
agent_task_timeout_s = 900
print_wait_ceiling_s = 3600

[[hooks]]
event = "PreToolUse"
matcher = "Shell"
command = "echo pre"
timeout = 5

[[hooks]]
event = "Stop"
command = "echo stop"

[services.moonshot_search]
base_url = "https://api.kimi.com/coding/v1/search"
api_key = "sk-search"
custom_headers = { "X-Search" = "1" }

[services.moonshot_fetch]
base_url = "https://api.kimi.com/coding/v1/fetch"
api_key = "sk-fetch"

[services.web_search]
[services.web_search.primary]
provider = "tavily"
api_key = "sk-tavily"
timeout_ms = 15000
[services.web_search.primary.options]
search_depth = "advanced"
[services.web_search.secondary]
provider = "duckduckgo"

[browser]
enabled = false
chrome_port = 9223

[notifications]
claim_stale_after_ms = 15000
`;

describe('harness config TOML loader', () => {
  it('parses the current config.toml shape through explicit field mappings', () => {
    const config = parseConfigString(COMPLETE_TOML, 'config.toml');

    expect(config.defaultModel).toBe('kimi-code/kimi-for-coding');
    expect(config.defaultThinking).toBe(true);
    expect(config.defaultPermissionMode).toBe('auto');
    expect(config.defaultSessionMode).toBe('plan');
    expect(config.mergeAllAvailableSkills).toBe(true);
    expect(config.extraSkillDirs).toEqual(['~/team-skills', '.agents/team-skills']);
    expect(config.telemetry).toBe(false);
    expect(config.providers['managed:ody-code']).toMatchObject({
      type: 'kimi',
      baseUrl: 'https://api.kimi.com/coding/v1',
      apiKey: 'sk-file',
      env: { GOOGLE_CLOUD_PROJECT: 'project-1' },
      customHeaders: { 'X-Test': '1' },
    });
    expect(config.models?.['kimi-code/kimi-for-coding']).toMatchObject({
      provider: 'managed:ody-code',
      model: 'kimi-for-coding',
      maxContextSize: 262144,
      capabilities: ['image_in', 'thinking', 'video_in'],
      displayName: 'Kimi for Coding',
    });
    expect(config.thinking).toEqual({ mode: 'auto', effort: 'medium' });
    expect(config.permission).toEqual({
      rules: [
        {
          decision: 'deny',
          scope: 'user',
          pattern: 'Bash(rm *)',
          reason: 'no rm',
        },
        {
          decision: 'allow',
          scope: 'user',
          pattern: 'Read(src/**)',
          reason: 'read src',
        },
      ],
    });
    expect(config.loopControl).toMatchObject({
      maxStepsPerTurn: 42,
      maxRetriesPerStep: 3,
      reservedContextSize: 50000,
      compactionTriggerRatio: 0.85,
    });
    expect(config.background?.agentTaskTimeoutS).toBe(900);
    expect(config.hooks).toEqual([
      {
        event: 'PreToolUse',
        matcher: 'Shell',
        command: 'echo pre',
        timeout: 5,
      },
      {
        event: 'Stop',
        command: 'echo stop',
      },
    ]);
    expect(config.services?.moonshotSearch?.customHeaders).toEqual({ 'X-Search': '1' });
    expect(config.services?.moonshotFetch?.apiKey).toBe('sk-fetch');
    expect(config.services?.webSearch?.primary.provider).toBe('tavily');
    expect(config.services?.webSearch?.primary.apiKey).toBe('sk-tavily');
    expect(config.services?.webSearch?.primary.timeoutMs).toBe(15000);
    expect(config.services?.webSearch?.primary.options).toEqual({ searchDepth: 'advanced' });
    expect(config.services?.webSearch?.secondary?.provider).toBe('duckduckgo');
    expect(config.browser).toEqual({ enabled: false, chromePort: 9223 });

    expect('theme' in config).toBe(false);
    expect(config.raw?.['theme']).toBe('dark');
    expect(config.raw?.['notifications']).toEqual({ claim_stale_after_ms: 15000 });
  });

  it('parses and round-trips split_plan_compaction_ratio under loop_control', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'split-ratio.toml');
    const config = parseConfigString('[loop_control]\nsplit_plan_compaction_ratio = 0.5\n', configPath);
    expect(config.loopControl?.splitPlanCompactionRatio).toBe(0.5);

    await writeConfigFile(configPath, config);
    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('split_plan_compaction_ratio = 0.5');
    const roundTripped = parseConfigString(text, configPath);
    expect(roundTripped.loopControl?.splitPlanCompactionRatio).toBe(0.5);
  });

  it('parses and round-trips normal_task_compaction_ratio under loop_control', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'normal-task-ratio.toml');
    const config = parseConfigString('[loop_control]\nnormal_task_compaction_ratio = 0.4\n', configPath);
    expect(config.loopControl?.normalTaskCompactionRatio).toBe(0.4);

    await writeConfigFile(configPath, config);
    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('normal_task_compaction_ratio = 0.4');
    const roundTripped = parseConfigString(text, configPath);
    expect(roundTripped.loopControl?.normalTaskCompactionRatio).toBe(0.4);
  });

  it('parses and round-trips test_review.enabled and mode_models.test_review', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'test-review.toml');
    const config = parseConfigString(
      '[test_review]\nenabled = true\n\n[mode_models]\ntest_review = "ody-code/reviewer"\n',
      configPath,
    );
    expect(config.testReview?.enabled).toBe(true);
    expect(config.modeModels?.testReview).toBe('ody-code/reviewer');

    await writeConfigFile(configPath, config);
    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('enabled = true');
    expect(text).toContain('test_review = "ody-code/reviewer"');
    const roundTripped = parseConfigString(text, configPath);
    expect(roundTripped.testReview?.enabled).toBe(true);
    expect(roundTripped.modeModels?.testReview).toBe('ody-code/reviewer');
  });

  it('round-trips services.web_search with provider-specific options', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'web-search.toml');
    const toml = `
[services.web_search.primary]
provider = "tavily"
api_key = "sk-tavily"
timeout_ms = 15000
[services.web_search.primary.options]
search_depth = "advanced"
[services.web_search.secondary]
provider = "duckduckgo"
`;
    const config = parseConfigString(toml, configPath);
    expect(config.services?.webSearch?.primary.provider).toBe('tavily');
    expect(config.services?.webSearch?.primary.options).toEqual({ searchDepth: 'advanced' });

    await writeConfigFile(configPath, config);
    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('[services.web_search.primary]');
    expect(text).toContain('provider = "tavily"');
    expect(text).toContain('search_depth = "advanced"');

    const roundTripped = parseConfigString(text, configPath);
    expect(roundTripped.services?.webSearch?.primary.provider).toBe('tavily');
    expect(roundTripped.services?.webSearch?.primary.options).toEqual({ searchDepth: 'advanced' });
  });

  it('round-trips a custom registry source field on a provider', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'round-trip.toml');
    const toml = `
[providers.custom]
type = "openai"
base_url = "https://custom.example/v1"
api_key = "sk-test"
source = { kind = "apiJson", url = "https://registry.example/api.json", apiKey = "sk-registry" }
`;
    const config = parseConfigString(toml, configPath);
    expect(config.providers['custom']).toMatchObject({
      type: 'openai',
      baseUrl: 'https://custom.example/v1',
      apiKey: 'sk-test',
      source: { kind: 'apiJson', url: 'https://registry.example/api.json', apiKey: 'sk-registry' },
    });

    await writeConfigFile(configPath, config);
    const text = await readFile(configPath, 'utf-8');
    const roundTripped = parseConfigString(text, configPath);
    expect(roundTripped.providers['custom']?.source).toEqual({
      kind: 'apiJson',
      url: 'https://registry.example/api.json',
      apiKey: 'sk-registry',
    });
  });

  it('loads defaults for absent files and writes typed fields without dropping raw sections', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'config.toml');

    expect(readConfigFile(configPath)).toEqual({ providers: {} });

    const config = parseConfigString(COMPLETE_TOML, configPath);
    const loopControl = config.loopControl;
    expect(loopControl).toBeDefined();
    await writeConfigFile(configPath, {
      ...config,
      defaultModel: 'kimi-code/kimi-for-coding',
      loopControl: {
        ...loopControl!,
        maxStepsPerTurn: 7,
      },
    });

    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('default_model = "kimi-code/kimi-for-coding"');
    expect(text).toContain('default_permission_mode = "auto"');
    expect(text).toContain('extra_skill_dirs = [ "~/team-skills", ".agents/team-skills" ]');
    expect(text).toContain('telemetry = false');
    expect(text).not.toContain('default_yolo');
    expect(text).toContain('[[permission.rules]]');
    expect(text).toContain('pattern = "Bash(rm *)"');
    expect(text).toContain('pattern = "Read(src/**)"');
    expect(text).not.toContain('[[permission.allow]]');
    expect(text).toContain('max_steps_per_turn = 7');
    expect(text).toContain('GOOGLE_CLOUD_PROJECT = "project-1"');
    expect(text).toContain('theme = "dark"');
    expect(text).toContain('claim_stale_after_ms = 15000');
    expect(text).toContain('[[hooks]]');
    expect(text).toContain('event = "PreToolUse"');
    expect(text).toContain('command = "echo pre"');
    expect(text).toContain('enabled = false');
    expect(text).toContain('chrome_port = 9223');

    const reloaded = readConfigFile(configPath);
    expect(reloaded.loopControl?.maxStepsPerTurn).toBe(7);
    expect(reloaded.hooks?.[0]?.event).toBe('PreToolUse');
    expect(reloaded.raw?.['theme']).toBe('dark');
    expect(reloaded.browser).toEqual({ enabled: false, chromePort: 9223 });
  });

  it('creates a parseable default config scaffold without changing runtime defaults', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'config.toml');

    await ensureConfigFile(configPath);

    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('Runtime settings for Ody Code.');
    expect(text).not.toMatch(/^default_thinking =/m);
    expect(text).not.toMatch(/^default_model =/m);

    const config = readConfigFile(configPath);
    expect(config.providers).toEqual({});
    expect(config.defaultModel).toBeUndefined();
    expect(config.defaultThinking).toBeUndefined();
  });

  it('does not overwrite an existing config file', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'config.toml');
    const existing = 'default_model = "custom"\n';
    await writeFile(configPath, existing, 'utf-8');

    await ensureConfigFile(configPath);

    await expect(readFile(configPath, 'utf-8')).resolves.toBe(existing);
  });

  it('drops deprecated default_yolo when rewriting config files', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'config.toml');
    const config = parseConfigString('default_yolo = true\n', configPath);

    expect(config.defaultPermissionMode).toBeUndefined();

    await writeConfigFile(configPath, config);

    const text = await readFile(configPath, 'utf-8');
    expect(text).not.toContain('default_yolo');
    expect(text).not.toContain('default_permission_mode');
  });

  it('rejects invalid TOML and invalid schema with OdyError(config.invalid)', () => {
    expectOdyErrorCode(
      () => parseConfigString('[[[', 'broken.toml'),
      ErrorCodes.CONFIG_INVALID,
    );
    expectOdyErrorCode(
      () =>
        parseConfigString(
          `
[providers.bad]
type = "not-a-provider"
`,
          'broken.toml',
        ),
      ErrorCodes.CONFIG_INVALID,
    );
    expectOdyErrorCode(
      () =>
        parseConfigString(
          `
[[permission.rules]]
decision = "deny"
pattern = "Bash(rm *"
`,
          'broken.toml',
        ),
      ErrorCodes.CONFIG_INVALID,
    );
  });

  it('parses hooks config from TOML arrays of tables', () => {
    const config = parseConfigString(
      `
[[hooks]]
event = "PreToolUse"
matcher = "Shell"
command = "echo hi"
timeout = 5
`,
      'hooks.toml',
    );

    expect(config.hooks).toEqual([
      {
        event: 'PreToolUse',
        matcher: 'Shell',
        command: 'echo hi',
        timeout: 5,
      },
    ]);
  });

  it('rejects invalid hooks config', () => {
    expectOdyErrorCode(
      () =>
        parseConfigString(
          `
hooks = [{ type = "pre-tool-call", command = "echo hi" }]
`,
          'hooks.toml',
        ),
      ErrorCodes.CONFIG_INVALID,
    );
  });
});

describe('harness config schema and patch merge', () => {
  it('accepts the empty public config and requires model context size in full configs', () => {
    expect(OdyConfigSchema.parse({})).toEqual({ providers: {} });
    expect(() =>
      validateConfig({
        providers: {
          local: { type: 'openai', apiKey: 'sk-test' },
        },
        models: {
          broken: { provider: 'local', model: 'gpt-test' },
        },
      }),
    ).toThrow(/max_context_size/);
  });

  it('parses modeModels from TOML and round-trips through OdyConfigSchema', () => {
    const config = parseConfigString(`
[mode_models]
plan   = "opus-alias"
design = "sonnet-alias"
`);
    expect(config.modeModels).toEqual({ plan: 'opus-alias', design: 'sonnet-alias' });
  });

  it('accepts a partial modeModels (only design set)', () => {
    const config = OdyConfigSchema.parse({ modeModels: { design: 'sonnet-alias' } });
    expect(config.modeModels?.design).toBe('sonnet-alias');
    expect(config.modeModels?.plan).toBeUndefined();
  });

  it('deep-merges validated patches while preserving existing typed and raw data', () => {
    const base = parseConfigString(COMPLETE_TOML);
    const merged = mergeConfigPatch(base, {
      providers: {
        'managed:ody-code': {
          apiKey: 'sk-patched',
          baseUrl: undefined,
        },
      },
      models: {
        'kimi-code/kimi-for-coding': {
          capabilities: ['tool_use'],
        },
      },
      thinking: {
        effort: 'high',
      },
    });

    expect(merged.providers['managed:ody-code']).toMatchObject({
      type: 'kimi',
      baseUrl: 'https://api.kimi.com/coding/v1',
      apiKey: 'sk-patched',
      env: { GOOGLE_CLOUD_PROJECT: 'project-1' },
    });
    expect(merged.models?.['kimi-code/kimi-for-coding']).toMatchObject({
      provider: 'managed:ody-code',
      model: 'kimi-for-coding',
      maxContextSize: 262144,
      capabilities: ['tool_use'],
    });
    expect(merged.thinking).toEqual({ mode: 'auto', effort: 'high' });
    expect(merged.hooks).toEqual(base.hooks);
    expect(merged.raw?.['theme']).toBe('dark');
  });

  it('rejects unknown fields in config patches', () => {
    expectOdyErrorCode(
      () => mergeConfigPatch({ providers: {} }, { theme: 'dark' } as never),
      ErrorCodes.CONFIG_INVALID,
    );
  });

  it('replaces hooks arrays in config patches', () => {
    const base = parseConfigString(COMPLETE_TOML);
    const merged = mergeConfigPatch(base, {
      hooks: [{ event: 'Notification', matcher: 'task_completed', command: 'echo notified' }],
    });

    expect(merged.hooks).toEqual([
      { event: 'Notification', matcher: 'task_completed', command: 'echo notified' },
    ]);
  });

  it('accepts maxOutputSize on a model alias and round-trips it', () => {
    const parsed = OdyConfigSchema.parse({
      providers: { local: { type: 'anthropic', apiKey: 'sk-test' } },
      models: {
        opus: {
          provider: 'local',
          model: 'claude-opus-4-7',
          maxContextSize: 200000,
          maxOutputSize: 32000,
        },
      },
    });
    expect(parsed.models?.['opus']).toMatchObject({
      maxContextSize: 200000,
      maxOutputSize: 32000,
    });
  });

  it('leaves maxOutputSize undefined when omitted', () => {
    const parsed = OdyConfigSchema.parse({
      providers: { local: { type: 'anthropic', apiKey: 'sk-test' } },
      models: {
        opus: {
          provider: 'local',
          model: 'claude-opus-4-7',
          maxContextSize: 200000,
        },
      },
    });
    expect(parsed.models?.['opus']?.maxOutputSize).toBeUndefined();
  });

  it('rejects maxOutputSize <= 0', () => {
    expect(() =>
      OdyConfigSchema.parse({
        providers: { local: { type: 'anthropic', apiKey: 'sk-test' } },
        models: {
          opus: {
            provider: 'local',
            model: 'claude-opus-4-7',
            maxContextSize: 200000,
            maxOutputSize: 0,
          },
        },
      }),
    ).toThrow();
  });
});

describe('config path env override', () => {
  it('uses ODY_CODE_HOME when no explicit homeDir is supplied', () => {
    const saved = process.env['ODY_CODE_HOME'];
    try {
      process.env['ODY_CODE_HOME'] = '/tmp/kimi-from-env';

      expect(resolveOdyHome()).toBe('/tmp/kimi-from-env');
      expect(resolveOdyHome('/tmp/kimi-explicit')).toBe('/tmp/kimi-explicit');
      expect(resolveConfigPath({})).toBe('/tmp/kimi-from-env/config.toml');
      expect(resolveConfigPath({ configPath: '/tmp/custom.toml' })).toBe('/tmp/custom.toml');
    } finally {
      if (saved === undefined) delete process.env['ODY_CODE_HOME'];
      else process.env['ODY_CODE_HOME'] = saved;
    }
  });
});

describe('config value env override helpers', () => {
  it('parses boolean env values', () => {
    expect(parseBooleanEnv('1')).toBe(true);
    expect(parseBooleanEnv(' true ')).toBe(true);
    expect(parseBooleanEnv('yes')).toBe(true);
    expect(parseBooleanEnv('on')).toBe(true);
    expect(parseBooleanEnv('0')).toBe(false);
    expect(parseBooleanEnv(' false ')).toBe(false);
    expect(parseBooleanEnv('no')).toBe(false);
    expect(parseBooleanEnv('off')).toBe(false);
    expect(parseBooleanEnv('')).toBeUndefined();
    expect(parseBooleanEnv('maybe')).toBeUndefined();
  });

  it('resolves env before config before default', () => {
    expect(
      resolveConfigValue({
        env: { KIMI_TEST_FLAG: '0' },
        envKey: 'KIMI_TEST_FLAG',
        configValue: true,
        defaultValue: true,
        parseEnv: parseBooleanEnv,
      }),
    ).toBe(false);

    expect(
      resolveConfigValue({
        env: {},
        envKey: 'KIMI_TEST_FLAG',
        configValue: false,
        defaultValue: true,
        parseEnv: parseBooleanEnv,
      }),
    ).toBe(false);

    expect(
      resolveConfigValue({
        env: {},
        envKey: 'KIMI_TEST_FLAG',
        defaultValue: true,
        parseEnv: parseBooleanEnv,
      }),
    ).toBe(true);
  });

  it('ignores invalid env values', () => {
    expect(
      resolveConfigValue({
        env: { KIMI_TEST_FLAG: 'invalid' },
        envKey: 'KIMI_TEST_FLAG',
        configValue: false,
        defaultValue: true,
        parseEnv: parseBooleanEnv,
      }),
    ).toBe(false);
  });
});

describe('modeModels code-review fields', () => {
  const TOML_WITH_CODE_REVIEW = `
default_model = "base"

[mode_models]
plan = "plan-model"
review = "generic-reviewer"
code_review = "deepseek-coder"
code_review_request = "claude-3-5-sonnet"
code_review_receive = "claude-3-5-sonnet"
`;

  it('parseConfigString parses new code_review fields as camelCase', () => {
    const config = parseConfigString(TOML_WITH_CODE_REVIEW, 'test.toml');
    expect(config.modeModels).toBeDefined();
    expect(config.modeModels!.plan).toBe('plan-model');
    expect(config.modeModels!.review).toBe('generic-reviewer');
    expect(config.modeModels!.codeReview).toBe('deepseek-coder');
    expect(config.modeModels!.codeReviewRequest).toBe('claude-3-5-sonnet');
    expect(config.modeModels!.codeReviewReceive).toBe('claude-3-5-sonnet');
  });

  it('configToTomlData round-trips code_review fields as snake_case', () => {
    const config = parseConfigString(TOML_WITH_CODE_REVIEW, 'test.toml');
    const data = configToTomlData(config);
    const modeModels = data['mode_models'] as Record<string, unknown>;
    expect(modeModels).toBeDefined();
    expect(modeModels['plan']).toBe('plan-model');
    expect(modeModels['review']).toBe('generic-reviewer');
    expect(modeModels['code_review']).toBe('deepseek-coder');
    expect(modeModels['code_review_request']).toBe('claude-3-5-sonnet');
    expect(modeModels['code_review_receive']).toBe('claude-3-5-sonnet');
  });

  it('writeConfigFile then readConfigFile round-trips all code_review fields', async () => {
    const dir = makeTempDir();
    const configPath = join(dir, 'config.toml');
    const config = parseConfigString(TOML_WITH_CODE_REVIEW, configPath);
    await writeConfigFile(configPath, config);
    const text = await readFile(configPath, 'utf-8');
    expect(text).toContain('code_review = "deepseek-coder"');
    expect(text).toContain('code_review_request = "claude-3-5-sonnet"');
    expect(text).toContain('code_review_receive = "claude-3-5-sonnet"');
    const roundTripped = parseConfigString(text, configPath);
    expect(roundTripped.modeModels!.codeReviewRequest).toBe('claude-3-5-sonnet');
    expect(roundTripped.modeModels!.codeReviewReceive).toBe('claude-3-5-sonnet');
  });
});

describe('OdyConfig sessionMode schema', () => {
  it('accepts all runtime modes for sessionMode and defaultSessionMode', () => {
    for (const mode of ['plan', 'design', 'product', 'game-design', 'normal'] as const) {
      expect(validateConfig({ sessionMode: mode }).sessionMode).toBe(mode);
      expect(validateConfig({ defaultSessionMode: mode }).defaultSessionMode).toBe(mode);
    }
  });

  it('rejects unknown modes', () => {
    expect(() => validateConfig({ sessionMode: 'foo' as any })).toThrow();
    expect(() => validateConfig({ defaultSessionMode: 'foo' as any })).toThrow();
  });
});

describe('configToTomlData round-trips runtime modes', () => {
  it('preserves normal and product in TOML output', () => {
    const config = validateConfig({
      sessionMode: 'product',
      defaultSessionMode: 'normal',
    });
    const tomlData = configToTomlData(config);
    expect(tomlData['session_mode']).toBe('product');
    expect(tomlData['default_session_mode']).toBe('normal');
  });
});

describe('microagentBudget config', () => {
  it('C1: OdyConfigSchema accepts microagentBudget.maxTokens', () => {
    const config = OdyConfigSchema.parse({
      microagentBudget: { maxTokens: 512 },
    });
    expect(config.microagentBudget?.maxTokens).toBe(512);
  });

  it('C2: OdyConfigSchema rejects negative maxTokens', () => {
    expect(() =>
      OdyConfigSchema.parse({
        microagentBudget: { maxTokens: -1 },
      }),
    ).toThrow();
  });

  it('C3: OdyConfigPatchSchema accepts microagentBudget', () => {
    const patch = OdyConfigPatchSchema.parse({
      microagentBudget: { maxTokens: 0 },
    });
    expect(patch.microagentBudget?.maxTokens).toBe(0);
  });

  it('round-trips microagent_budget through TOML parse/write', () => {
    const toml = '[microagent_budget]\nmax_tokens = 512\n';
    const config = parseConfigString(toml, 'test.toml');
    expect(config.microagentBudget?.maxTokens).toBe(512);

    const data = configToTomlData(config);
    const section = data['microagent_budget'] as Record<string, unknown>;
    expect(section).toBeDefined();
    expect(section['max_tokens']).toBe(512);
  });

  it('omits microagent_budget section when not configured', () => {
    const config = OdyConfigSchema.parse({});
    const data = configToTomlData(config);
    expect(data).not.toHaveProperty('microagent_budget');
  });
});
