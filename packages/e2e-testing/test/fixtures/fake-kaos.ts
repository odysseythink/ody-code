import type { Environment, Kaos, KaosProcess } from '@odysseythink/kaos';

function notImplemented(method: string): never {
  throw new Error(`FakeKaos.${method} not implemented — override in test`);
}

export const FAKE_OS_ENV: Environment = {
  osKind: 'Linux',
  osArch: 'x86_64',
  osVersion: 'test',
  shellName: 'bash',
  shellPath: '/bin/bash',
};

export function createFakeKaos(overrides?: Partial<Kaos>): Kaos {
  let cwd = overrides?.getcwd?.() ?? '/workspace';
  const base: Kaos = {
    name: 'fake',
    osEnv: FAKE_OS_ENV,
    pathClass: () => 'posix',
    normpath: (p: string) => p,
    gethome: () => '/home/test',
    getcwd: () => cwd,
    withCwd: (next: string) => createFakeKaos({ ...overrides, getcwd: () => next }),
    chdir: async (next: string) => {
      cwd = next;
    },
    stat: () => notImplemented('stat'),
    iterdir: () => notImplemented('iterdir'),
    glob: () => notImplemented('glob'),
    readBytes: () => notImplemented('readBytes'),
    readText: () => notImplemented('readText'),
    readLines: () => notImplemented('readLines'),
    writeBytes: () => notImplemented('writeBytes'),
    writeText: () => notImplemented('writeText'),
    mkdir: () => notImplemented('mkdir'),
    exec: () => notImplemented('exec'),
    execWithEnv: () => notImplemented('execWithEnv'),
  };
  return { ...base, ...overrides } as Kaos;
}
