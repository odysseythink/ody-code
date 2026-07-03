import { mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import type { ChatProvider } from '@odysseythink/kosong';

import { makeRustBackend, makeTsBackend, cleanupHome } from './backends';
import { resolveRustBinaryPath } from './rust-binary';

export interface ResumeCrossHostResult {
  readonly tsFirstSessionId: string;
  readonly rustResumedSessionId: string;
  readonly tsFinalSessionId: string;
}

export async function runResumeCrossHost(
  mockLlm: ChatProvider,
): Promise<ResumeCrossHostResult> {
  const binaryPath = resolveRustBinaryPath();
  const homeDir = join(tmpdir(), `parity-resume-${Date.now()}`);
  await mkdir(homeDir, { recursive: true });

  const sessionId = 'resume-cross-host-001';
  let tsFirstSessionId = '';
  let rustResumedSessionId = '';
  let tsFinalSessionId = '';

  // Round 1: TS creates session and does a prompt
  const ts1 = await makeTsBackend({ homeDir, mockLlm });
  try {
    const summary = await ts1.client.createSession({ workDir: homeDir, id: sessionId, model: 'mock' });
    tsFirstSessionId = summary.id;
    await ts1.client.prompt({ sessionId: summary.id, input: [{ type: 'text', text: 'hello' }] });
    await ts1.client.closeSession({ sessionId: summary.id });
  } finally {
    await ts1.close();
  }

  // Round 2: Rust resumes the session
  const rust = await makeRustBackend({ homeDir, binaryPath, transport: 'stdio', extraArgs: ['--mock-provider'] });
  try {
    const resumed = await rust.client.resumeSession({ id: sessionId });
    rustResumedSessionId = resumed.id ?? '';
    await rust.client.prompt({ sessionId: rustResumedSessionId, input: [{ type: 'text', text: 'continue' }] });
    await rust.client.closeSession({ sessionId });
  } finally {
    await rust.close();
  }

  // Round 3: TS resumes the session again
  const ts2 = await makeTsBackend({ homeDir, mockLlm });
  try {
    const final = await ts2.client.resumeSession({ id: sessionId });
    tsFinalSessionId = final.id ?? '';
    await ts2.client.prompt({ sessionId: tsFinalSessionId, input: [{ type: 'text', text: 'finish' }] });
    await ts2.client.closeSession({ sessionId });
  } finally {
    await ts2.close();
    await cleanupHome(homeDir);
  }

  return { tsFirstSessionId, rustResumedSessionId, tsFinalSessionId };
}
