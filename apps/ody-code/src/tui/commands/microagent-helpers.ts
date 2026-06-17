export interface MicroagentWizardInput {
  readonly name: string;
  readonly triggers: readonly string[];
  readonly description: string;
}

export type MicroagentValidationError =
  | { readonly field: 'name'; readonly message: string }
  | { readonly field: 'triggers'; readonly message: string }
  | { readonly field: 'description'; readonly message: string };

export interface MicroagentValidationResult {
  readonly ok: boolean;
  readonly input?: MicroagentWizardInput;
  readonly error?: MicroagentValidationError;
}

const VALID_NAME_RE = /^[a-z0-9_-]+$/;

export function normalizeName(raw: string): string | undefined {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return undefined;

  // Reject when input is uppercase-only (no lowercase letters at all)
  const lowered = trimmed.toLowerCase();
  if (lowered !== trimmed && trimmed === trimmed.toUpperCase()) {
    return undefined;
  }

  if (!VALID_NAME_RE.test(lowered)) return undefined;
  return lowered;
}

const TRIGGER_SPLIT_RE = /[,，\s]+/;

export function normalizeTriggers(raw: string): readonly string[] | undefined {
  const tokens = raw.split(TRIGGER_SPLIT_RE);
  const seen = new Set<string>();
  const result: string[] = [];

  for (const token of tokens) {
    const cleaned = token.trim().toLowerCase();
    if (cleaned.length === 0) continue;
    if (seen.has(cleaned)) continue;
    seen.add(cleaned);
    result.push(cleaned);
  }

  if (result.length === 0) return undefined;
  return result.toSorted();
}

export function validateMicroagentInput(
  rawName: string,
  rawTriggers: string,
  rawDescription: string,
): MicroagentValidationResult {
  const name = normalizeName(rawName);
  if (name === undefined) {
    return { ok: false, error: { field: 'name', message: 'Name must be lowercase alphanumeric with - or _ only.' } };
  }

  const triggers = normalizeTriggers(rawTriggers);
  if (triggers === undefined) {
    return { ok: false, error: { field: 'triggers', message: 'At least one non-empty trigger keyword is required.' } };
  }

  const description = rawDescription.trim();
  if (description.length === 0) {
    return { ok: false, error: { field: 'description', message: 'Description is required.' } };
  }
  if (description.length > 200) {
    return { ok: false, error: { field: 'description', message: 'Description must be 200 characters or fewer.' } };
  }

  return { ok: true, input: { name, triggers, description } };
}

export function renderMicroagentFile(input: MicroagentWizardInput): string {
  const triggersYaml = input.triggers.map((t) => `  - ${t}`).join('\n');
  return [
    '---',
    `name: ${input.name}`,
    'type: knowledge',
    'triggers:',
    triggersYaml,
    `description: ${input.description}`,
    '---',
    '',
    `# ${input.name}`,
    '',
    '<!-- TODO: Add repo-specific conventions below. -->',
    '',
  ].join('\n');
}

import { mkdir, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import REUSE_CONVENTIONS_BODY from './microagent-templates/reuse-conventions.md';
import GLOSSARY_BODY from './microagent-templates/glossary.md';
import TESTING_BODY from './microagent-templates/testing.md';
import DOCUMENTATION_BODY from './microagent-templates/documentation.md';

export interface StarterTemplate {
  readonly fileName: string;
  readonly content: string;
}

const STARTER_TEMPLATES: readonly StarterTemplate[] = [
  { fileName: 'reuse-conventions.md', content: REUSE_CONVENTIONS_BODY },
  { fileName: 'glossary.md', content: GLOSSARY_BODY },
  { fileName: 'testing.md', content: TESTING_BODY },
  { fileName: 'documentation.md', content: DOCUMENTATION_BODY },
];

export interface InstalledFile {
  readonly fileName: string;
  readonly path: string;
}

export async function installStarterPackIfEmpty(targetDir: string): Promise<InstalledFile[]> {
  let entries: string[];
  try {
    entries = await readdir(targetDir);
  } catch {
    entries = [];
  }

  const markdownFiles = entries.filter((name) => name.endsWith('.md'));
  if (markdownFiles.length > 0) return [];

  await mkdir(targetDir, { recursive: true });

  const installed: InstalledFile[] = [];
  for (const template of STARTER_TEMPLATES) {
    const dest = join(targetDir, template.fileName);
    await writeFile(dest, template.content, 'utf-8');
    installed.push({ fileName: template.fileName, path: dest });
  }
  return installed;
}
