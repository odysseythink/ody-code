# 将 11 个技能内置到 ody 二进制文件 — Implementation Plan

**Goal:** 将 `.agents/skills/` 下 11 个技能迁移到 `packages/agent-core/src/skill/builtin/`，采用与 `mcp-config` 完全一致的模式（`.ts` + `.md` + `raw-text-plugin` 内联），确保 `ody` 单二进制文件在任何目录下运行时都能看到这些技能。

**Architecture:** 每个技能新增一个 `.ts` 模块在 `builtin/` 目录中，该模块通过 `import ... from './<skill-name>.md'` 将 Markdown 内容内联为字符串常量，在模块加载时调用 `parseSkillText` 解析为 `SkillDefinition`，并导出常量。`builtin/index.ts` 导入所有技能常量，在 `registerBuiltinSkills` 中依次调用 `registry.registerBuiltinSkill()` 注册。`registerBuiltinSkill` 默认不覆盖同名已有技能，因此先扫描到的用户/项目技能优先。

**Tech Stack:** TypeScript, vitest, tsdown, `raw-text-plugin`

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

### New files (22)
- `packages/agent-core/src/skill/builtin/dispatching-parallel-agents.md`
- `packages/agent-core/src/skill/builtin/dispatching-parallel-agents.ts`
- `packages/agent-core/src/skill/builtin/executing-plans.md`
- `packages/agent-core/src/skill/builtin/executing-plans.ts`
- `packages/agent-core/src/skill/builtin/finishing-a-development-branch.md`
- `packages/agent-core/src/skill/builtin/finishing-a-development-branch.ts`
- `packages/agent-core/src/skill/builtin/receiving-code-review.md`
- `packages/agent-core/src/skill/builtin/receiving-code-review.ts`
- `packages/agent-core/src/skill/builtin/requesting-code-review.md`
- `packages/agent-core/src/skill/builtin/requesting-code-review.ts`
- `packages/agent-core/src/skill/builtin/subagent-driven-development.md`
- `packages/agent-core/src/skill/builtin/subagent-driven-development.ts`
- `packages/agent-core/src/skill/builtin/sync-changelog.md`
- `packages/agent-core/src/skill/builtin/sync-changelog.ts`
- `packages/agent-core/src/skill/builtin/systematic-debugging.md`
- `packages/agent-core/src/skill/builtin/systematic-debugging.ts`
- `packages/agent-core/src/skill/builtin/test-driven-development.md`
- `packages/agent-core/src/skill/builtin/test-driven-development.ts`
- `packages/agent-core/src/skill/builtin/using-git-worktrees.md`
- `packages/agent-core/src/skill/builtin/using-git-worktrees.ts`
- `packages/agent-core/src/skill/builtin/verification-before-completion.md`
- `packages/agent-core/src/skill/builtin/verification-before-completion.ts`

### Modified files (2)
- `packages/agent-core/src/skill/builtin/index.ts`
- `packages/agent-core/test/skill/registry.test.ts`

### New test file (1)
- `packages/agent-core/test/skill/builtin-skills.test.ts`

### Deleted directories (11)
- `.agents/skills/dispatching-parallel-agents/`
- `.agents/skills/executing-plans/`
- `.agents/skills/finishing-a-development-branch/`
- `.agents/skills/receiving-code-review/`
- `.agents/skills/requesting-code-review/`
- `.agents/skills/subagent-driven-development/`
- `.agents/skills/sync-changelog/`
- `.agents/skills/systematic-debugging/`
- `.agents/skills/test-driven-development/`
- `.agents/skills/using-git-worktrees/`
- `.agents/skills/verification-before-completion/`

---

## Dependency Overview

```
Task 1 (Move .md files)
  |
  v
Task 2 (Create .ts modules) ───────┐
  |                                |
  v                                |
Task 3 (Update builtin/index.ts)   |
  |                                |
  v                                |
Task 4 (Write builtin-skills.test) │
  |                                │
  v                                │
Task 5 (Add override test)         │
  |                                │
  v                                │
Task 6 (Delete old dirs) <─────────┘
  |
  v
Task 7 (Full typecheck + build)
```

- Task 2 depends on Task 1（`.md` 文件必须先存在才能被 `.ts` 导入）
- Task 3 depends on Task 2（需要导入 `.ts` 模块中导出的常量）
- Task 4 depends on Task 3（测试需要 `builtin/index.ts` 导出的所有技能常量）
- Task 5 depends on Task 3（需要 `DISPATCHING_PARALLEL_AGENTS_SKILL` 常量）
- Task 6 depends on Task 1（必须先确认 `.md` 文件已成功复制到新位置）
- Task 7 depends on Task 3, 4, 5（需要所有代码和测试就绪后做全树验证）

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| 1 | 移动文件后，`.agents/skills/` 下原路径在文档或外部引用中失效 | 已用 `grep` 搜索全仓库，无引用 `.agents/skills/<name>/SKILL.md` 的文件 |
| 2 | 技能 `namespace: core` frontmatter 字段未被 `normalizeMetadata` 处理 | `normalizeMetadata` 只处理已知字段，未知字段原样保留，不影响功能 |
| 3 | Bundle 体积增加（11 个 markdown 文件内联） | 每个 skill 约 2–5KB，总计增加 <100KB，可忽略 |

---

## Spec-Coverage Table

| Spec 需求 | 覆盖任务 | 状态 |
|---|---|---|
| 将 11 个 SKILL.md 移动到 `builtin/` 目录 | Task 1 | covered |
| 为每个技能创建 `.ts` 模块（mcp-config 模式） | Task 2 | covered |
| 在 `builtin/index.ts` 注册所有新技能 | Task 3 | covered |
| 内置技能解析不失败，元数据正确 | Task 4 | covered |
| 用户技能优先覆盖内置技能 | Task 5 | covered |
| 清理旧 `.agents/skills/` 目录 | Task 6 | covered |
| 全树类型检查通过 | Task 7 | covered |
| 构建产物包含所有内置技能 | Task 7 | covered |
| 排除 `translate-docs` 等 4 个技能 | —（Out of scope）| no-op |
| 保留 `.agents/skills/` 扫描逻辑 | —（不涉及修改）| no-op |

---

### Task 1: 将 11 个 SKILL.md 文件移动到 builtin 目录

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/skill/builtin/dispatching-parallel-agents.md`
- Create: `packages/agent-core/src/skill/builtin/executing-plans.md`
- Create: `packages/agent-core/src/skill/builtin/finishing-a-development-branch.md`
- Create: `packages/agent-core/src/skill/builtin/receiving-code-review.md`
- Create: `packages/agent-core/src/skill/builtin/requesting-code-review.md`
- Create: `packages/agent-core/src/skill/builtin/subagent-driven-development.md`
- Create: `packages/agent-core/src/skill/builtin/sync-changelog.md`
- Create: `packages/agent-core/src/skill/builtin/systematic-debugging.md`
- Create: `packages/agent-core/src/skill/builtin/test-driven-development.md`
- Create: `packages/agent-core/src/skill/builtin/using-git-worktrees.md`
- Create: `packages/agent-core/src/skill/builtin/verification-before-completion.md`

**Steps:**

- [ ] 运行以下命令移动文件：

```bash
cd /Users/ranwei/workspace/ody-code
for name in \
  dispatching-parallel-agents \
  executing-plans \
  finishing-a-development-branch \
  receiving-code-review \
  requesting-code-review \
  subagent-driven-development \
  sync-changelog \
  systematic-debugging \
  test-driven-development \
  using-git-worktrees \
  verification-before-completion; do
  mv ".agents/skills/$name/SKILL.md" "packages/agent-core/src/skill/builtin/$name.md"
done
```

- [ ] 验证所有 11 个 `.md` 文件已存在于目标目录：

```bash
ls -1 packages/agent-core/src/skill/builtin/*.md | wc -l
# Expected output: 12（11 个新技能 + mcp-config.md）
```

- [ ] Commit：

```bash
git add packages/agent-core/src/skill/builtin/*.md
git rm -r .agents/skills/dispatching-parallel-agents \
  .agents/skills/executing-plans \
  .agents/skills/finishing-a-development-branch \
  .agents/skills/receiving-code-review \
  .agents/skills/requesting-code-review \
  .agents/skills/subagent-driven-development \
  .agents/skills/sync-changelog \
  .agents/skills/systematic-debugging \
  .agents/skills/test-driven-development \
  .agents/skills/using-git-worktrees \
  .agents/skills/verification-before-completion
git commit -m "chore: move 11 skill markdown files to builtin directory"
```

---

### Task 2: 创建 11 个内置技能的 `.ts` 模块

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/skill/builtin/dispatching-parallel-agents.ts`
- Create: `packages/agent-core/src/skill/builtin/executing-plans.ts`
- Create: `packages/agent-core/src/skill/builtin/finishing-a-development-branch.ts`
- Create: `packages/agent-core/src/skill/builtin/receiving-code-review.ts`
- Create: `packages/agent-core/src/skill/builtin/requesting-code-review.ts`
- Create: `packages/agent-core/src/skill/builtin/subagent-driven-development.ts`
- Create: `packages/agent-core/src/skill/builtin/sync-changelog.ts`
- Create: `packages/agent-core/src/skill/builtin/systematic-debugging.ts`
- Create: `packages/agent-core/src/skill/builtin/test-driven-development.ts`
- Create: `packages/agent-core/src/skill/builtin/using-git-worktrees.ts`
- Create: `packages/agent-core/src/skill/builtin/verification-before-completion.ts`

**Steps:**

- [ ] 使用以下脚本一次性创建全部 11 个 `.ts` 文件：

```bash
cd /Users/ranwei/workspace/ody-code
cat > /tmp/create-skills.sh <<'SCRIPT'
#!/bin/bash
set -e

declare -A skills=(
  ["dispatching-parallel-agents"]="DISPATCHING_PARALLEL_AGENTS"
  ["executing-plans"]="EXECUTING_PLANS"
  ["finishing-a-development-branch"]="FINISHING_A_DEVELOPMENT_BRANCH"
  ["receiving-code-review"]="RECEIVING_CODE_REVIEW"
  ["requesting-code-review"]="REQUESTING_CODE_REVIEW"
  ["subagent-driven-development"]="SUBAGENT_DRIVEN_DEVELOPMENT"
  ["sync-changelog"]="SYNC_CHANGELOG"
  ["systematic-debugging"]="SYSTEMATIC_DEBUGGING"
  ["test-driven-development"]="TEST_DRIVEN_DEVELOPMENT"
  ["using-git-worktrees"]="USING_GIT_WORKTREES"
  ["verification-before-completion"]="VERIFICATION_BEFORE_COMPLETION"
)

for skill in "${!skills[@]}"; do
  const="${skills[$skill]}"
  cat > "packages/agent-core/src/skill/builtin/$skill.ts" <<EOF
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import ${const}_BODY from './$skill.md';

const PSEUDO_PATH = 'builtin://$skill';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/$skill.md',
  skillDirName: '$skill',
  source: 'builtin',
  text: ${const}_BODY,
});

export const ${const}_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
EOF
  echo "Created packages/agent-core/src/skill/builtin/$skill.ts"
done
SCRIPT
bash /tmp/create-skills.sh
```

- [ ] 验证每个 `.ts` 文件内容正确，例如抽查 `dispatching-parallel-agents.ts`：

```bash
cat packages/agent-core/src/skill/builtin/dispatching-parallel-agents.ts
```

Expected content:

```typescript
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import DISPATCHING_PARALLEL_AGENTS_BODY from './dispatching-parallel-agents.md';

const PSEUDO_PATH = 'builtin://dispatching-parallel-agents';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/dispatching-parallel-agents.md',
  skillDirName: 'dispatching-parallel-agents',
  source: 'builtin',
  text: DISPATCHING_PARALLEL_AGENTS_BODY,
});

export const DISPATCHING_PARALLEL_AGENTS_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
```

- [ ] Commit：

```bash
git add packages/agent-core/src/skill/builtin/*.ts
git commit -m "chore: add built-in skill ts modules for 11 skills"
```

---

### Task 3: 更新 `builtin/index.ts` 导入并注册所有技能

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/src/skill/builtin/index.ts`（当前 8 行 → 新文件约 40 行）

**Steps:**

- [ ] 将 `packages/agent-core/src/skill/builtin/index.ts` 替换为以下内容：

```typescript
import type { SkillRegistry } from '../registry';
import { DISPATCHING_PARALLEL_AGENTS_SKILL } from './dispatching-parallel-agents';
import { EXECUTING_PLANS_SKILL } from './executing-plans';
import { FINISHING_A_DEVELOPMENT_BRANCH_SKILL } from './finishing-a-development-branch';
import { MCP_CONFIG_SKILL } from './mcp-config';
import { RECEIVING_CODE_REVIEW_SKILL } from './receiving-code-review';
import { REQUESTING_CODE_REVIEW_SKILL } from './requesting-code-review';
import { SUBAGENT_DRIVEN_DEVELOPMENT_SKILL } from './subagent-driven-development';
import { SYNC_CHANGELOG_SKILL } from './sync-changelog';
import { SYSTEMATIC_DEBUGGING_SKILL } from './systematic-debugging';
import { TEST_DRIVEN_DEVELOPMENT_SKILL } from './test-driven-development';
import { USING_GIT_WORKTREES_SKILL } from './using-git-worktrees';
import { VERIFICATION_BEFORE_COMPLETION_SKILL } from './verification-before-completion';

export function registerBuiltinSkills(registry: SkillRegistry): void {
  registry.registerBuiltinSkill(DISPATCHING_PARALLEL_AGENTS_SKILL);
  registry.registerBuiltinSkill(EXECUTING_PLANS_SKILL);
  registry.registerBuiltinSkill(FINISHING_A_DEVELOPMENT_BRANCH_SKILL);
  registry.registerBuiltinSkill(MCP_CONFIG_SKILL);
  registry.registerBuiltinSkill(RECEIVING_CODE_REVIEW_SKILL);
  registry.registerBuiltinSkill(REQUESTING_CODE_REVIEW_SKILL);
  registry.registerBuiltinSkill(SUBAGENT_DRIVEN_DEVELOPMENT_SKILL);
  registry.registerBuiltinSkill(SYNC_CHANGELOG_SKILL);
  registry.registerBuiltinSkill(SYSTEMATIC_DEBUGGING_SKILL);
  registry.registerBuiltinSkill(TEST_DRIVEN_DEVELOPMENT_SKILL);
  registry.registerBuiltinSkill(USING_GIT_WORKTREES_SKILL);
  registry.registerBuiltinSkill(VERIFICATION_BEFORE_COMPLETION_SKILL);
}

export {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
};
```

- [ ] 运行 `packages/agent-core` 的类型检查确认无编译错误：

```bash
cd packages/agent-core && pnpm typecheck
# Expected: 无错误，exit code 0
```

- [ ] Commit：

```bash
git add packages/agent-core/src/skill/builtin/index.ts
git commit -m "chore: register all 11 built-in skills in builtin/index.ts"
```

---

### Task 4: 编写 `builtin-skills.test.ts` 验证所有内置技能解析正确

**Depends on:** Task 3

**Files:**
- Create: `packages/agent-core/test/skill/builtin-skills.test.ts`

**Steps:**

- [ ] 创建测试文件 `packages/agent-core/test/skill/builtin-skills.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';
import {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
} from '../../src/skill/builtin';

const BUILTIN_SKILLS = [
  { skill: DISPATCHING_PARALLEL_AGENTS_SKILL, name: 'dispatching-parallel-agents' },
  { skill: EXECUTING_PLANS_SKILL, name: 'executing-plans' },
  { skill: FINISHING_A_DEVELOPMENT_BRANCH_SKILL, name: 'finishing-a-development-branch' },
  { skill: MCP_CONFIG_SKILL, name: 'mcp-config' },
  { skill: RECEIVING_CODE_REVIEW_SKILL, name: 'receiving-code-review' },
  { skill: REQUESTING_CODE_REVIEW_SKILL, name: 'requesting-code-review' },
  { skill: SUBAGENT_DRIVEN_DEVELOPMENT_SKILL, name: 'subagent-driven-development' },
  { skill: SYNC_CHANGELOG_SKILL, name: 'sync-changelog' },
  { skill: SYSTEMATIC_DEBUGGING_SKILL, name: 'systematic-debugging' },
  { skill: TEST_DRIVEN_DEVELOPMENT_SKILL, name: 'test-driven-development' },
  { skill: USING_GIT_WORKTREES_SKILL, name: 'using-git-worktrees' },
  { skill: VERIFICATION_BEFORE_COMPLETION_SKILL, name: 'verification-before-completion' },
];

describe('built-in skills', () => {
  it('has exactly 12 built-in skills', () => {
    expect(BUILTIN_SKILLS).toHaveLength(12);
  });

  it.each(BUILTIN_SKILLS)('skill "$name" has correct metadata', ({ skill, name }) => {
    expect(skill.name).toBe(name);
    expect(skill.source).toBe('builtin');
    expect(skill.path).toBe(`builtin://${name}`);
    expect(skill.dir).toBe(`builtin://${name}`);
    expect(skill.content.length).toBeGreaterThan(0);
    expect(skill.description.length).toBeGreaterThan(0);
  });

  it('all skills are sorted alphabetically in listSkills', () => {
    const { SkillRegistry } = require('../../src/skill/registry');
    const registry = new SkillRegistry();
    for (const { skill } of BUILTIN_SKILLS) {
      registry.registerBuiltinSkill(skill);
    }
    const listed = registry.listSkills();
    const names = listed.map((s) => s.name);
    const sorted = [...names].sort((a, b) => a.localeCompare(b));
    expect(names).toEqual(sorted);
  });
});
```

- [ ] 运行测试：

```bash
cd packages/agent-core && pnpm test -- test/skill/builtin-skills.test.ts
# Expected: 14 tests pass（1 + 12 + 1）
```

- [ ] Commit：

```bash
git add packages/agent-core/test/skill/builtin-skills.test.ts
git commit -m "test: verify all built-in skills parse and register correctly"
```

---

### Task 5: 在 `registry.test.ts` 追加用户技能覆盖内置技能的测试

**Depends on:** Task 3

**Files:**
- Modify: `packages/agent-core/test/skill/registry.test.ts`

**Steps:**

- [ ] 在 `registry.test.ts` 文件末尾、`sectionFor` 函数之前插入以下内容：

```typescript
describe('built-in skill shadowing', () => {
  it('user skill shadows built-in skill of the same name', () => {
    const { SkillRegistry } = require('../../src/skill/registry');
    const { DISPATCHING_PARALLEL_AGENTS_SKILL } = require('../../src/skill/builtin');
    const registry = new SkillRegistry();

    // Register a user skill first
    registry.register({
      name: 'dispatching-parallel-agents',
      description: 'user version',
      path: '/tmp/user/dispatching-parallel-agents/SKILL.md',
      dir: '/tmp/user/dispatching-parallel-agents',
      content: '',
      metadata: { type: 'prompt' },
      source: 'user',
    });

    // Then register the built-in skill
    registry.registerBuiltinSkill(DISPATCHING_PARALLEL_AGENTS_SKILL);

    const found = registry.getSkill('dispatching-parallel-agents');
    expect(found).toBeDefined();
    expect(found!.source).toBe('user');
    expect(found!.description).toBe('user version');
  });

  it('built-in skill registers when no user shadow exists', () => {
    const { SkillRegistry } = require('../../src/skill/registry');
    const { DISPATCHING_PARALLEL_AGENTS_SKILL } = require('../../src/skill/builtin');
    const registry = new SkillRegistry();

    registry.registerBuiltinSkill(DISPATCHING_PARALLEL_AGENTS_SKILL);

    const found = registry.getSkill('dispatching-parallel-agents');
    expect(found).toBeDefined();
    expect(found!.source).toBe('builtin');
    expect(found!.path).toBe('builtin://dispatching-parallel-agents');
  });
});
```

> 注意：插入位置在 `function sectionFor(...)` 之前，即在第 122–128 行之间。

- [ ] 运行 `registry.test.ts` 确保新测试和原有测试全部通过：

```bash
cd packages/agent-core && pnpm test -- test/skill/registry.test.ts
# Expected: 原有 5 个测试 + 新增 2 个测试 = 7 tests pass
```

- [ ] Commit：

```bash
git add packages/agent-core/test/skill/registry.test.ts
git commit -m "test: verify user skills shadow built-in skills"
```

---

### Task 6: 删除旧 `.agents/skills/` 目录

**Depends on:** Task 1（已确认 `.md` 文件成功移动）

**Files:**
- Delete: `.agents/skills/dispatching-parallel-agents/`
- Delete: `.agents/skills/executing-plans/`
- Delete: `.agents/skills/finishing-a-development-branch/`
- Delete: `.agents/skills/receiving-code-review/`
- Delete: `.agents/skills/requesting-code-review/`
- Delete: `.agents/skills/subagent-driven-development/`
- Delete: `.agents/skills/sync-changelog/`
- Delete: `.agents/skills/systematic-debugging/`
- Delete: `.agents/skills/test-driven-development/`
- Delete: `.agents/skills/using-git-worktrees/`
- Delete: `.agents/skills/verification-before-completion/`

**Steps：**

- [ ] 删除已迁移的 11 个目录：

```bash
cd /Users/ranwei/workspace/ody-code
for name in \
  dispatching-parallel-agents \
  executing-plans \
  finishing-a-development-branch \
  receiving-code-review \
  requesting-code-review \
  subagent-driven-development \
  sync-changelog \
  systematic-debugging \
  test-driven-development \
  using-git-worktrees \
  verification-before-completion; do
  rm -rf ".agents/skills/$name"
done
```

- [ ] 验证只保留被排除的 4 个技能：

```bash
ls -d .agents/skills/*/ | sort
# Expected:
# .agents/skills/gen-changesets/
# .agents/skills/gen-docs/
# .agents/skills/translate-docs/
# .agents/skills/write-tui/
```

- [ ] Commit：

```bash
git add .agents/skills/
git commit -m "chore: remove migrated skill directories from .agents/skills"
```

---

### Task 7: 全树类型检查与构建验证

**Depends on:** Task 3, 4, 5

**Files：** 无新增/修改（验证步骤）

**Steps：**

- [ ] 运行全工作区类型检查：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r --filter './packages/*' run typecheck
# Expected: 所有 packages 类型检查通过，exit code 0
```

- [ ] 运行 `packages/agent-core` 完整测试套件：

```bash
cd packages/agent-core && pnpm test
# Expected: 所有测试通过，exit code 0
```

- [ ] 构建 `apps/ody-code` 确认单二进制文件包含内置技能：

```bash
cd apps/ody-code && pnpm build
# Expected: tsdown 构建成功，exit code 0
```

- [ ] 手动验证：在构建产物中搜索内置技能字符串，确认 `.md` 内容被内联到 bundle：

```bash
grep -o 'builtin://dispatching-parallel-agents' apps/ody-code/dist/main.mjs | head -1
# Expected: 输出一行 `builtin://dispatching-parallel-agents`

grep 'Dispatching Parallel Agents' apps/ody-code/dist/main.mjs | head -1
# Expected: 输出包含该标题的一行，确认 markdown 内容已内联
```

- [ ] Commit（如有任何额外修复）：

```bash
# 如果类型检查或测试发现了需要修复的问题，修复后 commit
git commit -m "fix: resolve typecheck/build issues after skill builtin migration"
```

---

## Self-Review

- [ ] **1. Spec-coverage table:** 所有设计中的需求项都映射到了具体任务，无 GAP。
- [ ] **2. Placeholder scan:** 无 TODO/TBD，无 "implement later"，无 "similar to Task N" 引用。每个任务的代码、命令、预期输出均已完整给出。
- [ ] **3. No phantom tasks:** 每个任务都产生可验证的变更（文件创建/修改/删除、测试通过、构建成功）。无 `--allow-empty` 提交。
- [ ] **4. Dependency soundness:** 每个 `Depends on` 都指向更早的任务编号。Task 4/5 依赖 Task 3（符号已定义），Task 7 依赖 Task 3/4/5（代码和测试就绪）。
- [ ] **5. Caller & build soundness:** 本计划不修改任何共享签名/接口/类型。`builtin/index.ts` 的 `registerBuiltinSkills` 函数签名不变，只是内部增加了注册调用。`SkillRegistry` 的 `registerBuiltinSkill` 和 `register` 方法均未改动。Task 7 以全树 `pnpm -r typecheck` 结束。
- [ ] **6. Test-the-risk:**
  - Task 4 测试每个内置技能的 `name`/`source`/`path`/`content` 元数据，断言依赖的常量值来自 Task 2 中定义的 `PSEUDO_PATH` 和 `parseSkillText` 输出。
  - Task 5 测试用户覆盖行为：先 `register(userSkill)` 再 `registerBuiltinSkill(builtinSkill)`，断言 `getSkill` 返回 `source: 'user'`。这是对 `registerBuiltinSkill` → `register()`（不传 `replace`）→ `if (options.replace === true || !this.byName.has(key))` 逻辑的直接行为验证。
  - "must-survive" 场景：名为 `dispatching-parallel-agents` 的用户技能不会被内置技能覆盖 — 测试中先注册用户技能，断言最终 `source === 'user'`。
- [ ] **7. Type consistency:** 所有 `.ts` 模块使用 `SkillDefinition` 类型（来自 `../types`），`registerBuiltinSkills` 接收 `SkillRegistry` 参数（来自 `../registry`），与现有 `mcp-config.ts` 完全一致。
