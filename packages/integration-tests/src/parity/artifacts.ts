import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import type { NormalizedSnapshot, ParityDiff, ScenarioSnapshot } from './types';

function getReportDir(): string | undefined {
  const envDir = process.env['ODY_CODE_REPORT_DIR'];
  if (envDir !== undefined && envDir.length > 0) {
    return resolve(envDir);
  }
  return undefined;
}

export interface ParityArtifacts {
  readonly reportDir: string;
  readonly scenarioDir: string;
  readonly files: readonly string[];
}

export async function writeParityArtifacts(
  scenarioName: string,
  first: { readonly snapshot: ScenarioSnapshot; readonly normalized: NormalizedSnapshot },
  second: { readonly snapshot: ScenarioSnapshot; readonly normalized: NormalizedSnapshot },
  diff: ParityDiff,
): Promise<ParityArtifacts | undefined> {
  const reportDir = getReportDir();
  if (reportDir === undefined) {
    return undefined;
  }

  const scenarioDir = join(reportDir, 'parity', scenarioName);
  await mkdir(scenarioDir, { recursive: true });

  const files: string[] = [];
  const write = async (name: string, data: unknown) => {
    const path = join(scenarioDir, name);
    await writeFile(path, JSON.stringify(data, null, 2) + '\n', 'utf8');
    files.push(path);
  };

  await write('ts.snapshot.json', first.snapshot);
  await write('rust.snapshot.json', second.snapshot);
  await write('ts.normalized.json', first.normalized);
  await write('rust.normalized.json', second.normalized);
  await write('diff.json', diff);

  return { reportDir, scenarioDir, files };
}

export async function writeParityErrorArtifacts(
  scenarioName: string,
  backendLabel: string,
  error: unknown,
): Promise<ParityArtifacts | undefined> {
  const reportDir = getReportDir();
  if (reportDir === undefined) {
    return undefined;
  }

  const scenarioDir = join(reportDir, 'parity', scenarioName);
  await mkdir(scenarioDir, { recursive: true });

  const files: string[] = [];
  const write = async (name: string, data: unknown) => {
    const path = join(scenarioDir, name);
    await writeFile(path, JSON.stringify(data, null, 2) + '\n', 'utf8');
    files.push(path);
  };

  const errorPayload = {
    backend: backendLabel,
    message: error instanceof Error ? error.message : String(error),
    stack: error instanceof Error ? error.stack : undefined,
  };
  await write('error.json', errorPayload);

  return { reportDir, scenarioDir, files };
}
