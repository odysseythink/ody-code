import { z } from 'zod';
import type { Kaos } from '@odysseythink/kaos';
import type { BuiltinTool } from '#/agent/tool';
import type { ExecutableToolContext, ExecutableToolResult, RunnableToolExecution, ToolExecution } from '#/loop/types';
import type { TelemetryClient } from '#/telemetry';
import { resolvePathAccessPath } from '#/tools/policies/path-access';
import { toInputJsonSchema } from '@odysseythink/agent-core-shared';
import { literalRulePattern } from '#/tools/support/rule-match';
import type { WorkspaceConfig } from '#/tools/support/workspace';
import { GrepInputSchema, GrepTool } from '#/tools/builtin/file/grep';

export const MAX_MARKERS = 200;

// ---- Data models ----

export interface DebtLedgerMarker {
  readonly file: string;
  readonly line: number;
  readonly ceiling: string;
  readonly upgrade: string;
  readonly rot: boolean;
}

export const HarvestOdyMarkersInputSchema = z.object({
  path: z.string().optional().describe('Optional subdirectory or file to scan.'),
});

export type HarvestOdyMarkersInput = z.infer<typeof HarvestOdyMarkersInputSchema>;

export interface HarvestOdyMarkersOutput {
  readonly markdown: string;
  readonly markerCount: number;
  readonly rotRiskCount: number;
  readonly truncated: boolean;
}

// ---- Parser ----

const MARKER_RE =
  /^(.+?):(\d+):\s*(?:(?:\/\/)|#)\s*ody:\s*(.*)$/;

export function parseOdyMarker(rawLine: string): DebtLedgerMarker | null {
  const match = MARKER_RE.exec(rawLine);
  if (!match) return null;

  const file = match[1]!;
  const line = Number(match[2]);
  const body = match[3]!.trim();

  const commaIdx = body.indexOf(',');
  if (commaIdx === -1) {
    return { file, line, ceiling: body, upgrade: '', rot: true };
  }

  const ceiling = body.slice(0, commaIdx).trim();
  const upgrade = body.slice(commaIdx + 1).trim();
  const rot = upgrade === '';
  return { file, line, ceiling, upgrade, rot };
}

// ---- Renderer ----

export function renderDebtLedger(
  markers: readonly DebtLedgerMarker[],
  truncated: boolean,
): string {
  if (markers.length === 0) {
    return '未找到 `ody:` 债务标记。台账干净。';
  }

  const groups = new Map<string, DebtLedgerMarker[]>();
  for (const m of markers) {
    const list = groups.get(m.file);
    if (list) {
      list.push(m);
    } else {
      groups.set(m.file, [m]);
    }
  }

  const sortedFiles = [...groups.keys()].sort();

  const lines: string[] = [];
  for (const file of sortedFiles) {
    lines.push(`### ${file}`);
    const fileMarkers = groups.get(file)!;
    fileMarkers.sort((a, b) => a.line - b.line);
    for (const m of fileMarkers) {
      const rotTag = m.rot ? ' ⚠️ rot' : '';
      const upgradeDisplay = m.upgrade || '（未指定）';
      lines.push(
        `${m.file}:${m.line} — ${m.ceiling}。天花板：${m.ceiling}。升级：${upgradeDisplay}${rotTag}`,
      );
    }
    lines.push('');
  }

  const totalRot = markers.filter((m) => m.rot).length;
  lines.push(
    `**汇总**：${markers.length} 个标记，${totalRot} 个 rot 风险。`,
  );
  if (truncated) {
    lines.push('结果已截断至前 200 条；如需完整扫描，请指定更小的目录或文件。');
  }
  return lines.join('\n');
}

// ---- Tool ----

const TOOL_DESCRIPTION =
  'Scan the codebase for `// ody:` / `# ody:` simplification-debt markers and return a Chinese-first ledger report.';

export class HarvestOdyMarkersTool
  implements BuiltinTool<HarvestOdyMarkersInput>
{
  readonly name = 'HarvestOdyMarkers' as const;
  readonly description = TOOL_DESCRIPTION;
  readonly parameters: Record<string, unknown> =
    toInputJsonSchema(HarvestOdyMarkersInputSchema);

  constructor(
    private readonly kaos: Kaos,
    private readonly workspace: WorkspaceConfig,
    private readonly grepTool: GrepTool,
    private readonly telemetry: TelemetryClient,
  ) {}

  resolveExecution(args: HarvestOdyMarkersInput): ToolExecution {
    return {
      description: 'Harvesting ody: simplification debt markers',
      approvalRule: literalRulePattern(this.name, ''),
      execute: ({ signal }) => this.run(args, signal),
    };
  }

  private async run(
    args: HarvestOdyMarkersInput,
    signal: AbortSignal,
  ): Promise<ExecutableToolResult> {
    try {
      const scanPath = args.path ?? this.workspace.workspaceDir;

      const resolvedPath = resolvePathAccessPath(scanPath, {
        kaos: this.kaos,
        workspace: this.workspace,
        operation: 'search',
        policy: {
          guardMode: 'absolute-outside-allowed',
          checkSensitive: false,
        },
      });

      const grepInput = GrepInputSchema.parse({
        pattern: '(#|//) ?ody:',
        path: resolvedPath,
        output_mode: 'content',
        '-n': true,
        head_limit: MAX_MARKERS,
        include_ignored: false,
      });

      const grepExecution = this.grepTool.resolveExecution(grepInput);
      const grepCtx: ExecutableToolContext = {
        turnId: 'internal',
        toolCallId: 'internal',
        signal,
      };
      const grepResult = await (grepExecution as RunnableToolExecution).execute(grepCtx);

      if (grepResult.isError) {
        this.telemetry.track('debt_ledger_failed', {
          error: String(grepResult.output),
        });
        return {
          isError: true,
          output: `债务台账扫描失败：${String(grepResult.output)}`,
        };
      }

      const outputText = String(grepResult.output);
      const rawLines = outputText
        .split('\n')
        .filter((l: string) => l.trim() !== '');
      const markers = rawLines
        .map((line: string) => parseOdyMarker(line))
        .filter((m: DebtLedgerMarker | null): m is DebtLedgerMarker => m !== null);

      const truncated = outputText.includes('Results truncated');

      const markdown = renderDebtLedger(markers, truncated);

      this.telemetry.track('debt_ledger_harvested', {
        marker_count: markers.length,
        rot_risk_count: markers.filter((m: DebtLedgerMarker) => m.rot).length,
      });

      const output: HarvestOdyMarkersOutput = {
        markdown,
        markerCount: markers.length,
        rotRiskCount: markers.filter((m: DebtLedgerMarker) => m.rot).length,
        truncated,
      };

      return { isError: false, output: JSON.stringify(output) };
    } catch (error) {
      const message =
        error instanceof Error ? error.message : String(error);
      this.telemetry.track('debt_ledger_failed', { error: message });
      return {
        isError: true,
        output: `债务台账扫描失败：${message}`,
      };
    }
  }
}
