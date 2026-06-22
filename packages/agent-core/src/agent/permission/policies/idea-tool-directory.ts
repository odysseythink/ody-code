import { join, normalize } from 'pathe';

import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

export class IdeaToolDirectoryApprovePermissionPolicy implements PermissionPolicy {
  readonly name = 'idea-tool-directory-approve';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const cwd = this.agent.config.cwd;
    if (cwd.length === 0) return;

    const ideasDir = normalize(join(cwd, '.ody-code', 'ideas'));
    const prefix = ideasDir.endsWith('/') ? ideasDir : `${ideasDir}/`;

    const accesses = context.execution.accesses ?? [];
    let foundWriteUnderIdeas = false;
    for (const access of accesses) {
      if (access.kind !== 'file') continue;
      if (access.operation !== 'write' && access.operation !== 'readwrite') continue;
      const normalizedPath = normalize(access.path);
      if (!normalizedPath.startsWith(prefix)) {
        return;
      }
      foundWriteUnderIdeas = true;
    }

    if (!foundWriteUnderIdeas) return;
    return { kind: 'approve' };
  }
}
