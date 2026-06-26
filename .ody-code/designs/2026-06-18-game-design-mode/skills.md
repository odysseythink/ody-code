# Game Design Mode — 技能库集成

## Scope

本部分覆盖游戏设计技能库的构建时嵌入、命名空间 Skill 注册、可见性控制，以及 `skill.md` 核心内容向注入器的供给。

## 数据流

```
上游 .md 文件（33 个）
  → 构建脚本扫描并生成 packages/agent-core/src/skill/builtin/game-design/index.ts
  → index.ts import 每个 .md 作为字符串
  → 构建器（tsdown + raw-text-plugin）将 .md 内容内联到 bundle
  → registerBuiltinSkills() 调用 registerGameDesignSkills(registry)
  → 生成带 game-design/<name> 名称和 hiddenInModes 的 SkillDefinition
  → SkillRegistry 按名称索引
  → GameDesignInjector 调用 getModelSkillListing('game-design') 获取过滤后的清单
  → 模型通过 Skill 工具调用 game-design/<name>
```

## 文件布局

### 源文件目录

将上游 33 个 .md 文件复制到：

```
packages/agent-core/src/skill/builtin/game-design/sources/
├── skill.md                    # 主入口 [C:UPSTREAM]
├── index.md                    # 集合目录 [C:UPSTREAM]
├── game-design-methodology.md  # 22 个主模块 [C:UPSTREAM]
├── game-design-methodology--methodology-details.md  # 11 个 companion [C:UPSTREAM]
├── ...
└── game-team-management.md
```

### 生成文件

```
packages/agent-core/src/skill/builtin/game-design/index.ts   # 由脚本生成
```

生成文件结构：

```ts
import skillSource from './sources/skill.md';
import indexSource from './sources/index.md';
import gameDesignMethodologySource from './sources/game-design-methodology.md';
// ... 每个 .md 一个 import

export const GAME_DESIGN_SKILL: SkillDefinition;
export const GAME_DESIGN_INDEX_SKILL: SkillDefinition;
export const GAME_DESIGN_METHODOLOGY_SKILL: SkillDefinition;
// ...

export function registerGameDesignSkills(registry: SkillRegistry): void;
```

### 构建脚本

新增 `scripts/generate-game-design-skills.mjs`（或复用现有构建钩子）：

```
输入：packages/agent-core/src/skill/builtin/game-design/sources/*.md
输出：packages/agent-core/src/skill/builtin/game-design/index.ts

算法：
1. 读取 sources 目录下所有 .md 文件（排除非 .md）。
2. 对每个文件：
   a. stem ← basename(file, '.md')
   b. importName ← camelCase(stem) + 'Source'
   c. exportName ← 大写蛇形(stem) + '_SKILL'
   d. skillName ← stem === 'skill' ? 'game-design' : `game-design/${stem}`
3. 写入 index.ts：
   - import 所有 source
   - 对每个 source 调用 parseSkillText 得到 parsed
   - 构造 SkillDefinition：name=skillName, source='builtin', hiddenInModes=['normal','plan','design','office-hours']
   - export const GAME_DESIGN_xxx_SKILL
   - export function registerGameDesignSkills(registry) { registry.registerBuiltinSkill(GAME_DESIGN_xxx_SKILL) for each }
```

## 类型与接口

### 生成的 SkillDefinition 示例

```ts
const GAME_DESIGN_FLOW_STATE_SKILL: SkillDefinition = {
  name: 'game-design/flow-state-design-framework',
  description: '...',
  path: 'builtin://game-design/flow-state-design-framework',
  dir: 'builtin://game-design',
  content: flowStateDesignFrameworkSource,  // 内联字符串
  metadata: {
    name: 'game-design/flow-state-design-framework',
    description: '...',
    type: 'inline',
    hiddenInModes: ['normal', 'plan', 'design', 'office-hours'],
  },
  source: 'builtin',
};
```

### `skill.md` 主 Skill

```ts
const GAME_DESIGN_SKILL: SkillDefinition = {
  name: 'game-design',
  description: '游戏设计全流程助手...',
  // ...
  metadata: {
    name: 'game-design',
    description: '...',
    type: 'inline',
    hiddenInModes: ['normal', 'plan', 'design', 'office-hours'],
    disableModelInvocation: true,  // [C:INFERRED] 主 skill 不作为可调用 Skill，仅用于注入器读取其内容
  },
};
```

- `disableModelInvocation: true` 避免模型在 game-design 模式下意外调用 `game-design` 本身；注入器直接使用其 `content`。

### `index.md`

```ts
const GAME_DESIGN_INDEX_SKILL: SkillDefinition = {
  name: 'game-design/index',
  description: '游戏设计技能集合目录...',
  // ...
  metadata: {
    hiddenInModes: ['normal', 'plan', 'design', 'office-hours'],
    disableModelInvocation: true,  // [C:INFERRED] 目录仅作参考，不主动调用
  },
};
```

## 调用点

### 1. 注册 Builtin Skill

**文件**：`packages/agent-core/src/skill/builtin/index.ts`

```ts
import { registerGameDesignSkills } from './game-design';

export function registerBuiltinSkills(registry: SkillRegistry): void {
  // ... 现有 builtin skills ...
  registerGameDesignSkills(registry);
}
```

### 2. 注入器读取 `skill.md`

**文件**：`packages/agent-core/src/agent/injection/game-design-contract.ts`

```ts
import { GAME_DESIGN_SKILL } from '#/skill/builtin/game-design';

function gameDesignCoreWorkflow(): string {
  // 提取 skill.md 中 Phase 1-8 与输出规范部分
  return extractWorkflow(GAME_DESIGN_SKILL.content);
}
```

- [C:INFERRED] 注入器不直接注入完整 33 个文件，仅注入 `skill.md` 的核心流程与索引；子模块通过 Skill 清单让模型按需调用。

### 3. Skill 清单过滤

**文件**：`packages/agent-core/src/skill/registry.ts:153-160`

`getModelSkillListing('game-design')` 会自动调用 `listInvocableSkills('game-design')`，后者过滤掉 `disableModelInvocation` 与 `hiddenInModes` 包含 `'game-design'` 的 Skill。因此生成的子模块（`disableModelInvocation` 未设置、`hiddenInModes` 不含 `'game-design'`）会出现在清单中 [C:INFERRED]。

## 算法

### 构建脚本 `generateGameDesignSkillsIndex`

输入：`sourcesDir`
输出：`index.ts` 文件内容

```
1. files ← readdir(sourcesDir).filter(name.endsWith('.md')).sort()
2. imports ← []
3. exports ← []
4. registrations ← []
5. 对 files 中每个 file：
   a. stem ← basename(file, '.md')
   b. importName ← toCamelCase(stem) + 'Source'
   c. exportName ← toUpperSnake(stem) + '_SKILL'
   d. skillName ← stem === 'skill' ? 'game-design' : `game-design/${stem}`
   e. imports.push(`import ${importName} from './sources/${stem}.md';`)
   f. exports.push(buildSkillDefinition(exportName, skillName, importName))
   g. registrations.push(`registry.registerBuiltinSkill(${exportName});`)
6. 写入 index.ts：
   - import parseSkillText
   - import SkillDefinition type
   - imports
   - exports
   - export function registerGameDesignSkills(registry) { registrations }
```

### `buildSkillDefinition`

输入：`exportName`, `skillName`, `importName`
输出：TS 代码片段

```
1. parsedName ← `${exportName}_PARSED`
2. 输出：
   const parsedName = parseSkillText({
     skillMdPath: `builtin://game-design/${skillName}`,
     skillDirName: skillName,
     source: 'builtin',
     text: importName,
   });
   export const exportName: SkillDefinition = {
     ...parsedName,
     name: skillName,
     path: `builtin://game-design/${skillName}`,
     dir: 'builtin://game-design',
     metadata: {
       ...parsedName.metadata,
       name: skillName,
       type: parsedName.metadata.type ?? 'inline',
       hiddenInModes: ['normal', 'plan', 'design', 'office-hours'],
       ...(skillName === 'game-design' || skillName === 'game-design/index' ? { disableModelInvocation: true } : {}),
     },
   };
```

### `extractWorkflow`

输入：`skill.md` 完整内容
输出：注入器使用的 workflow 文本

```
1. 找到 "## 一、完整设计流程" 起始位置
2. 找到 "## 二、主题一致性检查" 或文件结束位置作为结束
3. 返回中间内容，保留 Markdown 表格与列表
4. 若未找到标记 → 返回完整内容（fallback）
```

## 错误处理

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|--------|---------|---------|---------|
| 构建时 sources 目录不存在 | 构建脚本抛出，CI 失败 | 无 | 确认上游文件已复制 |
| `.md` 文件 frontmatter 解析失败 | `parseSkillText` 抛出 `SkillParseError` | 无 | 修正上游文件格式 |
| 生成的 index.ts 类型错误 | TypeScript 编译失败 | 无 | 修正生成脚本 |
| 运行时 Skill 未找到 | SkillTool 返回错误结果 | 无 | 确认 skill 名称正确 |

## 测试断言

1. `packages/agent-core/test/skill/game-design-registration.test.ts`（新建）：
   - `registerGameDesignSkills(registry)` 后 `registry.getSkill('game-design/flow-state-design-framework')` 不为 undefined。
   - `registry.listInvocableSkills('normal')` 中不包含任何 `game-design/` 前缀 skill。
   - `registry.listInvocableSkills('game-design')` 包含至少 22 个主模块 skill。
   - `registry.getSkill('game-design').metadata.disableModelInvocation === true`。

2. `packages/agent-core/test/agent/injection/game-design-contract.test.ts`：
   - `gameDesignFullReminder(...)` 包含 `"game-design/flow-state-design-framework"` 或类似 skill 名称。
   - `gameDesignFullReminder(...)` 不包含 `"game-design/index"`（disableModelInvocation 的 skill 不出现在清单中）。

3. `packages/agent-core/test/skill/parser.test.ts` 新增（或单独测试）：
   - 生成的 `game-design/xxx` Skill 的 `metadata.hiddenInModes` 包含 `'normal'`、`'plan'`、`'design'`、`'office-hours'`。

4. 构建脚本测试：
   - 运行 `node scripts/generate-game-design-skills.mjs` 后 `packages/agent-core/src/skill/builtin/game-design/index.ts` 包含 33 个 `registerBuiltinSkill` 调用。
   - 运行 `pnpm build` 后 bundle 中包含技能 Markdown 字符串（可通过检查产物大小或搜索特定片段验证）。

## 本地说明

- 不采用运行时 `fs.readFile` 加载 .md，避免分发时文件依赖 [C:USER]。
- `raw-text-plugin.mjs` 已支持 `.md` import，因此生成文件只需普通 ESM import 即可在 build/test 中正确内联 [C:INFERRED]。
- 对 companion 文件（`--examples`、`--details` 等）统一使用 `game-design/<stem>` 名称，不根据 suffix 做特殊处理 [C:USER]。
- 主 `skill.md` 与 `index.md` 设为 `disableModelInvocation: true`，因为前者由注入器直接使用，后者仅作目录参考 [C:INFERRED]。
