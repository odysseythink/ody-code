import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'pathe';

import { log } from '#logging/logger';

import {
  collectMetadata,
  createEmptyScanState,
  renderSummary,
  scanWire,
  writeSummary,
  type WireScanState,
} from '../../memory/store';
import { encodeWorkDirKey } from '../../store/workdir-key';
import type { BuiltinHook, HookResult } from '../types';

export class SessionMemoryWriterBuiltin implements BuiltinHook {
  readonly id = 'session-memory-writer';
  private readonly states = new Map<string, WireScanState>();

  async run(
    input: Record<string, unknown>,
    ctx: {
      readonly cwd: string | undefined;
      readonly env: Readonly<Record<string, string | undefined>>;
      readonly signal?: AbortSignal;
      readonly timeout: number;
    },
  ): Promise<HookResult> {
    try {
      const sessionId = input['session_id'];
      if (typeof sessionId !== 'string' || sessionId.length === 0) {
        return { action: 'allow', stdout: 'missing session_id' };
      }

      const cwd = ctx.cwd ?? '.';
      const homeDir = ctx.env['ODY_CODE_HOME'] ?? join(homedir(), '.ody-code');
      const wdKey = encodeWorkDirKey(cwd);
      const sessionDir = join(homeDir, 'sessions', wdKey, sessionId);
      const wirePath = join(sessionDir, 'agents', 'main', 'wire.jsonl');

      let state = this.states.get(sessionId);
      if (state === undefined) {
        state = createEmptyScanState();
        this.states.set(sessionId, state);
      }

      await scanWire(wirePath, state);
      if (state.totalUserMessages === 0) {
        return { action: 'allow', stdout: 'no user messages to summarize' };
      }

      const startedAt = await readStartedAt(wirePath);
      const metadata = await collectMetadata(cwd, sessionId, startedAt);
      const content = renderSummary(state, metadata);
      await writeSummary(sessionDir, content);

      return { action: 'allow', stdout: 'summary updated' };
    } catch (error) {
      log.warn('session-memory-writer failed', { error });
      return { action: 'allow' };
    }
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

async function readStartedAt(wirePath: string): Promise<number> {
  try {
    const raw = await readFile(wirePath, 'utf8');
    const firstLine = raw.split('\n')[0];
    if (firstLine === undefined || firstLine.length === 0) {
      return Date.now();
    }
    const rec = JSON.parse(firstLine) as unknown;
    if (isRecord(rec) && typeof rec['created_at'] === 'number') {
      return rec['created_at'];
    }
  } catch {
    // Fall through to the default.
  }
  return Date.now();
}
