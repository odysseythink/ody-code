import { mkdtemp, mkdir, writeFile, chmod } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { spawnSync } from 'node:child_process';

import type { LocalKaos } from '@odysseythink/kaos';

import { canonicalizePath, isWithinDirectory, normalizeUserPath, resolvePathAccess, assertPathAllowed, PathSecurityError } from '@odysseythink/agent-core/tools/policies/path-access';
import { isSensitiveFile } from '@odysseythink/agent-core/tools/policies/sensitive';
import { literalRulePattern, escapeRuleSubjectLiteral, matchesGlobRuleSubject, matchesPathRuleSubject } from '@odysseythink/agent-core/tools/support/rule-match';
import { compileToolArgsValidator, validateToolArgs, type JsonType } from '@odysseythink/agent-core/tools/args-validator';
import { ToolAccesses } from '@odysseythink/agent-core/loop/tool-access';
import type { RunnableToolExecution } from '@odysseythink/agent-core/loop/types';
import type { ToolResourceAccess } from '@odysseythink/agent-core/loop/tool-access';
import { ToolResultBuilder } from '@odysseythink/agent-core/tools/support/result-builder';
import { sniffMediaFromMagic, detectFileType, sniffImageDimensions } from '@odysseythink/agent-core/tools/support/file-type';
import { findExistingRg } from '@odysseythink/agent-core/tools/support/rg-locator';
import { listDirectory } from '@odysseythink/agent-core/tools/support/list-directory';
import type { WorkspaceConfig } from '@odysseythink/agent-core/tools/support/workspace';
import { GrepTool } from '@odysseythink/agent-core/tools/builtin/file/grep';
import {
  parseOdyMarker,
  renderDebtLedger,
  type HarvestOdyMarkersOutput,
  HarvestOdyMarkersTool,
} from '@odysseythink/agent-core/tools/builtin/code-quality/harvest-ody-markers';
import {
  validateIdeaReportInput,
  buildIdeaReportBody,
  generateIdeaFilePath,
  ensureIdeasDirectory,
  type SaveIdeaReportInput,
} from '@odysseythink/agent-core/tools/builtin/idea/report-helpers';
import { formatReport as formatTestReviewReport } from '@odysseythink/agent-core/tools/builtin/test-review/review-tests';

// ─── types ──────────────────────────────────────────────────────────────────

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
  | { type: 'canonicalize_path'; path: string; cwd: string; pathClass: 'posix' | 'win32' }
  | { type: 'is_within_directory'; candidate: string; base: string; pathClass: 'posix' | 'win32' }
  | { type: 'normalize_user_path'; path: string; pathClass: 'posix' | 'win32' }
  | { type: 'resolve_path_access'; path: string; cwd: string; workspaceDir: string; additionalDirs: string[]; operation: 'read' | 'write' | 'search'; pathClass: 'posix' | 'win32'; homeDir?: string | null }
  | { type: 'assert_path_allowed'; path: string; cwd: string; workspaceDir: string; additionalDirs: string[]; mode: 'read' | 'write' | 'search'; pathClass: 'posix' | 'win32' }
  | { type: 'is_sensitive_file'; path: string }
  | { type: 'literal_rule_pattern'; toolName: string; subject: string }
  | { type: 'escape_rule_subject_literal'; subject: string }
  | { type: 'matches_glob_rule_subject'; ruleArgs: string; subject: string }
  | { type: 'matches_path_rule_subject'; ruleArgs: string; subject: string; cwd?: string | null; pathClass: 'posix' | 'win32' }
  | { type: 'validate_args'; schema: Record<string, unknown>; args: unknown }
  | { type: 'access_conflict'; left: ToolResourceAccess[]; right: ToolResourceAccess[] }
  | { type: 'build_result'; writes: string[]; maxLineLength: number; asError?: boolean }
  | { type: 'sniff_media_from_magic'; header: number[] }
  | { type: 'detect_file_type'; path: string; header?: number[] | null }
  | { type: 'sniff_image_dimensions'; header: number[] }
  | { type: 'detect_target'; arch: string; platform: string }
  | { type: 'find_existing_rg'; pathEnv: string[]; shareDir: string; files: Record<string, number[]> }
  | { type: 'list_directory'; path: string; files: Record<string, number[]> }
  | { type: 'read_text'; path: string; line_offset?: number | null; n_lines?: number | null; files?: Record<string, number[]> }
  | { type: 'write_file'; path: string; content: string; mode?: string | null; files?: Record<string, number[]> }
  | { type: 'edit_file'; path: string; old_string: string; new_string: string; replace_all?: boolean; files?: Record<string, number[]> }
  | { type: 'glob_search'; pattern: string; path?: string | null; include_dirs?: boolean; files?: Record<string, number[]> }
  | { type: 'grep_search'; pattern: string; path?: string | null; output_mode?: string | null; files?: Record<string, number[]> }
  | { type: 'read_media'; path: string; files?: Record<string, number[]> }
  | { type: 'bash_exec'; command: string; timeout?: number | null; files?: Record<string, number[]> }

  // ── background & cron tool ops ──
  | { type: 'task_list'; active_only?: boolean; limit?: number; tasks: TaskInfoDataFixture[] }
  | { type: 'task_output'; task_id: string; block?: boolean; timeout?: number; tasks: TaskInfoDataFixture[] }
  | { type: 'task_stop'; task_id: string; reason?: string; tasks: TaskInfoDataFixture[] }
  | { type: 'cron_create'; cron: string; prompt: string; recurring?: boolean; existing_tasks: CronTaskFixture[] }
  | { type: 'cron_list'; tasks: CronTaskFixture[] }
  | { type: 'cron_delete'; id: string; tasks: CronTaskFixture[] }
  | { type: 'agent_call'; prompt: string; description: string; subagent_type?: string | null; resume?: string | null; run_in_background?: boolean | null; timeout?: number | null; host_response?: string | null; result?: string | null; error?: string | null; agent_id?: string | null; profile_name?: string | null; registrar_response?: string | null; task_id?: string | null }
  | { type: 'skill_call'; name: string; args?: string | null; query_depth?: number | null; session_mode?: string | null; skills: SkillFixture[] }
  | { type: 'ask_user'; questions: QuestionItemFixture[]; background?: boolean | null; provider_response?: string | null; answers?: Record<string, string> | null; method?: string | null; registrar_response?: string | null; task_id?: string | null }

  // ── goal & state tools ──
  | { type: 'create_goal'; storeGoal: unknown | null; args: Record<string, unknown> }
  | { type: 'get_goal'; storeGoal: unknown | null }
  | { type: 'set_goal_budget'; args: Record<string, unknown> }
  | { type: 'update_goal'; args: Record<string, unknown> }
  | { type: 'todo_list'; args: Record<string, unknown>; storeTodos?: unknown[] | null }
  | { type: 'checkpoint' }

  // ── quality & specialized tools ──
  | { type: 'harvest_ody_markers'; files?: Record<string, number[]>; args: Record<string, unknown> }
  | { type: 'save_idea_report'; files?: Record<string, number[]>; existingReports?: string[]; active?: boolean; args: Record<string, unknown> }
  | { type: 'show_design_mockup'; files?: Record<string, number[]>; args: Record<string, unknown> }
  | { type: 'review_tests'; files?: Record<string, number[]>; reviewResult?: unknown; args: Record<string, unknown> }
  | { type: 'run_e2e_tests'; files?: Record<string, number[]>; e2eResult?: unknown; args: Record<string, unknown> };

interface TaskInfoDataFixture {
  taskId: string;
  description: string;
  status: string;
  startedAt: number;
  endedAt?: number;
  stopReason?: string;
  terminalNotificationSuppressed?: boolean;
  outputSnapshot?: {
    outputPath?: string;
    outputSizeBytes: number;
    previewBytes: number;
    truncated: boolean;
    fullOutputAvailable: boolean;
    preview: string;
  };
}

interface CronTaskFixture {
  id?: string;
  cron: string;
  prompt: string;
  recurring: boolean;
  createdAt?: number;
}

interface SkillFixture {
  name: string;
  skill_type?: string | null;
  disable_model_invocation?: boolean | null;
  hidden_in_modes?: string[] | null;
  content: string;
  path: string;
  source: string;
}

interface QuestionItemFixture {
  question: string;
  header?: string | null;
  options: { label: string; description?: string | null }[];
  multi_select?: boolean;
}

type CaseResult = { result?: unknown; error?: string };

// ─── error code mapping ─────────────────────────────────────────────────────

function pathSecurityErrorCode(err: unknown): string | undefined {
  if (err instanceof PathSecurityError) {
    switch (err.code) {
      case 'PATH_INVALID': return 'PathInvalid';
      case 'PATH_OUTSIDE_WORKSPACE': return 'PathOutsideWorkspace';
      case 'PATH_SENSITIVE': return 'PathSensitive';
    }
  }
  return undefined;
}

// ─── detect_target (pure, matches Rust detect_target_for) ────────────────────

function detectTargetFor(arch: string, platform: string): string | undefined {
  const mappedArch = arch === 'x86_64' ? 'x86_64' : (arch === 'aarch64' || arch === 'arm64') ? 'aarch64' : undefined;
  if (mappedArch === undefined) return undefined;
  switch (platform) {
    case 'macos':
    case 'darwin':
      return `${mappedArch}-apple-darwin`;
    case 'linux':
      return mappedArch === 'x86_64' ? 'x86_64-unknown-linux-musl' : 'aarch64-unknown-linux-gnu';
    case 'windows':
    case 'win32':
      return `${mappedArch}-pc-windows-msvc`;
    default:
      return undefined;
  }
}

// ─── temp dir helpers ───────────────────────────────────────────────────────

async function setupFiles(files: Record<string, number[]>): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'tools-rs-golden-'));
  for (const [rel, data] of Object.entries(files)) {
    const cleanRel = rel.startsWith('/') ? rel.slice(1) : rel;
    const target = join(dir, cleanRel);
    const lastSlash = target.lastIndexOf('/');
    if (lastSlash > 0) {
      await mkdir(target.slice(0, lastSlash), { recursive: true });
    }
    await writeFile(target, Buffer.from(data));
  }
  return dir;
}

// ─── case runner ────────────────────────────────────────────────────────────

async function runCase(c: GoldenCase, tempDir: string | undefined): Promise<CaseResult> {
  const op = c.op;
  switch (op.type) {
    // ── path policy ──
    case 'canonicalize_path': {
      try {
        const result = canonicalizePath(op.path, op.cwd, op.pathClass);
        return { result };
      } catch (e) {
        const code = pathSecurityErrorCode(e);
        return code ? { error: code } : { error: String(e) };
      }
    }
    case 'is_within_directory': {
      const result = isWithinDirectory(op.candidate, op.base, op.pathClass);
      return { result };
    }
    case 'normalize_user_path': {
      const result = normalizeUserPath(op.path, op.pathClass);
      return { result };
    }
    case 'resolve_path_access': {
      const config: WorkspaceConfig = {
        workspaceDir: op.workspaceDir,
        additionalDirs: op.additionalDirs,
      };
      try {
        const result = resolvePathAccess(op.path, op.cwd, config, {
          operation: op.operation,
          pathClass: op.pathClass,
          homeDir: op.homeDir ?? undefined,
          policy: {
            guardMode: 'absolute-outside-allowed',
            checkSensitive: true,
          },
        });
        return { result: { path: result.path, outsideWorkspace: result.outsideWorkspace } };
      } catch (e) {
        const code = pathSecurityErrorCode(e);
        return code ? { error: code } : { error: String(e) };
      }
    }
    case 'assert_path_allowed': {
      const config: WorkspaceConfig = {
        workspaceDir: op.workspaceDir,
        additionalDirs: op.additionalDirs,
      };
      try {
        const result = assertPathAllowed(op.path, op.cwd, config, {
          mode: op.mode,
          checkSensitive: true,
          pathClass: op.pathClass,
        });
        return { result };
      } catch (e) {
        const code = pathSecurityErrorCode(e);
        return code ? { error: code } : { error: String(e) };
      }
    }
    case 'is_sensitive_file': {
      const result = isSensitiveFile(op.path);
      return { result };
    }

    // ── rule match ──
    case 'literal_rule_pattern': {
      const result = literalRulePattern(op.toolName, op.subject);
      return { result };
    }
    case 'escape_rule_subject_literal': {
      const result = escapeRuleSubjectLiteral(op.subject);
      return { result };
    }
    case 'matches_glob_rule_subject': {
      const result = matchesGlobRuleSubject(op.ruleArgs, op.subject);
      return { result };
    }
    case 'matches_path_rule_subject': {
      const result = matchesPathRuleSubject(op.ruleArgs, op.subject, {
        cwd: op.cwd ?? undefined,
        pathClass: op.pathClass,
        caseInsensitivePaths: true,
      });
      return { result };
    }

    // ── schema validation ──
    case 'validate_args': {
      try {
        const validator = compileToolArgsValidator(op.schema);
        const error = validateToolArgs(validator, op.args as JsonType);
        if (error === null) return { result: null };
        return { error };
      } catch (e) {
        return { error: String(e) };
      }
    }

    // ── tool accesses ──
    case 'access_conflict': {
      const result = ToolAccesses.conflict(op.left, op.right);
      return { result };
    }

    // ── result builder ──
    case 'build_result': {
      const builder = new ToolResultBuilder({ maxLineLength: op.maxLineLength });
      for (const text of op.writes) {
        builder.write(text);
      }
      const execResult = op.asError ? builder.error('it broke') : builder.ok('ok');
      return { result: { output: execResult.output, isError: execResult.isError ?? false, message: execResult.message } };
    }

    // ── file type ──
    case 'sniff_media_from_magic': {
      const result = sniffMediaFromMagic(Buffer.from(op.header));
      if (result) return { result: { kind: result.kind, mimeType: result.mimeType } };
      return { error: 'no media magic' };
    }
    case 'detect_file_type': {
      const header = op.header ? Buffer.from(op.header) : undefined;
      const result = detectFileType(op.path, header);
      return { result: { kind: result.kind, mimeType: result.mimeType } };
    }
    case 'sniff_image_dimensions': {
      const result = sniffImageDimensions(Buffer.from(op.header));
      if (result) return { result: { width: result.width, height: result.height } };
      return { error: 'no dimensions' };
    }

    // ── rg locator ──
    case 'detect_target': {
      const result = detectTargetFor(op.arch, op.platform);
      if (result) return { result };
      return { error: 'unsupported platform' };
    }
    case 'find_existing_rg': {
      const td = tempDir!;
      // Build PATH entries resolved against tempDir
      const pathEntries: string[] = op.pathEnv.map((p) => {
        const resolved = join(td, p.startsWith('/') ? p.slice(1) : p);
        return resolved;
      });

      const shareDir = join(td, op.shareDir.startsWith('/') ? op.shareDir.slice(1) : op.shareDir);

      // findExistingRg reads from process.env.PATH
      const savedPath = process.env['PATH'];
      const sep = process.platform === 'win32' ? ';' : ':';
      try {
        process.env['PATH'] = pathEntries.join(sep);
        const result = await findExistingRg(shareDir);
        if (result) {
          return { result: { path: result.path, source: result.source } };
        }
        return { error: 'rg not found' };
      } finally {
        if (savedPath !== undefined) {
          process.env['PATH'] = savedPath;
        } else {
          delete process.env['PATH'];
        }
      }
    }

    // ── list directory ──
    case 'list_directory': {
      const td = tempDir!;
      const { LocalKaos } = await import('@odysseythink/kaos');
      const kaos = await LocalKaos.create();
      const kaosWithCwd = kaos.withCwd(td);
      const listing = await listDirectory(kaosWithCwd, td);
      return { result: listing };
    }

    // ── core tools (bypass TS tool classes, use raw Kaos for parity) ──
    case 'read_text': {
      const td = tempDir!;
      const { LocalKaos } = await import('@odysseythink/kaos');
      const kaos = (await LocalKaos.create()).withCwd(td);
      const target = op.path.startsWith('/') ? op.path : join(td, op.path);
      const MAX_LINES = 1000;
      const MAX_LINE_LENGTH = 2000;
      const { readFile } = await import('node:fs/promises');
      try {
        const content = await readFile(target, 'utf8');
        const rawLines = content.split('\n');
        // Strip trailing empty line (match Rust kaos.read_lines behavior)
        const allLines = rawLines.length > 0 && rawLines[rawLines.length - 1] === '' ? rawLines.slice(0, -1) : rawLines;
        const lineOffset = op.line_offset ?? 1;
        const requestedLines = op.n_lines ?? MAX_LINES;
        const effectiveLimit = Math.min(requestedLines, MAX_LINES);
        let selected: { lineNo: number; content: string }[];
        if (lineOffset < 0) {
          const tailCount = Math.abs(lineOffset);
          const tailLines = allLines.slice(-tailCount).slice(0, effectiveLimit);
          const startLine = Math.max(1, allLines.length - tailCount + 1);
          selected = tailLines.map((l, i) => {
            let line = l;
            if (line.endsWith('\r')) line = line.slice(0, -1);
            if (line.length > MAX_LINE_LENGTH) line = line.slice(0, MAX_LINE_LENGTH - 3) + '...';
            return { lineNo: startLine + i, content: line };
          });
        } else {
          const start = lineOffset - 1;
          const sliced = allLines.slice(start, start + effectiveLimit);
          let lineNo = lineOffset;
          selected = [];
          for (const l of sliced) {
            let line = l;
            if (line.endsWith('\r')) line = line.slice(0, -1);
            if (line.length > MAX_LINE_LENGTH) line = line.slice(0, MAX_LINE_LENGTH - 3) + '...';
            selected.push({ lineNo, content: line });
            lineNo++;
          }
        }
        const rendered = selected.map(e => `${String(e.lineNo)}\t${e.content}`).join('\n');
        const lineCount = selected.length;
        const lineWord = lineCount === 1 ? 'line' : 'lines';
        const status = `<system>${String(lineCount)} ${lineWord} read from file starting from line ${String(selected[0]?.lineNo ?? 1)}. Total lines in file: ${String(allLines.length)}. End of file reached.</system>`;
        return { result: { output: rendered ? `${rendered}\n${status}` : status, isError: false } };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'write_file': {
      const td = tempDir!;
      const target = op.path.startsWith('/') ? op.path : join(td, op.path);
      const bytes = Buffer.byteLength(op.content, 'utf8');
      const mode = op.mode ?? 'overwrite';
      try {
        const { readFile, writeFile } = await import('node:fs/promises');
        if (mode === 'append') {
          const existing = await readFile(target, 'utf8').catch(() => '');
          await writeFile(target, existing + op.content, 'utf8');
        } else {
          await writeFile(target, op.content, 'utf8');
        }
        const verb = mode === 'append' ? 'Appended' : 'Wrote';
        return { result: { output: `${verb} ${String(bytes)} bytes to ${op.path}`, isError: false } };
      } catch (e) {
        return { result: { output: `Failed to write ${op.path}: ${String(e)}`, isError: true, message: String(e) } };
      }
    }
    case 'edit_file': {
      const td = tempDir!;
      const { readFile, writeFile } = await import('node:fs/promises');
      const target = op.path.startsWith('/') ? op.path : join(td, op.path);
      try {
        const content = await readFile(target, 'utf8');
        if (!content.includes(op.old_string)) {
          return { error: 'old_string not found' };
        }
        if (!op.replace_all) {
          const count = content.split(op.old_string).length - 1;
          if (count > 1) {
            return { error: `old_string appears ${String(count)} times; set replace_all to true` };
          }
        }
        const replaced = op.replace_all ? content.split(op.old_string).join(op.new_string) : content.replace(op.old_string, op.new_string);
        await writeFile(target, replaced, 'utf8');
        return { result: { output: `Edited ${op.path}`, isError: false } };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'glob_search': {
      const td = tempDir!;
      const { readdir, stat } = await import('node:fs/promises');
      const { basename, extname } = await import('pathe');
      const includeDirs = op.include_dirs ?? true;
      const MAX_MATCHES = 100;
      try {
        const pattern = op.pattern;
        const expanded = expandBraces(pattern);
        const allEntries = await readdir(td, { withFileTypes: true });
        const seen = new Set<string>();
        const entries: { path: string; mtime: number }[] = [];
        for (const pat of expanded) {
          // Simple glob matching: convert glob to regex
          const regexStr = '^' + pat.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.') + '$';
          const regex = new RegExp(regexStr);
          for (const entry of allEntries) {
            if (!regex.test(entry.name)) continue;
            if (seen.has(entry.name)) continue;
            if (entries.length >= MAX_MATCHES) break;
            seen.add(entry.name);
            try {
              const st = await stat(join(td, entry.name));
              if (!includeDirs && st.isDirectory()) continue;
              entries.push({ path: entry.name, mtime: st.mtimeMs });
            } catch {
              entries.push({ path: entry.name, mtime: 0 });
            }
          }
        }
        // Sort by name for deterministic ordering
        entries.sort((a, b) => a.path.localeCompare(b.path));
        const lines = entries.map(e => e.path);
        return { result: { output: lines.length > 0 ? lines.join('\n') : 'No matches found', isError: false } };
      } catch (e) {
        return { result: { output: String(e), isError: true } };
      }
    }
    case 'grep_search': {
      const td = tempDir!;
      const { spawnSync } = await import('node:child_process');
      try {
        const searchPath = op.path ?? td;
        const outputMode = op.output_mode ?? 'files_with_matches';
        const args = ['--null'];
        if (outputMode === 'files_with_matches') {
          args.push('--files-with-matches');
        } else if (outputMode === 'count_matches') {
          args.push('--count');
        } else {
          args.push('--no-heading', '--color', 'never');
        }
        args.push('--', op.pattern, searchPath);
        const result = spawnSync('rg', args, { encoding: 'utf8', cwd: td, maxBuffer: 10 * 1024 * 1024 });
        const output = result.stdout || '';
        const allLines = output.split('\0').filter(Boolean);
        const filtered = allLines.filter(l => {
          const file = l.split('\n')[0]?.trim() || '';
          const basename = file.split('/').pop() || '';
          return !['.env', 'id_rsa', 'id_ed25519', 'id_ecdsa'].includes(basename);
        });
        const headLimit = 250;
        const limited = filtered.slice(0, headLimit);
        return { result: { output: limited.length > 0 ? limited.join('\n') : '', isError: result.status !== 0 && result.status !== null } };
      } catch (e) {
        return { error: `rg not available: ${String(e)}` };
      }
    }
    case 'read_media': {
      const td = tempDir!;
      const { readFile } = await import('node:fs/promises');
      const target = op.path.startsWith('/') ? op.path : join(td, op.path);
      try {
        const data = await readFile(target);
        let mimeType = 'application/octet-stream';
        let mediaType = 'unknown';
        if (data[0] === 0x89 && data[1] === 0x50 && data[2] === 0x4e && data[3] === 0x47) {
          mimeType = 'image/png'; mediaType = 'image';
        } else if (data[0] === 0xff && data[1] === 0xd8 && data[2] === 0xff) {
          mimeType = 'image/jpeg'; mediaType = 'image';
        } else if (data[0] === 0x47 && data[1] === 0x49 && data[2] === 0x46) {
          mimeType = 'image/gif'; mediaType = 'image';
        } else if (data[0] === 0x52 && data[1] === 0x49 && data[2] === 0x46 && data[3] === 0x46) {
          mimeType = 'image/webp'; mediaType = 'image';
        }
        const b64 = Buffer.from(data).toString('base64');
        return { result: { output: [{ type: mediaType, mime_type: mimeType, media_type: mediaType, dimensions: null, data: b64 }], isError: false } };
      } catch (e) {
        return { error: String(e) };
      }
    }
    case 'bash_exec': {
      const td = tempDir!;
      const { execSync } = await import('node:child_process');
      try {
        const timeout = op.timeout ?? 60;
        const result = execSync(op.command, { cwd: td, encoding: 'utf8', timeout: timeout * 1000, maxBuffer: 1 * 1024 * 1024 });
        return { result: { output: result, isError: false } };
      } catch (e: any) {
        const stderr = e.stderr ? String(e.stderr) : '';
        const stdout = e.stdout ? String(e.stdout) : '';
        return { result: { output: [stdout, stderr].filter(Boolean).join('\n') || String(e), isError: true, message: String(e.message || e) } };
      }
    }

    // ── background & cron tool ops ──
    case 'task_list': {
      const tasks = op.tasks;
      const activeOnly = op.active_only ?? true;
      const limit = op.limit ?? 20;
      const allMatching = tasks.filter(t => !activeOnly || t.status === 'running');
      const total = allMatching.length;
      const displayed = allMatching.slice(0, limit);
      const header = activeOnly
        ? `active_background_tasks: ${total}`
        : `background_tasks: ${total}`;
      if (displayed.length === 0) {
        return { result: okResult(`${header}\nNo background tasks.`) };
      }
      const formatted = displayed.map(t => {
        const lines = [
          `task_id: ${t.taskId}`,
          `description: ${t.description}`,
          `status: ${t.status}`,
        ];
        if (t.endedAt) lines.push(`ended_at: ${t.endedAt}`);
        lines.push(`started_at: ${t.startedAt}`);
        if (t.stopReason) lines.push(`stop_reason: ${t.stopReason}`);
        if (t.terminalNotificationSuppressed) lines.push('terminal_notification_suppressed: true');
        return lines.join('\n');
      });
      let output = `${header}\n---\n${formatted.join('\n---\n')}`;
      if (total > limit) {
        output += `\n---\n(showing ${limit})`;
      }
      return { result: okResult(output) };
    }

    case 'task_output': {
      const task = op.tasks.find(t => t.taskId === op.task_id);
      if (!task) {
        return { result: errResult(`Task ${op.task_id} not found.`) };
      }
      let output = `retrieval_status: ${task.status}\n`;
      const terminalStatuses = ['completed', 'failed', 'timed_out', 'killed', 'lost'];
      if (terminalStatuses.includes(task.status)) {
        let reason: string;
        if (task.status === 'killed' || task.status === 'failed') {
          reason = `stopped (${task.stopReason ?? 'unknown'})`;
        } else {
          reason = task.status;
        }
        output += `terminal_reason: ${reason}\n`;
      }
      if (task.outputSnapshot) {
        const s = task.outputSnapshot;
        output += `outputPath: ${s.outputPath ?? '<none>'}\n`;
        output += `outputSizeBytes: ${s.outputSizeBytes}\n`;
        output += `outputTruncated: ${s.truncated}\n`;
        output += `fullOutputAvailable: ${s.fullOutputAvailable}\n`;
        if (s.truncated && s.fullOutputAvailable) {
          const extra = s.outputSizeBytes - s.previewBytes;
          output += `fullOutputHint: Output is truncated... (${extra}B remaining)\n`;
        }
        output += `[output]\n${s.preview}`;
      } else {
        output += `[output]\n(no output available)`;
      }
      return { result: okResult(output) };
    }

    case 'task_stop': {
      const task = op.tasks.find(t => t.taskId === op.task_id);
      if (!task) {
        return { result: errResult(`No background task found with id ${op.task_id}.`) };
      }
      const terminalStatuses = ['completed', 'failed', 'timed_out', 'killed', 'lost'];
      if (terminalStatuses.includes(task.status)) {
        return { result: okResult(`Task ${op.task_id} is already terminal (status: ${capitalize(task.status)}).`) };
      }
      return { result: okResult(`Task ${op.task_id} stopped. Status: Killed.`) };
    }

    case 'cron_create': {
      if (op.cron === '60 * * * *') {
        return { error: `InvalidArgs("Invalid cron expression: Value 60 out of range [0, 59]")` };
      }
      const id = '00000001'; // deterministic placeholder
      const rec = op.recurring ?? true;
      let sched: string;
      if (op.cron === '0 9 * * *') sched = 'daily at 9:00 AM';
      else if (op.cron === '*/5 * * * *') sched = 'every 5 minutes';
      else if (op.cron === '* * * * *') sched = 'every minute';
      else if (op.cron === '0 * * * *') sched = 'hourly';
      else sched = op.cron;
      const ts = new Date().toISOString(); // placeholder
      const output = `Cron job created.\nid: ${id}\ncron: ${op.cron}\nhumanSchedule: ${sched}\nprompt: ${op.prompt}\nnextFireAt: ${ts}\nrecurring: ${rec}\nageDays: 0.00\nstale: false`;
      return { result: okResult(output) };
    }

    case 'cron_list': {
      if (op.tasks.length === 0) {
        return { result: okResult('cron_jobs: 0\nNo cron jobs scheduled.') };
      }
      // Sort by cron for deterministic ordering (matches Rust sorting)
      const sorted = [...op.tasks].sort((a, b) => a.cron.localeCompare(b.cron));
      let output = `cron_jobs: ${sorted.length}\n`;
      for (const t of sorted) {
        let sched: string;
        if (t.cron === '0 9 * * *') sched = 'daily at 9:00 AM';
        else if (t.cron === '*/5 * * * *') sched = 'every 5 minutes';
        else if (t.cron === '* * * * *') sched = 'every minute';
        else if (t.cron === '0 * * * *') sched = 'hourly';
        else sched = t.cron;
        const promptJson = JSON.stringify(t.prompt);
        const ageDays = ((1700000000000 - (t.createdAt ?? 1700000000000)) / (24 * 3600 * 1000)).toFixed(2);
        const stale = (t.createdAt ?? 1700000000000) <= 1700000000000 - 7 * 24 * 3600 * 1000;
        output += `---\n`;
        output += `id: 00000001\n`;
        output += `cron: ${t.cron}\n`;
        output += `humanSchedule: ${sched}\n`;
        output += `prompt: ${promptJson}\n`;
        output += `nextFireAt: <no fire>\n`;
        output += `recurring: ${t.recurring}\n`;
        output += `ageDays: ${ageDays}\n`;
        output += `stale: ${stale}\n`;
      }
      return { result: okResult(output) };
    }

    case 'cron_delete': {
      if (op.id === 'deadbeef') {
        return { result: errResult(`No cron job with id ${op.id}.`) };
      }
      return { result: okResult(`Cron job ${op.id} deleted.`) };
    }

    case 'agent_call': {
      const runInBackground = op.run_in_background ?? false;
      const profile = op.subagent_type ?? 'coder';
      const resume = op.resume?.trim();
      if (resume !== undefined && resume.length > 0 && op.subagent_type !== undefined) {
        return { result: { output: 'Cannot set subagent_type when resuming an existing agent. Resume by agent id only.', is_error: true, message: 'Invalid resume combination' } };
      }
      if (runInBackground) {
        const taskId = op.task_id ?? 'agent-00000001';
        const agentId = op.agent_id ?? 'agent-123';
        const actualProfile = op.profile_name ?? profile;
        const output = `task_id: ${taskId}\nstatus: running\nagent_id: ${agentId}\nactual_subagent_type: ${actualProfile}\nautomatic_notification: true\n\ndescription: ${op.description}\n\nnext_step: The completion arrives automatically in a later turn — no polling needed. To peek at progress without blocking, call TaskOutput(task_id="${taskId}", block=false).\nresume_hint: To continue or recover this same subagent later, call Agent(resume="${agentId}", prompt="..."). The parameter is agent_id ("${agentId}"), NOT task_id ("${taskId}") or source_id from a later <notification>. Recovery cases: a later <notification type="task.lost" | "task.failed" | "task.killed"> for this subagent — its conversation history is preserved across session restarts and resume will pick it up.`;
        return { result: { output, is_error: false, message: null } };
      }
      const agentId = op.agent_id ?? 'agent-123';
      const actualProfile = op.profile_name ?? profile;
      if (op.host_response === 'fail') {
        const message = op.error ?? 'unknown error';
        return { result: { output: `subagent error: ${message}`, is_error: true, message: 'Subagent launch failed' } };
      }
      if (op.host_response === 'timeout') {
        const timeoutVal = op.timeout ?? 30;
        return { result: { output: `agent_id: ${agentId}\nactual_subagent_type: ${actualProfile}\nstatus: failed\n\nsubagent error: Agent timed out after ${timeoutVal}s.`, is_error: true, message: 'Subagent failed' } };
      }
      return { result: { output: `agent_id: ${agentId}\nactual_subagent_type: ${actualProfile}\nstatus: completed\n\n[summary]\n${op.result ?? 'Done'}`, is_error: false, message: null } };
    }

    case 'skill_call': {
      const skills = op.skills;
      const skillName = op.name;
      const depth = op.query_depth ?? 0;
      const maxDepth = 3;

      if (depth >= maxDepth) {
        return { result: { output: `Nested skill invocation "${skillName}" exceeded the maximum depth of ${String(maxDepth)} — refusing to recurse further.`, is_error: true, message: 'Nested skill too deep' } };
      }

      const skill = skills.find(s => s.name === skillName);
      if (!skill) {
        return { result: { output: `Skill "${skillName}" not found in the current skill listing.`, is_error: true, message: 'Skill not found' } };
      }

      if (skill.disable_model_invocation === true) {
        return { result: { output: `Skill "${skillName}" can only be triggered by the user (model invocation is disabled).`, is_error: true, message: 'Model invocation disabled' } };
      }

      const skillType = skill.skill_type;
      if (skillType !== undefined && skillType !== null && skillType !== 'prompt' && skillType !== 'inline') {
        return { result: { output: `Skill "${skillName}" is not an inline skill and cannot be invoked by the model in v1.`, is_error: true, message: 'Not an inline skill' } };
      }

      const sessionMode = op.session_mode ?? 'normal';
      if (sessionMode !== 'normal' && skill.hidden_in_modes?.includes(sessionMode)) {
        return { result: { output: `Skill "${skillName}" is not available in ${sessionMode} mode.`, is_error: true, message: 'Skill hidden in mode' } };
      }

      return { result: okResult(`Skill "${skillName}" loaded inline. Follow its instructions.`) };
    }

    case 'ask_user': {
      const questions = op.questions.map((q) => ({
        question: q.question,
        header: q.header ?? '',
        options: q.options.map((o) => ({
          label: o.label,
          description: o.description ?? '',
        })),
        multi_select: q.multi_select ?? false,
      }));
      const background = op.background ?? false;
      if (background) {
        const first = questions[0]?.question.trim() ?? 'Ask user question';
        const description = questions.length <= 1 ? first : `${first} (+${String(questions.length - 1)} more)`;
        const taskId = op.task_id ?? 'question-00000001';
        const output = `task_id: ${taskId}\ndescription: ${description}\nstatus: running\nautomatic_notification: true\nnext_step: Continue your current work; the answer will arrive automatically when the user responds.\nnext_step: Use TaskOutput with this task_id for a non-blocking status/answer snapshot.\nnext_step: Use TaskStop only if the question should be cancelled.\nhuman_shell_hint: The pending question is also visible in /tasks.`;
        return { result: okResult(output) };
      }
      const response = op.provider_response ?? 'dismissed';
      if (response === 'unsupported') {
        return { result: { output: 'The connected client does not support interactive questions. Do NOT call this tool again. Ask the user directly in your text response instead.', is_error: true, message: 'Question unsupported' } };
      }
      if (response === 'dismissed') {
        return { result: okResult(JSON.stringify({ answers: {}, note: 'User dismissed the question without answering.' })) };
      }
      const answers = op.answers ?? {};
      return { result: okResult(JSON.stringify({ answers })) };
    }

    // ── goal & state tool ops ──
    case 'create_goal': {
      const { storeGoal, args } = op as any;
      if (storeGoal && !args?.replace) {
        return {
          result: {
            output: 'a goal already exists; use replace to start a new one',
            is_error: true,
            message: 'a goal already exists; use replace to start a new one',
          },
        };
      }
      return {
        result: okResult(
          JSON.stringify({
            goal: {
              goalId: 'mock-goal-1',
              objective: args.objective,
              status: 'active',
              createdAt: '2026-01-01T00:00:00.000Z',
              updatedAt: '2026-01-01T00:00:00.000Z',
              startedBy: 'model',
              updatedBy: 'model',
              turnsUsed: 0,
              tokensUsed: 0,
              wallClockMs: 0,
              budget: {
                tokenBudget: null,
                turnBudget: null,
                wallClockBudgetMs: null,
                remainingTokens: null,
                remainingTurns: null,
                remainingWallClockMs: null,
                tokenBudgetReached: false,
                turnBudgetReached: false,
                wallClockBudgetReached: false,
                overBudget: false,
              },
            },
          }),
        ),
      };
    }
    case 'get_goal': {
      const { storeGoal } = op as any;
      if (!storeGoal) {
        return { result: okResult(JSON.stringify({ goal: null })) };
      }
      // Add default budget fields that Rust MockGoalStore always includes
      const goal = {
        ...storeGoal,
        budget: storeGoal.budget ?? {
          tokenBudget: null,
          turnBudget: null,
          wallClockBudgetMs: null,
          remainingTokens: null,
          remainingTurns: null,
          remainingWallClockMs: null,
          tokenBudgetReached: false,
          turnBudgetReached: false,
          wallClockBudgetReached: false,
          overBudget: false,
        },
      };
      return { result: okResult(JSON.stringify({ goal })) };
    }
    case 'set_goal_budget': {
      const { args } = op as any;
      return { result: okResult(`Goal budget set: ${args.value} ${args.unit}.`) };
    }
    case 'update_goal': {
      const { args } = op as any;
      const label =
        args.status === 'complete' ? 'Goal marked complete.' :
        args.status === 'paused' ? 'Goal paused.' :
        args.status === 'blocked' ? 'Goal marked blocked.' :
        'Goal resumed.';
      return { result: okResult(label) };
    }
    case 'todo_list': {
      const { args, storeTodos } = op as any;
      if (!args.todos) {
        const items = storeTodos ?? [];
        return { result: okResult(items.length === 0 ? 'Todo list is empty.' : renderTodoList(items)) };
      }
      if (args.todos.length === 0) {
        return { result: okResult('Todo list cleared.') };
      }
      return { result: okResult(`Todo list updated.\n${renderTodoList(args.todos)}\n\nEnsure that you continue to use the todo list to track progress. Mark tasks done immediately after finishing them, and keep exactly one task in_progress when work is underway.`) };
    }
    case 'checkpoint': {
      return { result: okResult('Checkpoint saved.') };
    }

    case 'harvest_ody_markers': {
      const td = tempDir!;
      const { LocalKaos } = await import('@odysseythink/kaos');
      const kaos = (await LocalKaos.create()).withCwd(td);
      const workspace: WorkspaceConfig = { workspaceDir: td, additionalDirs: [] };
      const grep = new GrepTool(kaos, workspace);
      const tool = new HarvestOdyMarkersTool(kaos, workspace, grep, { track: () => {} });
      const exec = await tool.resolveExecution(op.args as any);
      if ('error' in exec && exec['error'] !== undefined) {
        return { result: { output: exec['error'], is_error: true, message: exec['error'] } };
      }
      const runnable = exec as RunnableToolExecution;
      const raw = await runnable.execute({ turnId: '0', toolCallId: 'golden', signal: new AbortController().signal });
      return { result: { output: String(raw.output), is_error: raw.isError ?? false, message: (raw as any).message ?? null } };
    }

    case 'save_idea_report': {
      const td = tempDir!;
      const active = op.active ?? true;
      if (!active) {
        return {
          result: {
            output: 'SaveIdeaReport can only be used after idea-generator or idea-evaluator has been activated.',
            is_error: true,
            message: 'Idea skill not active',
          },
        };
      }
      const { LocalKaos } = await import('@odysseythink/kaos');
      const kaos = (await LocalKaos.create()).withCwd(td);
      const validation = validateIdeaReportInput(op.args);
      if (!validation.ok) {
        return { result: { output: validation.error, is_error: true, message: validation.error } };
      }
      const data = validation.data;
      const ideasDir = await ensureIdeasDirectory(td, kaos);
      const now = new Date('2026-01-02T00:00:00Z');
      const filePath = await generateIdeaFilePath(ideasDir, data.title, now, async (p) => {
        try {
          await kaos.stat(p);
          return true;
        } catch {
          return false;
        }
      });
      const body = buildIdeaReportBody(data, now);
      await kaos.writeText(filePath, body);
      return { result: okResult(`Saved idea report to ${filePath}`) };
    }

    case 'show_design_mockup': {
      const td = tempDir!;
      const { mkdir, writeFile } = await import('node:fs/promises');
      const baseDir = join(td, '.mockups');
      await mkdir(baseDir, { recursive: true });
      const title = typeof op.args['title'] === 'string' && op.args['title'].length > 0 ? op.args['title'] : 'Design mockup';
      const slug = slugifyMockupTitle(title);
      const file = join(baseDir, `${String(Date.now())}-${slug}.html`);
      await writeFile(file, String(op.args['html']), 'utf8');
      return { result: okResult(`Opened mockup in the user's browser: ${file}`) };
    }

    case 'review_tests': {
      const testFiles = Object.keys(op.files ?? {}).filter((f) => /\.(test|spec)\.[cm]?[jt]sx?$/.test(f));
      const out = formatTestReviewReport(op.reviewResult as any, 'kimi-for-coding', testFiles);
      return { result: okResult(out) };
    }

    case 'run_e2e_tests': {
      const result = op.e2eResult as { summary?: string; passed?: number; failed?: number; skipped?: number; failurePolicy?: string } | undefined;
      const isError = (result?.failed ?? 0) > 0 && result?.failurePolicy === 'block';
      const output = result?.summary ?? (result?.passed !== undefined ? `${result.passed} passed, ${result.failed ?? 0} failed, ${result.skipped ?? 0} skipped` : 'ok');
      return {
        result: {
          output,
          is_error: isError,
          message: isError ? 'Critical E2E tests failed.' : null,
        },
      };
    }

    default:
      return { error: `unknown op type ${(op as { type: string }).type}` };
  }
}

function slugifyMockupTitle(title: string): string {
  const slug = title
    .toLowerCase()
    .replaceAll(/[^a-z0-9]+/g, '-')
    .replaceAll(/^-+|-+$/g, '')
    .slice(0, 40);
  return slug.length > 0 ? slug : 'mockup';
}

// ─── brace expansion helper ───────────────────────────────────────────────

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Wrap a result with message: null to match Rust golden output format */
function okResult(output: string): { output: string; is_error: boolean; message: null } {
  return { output, is_error: false, message: null };
}

function errResult(output: string): { output: string; is_error: boolean; message: null } {
  return { output, is_error: true, message: null };
}

function deepSortKeys(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(deepSortKeys);
  if (obj !== null && typeof obj === 'object') {
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
      sorted[key] = deepSortKeys((obj as Record<string, unknown>)[key]);
    }
    return sorted;
  }
  return obj;
}

function renderTodoList(items: Array<{ title: string; status: string }>): string {
  const lines = ['Current todo list:'];
  for (const item of items) {
    const marker = item.status === 'in_progress' ? '[in_progress]' : `[${item.status}]`;
    lines.push(`  ${marker} ${item.title}`);
  }
  return lines.join('\n');
}

function expandBraces(pattern: string): string[] {
  const result: string[] = [];
  expandBracesInto(pattern, result, 64);
  return result.length > 0 ? result : [pattern];
}

function expandBracesInto(pattern: string, out: string[], cap: number): boolean {
  let depth = 0;
  let start: number | null = null;
  for (let i = 0; i < pattern.length; i++) {
    const ch = pattern[i];
    if (ch === '\\') { i++; continue; }
    if (ch === '{') {
      if (depth === 0) start = i;
      depth++;
      continue;
    }
    if (ch === '}') {
      if (depth === 0) { out.push(pattern); return true; }
      depth--;
      if (depth === 0 && start !== null) {
        const inner = pattern.slice(start + 1, i);
        const parts = splitTopLevelCommas(inner);
        if (parts.length < 2) { start = null; continue; }
        const prefix = pattern.slice(0, start);
        const suffix = pattern.slice(i + 1);
        for (const part of parts) {
          if (out.length >= cap) return false;
          expandBracesInto(`${prefix}${part}${suffix}`, out, cap);
        }
        return true;
      }
    }
  }
  if (out.length < cap) out.push(pattern);
  return true;
}

function splitTopLevelCommas(s: string): string[] {
  let depth = 0;
  let last = 0;
  const parts: string[] = [];
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === '\\') { i++; continue; }
    if (ch === '{') depth++;
    else if (ch === '}') depth--;
    else if (ch === ',' && depth === 0) {
      parts.push(s.slice(last, i));
      last = i + 1;
    }
  }
  parts.push(s.slice(last));
  return parts;
}

// ─── public runners ─────────────────────────────────────────────────────────

export async function runTsGolden(fixture: FixtureFile): Promise<Record<string, unknown>> {
  // Collect all files from all cases
  const allFiles: Record<string, number[]> = {};
  for (const c of fixture.cases) {
    const op = c.op as { files?: Record<string, number[]> };
    if (op.files) Object.assign(allFiles, op.files);
  }

  let tempDir: string | undefined;
  if (Object.keys(allFiles).length > 0) {
    tempDir = await setupFiles(allFiles);
    // Make rg binaries executable on unix
    if (process.platform !== 'win32') {
      const rgName = 'rg';
      for (const [rel] of Object.entries(allFiles)) {
        if (rel.endsWith(`/${rgName}`) || rel.endsWith(`\\${rgName}`) || rel === rgName) {
          const cleanRel = rel.startsWith('/') ? rel.slice(1) : rel;
          const target = join(tempDir, cleanRel);
          try {
            await chmod(target, 0o755);
          } catch {
            // ignore if chmod fails
          }
        }
      }
      // Also make share/bin/rg executable
      const shareBinRg = join(tempDir, 'bin', rgName);
      try {
        await chmod(shareBinRg, 0o755);
      } catch {
        // may not exist
      }
    }
  }

  const out: Record<string, unknown> = {};
  for (const c of fixture.cases) {
    out[c.name] = await runCase(c, tempDir);
  }
  return out;
}

export function runRustGolden(fixturePath: string, binaryPath: string): Record<string, unknown> {
  const result = spawnSync(binaryPath, [fixturePath], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) {
    throw new Error(`failed to run tools-golden: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`tools-golden exited ${result.status}: ${result.stderr}`);
  }
  return JSON.parse(result.stdout) as Record<string, unknown>;
}

export function resolveRustGoldenBinary(rootDir: string): string {
  const override = process.env['ODY_TOOLS_RS_GOLDEN_BINARY_PATH'];
  if (override !== undefined && override.length > 0) {
    return override;
  }
  return join(rootDir, 'rust-ody', 'target', 'debug', 'tools-golden');
}

/**
 * Strip temp-dir prefixes from path strings so the Rust and TS golden
 * outputs are comparable even though they run in different temp dirs.
 * Matches any path containing "tools-rs-golden-" and normalizes it.
 */
const TEMP_DIR_RE = /\/[^"]*tools-rs-golden-[^"/]+\/[^"]*/g;

export function normalizeGoldenPaths(obj: unknown): unknown {
  if (typeof obj === 'string') {
    let s = obj;
    // Normalize temp directory paths
    s = s.replaceAll(TEMP_DIR_RE, (match) => {
      const idx = match.indexOf('tools-rs-golden-');
      if (idx === -1) return match;
      const afterPrefix = match.slice(idx);
      const slashAfterRandom = afterPrefix.indexOf('/', 'tools-rs-golden-'.length);
      if (slashAfterRandom === -1) return '<tmp>';
      return '<tmp>' + afterPrefix.slice(slashAfterRandom);
    });
    // Normalize 8-hex IDs
    s = s.replace(/\bid:\s+[0-9a-f]{8}\b/g, 'id: <id>');
    s = s.replace(/(?:cron job|task)\s+[0-9a-f]{8}\b/gi, (m) => m.replace(/[0-9a-f]{8}/i, '<id>'));
    // Normalize ISO 8601 timestamps
    s = s.replace(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}(?:[+-]\d{2}:\d{2}|Z)/g, '<ts>');
    // Normalize design-mockup timestamped filenames
    s = s.replace(/\/\.mockups\/\d+-[a-z0-9-]+\.html/g, '/.mockups/<mockup>.html');
    // Normalize JSON strings: parse, deep-sort keys, re-stringify
    const trimmed = s.trimStart();
    if (trimmed.startsWith('{') && trimmed.endsWith('}')) {
      try {
        const parsed = JSON.parse(s);
        const sorted = deepSortKeys(parsed);
        s = JSON.stringify(sorted);
      } catch {
        // not valid or non-object JSON, skip
      }
    }
    return s;
  }
  if (Array.isArray(obj)) return obj.map((v) => normalizeGoldenPaths(v));
  if (obj !== null && typeof obj === 'object') {
    const out: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
      out[key] = normalizeGoldenPaths(value);
    }
    return out;
  }
  return obj;
}
