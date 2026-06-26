/**
 * G2-B gate smoke test: start `ody serve` in-process on a TCP port, connect with
 * `SDKRpcClient.connect()`, create a session, send a prompt, and receive the
 * event stream end-to-end.
 *
 * The LLM provider is mocked via `vi.mock('@odysseythink/kosong')` so no real
 * network calls are made. Opt-in via `ODY_E2E=1` to keep the default test suite
 * fast.
 */

import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:net';
import { PassThrough } from 'node:stream';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SDKRpcClient, type Event } from '@odysseythink/ody-code-sdk';
import type { ChatProvider, StreamedMessage, StreamedMessagePart, TokenUsage } from '@odysseythink/kosong';

import { handleServe, type ServeDeps } from '#cli/sub/serve';
import { getVersion } from '#cli/version';

const ENABLED = process.env['ODY_E2E'] === '1';
const TEST_TIMEOUT_MS = 60_000;

// Fake provider that returns a deterministic text stream.
vi.mock('@odysseythink/kosong', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@odysseythink/kosong')>();

  function fakeStream(text: string): StreamedMessage {
    const parts: StreamedMessagePart[] = text.split('').map((char) => ({ type: 'text', text: char }));
    const usage: TokenUsage = {
      inputOther: 1,
      output: text.length,
      inputCacheRead: 0,
      inputCacheCreation: 0,
    };
    return {
      id: 'fake-response-id',
      usage,
      finishReason: 'completed',
      rawFinishReason: 'stop',
      [Symbol.asyncIterator](): AsyncIterator<StreamedMessagePart> {
        let index = 0;
        return {
          async next() {
            if (index < parts.length) {
              return { value: parts[index++]!, done: false };
            }
            return { value: undefined, done: true };
          },
        };
      },
    };
  }

  function createFakeProvider(): ChatProvider {
    return {
      name: 'fake',
      modelName: 'fake-model',
      thinkingEffort: null,
      async generate(systemPrompt): Promise<StreamedMessage> {
        void systemPrompt;
        return fakeStream('hello');
      },
      withThinking() {
        return createFakeProvider();
      },
    };
  }

  return {
    ...actual,
    createProvider: (config: unknown): ChatProvider => {
      void config;
      return createFakeProvider();
    },
  };
});

async function writeTestConfig(homeDir: string): Promise<void> {
  await writeFile(
    join(homeDir, 'config.toml'),
    `
default_model = "fake-model"
default_permission_mode = "yolo"

[providers.fake]
type = "kimi"
base_url = "https://example.test/v1"
api_key = "sk-test"

[models.fake-model]
provider = "fake"
model = "fake-model"
max_context_size = 1000
`,
    'utf-8',
  );
}

interface WritableSpy {
  readonly chunks: Uint8Array[];
  write(chunk: Uint8Array): boolean;
  end(cb?: () => void): WritableSpy;
  on(_event: string, _listener: unknown): WritableSpy;
}

function makeWritableSpy(): WritableSpy {
  const chunks: Uint8Array[] = [];
  const self: WritableSpy = {
    chunks,
    write(chunk: Uint8Array) {
      chunks.push(chunk);
      return true;
    },
    end(cb) {
      cb?.();
      return self;
    },
    on() {
      return self;
    },
  };
  return self;
}

function lastReadyJson(spy: WritableSpy): { type: string; host: string; port: number; token: string } {
  const lines = Buffer.concat(spy.chunks).toString('utf-8').split('\n');
  for (let i = lines.length - 1; i >= 0; i--) {
    const line = lines[i]!.trim();
    if (!line) continue;
    try {
      const parsed = JSON.parse(line) as Record<string, unknown>;
      if (parsed['type'] === 'ready') return parsed as { type: string; host: string; port: number; token: string };
    } catch {
      // ignore non-JSON lines
    }
  }
  throw new Error('No ready message found on stderr');
}

describe.skipIf(!ENABLED)('G2-B smoke: ody serve end-to-end', () => {
  let homeDir: string;
  let workDir: string;
  let serverController: { close(): Promise<void> } | undefined;

  beforeEach(async () => {
    homeDir = await mkdtemp(join(tmpdir(), 'ody-g2b-home-'));
    workDir = await mkdtemp(join(tmpdir(), 'ody-g2b-work-'));
    await writeTestConfig(homeDir);
  });

  afterEach(async () => {
    await serverController?.close().catch(() => {});
    serverController = undefined;
    await rm(homeDir, { recursive: true, force: true }).catch(() => {});
    await rm(workDir, { recursive: true, force: true }).catch(() => {});
  });

  it(
    'creates a session over TCP, sends a prompt, and receives the event stream',
    async () => {
      const { createCoreServer } = await import('@odysseythink/ody-code-sdk');
      const stderr = makeWritableSpy();

      const deps: ServeDeps = {
        version: getVersion(),
        createServer,
        createCoreServer,
        stdin: new PassThrough(),
        stdout: new PassThrough(),
        stderr: stderr as unknown as NodeJS.WriteStream,
        exit: (code: number) => {
          throw new Error(`ody serve exited with code ${code}`);
        },
      };

      serverController = await handleServe(deps, { host: '127.0.0.1', port: 0, homeDir });

      const ready = lastReadyJson(stderr);
      expect(ready['type']).toBe('ready');
      expect(ready.host).toBe('127.0.0.1');
      expect(ready.port).toBeGreaterThan(0);
      expect(ready.token).toMatch(/^ody_[A-Za-z0-9_-]+$/);

      const client = await SDKRpcClient.connect({
        transport: { host: ready.host, port: ready.port },
        token: ready.token,
        homeDir,
      });

      try {
        const session = await client.createSession({ workDir });
        expect(session.id).toBeDefined();
        expect(session.workDir).toBe(workDir);

        const events: Event[] = [];
        const turnEnded = new Promise<void>((resolve, reject) => {
          const timeout = setTimeout(() => {
            reject(new Error(`turn.ended not received within ${TEST_TIMEOUT_MS}ms; events=${events.map((e) => e.type).join(',')}`));
          }, TEST_TIMEOUT_MS - 5_000);
          const unsubscribe = client.onEvent((event) => {
            events.push(event);
            if (event.type === 'turn.ended') {
              clearTimeout(timeout);
              unsubscribe();
              resolve();
            }
          });
        });

        await client.prompt({
          sessionId: session.id,
          input: [{ type: 'text', text: 'hi' }],
        });

        await turnEnded;
        // Allow fire-and-forget event RPCs to settle before tearing down the
        // transport, avoiding unhandled rejection noise in the test output.
        await new Promise((resolve) => setTimeout(resolve, 200));

        expect(events.some((e) => e.type === 'turn.started')).toBe(true);
        const assistantDeltas = events.filter((e): e is Extract<Event, { type: 'assistant.delta' }> => e.type === 'assistant.delta');
        expect(assistantDeltas.length).toBeGreaterThan(0);
        expect(assistantDeltas.map((e) => e.delta).join('')).toBe('hello');
        const ended = events.find((e): e is Extract<Event, { type: 'turn.ended' }> => e.type === 'turn.ended');
        expect(ended).toBeDefined();
        expect(ended!.reason).toBe('completed');
      } finally {
        await serverController.close().catch(() => {});
      }
    },
    TEST_TIMEOUT_MS,
  );
});
