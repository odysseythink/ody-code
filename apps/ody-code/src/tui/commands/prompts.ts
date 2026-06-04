import {
  catalogModelToAlias,
  inferWireType,
  type Catalog,
  type CatalogModel,
  type ModelAlias,
} from '@odysseythink/kimi-code-sdk';
import { capabilitiesForModel } from '@odysseythink/kimi-code-oauth';
import type {
  ManagedKimiCodeModelInfo,
  OpenPlatformDefinition,
} from '@odysseythink/kimi-code-oauth';

import { ApiKeyInputDialogComponent, type ApiKeyInputResult } from '../components/dialogs/api-key-input-dialog';
import { ChoicePickerComponent, type ChoiceOption } from '../components/dialogs/choice-picker';
import { FeedbackInputDialogComponent, type FeedbackInputDialogResult } from '../components/dialogs/feedback-input-dialog';
import { ModelSelectorComponent } from '../components/dialogs/model-selector';
import { PlatformSelectorComponent } from '../components/dialogs/platform-selector';
import { TextInputDialogComponent, type TextInputResult } from '../components/dialogs/text-input-dialog';
import type { SlashCommandHost } from './dispatch';

export function promptPlatformSelection(host: SlashCommandHost): Promise<string | undefined> {
  return new Promise((resolve) => {
    const selector = new PlatformSelectorComponent({
      colors: host.state.theme.colors,
      onSelect: (platformId) => {
        host.restoreEditor();
        resolve(platformId);
      },
      onCancel: () => {
        host.restoreEditor();
        resolve(undefined);
      },
    });
    host.mountEditorReplacement(selector);
  });
}

export function promptLogoutProviderSelection(
  host: SlashCommandHost,
  options: readonly ChoiceOption[],
  currentValue: string | undefined,
): Promise<string | undefined> {
  return new Promise((resolve) => {
    const picker = new ChoicePickerComponent({
      title: 'Select a provider to log out',
      options,
      currentValue,
      colors: host.state.theme.colors,
      onSelect: (value) => {
        host.restoreEditor();
        resolve(value);
      },
      onCancel: () => {
        host.restoreEditor();
        resolve(undefined);
      },
    });
    host.mountEditorReplacement(picker);
  });
}

export function promptFeedbackInput(host: SlashCommandHost): Promise<string | undefined> {
  return new Promise((resolve) => {
    const dialog = new FeedbackInputDialogComponent((result: FeedbackInputDialogResult) => {
      host.restoreEditor();
      resolve(result.kind === 'ok' ? result.value : undefined);
    }, host.state.theme.colors);
    host.mountEditorReplacement(dialog);
  });
}

export function promptApiKey(
  host: SlashCommandHost,
  platformName: string,
  subtitleLines: readonly string[] = ['Your key will be saved to ~/.ody-code/config.toml'],
): Promise<string | undefined> {
  return new Promise((resolve) => {
    const dialog = new ApiKeyInputDialogComponent(
      platformName,
      subtitleLines,
      (result: ApiKeyInputResult) => {
        host.restoreEditor();
        resolve(result.kind === 'ok' ? result.value : undefined);
      },
      host.state.theme.colors,
    );
    host.mountEditorReplacement(dialog);
  });
}

export function promptCatalogProviderSelection(host: SlashCommandHost, catalog: Catalog): Promise<string | undefined> {
  return new Promise((resolve) => {
    const options: ChoiceOption[] = Object.entries(catalog)
      .filter(([, entry]) => inferWireType(entry) !== undefined)
      .map(([id, entry]) => ({
        value: id,
        label: entry.name ?? id,
        description:
          typeof entry.api === 'string' && entry.api.length > 0 ? entry.api : undefined,
      }))
      .toSorted((a, b) => a.label.localeCompare(b.label));

    if (options.length === 0) {
      host.showError('Catalog has no providers with supported wire types.');
      resolve(undefined);
      return;
    }

    const picker = new ChoicePickerComponent({
      title: 'Select a provider',
      options,
      colors: host.state.theme.colors,
      searchable: true,
      onSelect: (value) => {
        host.restoreEditor();
        resolve(value);
      },
      onCancel: () => {
        host.restoreEditor();
        resolve(undefined);
      },
    });
    host.mountEditorReplacement(picker);
  });
}

export async function promptModelSelectionForOpenPlatform(
  host: SlashCommandHost,
  models: ManagedKimiCodeModelInfo[],
  platform: OpenPlatformDefinition,
): Promise<{ model: ManagedKimiCodeModelInfo; thinking: boolean } | undefined> {
  const modelDict: Record<string, ModelAlias> = {};
  for (const m of models) {
    modelDict[`${platform.id}/${m.id}`] = {
      provider: platform.id,
      model: m.id,
      maxContextSize: m.contextLength,
      capabilities: capabilitiesForModel(m),
      displayName: m.displayName,
    };
  }
  const selection = await runModelSelector(host, modelDict);
  if (selection === undefined) return undefined;
  const model = models.find((m) => `${platform.id}/${m.id}` === selection.alias);
  return model ? { model, thinking: selection.thinking } : undefined;
}

export async function promptModelSelectionForCatalog(
  host: SlashCommandHost,
  providerId: string,
  models: CatalogModel[],
): Promise<{ model: CatalogModel; thinking: boolean } | undefined> {
  const modelDict: Record<string, ModelAlias> = {};
  for (const m of models) {
    modelDict[`${providerId}/${m.id}`] = catalogModelToAlias(providerId, m);
  }
  const selection = await runModelSelector(host, modelDict);
  if (selection === undefined) return undefined;
  const model = models.find((m) => `${providerId}/${m.id}` === selection.alias);
  return model ? { model, thinking: selection.thinking } : undefined;
}

export function promptCustomProviderName(
  host: SlashCommandHost,
  existingProviders: Record<string, unknown>,
): Promise<string | undefined> {
  return new Promise((resolve) => {
    const dialog = new TextInputDialogComponent({
      title: 'Provider name',
      subtitleLines: ['Letters, digits, and underscores only. Must start with a letter.'],
      footer: 'Enter to confirm  ·  Esc to cancel',
      validate: (value) => {
        const re = /^[a-zA-Z][a-zA-Z0-9_]*$/;
        if (!re.test(value)) return 'Must start with a letter and contain only letters, digits, and underscores.';
        if (value === 'managed:ody-code') return 'This name is reserved.';
        if (existingProviders[value] !== undefined) return `Provider "${value}" already exists.`;
        return undefined;
      },
      onDone: (result: TextInputResult) => {
        host.restoreEditor();
        resolve(result.kind === 'ok' ? result.value : undefined);
      },
      colors: host.state.theme.colors,
    });
    host.mountEditorReplacement(dialog);
  });
}

export function promptCustomBaseUrl(
  host: SlashCommandHost,
  defaultBaseUrl: string,
): Promise<string | undefined> {
  return new Promise((resolve) => {
    const dialog = new TextInputDialogComponent({
      title: 'Base URL (optional)',
      subtitleLines: [`Default: ${defaultBaseUrl}`],
      footer: 'Enter to use default  ·  Esc to cancel',
      defaultValue: defaultBaseUrl,
      validate: (value) => {
        if (value.length === 0) return 'Base URL cannot be empty.';
        try {
          new URL(value);
          return undefined;
        } catch {
          return 'Invalid URL.';
        }
      },
      onDone: (result: TextInputResult) => {
        host.restoreEditor();
        resolve(result.kind === 'ok' ? result.value : undefined);
      },
      colors: host.state.theme.colors,
    });
    host.mountEditorReplacement(dialog);
  });
}

export async function promptModelSelectionForProviderLogin(
  host: SlashCommandHost,
  providerName: string,
  models: import('@odysseythink/kimi-code-oauth').ProviderModelInfo[],
): Promise<{ model: import('@odysseythink/kimi-code-oauth').ProviderModelInfo; thinking: boolean } | undefined> {
  const modelDict: Record<string, import('@odysseythink/kimi-code-sdk').ModelAlias> = {};
  for (const m of models) {
    modelDict[`${providerName}/${m.id}`] = {
      provider: providerName,
      model: m.id,
      maxContextSize: m.contextLength,
      capabilities: [
        ...(m.supportsToolUse ? ['tool_use'] : []),
        ...(m.supportsReasoning ? ['thinking'] : []),
        ...(m.supportsImageIn ? ['image_in'] : []),
        ...(m.supportsVideoIn ? ['video_in'] : []),
      ],
      displayName: m.displayName,
    };
  }
  const selection = await runModelSelector(host, modelDict);
  if (selection === undefined) return undefined;
  const model = models.find((m) => `${providerName}/${m.id}` === selection.alias);
  return model ? { model, thinking: selection.thinking } : undefined;
}

export function runModelSelector(
  host: SlashCommandHost,
  modelDict: Record<string, ModelAlias>,
): Promise<{ alias: string; thinking: boolean } | undefined> {
  return new Promise((resolve) => {
    const firstAlias = Object.keys(modelDict)[0] ?? '';
    const caps = modelDict[firstAlias]?.capabilities ?? [];
    const initialThinking = caps.includes('always_thinking') || caps.includes('thinking');
    const selector = new ModelSelectorComponent({
      models: modelDict,
      currentValue: firstAlias,
      currentThinking: initialThinking,
      colors: host.state.theme.colors,
      searchable: true,
      onSelect: ({ alias, thinking }) => {
        host.restoreEditor();
        resolve({ alias, thinking });
      },
      onCancel: () => {
        host.restoreEditor();
        resolve(undefined);
      },
    });
    host.mountEditorReplacement(selector);
  });
}
