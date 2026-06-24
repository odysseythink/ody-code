import picomatch from 'picomatch';
import type { ResolvedE2EConfig } from './config';
import type { AffectedTool, E2EPriority, ImpactAnalysisResult } from './types';
import { TOOL_IMPACT_MAP } from './impact-map';

function computePriority(toolId: string, criticalTools: string[]): E2EPriority {
  return criticalTools.includes(toolId) ? 'critical' : 'important';
}

function anyGlobMatches(globs: string[], file: string): boolean {
  const normalized = file.replace(/\\/g, '/');
  return globs.some((glob) => picomatch.isMatch(normalized, glob));
}

export class ImpactAnalyzer {
  static analyze(
    changedFiles: string[],
    config: ResolvedE2EConfig,
  ): ImpactAnalysisResult {
    const result = new Map<string, E2EPriority>();

    for (const file of changedFiles) {
      for (const [toolId, globs] of Object.entries(TOOL_IMPACT_MAP)) {
        if (anyGlobMatches(globs, file)) {
          const priority = computePriority(toolId, config.criticalTools);
          const current = result.get(toolId);
          const priorityOrder: E2EPriority[] = ['critical', 'important', 'nice-to-have'];
          if (current === undefined || priorityOrder.indexOf(priority) < priorityOrder.indexOf(current)) {
            result.set(toolId, priority);
          }
        }
      }
    }

    if (result.size === 0 && config.strategy === 'always') {
      result.set('general', 'nice-to-have');
    }

    const affected: AffectedTool[] = [];
    for (const [toolId, priority] of result) {
      if (config.strategy === 'critical-only' && priority !== 'critical') continue;
      affected.push({ toolId, priority });
    }

    return { affectedTools: affected };
  }
}
