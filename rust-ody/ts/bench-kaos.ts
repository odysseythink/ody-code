#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { spawnSync } from 'node:child_process';

const N = Number(process.argv[2] ?? '1000');
const root = join(tmpdir(), `kaos-bench-${Date.now()}`);

async function setup(): Promise<void> {
  await mkdir(root, { recursive: true });
  for (let i = 0; i < N; i++) {
    await writeFile(
      join(root, `file-${i.toString().padStart(6, '0')}.txt`),
      'x'.repeat(100),
    );
  }
}

async function cleanup(): Promise<void> {
  await rm(root, { recursive: true, force: true });
}

function bench(name: string, fn: () => void): void {
  const t0 = performance.now();
  fn();
  const t1 = performance.now();
  console.log(`${name}: ${(t1 - t0).toFixed(2)} ms`);
}

async function main(): Promise<void> {
  console.log(`Setting up ${N} files in ${root}...`);
  await setup();
  try {
    bench('stat single', () => {
      spawnSync(
        'node',
        [
          '-e',
          `require('fs').statSync(${JSON.stringify(join(root, 'file-000000.txt'))})`,
        ],
        { stdio: 'inherit' },
      );
    });

    bench('glob *.txt', () => {
      const glob = spawnSync(
        'node',
        [
          '-e',
          `
const fs = require('fs');
const path = require('path');
const files = fs.readdirSync(${JSON.stringify(root)}).filter(f => f.endsWith('.txt'));
console.log(files.length);
`,
        ],
        { encoding: 'utf8' },
      );
      if (glob.status !== 0) throw new Error(glob.stderr ?? 'glob failed');
    });

    bench('read 100 files', () => {
      for (let i = 0; i < 100; i++) {
        readFileSync(
          join(root, `file-${i.toString().padStart(6, '0')}.txt`),
          'utf8',
        );
      }
    });
  } finally {
    await cleanup();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
