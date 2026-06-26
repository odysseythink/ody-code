# 将 `.agents/skills` 技能内置到 ody 二进制文件

## Scope In/Out

### In
- 将 `.agents/skills/` 下 **11 个技能**的内置化迁移到 `packages/agent-core/src/skill/builtin/`
- 采用与现有 `mcp-config` 完全一致的模式（`.ts` + `.md`，`raw-text-plugin` 内联）
- 在 `SkillRegistry` 中通过 `registerBuiltinSkills` 统一注册
- 确保 `ody` 单二进制文件在任何目录下运行时都能看到这些技能
- 为每个内置技能添加基础测试，验证解析和注册不失败

### Out
- ~~`translate-docs`、`gen-changesets`、`gen-docs`、`write-tui`~~ — 用户明确排除 [C:USER]
- ~~修改 `apps/ody-code` 构建脚本或 `tsdown.define` 注入机制~~ — 复用现有的 `raw-text-plugin`，不需要新增构建时脚本 [C:USER]
- ~~禁用项目目录 `.agents/skills/` 扫描~~ — 用户撤销此要求，保留现有扫描逻辑 [C:USER]
- ~~新增 `SkillSource` 类型~~ — 复用现有的 `builtin` source [C:USER]
- ~~代码生成脚本自动生成 `.ts` 文件~~ — 保持手动编写的 `.ts` 文件与 mcp-config 一致 [C:USER]

---

## 方案对比

| # | 方案 | 描述 | 优点 | 缺点 |
|---|---|---|---|---|
| 1 | **逐技能复制 mcp-config 模式（推荐）** | 将每个 `SKILL.md` 移到 `packages/agent-core/src/skill/builtin/`，配一个 `.ts` 导入并解析，在 `index.ts` 统一注册 | 与现有代码完全一致；无需额外构建脚本；vitest 天然支持 | 需要移动文件；每个技能有少量样板代码 |
| 2 | 构建时 JSON 注入 | 不移动文件，在 `apps/ody-code` 构建时通过脚本读取 `.agents/skills/`，打包成 JSON 常量注入 bundle | 不改变文件位置 | 需要额外构建脚本；运行时解析逻辑更复杂；与 mcp-config 模式不一致 |

**选定方案：1** [C:USER]

---

## Architecture

```
.agents/skills/<skill-name>/SKILL.md
    |
    |  move
    v
packages/agent-core/src/skill/builtin/<skill-name>.md
packages/agent-core/src/skill/builtin/<skill-name>.ts   (new)
    |
    |  import + parseSkillText
    v
SkillDefinition { source: 'builtin', path: 'builtin://<skill-name>', ... }
    |
    |  registerBuiltinSkills(SkillRegistry)
    v
SkillRegistry.byName  Map<string, SkillDefinition>
```

### 数据流

1. **构建时**：`tsdown`（`apps/ody-code`）和 `vitest`（`packages/agent-core`）通过 `raw-text-plugin` 将 `.md` 文件内联为 JS 字符串常量。
2. **运行时**：`Session.loadSkills()` 调用 `registerBuiltinSkills(registry)`，后者遍历所有内置技能定义并调用 `registry.registerBuiltinSkill(skill)`。
3. **注册时**：`registerBuiltinSkill` 将 `source` 设为 `'builtin'`，然后调用 `register()`。如果 `byName` 中已存在同名技能（来自用户/项目目录扫描），**不覆盖** — 这实现了"用户技能优先"策略 [C:USER]。

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | 排除后的 11 个技能都只有单个 `SKILL.md` 文件，没有附属文件 | **High** | 如果有附属文件，mcp-config 模式会丢失它们 | 已用 `find .agents/skills -type f` 验证 [C:INFERRED] |
| 2 | `packages/agent-core/vitest.config.ts` 的 `rawTextPlugin` 能正确解析新 `.md` 导入 | **High** | 测试会失败 | 已读取 vitest.config.ts 确认 [C:INFERRED] |
| 3 | 内置技能的 `SKILL.md` frontmatter 格式正确，不需要运行时容错 | **High** | 运行时解析错误会阻止 Session 创建 | 这些文件已在生产中使用，且会在测试中被验证 [C:USER] |
| 4 | `registerBuiltinSkill` 的 `!byName.has(key)` 逻辑足以实现"用户技能覆盖内置技能" | **High** | 内置技能可能覆盖用户技能 | 代码已验证：`register` 方法在 `options.replace !== true && byName.has(key)` 时跳过覆盖 [C:INFERRED] |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 移动文件后，`.agents/skills/` 下原路径在文档或外部引用中失效 | Low | Low | 搜索并更新所有引用 `.agents/skills/<name>/SKILL.md` 的文档/脚本 |
| 2 | 技能内容中的 `namespace: core` frontmatter 字段未被 `normalizeMetadata` 处理，可能产生意外 metadata | Low | Low | 确认 `normalizeMetadata` 只处理已知字段，未知字段原样保留，不影响功能 |
| 3 | 构建产物体积增加（11 个 markdown 文件内联到 bundle） | Medium | Low | 每个 skill 约 2-5KB，总计增加 <100KB，可忽略 |

---

## Component Details

### 内置技能 `.ts` 文件模板

每个内置技能遵循与 `mcp-config.ts` 完全相同的结构 [C:UPSTREAM]：

```typescript
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import <SKILL_NAME>_BODY from './<skill-name>.md';

const PSEUDO_PATH = 'builtin://<skill-name>';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/<skill-name>.md',
  skillDirName: '<skill-name>',
  source: 'builtin',
  text: <SKILL_NAME>_BODY,
});

export const <SKILL_NAME>_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
```

**11 个要迁移的技能**（按字母顺序）：
1. `dispatching-parallel-agents`
2. `executing-plans`
3. `finishing-a-development-branch`
4. `receiving-code-review`
5. `requesting-code-review`
6. `subagent-driven-development`
7. `sync-changelog`
8. `systematic-debugging`
9. `test-driven-development`
10. `using-git-worktrees`
11. `verification-before-completion`

### `builtin/index.ts` 修改

```typescript
import type { SkillRegistry } from '../registry';
import { DISPATCHING_PARALLEL_AGENTS_SKILL } from './dispatching-parallel-agents';
import { EXECUTING_PLANS_SKILL } from './executing-plans';
// ... 其他 9 个
import { MCP_CONFIG_SKILL } from './mcp-config';

export function registerBuiltinSkills(registry: SkillRegistry): void {
  registry.registerBuiltinSkill(DISPATCHING_PARALLEL_AGENTS_SKILL);
  registry.registerBuiltinSkill(EXECUTING_PLANS_SKILL);
  // ... 其他 9 个
  registry.registerBuiltinSkill(MCP_CONFIG_SKILL);
}

export {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  // ... 其他 9 个
  MCP_CONFIG_SKILL,
};
```

---

## Call-site Integration

### 修改点 1：新增内置技能 `.ts` + `.md` 文件

**位置**：`packages/agent-core/src/skill/builtin/`
**操作**：为每个技能新建 `.ts` 和 `.md` 文件

### 修改点 2：更新 `builtin/index.ts`

**位置**：`packages/agent-core/src/skill/builtin/index.ts`
**当前**：约 8 行，只导入和注册 `MCP_CONFIG_SKILL`
**变更**：导入所有 11 个新技能，在 `registerBuiltinSkills` 中依次注册
**加载顺序影响**：`registerBuiltinSkill` 调用 `register()` 时不传 `replace: true`，所以先加载的用户/项目同名技能会保留，后注册的内置技能被跳过 [C:INFERRED]。

### 修改点 3：移动 `.agents/skills/` 中的技能文件

**位置**：`.agents/skills/<skill-name>/SKILL.md` → `packages/agent-core/src/skill/builtin/<skill-name>.md`
**操作**：将 11 个 `SKILL.md` 文件复制/移动到新的 builtin 目录
**原目录清理**：`.agents/skills/` 下保留被排除的 4 个技能目录（`translate-docs`、`gen-changesets`、`gen-docs`、`write-tui`），其余 11 个目录删除 [C:USER]

---

## Error & Degradation

| Error Class | Immediate Handling | Degradation Path | Recovery Condition |
|---|---|---|---|
| `SkillParseError`（frontmatter 格式错误） | 直接抛出，阻止 Session 创建 | 无 — 内置技能格式错误属于代码库 bug | 修复 `.md` 文件并重新构建 [C:USER] |
| `UnsupportedSkillTypeError` | 直接抛出，阻止 Session 创建 | 无 | 修复 `type` frontmatter 字段 |
| `FrontmatterError`（YAML 解析失败） | 直接抛出，阻止 Session 创建 | 无 | 修复 frontmatter YAML 语法 |

---

## Test Plan

### 测试 1：每个内置技能解析不失败

**文件**：`packages/agent-core/test/skill/builtin-skills.test.ts`（新建）
**断言**：
- 对每个内置技能 `SKILL_DEFINITION`，`expect(SKILL_DEFINITION.name).toBe('<expected-name>')`
- `expect(SKILL_DEFINITION.source).toBe('builtin')`
- `expect(SKILL_DEFINITION.path).toBe('builtin://<skill-name>')`
- `expect(SKILL_DEFINITION.content.length).toBeGreaterThan(0)`

### 测试 2：用户技能覆盖内置技能

**文件**：`packages/agent-core/test/skill/registry.test.ts`（追加到现有测试文件）
**断言**：
- 先 `registry.register({ name: 'dispatching-parallel-agents', source: 'user', ... })`
- 再 `registry.registerBuiltinSkill(DISPATCHING_PARALLEL_AGENTS_SKILL)`
- `expect(registry.getSkill('dispatching-parallel-agents')?.source).toBe('user')`

### 测试 3：内置技能在技能列表中可见

**断言**：
- `registry.listSkills()` 返回的内置技能数量 ≥ 12（11 个新技能 + mcp-config）
- 每个内置技能的 `source` 为 `'builtin'`

### Done Criteria

```bash
# packages/agent-core 测试通过
cd packages/agent-core && pnpm test

# apps/ody-code 构建成功
cd apps/ody-code && pnpm build

# 构建后的 ody 二进制文件无需文件系统扫描即可列出内置技能
#（可通过集成测试或手动验证）
```

---

## Self-Review

### 1. 最高风险决策审查

**决策 A：使用 `registerBuiltinSkill` 的默认行为实现"用户技能优先"**

输入与期望输出：
1. **正常情况**：`byName` 为空 → `registerBuiltinSkill` 成功注册内置技能 ✅
2. **用户覆盖**：先注册 `source='user'` 的同名技能，再调用 `registerBuiltinSkill` → `byName` 保留用户技能，内置技能被跳过 ✅
3. ** adversarial 情况**：`registerBuiltinSkill` 被调用两次（重复注册）→ 第一次成功，第二次因 `!byName.has(key)` 为 false 被跳过，无异常 ✅

已验证：`registerBuiltinSkill` → `register(skill)`（不传 `replace`）→ `if (options.replace === true || !this.byName.has(key))` 注册。当 `byName` 已有同名技能时跳过。[C:INFERRED]

### 2. 四透镜审查

- **Security**：内置技能内容为纯文本 markdown，无敏感数据过滤需求。`skillMdPath` 使用伪路径 `/builtin/skills/<name>.md`，不会泄露真实文件系统路径 [C:INFERRED]。
- **Test**：每个技能有 `name`、`source`、`path`、`content` 断言；用户覆盖场景有 must-pass 测试；技能列表可见性有数量断言。无 must-reject 场景需要额外覆盖（内置技能解析失败即抛错，属于代码库 bug，不应在测试中模拟）。[C:INFERRED]
- **Ops**：11 个技能全部在 `Session` 构造时同步注册，无额外 I/O 延迟；每个技能增加 <5KB bundle 体积，总计 <100KB；伪路径 `builtin://<name>` 不会与真实文件系统路径冲突 [C:INFERRED]。
- **Integration**：`raw-text-plugin` 已在 `packages/agent-core/vitest.config.ts` 和 `apps/ody-code/tsdown.config.ts` 中配置；`registerBuiltinSkills` 已在 `Session.loadSkills()` 中被调用；`SkillRegistry.registerBuiltinSkill` 和 `register` 方法代码已核实 [C:INFERRED]。
- **Scope**：这是一个单一、连贯的子系统变更（技能内置化），不需要分解 [C:INFERRED]。
