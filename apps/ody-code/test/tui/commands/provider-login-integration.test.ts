import { describe, expect, it } from 'vitest';
import { applyProviderLoginConfig, getProviderLoginDefinition } from '@odysseythink/kimi-code-oauth';
import {
  mergeConfigPatch,
  readConfigFile,
  writeConfigFile,
} from '@odysseythink/agent-core';
import { getDefaultConfig } from '@odysseythink/agent-core';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('provider login end-to-end', () => {
  it('writes deepseek_1 to config.toml and reads it back', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'ody-test-'));
    const configPath = join(dir, 'config.toml');

    // Start with empty config
    const config = getDefaultConfig();

    const definition = getProviderLoginDefinition('deepseek')!;
    const models = [
      { id: 'deepseek-chat', contextLength: 64000, supportsToolUse: true, supportsReasoning: false, supportsImageIn: false, supportsVideoIn: false },
    ];

    applyProviderLoginConfig(config as any, {
      providerName: 'deepseek_1',
      definition,
      baseUrl: 'https://api.deepseek.com/v1',
      apiKey: 'sk-test',
      models,
      selectedModel: models[0]!,
      thinking: false,
    });

    // Simulate setConfig: merge with disk config and write
    const merged = mergeConfigPatch(readConfigFile(configPath), {
      providers: config.providers,
      models: config.models,
      defaultModel: config.defaultModel,
      defaultThinking: config.defaultThinking,
    });
    await writeConfigFile(configPath, merged);

    // Read back
    const readBack = readConfigFile(configPath);

    expect(readBack.providers['deepseek_1']).toMatchObject({
      type: 'deepseek',
      baseUrl: 'https://api.deepseek.com/v1',
      apiKey: 'sk-test',
    });

    // Also verify TOML content
    const tomlText = readFileSync(configPath, 'utf-8');
    expect(tomlText).toContain('[providers.deepseek_1]');
    expect(tomlText).toContain('type = "deepseek"');
  });
});
