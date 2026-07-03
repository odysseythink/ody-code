import { mkdir, writeFile } from 'node:fs/promises';
import type { Scenario } from '../types';

export const kaosOpsScenario: Scenario = {
  name: 'kaos-ops',
  async run(backend) {
    const home = backend.homeDir;

    // Prepare a small fixture tree under the backend's home dir.
    await mkdir(`${home}/sub`, { recursive: true });
    await writeFile(`${home}/a.txt`, 'hello');
    await writeFile(`${home}/sub/b.txt`, 'world');

    const responses: unknown[] = [];

    responses.push(
      await backend.envCall('env.getcwd', {}),
      await backend.envCall('env.stat', { path: 'a.txt' }),
      await backend.envCall('env.glob', { path: '.', pattern: '**/*.txt' }),
      await backend.envCall('env.readText', { path: 'a.txt' }),
      await backend.envCall('env.writeText', { path: 'out.txt', text: '!' }),
      await backend.envCall('env.exec', { command: '/bin/sh', args: ['-c', 'printf hello'] }),
    );

    return { responses, events: [] };
  },
};
