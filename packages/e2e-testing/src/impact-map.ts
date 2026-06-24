/**
 * Static map from tool class name to repo‑relative file paths that indicate
 * changes likely to affect the tool. Paths are matched with picomatch.
 */
export const TOOL_IMPACT_MAP: Record<string, string[]> = {
  ExitPlanModeTool: [
    'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    'packages/agent-core/src/agent/session-mode/index.ts',
    'packages/agent-core/src/agent/injection/plan-mode.ts',
  ],
  EnterPlanModeTool: [
    'packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts',
  ],
};
