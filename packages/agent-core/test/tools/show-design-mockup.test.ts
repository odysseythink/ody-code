import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import type { OpenExternalRequest, OpenExternalResponse } from '../../src/rpc';
import { ShowDesignMockupTool } from '../../src/tools/builtin/visual/show-design-mockup';

interface MockupAgentStub {
  sessionModeFilePath: string | null;
  openExternal?: (request: OpenExternalRequest) => Promise<OpenExternalResponse>;
}

function mockupAgent(stub: MockupAgentStub): Agent {
  return {
    sessionMode: {
      get sessionModeFilePath() {
        return stub.sessionModeFilePath;
      },
    },
    rpc: stub.openExternal === undefined ? {} : { openExternal: stub.openExternal },
  } as unknown as Agent;
}

async function run(tool: ShowDesignMockupTool, html: string, title?: string) {
  const execution = tool.resolveExecution({ html, title });
  if (!('execute' in execution)) throw new Error('expected a runnable execution');
  return execution.execute({} as never);
}

let dir: string;

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), 'mockup-test-'));
});

afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

describe('ShowDesignMockupTool', () => {
  it('writes the mockup next to the design file and opens it via openExternal', async () => {
    const openExternal = vi.fn(async (_request: OpenExternalRequest) => ({ opened: true }));
    const agent = mockupAgent({ sessionModeFilePath: join(dir, 'design.md'), openExternal });
    const tool = new ShowDesignMockupTool(agent);

    const result = await run(tool, '<html><body>Hi</body></html>', 'Footer layout');

    expect(result.isError).toBe(false);
    expect(openExternal).toHaveBeenCalledTimes(1);
    const arg = openExternal.mock.calls[0]![0];
    expect(arg.url.startsWith('file://')).toBe(true);
    expect(arg.title).toBe('Footer layout');

    // The file lives under <design dir>/.mockups and contains the HTML.
    const written = fileURLToPath(arg.url);
    expect(written).toContain(join(dir, '.mockups'));
    expect(await readFile(written, 'utf8')).toContain('<body>Hi</body>');
    expect(result.output).toContain('Opened mockup');
  });

  it('reports a graceful error when the host has no openExternal', async () => {
    const agent = mockupAgent({ sessionModeFilePath: join(dir, 'design.md') });
    const tool = new ShowDesignMockupTool(agent);

    const result = await run(tool, '<html></html>');

    expect(result.isError).toBe(true);
    expect(result.output).toContain('not available');
  });

  it('still reports the written path when the host declines to open it', async () => {
    const openExternal = vi.fn(async (_request: OpenExternalRequest) => ({
      opened: false,
      error: 'no browser',
    }));
    const agent = mockupAgent({ sessionModeFilePath: join(dir, 'design.md'), openExternal });
    const tool = new ShowDesignMockupTool(agent);

    const result = await run(tool, '<html></html>', 'X');

    expect(result.isError).toBe(false);
    expect(result.output).toContain('did not open it');
    expect(result.output).toContain('no browser');
  });
});
