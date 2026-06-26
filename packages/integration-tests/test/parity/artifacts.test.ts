import { describe, expect, it } from 'vitest';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { writeParityArtifacts, writeParityErrorArtifacts } from '../../src/parity/artifacts';
import type { NormalizedSnapshot, ParityDiff, ScenarioSnapshot } from '../../src/parity/types';

describe('parity artifacts', () => {
  async function withReportDir<T>(fn: (reportDir: string) => Promise<T>): Promise<T> {
    const reportDir = await mkdtemp(join(tmpdir(), 'parity-artifacts-'));
    const prev = process.env['ODY_CODE_REPORT_DIR'];
    process.env['ODY_CODE_REPORT_DIR'] = reportDir;
    try {
      return await fn(reportDir);
    } finally {
      if (prev === undefined) {
        delete process.env['ODY_CODE_REPORT_DIR'];
      } else {
        process.env['ODY_CODE_REPORT_DIR'] = prev;
      }
      await rm(reportDir, { recursive: true, force: true });
    }
  }

  it('writes diff artifacts when ODY_CODE_REPORT_DIR is set', async () => {
    await withReportDir(async (reportDir) => {
      const firstSnapshot: ScenarioSnapshot = { responses: ['a'], events: [] };
      const secondSnapshot: ScenarioSnapshot = { responses: ['b'], events: [] };
      const firstNormalized: NormalizedSnapshot = { responses: ['a'], events: [] };
      const secondNormalized: NormalizedSnapshot = { responses: ['b'], events: [] };
      const diff: ParityDiff = {
        scenarioName: 'test-scenario',
        ts: firstNormalized,
        rust: secondNormalized,
        diffs: [{ path: '$.responses[0]', tsValue: 'a', rustValue: 'b' }],
      };

      const result = await writeParityArtifacts(
        'test-scenario',
        { snapshot: firstSnapshot, normalized: firstNormalized },
        { snapshot: secondSnapshot, normalized: secondNormalized },
        diff,
      );

      expect(result).toBeDefined();
      expect(result!.reportDir).toBe(reportDir);
      expect(result!.files).toHaveLength(5);

      const readJson = async (name: string) => JSON.parse(await readFile(join(result!.scenarioDir, name), 'utf8'));
      expect(await readJson('ts.snapshot.json')).toEqual({ responses: ['a'], events: [] });
      expect(await readJson('rust.snapshot.json')).toEqual({ responses: ['b'], events: [] });
      expect(await readJson('ts.normalized.json')).toEqual({ responses: ['a'], events: [] });
      expect(await readJson('rust.normalized.json')).toEqual({ responses: ['b'], events: [] });
      expect(await readJson('diff.json')).toEqual(diff);
    });
  });

  it('returns undefined when ODY_CODE_REPORT_DIR is not set', async () => {
    const prev = process.env['ODY_CODE_REPORT_DIR'];
    delete process.env['ODY_CODE_REPORT_DIR'];
    try {
      const diff: ParityDiff = {
        scenarioName: 'ignored',
        ts: { responses: [], events: [] },
        rust: { responses: [], events: [] },
        diffs: [],
      };
      const result = await writeParityArtifacts(
        'ignored',
        { snapshot: { responses: [], events: [] }, normalized: { responses: [], events: [] } },
        { snapshot: { responses: [], events: [] }, normalized: { responses: [], events: [] } },
        diff,
      );
      expect(result).toBeUndefined();
    } finally {
      if (prev !== undefined) process.env['ODY_CODE_REPORT_DIR'] = prev;
    }
  });

  it('writes error artifacts with message and stack', async () => {
    await withReportDir(async (reportDir) => {
      const error = new Error('backend crashed');
      const result = await writeParityErrorArtifacts('error-scenario', 'makeB', error);

      expect(result).toBeDefined();
      expect(result!.reportDir).toBe(reportDir);
      expect(result!.files).toHaveLength(1);

      const payload = JSON.parse(await readFile(join(result!.scenarioDir, 'error.json'), 'utf8'));
      expect(payload.backend).toBe('makeB');
      expect(payload.message).toBe('backend crashed');
      expect(payload.stack).toContain('Error: backend crashed');
    });
  });
});
