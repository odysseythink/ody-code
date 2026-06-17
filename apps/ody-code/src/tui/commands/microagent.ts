import { existsSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { isExperimentalFlagEnabled } from './experimental-flags';
import type { SlashCommandHost } from './dispatch';
import { installStarterPackIfEmpty, renderMicroagentFile, validateMicroagentInput } from './microagent-helpers';
import { TextInputDialogComponent } from '../components/dialogs/text-input-dialog';
import { QuestionDialogComponent } from '../components/dialogs/question-dialog';
import type { TextInputResult } from '../components/dialogs/text-input-dialog';
import type { PendingQuestion, QuestionPanelResponse } from '#/tui/reverse-rpc/types';

export async function handleMicroagentCommand(host: SlashCommandHost, _args: string): Promise<void> {
  if (!isExperimentalFlagEnabled('repo-knowledge')) {
    host.showError('Microagent authoring requires the repo-knowledge experimental flag.');
    return;
  }

  const workDir = host.state.appState.workDir;
  if (!workDir || workDir.length === 0) {
    host.showError('No active workspace. Open a project directory first.');
    return;
  }

  const microagentsDir = join(workDir, '.ody-code', 'microagents');

  // Ensure directory exists and install starters if empty
  try {
    const installed = await installStarterPackIfEmpty(microagentsDir);
    for (const file of installed) {
      host.track('starter_microagent_installed', { file_name: file.fileName });
    }
    if (installed.length > 0) {
      host.showNotice(
        'Starter microagents installed',
        installed.map((f) => f.fileName).join(', '),
      );
    }
  } catch (error) {
    // Starter installation failure is non-fatal; continue with wizard
    host.showStatus(`Starter installation skipped: ${String(error)}`);
  }

  // Step 1: collect name
  const name = await promptForName(host);
  if (name === undefined) return;

  // Step 2: collect triggers
  const triggers = await promptForTriggers(host);
  if (triggers === undefined) return;

  // Step 3: collect description
  const description = await promptForDescription(host);
  if (description === undefined) return;

  // Validate
  const validation = validateMicroagentInput(name, triggers, description);
  if (!validation.ok) {
    host.showError(`Invalid ${validation.error!.field}: ${validation.error!.message}`);
    return;
  }

  const input = validation.input!;
  const targetPath = join(microagentsDir, `${input.name}.md`);

  // Overwrite check
  if (existsSync(targetPath)) {
    const confirmed = await confirmOverwrite(host, input.name);
    if (!confirmed) {
      host.showStatus('Microagent creation cancelled.');
      return;
    }
  }

  // Write
  const content = renderMicroagentFile(input);
  try {
    await mkdir(microagentsDir, { recursive: true });
    await writeFile(targetPath, content, 'utf-8');
  } catch (error) {
    host.track('microagent_create_failed', {
      reason: 'write_error',
      error: String(error),
    });
    host.showError(`Failed to write microagent: ${String(error)}`);
    return;
  }

  host.track('microagent_created', {
    name: input.name,
    trigger_count: input.triggers.length,
  });
  host.showNotice('Microagent created', targetPath);
}

// —— Dialog helpers ——

function promptForName(host: SlashCommandHost): Promise<string | undefined> {
  return new Promise((resolve) => {
    const dialog = new TextInputDialogComponent({
      title: 'Microagent name',
      subtitleLines: ['Enter a short name for the microagent file.'],
      footer: 'Only lowercase letters, digits, hyphens, and underscores.',
      validate: (value: string) => {
        const trimmed = value.trim();
        if (trimmed.length === 0) return 'Name is required.';
        if (!/^[a-z0-9_-]+$/.test(trimmed)) return 'Only a-z, 0-9, hyphens and underscores allowed.';
        return undefined;
      },
      onDone: (result: TextInputResult) => {
        host.restoreEditor();
        if (result.kind === 'ok') {
          resolve(result.value.trim());
        } else {
          resolve(undefined);
        }
      },
      colors: host.state.theme.colors,
    });
    host.mountEditorReplacement(dialog);
  });
}

function promptForTriggers(host: SlashCommandHost): Promise<string | undefined> {
  return new Promise((resolve) => {
    const dialog = new TextInputDialogComponent({
      title: 'Trigger keywords',
      subtitleLines: [
        'Enter comma-separated trigger keywords.',
        'The microagent is injected when these appear in user messages.',
      ],
      footer: 'Example: component, page, 组件',
      validate: (value: string) => {
        const trimmed = value.trim();
        if (trimmed.length === 0) return 'At least one trigger keyword is required.';
        return undefined;
      },
      onDone: (result: TextInputResult) => {
        host.restoreEditor();
        if (result.kind === 'ok') {
          resolve(result.value.trim());
        } else {
          resolve(undefined);
        }
      },
      colors: host.state.theme.colors,
    });
    host.mountEditorReplacement(dialog);
  });
}

function promptForDescription(host: SlashCommandHost): Promise<string | undefined> {
  return new Promise((resolve) => {
    const dialog = new TextInputDialogComponent({
      title: 'Description',
      subtitleLines: ['Enter a one-line description for this microagent.'],
      footer: 'Max 200 characters.',
      validate: (value: string) => {
        const trimmed = value.trim();
        if (trimmed.length === 0) return 'Description is required.';
        if (trimmed.length > 200) return `Too long (${trimmed.length}/200 characters).`;
        return undefined;
      },
      onDone: (result: TextInputResult) => {
        host.restoreEditor();
        if (result.kind === 'ok') {
          resolve(result.value.trim());
        } else {
          resolve(undefined);
        }
      },
      colors: host.state.theme.colors,
    });
    host.mountEditorReplacement(dialog);
  });
}

function confirmOverwrite(host: SlashCommandHost, name: string): Promise<boolean> {
  return new Promise((resolve) => {
    const request: PendingQuestion = {
      data: {
        id: `microagent-overwrite-${Date.now()}`,
        tool_call_id: '',
        questions: [
          {
            question: `A microagent named "${name}" already exists. Overwrite it?`,
            header: 'Overwrite',
            multi_select: false,
            options: [
              { label: 'Yes, overwrite', description: 'Replace the existing file.' },
              { label: 'No, cancel', description: 'Keep the existing file.' },
            ],
          },
        ],
      },
    };
    const dialog = new QuestionDialogComponent(
      request,
      (response: QuestionPanelResponse) => {
        host.restoreEditor();
        const answers = response.answers ?? [];
        resolve(answers.includes('Yes, overwrite'));
      },
      host.state.theme.colors,
    );
    host.mountEditorReplacement(dialog);
  });
}
