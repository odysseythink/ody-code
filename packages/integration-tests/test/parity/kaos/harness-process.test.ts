import { describe, expect, it } from 'vitest';
import { LocalKaos } from '@odysseythink/kaos';
import { runTsCase } from '../../../src/parity/kaos-golden';

describe('kaos golden harness process ops', () => {
  it('exec captures stdout/stderr/exitCode', async () => {
    const kaos = await LocalKaos.create();
    const result = await runTsCase(
      kaos,
      {
        name: 'exec echo',
        op: {
          type: 'exec',
          command: '/bin/echo',
          args: ['-n', 'hello'],
        },
        expected: {},
      },
      process.cwd(),
    );
    expect(result).toEqual({
      result: {
        stdout: [104, 101, 108, 108, 111],
        stderr: [],
        exitCode: 0,
      },
    });
  });
});
