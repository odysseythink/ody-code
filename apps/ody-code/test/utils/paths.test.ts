import { createHash } from 'node:crypto';
import { homedir } from 'node:os';
import { join } from 'node:path';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  getDataDir,
  getInputHistoryFile,
  getLogDir,
  getUpdateInstallStateFile,
  getUpdateStateFile,
} from '#/utils/paths';

const originalEnv = { ...process.env };

beforeEach(() => {
  delete process.env['ODY_CODE_HOME'];
});

afterEach(() => {
  process.env = { ...originalEnv };
});

describe('getDataDir', () => {
  it('returns ~/.ody-code when ODY_CODE_HOME is not set', () => {
    expect(getDataDir()).toBe(join(homedir(), '.ody-code'));
  });

  it('returns ODY_CODE_HOME when set', () => {
    process.env['ODY_CODE_HOME'] = '/tmp/kimi-test-data';
    expect(getDataDir()).toBe('/tmp/kimi-test-data');
  });

  it('returns ODY_CODE_HOME even if it is a relative path', () => {
    process.env['ODY_CODE_HOME'] = 'relative/path';
    expect(getDataDir()).toBe('relative/path');
  });
});

describe('getLogDir', () => {
  it('returns <dataDir>/logs', () => {
    expect(getLogDir()).toBe(join(homedir(), '.ody-code', 'logs'));
  });

  it('respects ODY_CODE_HOME', () => {
    process.env['ODY_CODE_HOME'] = '/z';
    expect(getLogDir()).toBe(join('/z', 'logs'));
  });
});

describe('getUpdateStateFile', () => {
  it('returns <dataDir>/updates/latest.json', () => {
    expect(getUpdateStateFile()).toBe(join(homedir(), '.ody-code', 'updates', 'latest.json'));
  });

  it('respects ODY_CODE_HOME', () => {
    process.env['ODY_CODE_HOME'] = '/updates-home';
    expect(getUpdateStateFile()).toBe(join('/updates-home', 'updates', 'latest.json'));
  });
});

describe('getUpdateInstallStateFile', () => {
  it('returns <dataDir>/updates/install.json', () => {
    expect(getUpdateInstallStateFile()).toBe(
      join(homedir(), '.ody-code', 'updates', 'install.json'),
    );
  });

  it('respects ODY_CODE_HOME', () => {
    process.env['ODY_CODE_HOME'] = '/updates-home';
    expect(getUpdateInstallStateFile()).toBe(join('/updates-home', 'updates', 'install.json'));
  });
});

describe('getInputHistoryFile', () => {
  it('returns <dataDir>/user-history/<md5(workDir)>.jsonl', () => {
    const workDir = '/home/user/project';
    const hash = createHash('md5').update(workDir, 'utf-8').digest('hex');
    expect(getInputHistoryFile(workDir)).toBe(
      join(homedir(), '.ody-code', 'user-history', `${hash}.jsonl`),
    );
  });

  it('respects ODY_CODE_HOME', () => {
    process.env['ODY_CODE_HOME'] = '/custom/data';
    const hash = createHash('md5').update('/proj', 'utf-8').digest('hex');
    expect(getInputHistoryFile('/proj')).toBe(
      join('/custom/data', 'user-history', `${hash}.jsonl`),
    );
  });
});
