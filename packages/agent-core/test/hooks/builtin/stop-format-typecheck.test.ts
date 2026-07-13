import { mkdtempSync, writeFileSync, mkdirSync, chmodSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { EditAccumulatorBuiltin } from '../../../src/session/hooks/builtin/edit-accumulator';
import { StopFormatTypecheckBuiltin } from '../../../src/session/hooks/builtin/stop-format-typecheck';

function makeProject(tscBody: string): { dir: string; acc: EditAccumulatorBuiltin } {
  const dir = mkdtempSync(join(tmpdir(), 'kimi-hooks-stop-'));
  const binDir = join(dir, 'node_modules', '.bin');
  mkdirSync(binDir, { recursive: true });

  writeFileSync(join(dir, '.prettierrc'), '{}');
  writeFileSync(join(binDir, 'prettier'), '#!/bin/sh\nexit 0', 'utf-8');
  chmodSync(join(binDir, 'prettier'), 0o755);

  writeFileSync(join(dir, 'tsconfig.json'), '{}');
  writeFileSync(join(binDir, 'tsc'), `#!/bin/sh\n${tscBody}`, 'utf-8');
  chmodSync(join(binDir, 'tsc'), 0o755);

  return { dir, acc: new EditAccumulatorBuiltin() };
}

async function collect(acc: EditAccumulatorBuiltin, dir: string, path: string): Promise<void> {
  await acc.run(
    { toolName: 'Edit', toolInput: { path } },
    { cwd: dir, env: {}, timeout: 5 },
  );
}

describe('StopFormatTypecheckBuiltin', () => {
  it('allows when formatter and tsc both succeed', async () => {
    const { dir, acc } = makeProject('exit 0');
    await collect(acc, dir, 'src/a.ts');
    const builtin = new StopFormatTypecheckBuiltin(acc);
    const result = await builtin.run({ stopHookActive: false }, { cwd: dir, env: {}, timeout: 5 });
    expect(result.action).toBe('allow');
    expect(acc.readAndClear()).toEqual([]);
  });

  it('blocks when tsc fails and reports stderr as reason', async () => {
    const { dir, acc } = makeProject('echo "type error" >&2\nexit 2');
    await collect(acc, dir, 'src/a.ts');
    const builtin = new StopFormatTypecheckBuiltin(acc);
    const result = await builtin.run({ stopHookActive: false }, { cwd: dir, env: {}, timeout: 5 });
    expect(result.action).toBe('block');
    expect(result.reason).toContain('type error');
    expect(acc.readAndClear()).toEqual([]);
  });

  it('allows with no files accumulated', async () => {
    const { dir, acc } = makeProject('exit 0');
    const builtin = new StopFormatTypecheckBuiltin(acc);
    const result = await builtin.run({ stopHookActive: false }, { cwd: dir, env: {}, timeout: 5 });
    expect(result.action).toBe('allow');
    expect(result.stdout).toContain('no files to check');
  });

  it('blocks when formatter fails', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'kimi-hooks-stop-fmt-'));
    const binDir = join(dir, 'node_modules', '.bin');
    mkdirSync(binDir, { recursive: true });
    writeFileSync(join(dir, '.prettierrc'), '{}');
    writeFileSync(join(binDir, 'prettier'), '#!/bin/sh\necho "format error" >&2\nexit 2', 'utf-8');
    chmodSync(join(binDir, 'prettier'), 0o755);
    writeFileSync(join(dir, 'tsconfig.json'), '{}');
    writeFileSync(join(binDir, 'tsc'), '#!/bin/sh\nexit 0', 'utf-8');
    chmodSync(join(binDir, 'tsc'), 0o755);

    const acc = new EditAccumulatorBuiltin();
    await collect(acc, dir, 'src/a.ts');
    const builtin = new StopFormatTypecheckBuiltin(acc);
    const result = await builtin.run({ stopHookActive: false }, { cwd: dir, env: {}, timeout: 5 });
    expect(result.action).toBe('block');
    expect(result.reason).toContain('format error');
  });
});
