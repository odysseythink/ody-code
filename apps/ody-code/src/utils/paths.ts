/**
 * CLI-owned data path helpers.
 *
 * These paths are for local app data such as logs and input history. Config
 * files are owned by Core/SDK and intentionally do not live behind this module.
 */

import { createHash } from 'node:crypto';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

import {
  ODY_CODE_DATA_DIR_NAME,
  ODY_CODE_HOME_ENV,
  ODY_CODE_INPUT_HISTORY_DIR_NAME,
  ODY_CODE_LOG_DIR_NAME,
  ODY_CODE_UPDATE_INSTALL_LOCK_FILE_NAME,
  ODY_CODE_UPDATE_INSTALL_STATE_FILE_NAME,
  ODY_CODE_UPDATE_DIR_NAME,
  ODY_CODE_UPDATE_STATE_FILE_NAME,
} from '#/constant/app';

/**
 * Return the root data directory for Ody Code.
 *
 * Priority: `ODY_CODE_HOME` env var > `~/.ody-code`.
 */
export function getDataDir(): string {
  const envDir = process.env[ODY_CODE_HOME_ENV];
  if (envDir) {
    return envDir;
  }
  return join(homedir(), ODY_CODE_DATA_DIR_NAME);
}

/**
 * Return the diagnostic log directory: `<dataDir>/logs/`.
 */
export function getLogDir(): string {
  return join(getDataDir(), ODY_CODE_LOG_DIR_NAME);
}

/**
 * Return the update cache file: `<dataDir>/updates/latest.json`.
 */
export function getUpdateStateFile(): string {
  return join(getDataDir(), ODY_CODE_UPDATE_DIR_NAME, ODY_CODE_UPDATE_STATE_FILE_NAME);
}

/**
 * Return the update install state file: `<dataDir>/updates/install.json`.
 */
export function getUpdateInstallStateFile(): string {
  return join(getDataDir(), ODY_CODE_UPDATE_DIR_NAME, ODY_CODE_UPDATE_INSTALL_STATE_FILE_NAME);
}

/**
 * Return the update install lock file: `<dataDir>/updates/install.lock`.
 */
export function getUpdateInstallLockFile(): string {
  return join(getDataDir(), ODY_CODE_UPDATE_DIR_NAME, ODY_CODE_UPDATE_INSTALL_LOCK_FILE_NAME);
}

/**
 * Return the user input history file for a given working directory.
 * Layout: `<share_dir>/user-history/<md5(cwd)>.jsonl`.
 */
export function getInputHistoryFile(workDir: string): string {
  const hash = createHash('md5').update(workDir, 'utf-8').digest('hex');
  return join(getDataDir(), ODY_CODE_INPUT_HISTORY_DIR_NAME, `${hash}.jsonl`);
}

/**
 * Compare two paths for equality, accounting for platform differences.
 * On Windows, comparison is case-insensitive and normalizes slashes.
 */
export function arePathsEqual(a: string, b: string): boolean {
  const resolvedA = resolve(a);
  const resolvedB = resolve(b);
  if (process.platform === 'win32') {
    return resolvedA.toLowerCase() === resolvedB.toLowerCase();
  }
  return resolvedA === resolvedB;
}
