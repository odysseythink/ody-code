import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createRPC, KimiCore, type CoreAPI, type SDKAPI } from '../../src';

const BASE_CONFIG = `
default_model = "kimi-code/kimi-for-coding"

[providers."managed:ody-code"]
type = "kimi"
api_key = "test-key"
base_url = "https://api.example/v1"

[models."kimi-code/kimi-for-coding"]
provider = "managed:ody-code"
model = "kimi-for-coding"
max_context_size = 1000000
`;

describe('plan-mode bootstrap from config.defaultSessionMode', () => {
  let tmp: string;
  let homeDir: string;
  let workDir: string;
  let configPath: string;

  beforeEach(async () => {
    tmp = await mkdtemp(join(tmpdir(), 'kimi-plan-mode-'));
    homeDir = join(tmp, 'home');
    workDir = join(tmp, 'work');
    configPath = join(tmp, 'config.toml');
    await mkdir(workDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(tmp, { recursive: true, force: true });
  });

  it('activates plan mode on a new session when config.defaultSessionMode is true', async () => {
    await writeFile(configPath, `default_plan_mode = true\n${BASE_CONFIG}`);
    const rpc = await createTestRpc();
    const created = await rpc.createSession({ workDir });
    await rpc.closeSession({ sessionId: created.id });

    expect(await countPlanModeEnters()).toBe(1);
  });

  it('leaves plan mode inactive when config.defaultSessionMode is absent', async () => {
    await writeFile(configPath, BASE_CONFIG);
    const rpc = await createTestRpc();
    const created = await rpc.createSession({ workDir });
    await rpc.closeSession({ sessionId: created.id });

    expect(await countPlanModeEnters()).toBe(0);
  });

  it('does not apply config.defaultSessionMode when resuming an existing session', async () => {
    await writeFile(configPath, BASE_CONFIG);
    const rpc = await createTestRpc();
    const created = await rpc.createSession({ workDir });
    await rpc.closeSession({ sessionId: created.id });

    // Turning the default on after the session already exists must not
    // retroactively push a resumed session into plan mode.
    await writeFile(configPath, `default_plan_mode = true\n${BASE_CONFIG}`);
    const freshRpc = await createTestRpc();
    await freshRpc.resumeSession({ sessionId: created.id });
    await freshRpc.closeSession({ sessionId: created.id });

    expect(await countPlanModeEnters()).toBe(0);
  });

  async function countPlanModeEnters(): Promise<number> {
    const suffix = join('agents', 'main', 'wire.jsonl');
    const entries = await readdir(homeDir, { recursive: true });
    const match = entries.find((entry) => entry.endsWith(suffix));
    if (match === undefined) {
      throw new Error('wire.jsonl not found under session home');
    }
    const lines = (await readFile(join(homeDir, match), 'utf-8'))
      .split('\n')
      .filter((line) => line.trim().length > 0);
    return lines.filter((line) => (JSON.parse(line) as { type?: string }).type === 'session_mode.enter')
      .length;
  }

  async function createTestRpc() {
    const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
    void new KimiCore(coreRpc, { homeDir, configPath });
    return sdkRpc({
      emitEvent: vi.fn(),
      requestApproval: vi.fn(async () => ({ decision: 'rejected' as const })),
      requestQuestion: vi.fn(async () => null),
      openExternal: vi.fn(async () => ({ opened: false })),
      toolCall: vi.fn(async () => ({ output: '' })),
    });
  }
});

describe('modeModels runtime config propagation', () => {
  let tmp: string;
  let homeDir: string;
  let workDir: string;
  let configPath: string;

  beforeEach(async () => {
    tmp = await mkdtemp(join(tmpdir(), 'kimi-mode-models-'));
    homeDir = join(tmp, 'home');
    workDir = join(tmp, 'work');
    configPath = join(tmp, 'config.toml');
    await mkdir(workDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(tmp, { recursive: true, force: true });
  });

  it('picks up modeModels added after session creation', async () => {
    await writeFile(
      configPath,
      `
default_model = "kimi-code/kimi-for-coding"

[providers."managed:ody-code"]
type = "kimi"
api_key = "test-key"
base_url = "https://api.example/v1"

[models."kimi-code/kimi-for-coding"]
provider = "managed:ody-code"
model = "kimi-for-coding"
max_context_size = 1000000

[models."plan-only-model"]
provider = "managed:ody-code"
model = "plan-only-model"
max_context_size = 1000000
`,
    );

    const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
    const core = new KimiCore(coreRpc, { homeDir, configPath });
    const sdk = await sdkRpc({
      emitEvent: vi.fn(),
      requestApproval: vi.fn(async () => ({ decision: 'rejected' as const })),
      requestQuestion: vi.fn(async () => null),
      openExternal: vi.fn(async () => ({ opened: false })),
      toolCall: vi.fn(async () => ({ output: '' })),
    });

    const session = await core.createSession({ workDir });

    // At this point modeModels is not configured.
    // Now add it via setKimiConfig (simulating a manual config edit or UI action).
    await core.setKimiConfig({
      modeModels: { plan: 'plan-only-model' },
    });

    // Enter plan mode — the Agent should see the newly added modeModels.
    await sdk.enterPlan({ sessionId: session.id, agentId: 'main', kind: 'plan' });

    const model = await sdk.getModel({ sessionId: session.id, agentId: 'main' });
    expect(model).toBe('plan-only-model');

    await core.closeSession({ sessionId: session.id });
  });
});
