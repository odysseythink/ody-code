import { describe, expect, it } from 'vitest';

import { EditAccumulatorBuiltin } from '../../../src/session/hooks/builtin/edit-accumulator';

describe('EditAccumulatorBuiltin', () => {
  it('collects Edit tool path and resolves it against cwd', async () => {
    const acc = new EditAccumulatorBuiltin();
    await acc.run(
      { toolName: 'Edit', toolInput: { path: 'src/foo.ts' } },
      { cwd: '/proj', env: {}, timeout: 5 },
    );
    expect(acc.readAndClear()).toEqual(['/proj/src/foo.ts']);
  });

  it('collects Write tool path', async () => {
    const acc = new EditAccumulatorBuiltin();
    await acc.run(
      { toolName: 'Write', toolInput: { path: 'src/bar.ts' } },
      { cwd: '/proj', env: {}, timeout: 5 },
    );
    expect(acc.readAndClear()).toEqual(['/proj/src/bar.ts']);
  });

  it('ignores non-edit tools like Read', async () => {
    const acc = new EditAccumulatorBuiltin();
    await acc.run(
      { toolName: 'Read', toolInput: { path: 'src/secret.ts' } },
      { cwd: '/proj', env: {}, timeout: 5 },
    );
    expect(acc.readAndClear()).toEqual([]);
  });

  it('deduplicates repeated paths', async () => {
    const acc = new EditAccumulatorBuiltin();
    await acc.run(
      { toolName: 'Edit', toolInput: { path: 'src/foo.ts' } },
      { cwd: '/proj', env: {}, timeout: 5 },
    );
    await acc.run(
      { toolName: 'Write', toolInput: { path: 'src/foo.ts' } },
      { cwd: '/proj', env: {}, timeout: 5 },
    );
    expect(acc.readAndClear()).toEqual(['/proj/src/foo.ts']);
  });

  it('clears the set after readAndClear', async () => {
    const acc = new EditAccumulatorBuiltin();
    await acc.run(
      { toolName: 'Edit', toolInput: { path: 'src/foo.ts' } },
      { cwd: '/proj', env: {}, timeout: 5 },
    );
    expect(acc.readAndClear()).toHaveLength(1);
    expect(acc.readAndClear()).toHaveLength(0);
  });
});
