import { spawn } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { performance } from 'node:perf_hooks';
import { promisify } from 'node:util';
import { execFile } from 'node:child_process';

const execFileAsync = promisify(execFile);

interface BenchmarkSample {
  readonly coldStartMs: number;
  readonly rssMb: number;
  readonly idleCpuPercent: number;
}

interface BenchmarkReport {
  readonly samples: readonly BenchmarkSample[];
  readonly avgColdStartMs: number;
  readonly avgRssMb: number;
  readonly avgIdleCpuPercent: number;
}

function makeHome(): string {
  const dir = mkdtempSync(join(tmpdir(), 'ody-host-bench-'));
  writeFileSync(
    join(dir, 'config.toml'),
    `default_model = "mock"\ndefault_provider = "local"\n\n[providers.local]\ntype = "kimi"\napi_key = "test"\n\n[models.mock]\nprovider = "local"\nmodel = "mock"\nmax_context_size = 4096\n`,
    'utf8',
  );
  return dir;
}

async function measureOnce(binaryPath: string): Promise<BenchmarkSample> {
  const homeDir = makeHome();

  const start = performance.now();
  const proc = spawn(binaryPath, ['serve', '--stdio', '--home', homeDir, '--mock-provider'], {
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  // Wait for ready message
  await new Promise<void>((resolve, reject) => {
    const onData = (data: Buffer) => {
      const text = data.toString('utf8');
      if (text.includes('ready')) {
        proc.stderr!.off('data', onData);
        resolve();
      }
    };
    proc.stderr!.on('data', onData);
    proc.on('error', reject);
    proc.on('exit', (code) => {
      if (code !== null && code !== 0) reject(new Error(`host exited with ${code}`));
    });
    setTimeout(() => reject(new Error('host ready timeout')), 30000);
  });
  const coldStartMs = performance.now() - start;

  // Sample RSS
  const pid = proc.pid!;
  const { stdout: rssLine } = await execFileAsync('ps', ['-o', 'rss=', '-p', String(pid)]);
  const rssKb = Number.parseInt(rssLine.trim(), 10);
  const rssMb = Number.isNaN(rssKb) ? 0 : rssKb / 1024;

  // Idle 5 seconds then sample CPU
  await new Promise((resolve) => setTimeout(resolve, 5000));
  const { stdout: cpuLine } = await execFileAsync('ps', ['-o', 'cputime=', '-p', String(pid)]);
  const cpuSec = parseCpuTime(cpuLine.trim());
  const idleCpuPercent = (cpuSec / 5) * 100;

  proc.kill('SIGTERM');
  await new Promise<void>((resolve) => proc.on('exit', () => resolve()));

  return { coldStartMs, rssMb, idleCpuPercent };
}

function parseCpuTime(value: string): number {
  const parts = value.split(':');
  if (parts.length === 2) {
    const [min, sec] = parts.map(Number);
    return (Number.isNaN(min) ? 0 : min) * 60 + (Number.isNaN(sec) ? 0 : sec);
  }
  if (parts.length === 3) {
    const [hour, min, sec] = parts.map(Number);
    return (Number.isNaN(hour) ? 0 : hour) * 3600 +
      (Number.isNaN(min) ? 0 : min) * 60 +
      (Number.isNaN(sec) ? 0 : sec);
  }
  return 0;
}

async function main() {
  const binaryPath = process.argv[2] ?? 'rust-ody/target/release/ody-host';
  const samples: BenchmarkSample[] = [];
  for (let i = 0; i < 3; i++) {
    try {
      samples.push(await measureOnce(binaryPath));
    } catch (err) {
      console.error(`Sample ${i} failed:`, err);
    }
  }

  if (samples.length === 0) {
    console.error('No successful benchmark samples');
    process.exit(1);
  }

  const avg = (arr: number[]) => arr.reduce((a, b) => a + b, 0) / arr.length;
  const report: BenchmarkReport = {
    samples,
    avgColdStartMs: Math.round(avg(samples.map((s) => s.coldStartMs)) * 100) / 100,
    avgRssMb: Math.round(avg(samples.map((s) => s.rssMb)) * 100) / 100,
    avgIdleCpuPercent: Math.round(avg(samples.map((s) => s.idleCpuPercent)) * 100) / 100,
  };

  console.log(JSON.stringify(report, null, 2));
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
