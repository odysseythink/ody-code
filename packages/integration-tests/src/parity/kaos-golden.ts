import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'pathe';
import { spawnSync } from 'node:child_process';
import { Readable } from 'node:stream';
import type { LocalKaos } from '@odysseythink/kaos';
import { detectEnvironment, normpath, KaosFileExistsError } from '@odysseythink/kaos';
import { decodeTextWithErrors, globPatternToRegex } from '@odysseythink/kaos/internal';

export interface FixtureFile {
  version: number;
  cases: GoldenCase[];
}

export interface GoldenCase {
  name: string;
  op: GoldenOp;
  expected: unknown;
}

export type GoldenOp =
  | { type: 'normpath'; input: string }
  | {
      type: 'detect_environment';
      platform: string;
      arch: string;
      release: string;
      env: Record<string, string>;
      files: string[];
      executables: Record<string, string>;
    }
  | { type: 'decode'; encoding: BufferEncoding; mode: 'strict' | 'replace' | 'ignore'; bytes: number[] }
  | { type: 'pattern_to_regex'; pattern: string; caseSensitive: boolean; inputs: string[] }
  | {
      type: 'read_bytes';
      path: string;
      n?: number;
      files: Record<string, number[]>;
    }
  | {
      type: 'read_text';
      path: string;
      encoding?: string;
      mode?: 'strict' | 'replace' | 'ignore';
      files: Record<string, number[]>;
    }
  | {
      type: 'read_lines';
      path: string;
      encoding?: string;
      mode?: 'strict' | 'replace' | 'ignore';
      files: Record<string, number[]>;
    }
  | {
      type: 'write_bytes';
      path: string;
      data: number[];
    }
  | {
      type: 'write_text';
      path: string;
      data: string;
      writeMode?: string;
      encoding?: string;
    }
  | {
      type: 'stat';
      path: string;
      followSymlinks?: boolean;
      files?: Record<string, number[]>;
    }
  | {
      type: 'iterdir';
      path: string;
      files?: Record<string, number[]>;
    }
  | {
      type: 'glob';
      path: string;
      pattern: string;
      caseSensitive?: boolean;
      files?: Record<string, number[]>;
    }
  | {
      type: 'mkdir';
      path: string;
      parents?: boolean;
      existOk?: boolean;
      files?: Record<string, number[]>;
    }
  | {
      type: 'chdir';
      path: string;
      files?: Record<string, number[]>;
    }
  | {
      type: 'exec';
      command: string;
      args: string[];
      env?: Record<string, string>;
      stdin?: number[];
      files?: Record<string, number[]>;
    }
  | {
      type: 'kill_tree';
      command: string;
      args: string[];
      files?: Record<string, number[]>;
      sleepMs: number;
    };

export async function runTsGolden(
  kaos: LocalKaos,
  fixture: FixtureFile,
): Promise<Record<string, unknown>> {
  const tempDir = await setupTempDir(fixture);
  if (tempDir) {
    await kaos.chdir(tempDir);
  }

  const out: Record<string, unknown> = {};
  for (const c of fixture.cases) {
    out[c.name] = await runTsCase(kaos, c, tempDir!);
  }
  return out;
}

export async function runTsCase(
  kaos: LocalKaos,
  c: GoldenCase,
  tempDir: string,
): Promise<unknown> {
  const op = c.op;
  switch (op.type) {
    case 'normpath':
      return { result: kaos.normpath(op.input) };
    case 'detect_environment': {
      const files = new Set(op.files);
      const env = await detectEnvironment({
        platform: op.platform,
        arch: op.arch,
        release: op.release,
        env: op.env,
        isFile: async (p) => files.has(p),
        findExecutable: async (name) => op.executables[name],
      });
      return { result: env };
    }
    case 'decode': {
      const buf = Buffer.from(op.bytes);
      try {
        const result = decodeTextWithErrors(buf, op.encoding, op.mode);
        return { result };
      } catch {
        return { error: 'decode error' };
      }
    }
    case 'pattern_to_regex': {
      const re = globPatternToRegex(op.pattern, op.caseSensitive);
      const matches = op.inputs.map((input) => re.test(input));
      const source = op.caseSensitive ? re.source : `(?i)${re.source}`;
      return { result: { regex: source, matches } };
    }
    case 'read_bytes': {
      const resolved = resolvePath(tempDir, op.path);
      try {
        const result = await kaos.readBytes(resolved, op.n);
        return { result: [...result] };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'read_text': {
      const resolved = resolvePath(tempDir, op.path);
      try {
        const result = await kaos.readText(resolved, {
          encoding: (op.encoding as BufferEncoding) ?? 'utf-8',
          errors: op.mode ?? 'strict',
        });
        return { result };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'read_lines': {
      const resolved = resolvePath(tempDir, op.path);
      try {
        const lines: string[] = [];
        for await (const line of kaos.readLines(resolved, {
          encoding: (op.encoding as BufferEncoding) ?? 'utf-8',
          errors: op.mode ?? 'strict',
        })) {
          lines.push(line);
        }
        return { result: lines };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'write_bytes': {
      const resolved = resolvePath(tempDir, op.path);
      try {
        const data = Buffer.from(op.data);
        const n = await kaos.writeBytes(resolved, data);
        const content = await readFile(resolved);
        return { result: { written: n, content: [...content] } };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'write_text': {
      const resolved = resolvePath(tempDir, op.path);
      try {
        const n = await kaos.writeText(resolved, op.data, {
          mode: (op.writeMode as 'w' | 'a') ?? 'w',
          encoding: (op.encoding as BufferEncoding) ?? 'utf-8',
        });
        const content = await readFile(resolved);
        return { result: { written: n, content: [...content] } };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'stat': {
      try {
        const s = await kaos.stat(op.path, { followSymlinks: op.followSymlinks ?? true });
        const isDir = (s.stMode & 0o170000) === 0o040000;
        return { result: { isDir, size: isDir ? 0 : s.stSize } };
      } catch (e) {
        return { error: canonicalIoError(e) };
      }
    }
    case 'iterdir': {
      try {
        const entries: string[] = [];
        for await (const p of kaos.iterdir(op.path)) {
          entries.push(relativeToTemp(p, tempDir));
        }
        entries.sort();
        return { result: entries };
      } catch (e) {
        return { error: canonicalIoError(e) };
      }
    }
    case 'glob': {
      try {
        const matches: string[] = [];
        for await (const p of kaos.glob(op.path, op.pattern, { caseSensitive: op.caseSensitive ?? true })) {
          matches.push(relativeToTemp(p, tempDir));
        }
        matches.sort();
        return { result: matches };
      } catch (e) {
        return { error: canonicalIoError(e) };
      }
    }
    case 'mkdir': {
      try {
        await kaos.mkdir(op.path, { parents: op.parents ?? false, existOk: op.existOk ?? false });
        return { result: { created: true } };
      } catch (e) {
        const msg = e instanceof KaosFileExistsError ? e.message : String(e);
        if (msg.includes('already exists but is not a directory')) {
          // Strip tempdir prefix for fixture compatibility
          const rel = relativeToTemp(msg.split(' already ')[0]!, tempDir);
          return { error: `${rel} already exists but is not a directory` };
        }
        return { error: canonicalIoError(e) };
      }
    }
    case 'chdir': {
      try {
        await kaos.chdir(op.path);
        return { result: { changed: true } };
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (msg.includes('Not a directory')) {
          return { error: 'not a directory' };
        }
        return { error: canonicalIoError(e) };
      }
    }
    case 'exec': {
      const allArgs = [op.command, ...op.args];
      const proc =
        op.env && Object.keys(op.env).length > 0
          ? await kaos.execWithEnv(allArgs, op.env)
          : await kaos.exec(...allArgs);
      if (op.stdin && op.stdin.length > 0) {
        proc.stdin.write(Buffer.from(op.stdin));
        proc.stdin.end();
      }
      const [stdout, stderr] = await Promise.all([
        streamToBuffer(proc.stdout),
        streamToBuffer(proc.stderr),
      ]);
      const exitCode = await proc.wait();
      return {
        result: {
          stdout: [...stdout],
          stderr: [...stderr],
          exitCode,
        },
      };
    }
    case 'kill_tree': {
      const allArgs = [op.command, ...op.args];
      const proc = await kaos.exec(...allArgs);
      await new Promise((resolve) => setTimeout(resolve, op.sleepMs));
      await proc.kill();
      await proc.wait();
      const marker = join(tempDir, 'pids.txt');
      let content = '';
      try {
        content = await readFile(marker, 'utf8');
      } catch {
        // marker may be absent if spawn failed before writing
      }
      for (const pid of content
        .trim()
        .split(/\s+/)
        .filter((s) => s.length > 0)) {
        try {
          process.kill(Number(pid), 0);
          throw new Error(`pid ${pid} still alive`);
        } catch (e) {
          if ((e as NodeJS.ErrnoException).code !== 'ESRCH') {
            throw e;
          }
        }
      }
      return { result: { killed: true } };
    }
    default:
      throw new Error(`unknown op type ${(op as { type: string }).type}`);
  }
}

async function streamToBuffer(readable: Readable): Promise<Buffer> {
  const chunks: Buffer[] = [];
  for await (const chunk of readable) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks);
}

function relativeToTemp(p: string, tempDir: string): string {
  const normalized = p.replace(/\\/g, '/');
  const base = tempDir.replace(/\\/g, '/').replace(/\/$/, '');
  return normalized.startsWith(base + '/') ? normalized.slice(base.length + 1) : normalized;
}

function canonicalIoError(e: unknown): string {
  if (e instanceof KaosFileExistsError) {
    return e.message.includes('not a directory') ? e.message : 'already exists';
  }
  if (e && typeof e === 'object' && 'code' in e) {
    const code = (e as { code: string }).code;
    if (code === 'ENOENT') return 'not found';
    if (code === 'EEXIST') return 'already exists';
    if (code === 'EACCES') return 'permission denied';
  }
  return String(e);
}

async function setupTempDir(fixture: FixtureFile): Promise<string | undefined> {
  const files = collectFiles(fixture);
  if (Object.keys(files).length === 0) return undefined;

  const dir = await mkdtemp(join(tmpdir(), 'kaos-golden-'));
  for (const [rel, bytes] of Object.entries(files)) {
    const full = join(dir, rel);
    if (rel.endsWith('/')) {
      await mkdir(full, { recursive: true });
    } else {
      await mkdir(dirname(full), { recursive: true });
      await writeFile(full, Buffer.from(bytes));
    }
  }
  return dir;
}

function collectFiles(fixture: FixtureFile): Record<string, number[]> {
  const out: Record<string, number[]> = {};
  for (const c of fixture.cases) {
    const op = c.op as { files?: Record<string, number[]> };
    if (op.files) Object.assign(out, op.files);
  }
  return out;
}

function resolvePath(tempDir: string | undefined, path: string): string {
  if (path.startsWith('/')) return path;
  if (tempDir) return join(tempDir, path);
  return path;
}

export function runRustGolden(fixturePath: string, binaryPath: string): Record<string, unknown> {
  const result = spawnSync(binaryPath, [fixturePath], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) {
    throw new Error(`failed to run kaos-golden: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`kaos-golden exited ${result.status}: ${result.stderr}`);
  }
  return JSON.parse(result.stdout) as Record<string, unknown>;
}

export function resolveRustGoldenBinary(rootDir: string): string {
  const override = process.env['ODY_KAOS_GOLDEN_BINARY_PATH'];
  if (override !== undefined && override.length > 0) {
    return override;
  }
  return join(rootDir, 'rust-ody', 'target', 'debug', 'kaos-golden');
}
