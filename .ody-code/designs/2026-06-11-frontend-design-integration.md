# Frontend-Design 能力引入系统设计

## Scope In / Out

### In
- [C:USER] 将上游 `frontend-design` skill 引入本系统作为内置 skill
- [C:USER] Skill 仅在 **design mode** 下注入（`hiddenInModes: ['normal', 'plan']`）
- [C:USER] 新增 `/frontend-design` TUI slash command，仅在 design mode 下可见
- [C:USER] 命令触发后启动专用前端设计流程，产出 `DESIGN.md` 设计文档
- [C:USER] 动态附录选择：根据设计意图匹配推荐附录，同时展示全部供用户选择
- [C:USER] 预检清单（40+项）作为 DESIGN.md 必填章节，agent 逐项确认
- [C:USER] 支持 Cancel（退出清理）和 Pause（保留进度）两种退出策略
- [C:USER] DESIGN.md 存放在 `.ody-code/designs/` 目录，完全采用 `stitch.md` 格式规范
- [C:UPSTREAM] 上游 skill 的核心内容（Brief Inference、Three Dials、Design System Map、AI Tells 等）

### Out
- [C:USER] **不新增** `frontend-design` sessionMode（利用现有 design mode）
- [C:USER] **不**在 normal/plan mode 下暴露 skill 或命令
- [C:INFERRED] Block Library（上游已移除）不在本版本中实现
- [C:INFERRED] Dashboard / Data Table / Native Mobile 等 Out-of-Scope 场景不额外处理
- [C:INFERRED] 图像生成工具集成（`generate_image`）不在本设计中定义，依赖环境已有能力

## Prior Art

### 上游系统（gpowers taste-skill）
- 1231 行核心 SKILL.md + 4 个附录（gpt-taste.md、image-to-code.md、redesign.md、stitch.md）
- 触发信号驱动附录加载：`website`, `landing page`, `frontend`, `UI`, `interface`, `portfolio`, `SaaS page`, `web app`
- 动态附录加载规则：主 skill 先加载，子信号匹配时追加对应附录，附录冲突时附录优先
- 完整 Pre-flight Checklist：40+ 项机械检查，全部通过才算完成

### 本系统现有机制
- `SkillRegistry` 支持 `hiddenInModes` 过滤（`normal` / `plan` / `design`）
- `BUILTIN_SLASH_COMMANDS` 支持 `hiddenInModes` 控制命令可见性
- `design-mode-contract.ts` 提供 design mode 的 system prompt injection 框架
- `.ody-code/designs/` 目录已存在，用于存放设计文件

## Architecture

```
User Input
  ├── 关键词触发（design mode） ──> SkillRegistry.listInvocableSkills('design')
  │                                   └── frontend-design skill 注入 system prompt
  │                                       └── Agent 按规范生成前端代码
  │
  └── /frontend-design 命令（design mode）
          └── TUI Command Registry (hiddenInModes: ['normal','plan'])
              └── 启动 FrontendDesignFlow
                  ├── 设计意图识别（Brief Inference）
                  ├── 附录推荐 + 用户选择
                  ├── 设计流程推进（Dial 设置、Design System 选择、布局设计）
                  ├── Pre-flight Check 逐项确认
                  └── 产出 DESIGN.md → .ody-code/designs/
                      ├── Cancel: 清理临时状态
                      └── Pause: 保留进度到 session 状态
```

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | [C:INFERRED] `hiddenInModes` 对 builtin skills 生效（现有 built-in skills 如 `receiving-code-review` 已使用） | High | Skill 会在所有 mode 下注入，污染 normal mode | 检查 `SkillRegistry.listInvocableSkills()` 实现 |
| 2 | [C:INFERRED] TUI command registry 的 `hiddenInModes` 在运行时根据当前 sessionMode 过滤 | High | 命令在错误 mode 下可见 | 检查 `apps/ody-code/src/tui/commands/registry.ts` 中命令渲染逻辑 |
| 3 | [C:INFERRED] design mode 的 system prompt injection 机制可以扩展以支持 frontend-design 的专用 contract | Medium | 无法为前端设计流程注入专用指令 | 阅读 `packages/agent-core/src/agent/injection/design-mode-contract.ts` |
| 4 | [C:INFERRED] 1231 行 skill 内容不会超出 design mode 的 token 预算 | Medium | system prompt 过长导致 truncation 或性能下降 | 实际测试时测量 token 数 |
| 5 | [C:INFERRED] stitch.md 格式可直接复用为本系统 DESIGN.md 格式，无需适配 | Medium | 设计文档格式不兼容 | 阅读 stitch.md 内容确认 |
| 6 | [C:INFERRED] 修改 `SkillRegistry.listInvocableSkills()` 移除 `sessionMode !== 'normal'` 条件后，不影响现有 skill 行为 | High | 现有 skill 在 normal mode 下被错误过滤 | 用 node -e 验证所有现有 built-in skill 的 hiddenInModes 组合 |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 1231 行 skill 内容导致 design mode system prompt 过长 | Medium | High | 测量 token 数；必要时将附录改为按需加载而非常驻 |
| 2 | 前端设计流程与现有 design mode 规则冲突（如 design mode 禁止写代码） | Medium | High | 明确定义前端设计流程的边界：产出 DESIGN.md 期间只写设计文件；代码生成在流程结束后进行 |
| 3 | 附录动态加载的实现复杂度（需要解析用户意图并匹配附录） | Medium | Medium | 先实现固定附录加载，V2 再加入动态推荐 |
| 4 | 用户混淆 `/design`（切换 mode）和 `/frontend-design`（启动流程） | High | Low | 命令描述清晰区分：`/design` = "Toggle design mode"，`/frontend-design` = "Start frontend design workflow" |

---

*以下部分将在后续 turn 中逐步填充：*

## Components

### 1. FrontendDesignSkill（内置 Skill）

[C:UPSTREAM] 将上游 `frontend-design/SKILL.md` 及其 4 个附录作为内置 skill 注册。

```typescript
// packages/agent-core/src/skill/builtin/frontend-design.ts
interface FrontendDesignSkillDefinition {
  name: 'frontend-design';
  description: string; // [C:UPSTREAM] 上游描述
  source: 'builtin';
  metadata: {
    type: 'inline';
    hiddenInModes: ['normal', 'plan']; // [C:USER] 仅在 design mode 可见
    whenToUse: string; // [C:UPSTREAM] 触发信号列表
  };
  content: string; // [C:UPSTREAM] 主 SKILL.md 内容
  // 附录作为独立文件存在，不合并到 content 中
  appendices: {
    'gpt-taste': string;
    'image-to-code': string;
    'redesign': string;
    'stitch': string;
  };
}
```

**注册位置**: `packages/agent-core/src/skill/builtin/index.ts`
**调用点**: `SkillRegistry.registerBuiltinSkill(FRONTEND_DESIGN_SKILL)`

### 2. FrontendDesignCommand（TUI Slash Command）

[C:USER] 新增 `/frontend-design` 命令，仅在 design mode 下可见。

```typescript
// apps/ody-code/src/tui/commands/registry.ts
{
  name: 'frontend-design',
  aliases: ['fd'],
  description: 'Start frontend design workflow (design mode only)',
  priority: 90,
  availability: 'idle-only',
  hiddenInModes: ['normal', 'plan'], // [C:USER]
}
```

**行为**: 命令触发后，向 Agent 发送一个特殊标记消息，启动 `FrontendDesignFlow`。

### 3. FrontendDesignFlow（设计流程控制器）

[C:USER] 管理从前端设计任务启动到 DESIGN.md 产出的完整流程。

```typescript
// packages/agent-core/src/agent/frontend-design/flow.ts
interface FrontendDesignFlow {
  state: 'idle' | 'briefing' | 'dialing' | 'appendix-select' | 'designing' | 'preflight' | 'done' | 'paused';
  
  // 启动流程
  start(userPrompt: string): Promise<FlowResult>;
  
  // 状态转换
  transition(to: FlowState, context?: FlowContext): Promise<void>;
  
  // 暂停 / 恢复
  pause(): Promise<PausedFlowState>;
  resume(state: PausedFlowState): Promise<void>;
  
  // 取消
  cancel(): Promise<void>;
}

interface PausedFlowState {
  state: FlowState;
  designRead: string;       // Section 0.B: one-line design read
  dialValues: DialValues;   // Section 1: three dials
  selectedAppendices: string[];
  partialDesignDoc: string; // 已生成的 DESIGN.md 内容
  timestamp: number;
}
```

### 4. AppendixSelector（附录选择器）

[C:USER] 根据用户输入识别设计意图，推荐匹配附录。

```typescript
// packages/agent-core/src/agent/frontend-design/appendix-selector.ts
interface AppendixSelector {
  // 输入: 用户的原始 prompt
  // 输出: 排序后的附录列表，第一项为推荐项
  select(userPrompt: string): AppendixRecommendation[];
}

interface AppendixRecommendation {
  name: string;           // e.g., 'gpt-taste', 'image-to-code'
  label: string;          // 显示名称
  description: string;    // 简短说明
  isRecommended: boolean; // 是否基于意图匹配推荐
  triggerSignals: string[]; // 匹配到的触发信号
}
```

**算法**: `select(userPrompt)`
```
function select(userPrompt: string): AppendixRecommendation[]
  let scores = Map<string, number>()
  
  // 对每个附录的触发信号进行匹配（支持中英文）
  // [C:UPSTREAM] 上游触发信号为英文，但用户可能用中文表达相同意图
  for each appendix in APPENDICES:
    score = 0
    for each signal in appendix.triggerSignals:
      // 同时匹配原文和中文翻译信号
      if userPrompt.toLowerCase() contains signal.toLowerCase():
        score += 1
    scores[appendix.name] = score
  
  // 构建推荐列表
  recommendations = []
  for each appendix in APPENDICES:
    recommendations.push({
      name: appendix.name,
      label: appendix.label,
      description: appendix.description,
      isRecommended: scores[appendix.name] > 0,
      triggerSignals: matchedSignals(userPrompt, appendix.triggerSignals)
    })
  
  // 按推荐状态排序（推荐项在前），同组按名称排序
  return recommendations.sorted((a, b) => {
    if (a.isRecommended && !b.isRecommended) return -1
    if (!a.isRecommended && b.isRecommended) return 1
    return a.name.localeCompare(b.name)
  })
```

### 5. DesignDocumentBuilder（DESIGN.md 构建器）

[C:USER] 按照 `stitch.md` 格式规范构建 DESIGN.md。

```typescript
// packages/agent-core/src/agent/frontend-design/design-doc-builder.ts
interface DesignDocumentBuilder {
  build(context: DesignContext): string; // 返回完整的 DESIGN.md 内容
}

interface DesignContext {
  designRead: string;           // Section 0.B
  dialValues: DialValues;       // Section 1
  designSystem: string;         // Section 2
  stack: string;                // Section 3
  engineeringDirectives: string; // Section 4
  preFlightResult: PreFlightResult; // Section 14
  selectedAppendices: string[];
}

interface PreFlightResult {
  passed: boolean;
  checkedItems: PreFlightItem[];
}

interface PreFlightItem {
  id: string;        // e.g., 'zero-em-dashes'
  description: string; // 检查项描述
  passed: boolean;   // 是否通过
  notes?: string;    // 备注
}
```

### 6. PreFlightChecker（预检清单执行器）

[C:USER] 在产出 DESIGN.md 前执行 40+ 项强制检查。

```typescript
// packages/agent-core/src/agent/frontend-design/preflight-checker.ts
interface PreFlightChecker {
  check(designDoc: string): PreFlightResult;
}
```

**检查项来源**: [C:UPSTREAM] 上游 skill Section 14（Final Pre-Flight Check）
**执行方式**: Agent 在 system prompt 中被明确要求逐项确认，不是纯代码检查。

## Data Flow

### 流程 A：关键词自动触发（简单任务）

```
User（design mode）: "帮我设计一个 SaaS landing page"
  │
  ▼
Session.loadSkills() ──> SkillRegistry.listInvocableSkills('design')
  │                         └── 过滤后包含 frontend-design skill
  ▼
Agent.systemPrompt 渲染
  │  ├── system.md 模板
  │  ├── {{ KIMI_SKILLS }} ──> SkillRegistry.getModelSkillListing('design')
  │  │                         └── 包含 frontend-design 及其触发信号
  │  └── design-mode-contract.ts injection
  ▼
Agent 识别触发信号 "landing page"
  │
  ▼
Agent 按 frontend-design skill 规范生成代码
  ├── 执行 Brief Inference（Design Read）
  ├── 设置 Three Dials
  ├── 选择 Design System
  └── 输出符合规范的前端代码
```

### 流程 B：`/frontend-design` 命令触发（复杂任务）

```
User（design mode）: "/frontend-design"
  │
  ▼
TUI Command Registry
  │  ├── 检查 hiddenInModes: ['normal', 'plan'] → 当前为 design mode，显示命令
  │  └── 解析命令，调用 FrontendDesignFlow.start()
  ▼
FrontendDesignFlow.start("")
  │
  ▼
状态: briefing ──> Agent 执行 Brief Inference
  │                   └── 产出 one-line Design Read
  ▼
状态: dialing ──> Agent 设置 Three Dials
  │                   └── 产出 VARIANCE / MOTION / DENSITY 值
  ▼
状态: appendix-select ──> AppendixSelector.select(userPrompt)
  │                         └── 推荐匹配附录，展示全部选项
  │                         └── User 选择附录（或接受默认推荐）
  ▼
状态: designing ──> Agent 按 stitch.md 格式构建 DESIGN.md
  │                   ├── 注入选定附录内容
  │                   ├── 填充各章节
  │                   └── 实时写入 .ody-code/designs/YYYY-MM-DD-frontend-design-<slug>.md
  ▼
状态: preflight ──> Agent 执行 40+ 项 Pre-flight Check
  │                   └── 全部通过 → 标记 done
  │                   └── 任一项未通过 → 返回 designing 修正
  ▼
状态: done ──> 产出最终 DESIGN.md
  │
  ▼
Agent: "Design complete. Run /plan to turn this into an implementation plan."
```

### 流程 C：Pause / Resume

```
User: "/frontend-design pause"（或流程中触发 Pause）
  │
  ▼
FrontendDesignFlow.pause()
  │  ├── 将当前状态序列化为 PausedFlowState
  │  ├── 保存到 Session 的临时存储（内存或 session metadata）
  │  └── Agent: "Frontend design paused. Use /frontend-design resume to continue."
  ▼
...（后续对话）...
  ▼
User: "/frontend-design resume"
  │
  ▼
FrontendDesignFlow.resume(pausedState)
  │  ├── 恢复之前的状态和进度
  │  └── 从暂停点继续流程
```

## Error Handling

| Error Class | Immediate Handling | Degradation Path | Recovery |
|---|---|---|---|
| **Skill parse error**（内置 skill 加载失败） | 记录 warning，跳过该 skill | 不影响其他 skill；design mode 下前端设计能力不可用 | 修复 skill 文件后重启 session |
| **Command not available in current mode**（normal/plan 模式下用户尝试 `/frontend-design`） | TUI 不显示该命令；若通过其他方式触发则返回错误消息 | 提示用户先进入 design mode (`/design`) | 用户切换 mode 后重试 |
| **Design doc write failure**（DESIGN.md 写入失败） | 捕获 error，通知用户 | 内容保留在对话上下文中，用户可手动保存 | 检查目录权限后重试 |
| **Pre-flight check failure** | Agent 被要求返回 designing 状态修正 | 标记未通过项，针对性修改 | Agent 修正后重新检查 |
| **Pause state loss**（session 重启后丢失暂停状态） | 无状态可恢复 | 提示用户从头开始 | 未来可考虑持久化到磁盘 |
| **Token budget exceeded**（skill 内容过长导致 truncation） | 检测到 prompt 过长 | 仅注入核心 SKILL.md，附录不常驻，改为命令触发时按需加载 | 用户通过 `/frontend-design` 命令按需加载完整内容 |

## Testing

### 单元测试

1. **SkillRegistry 过滤测试**
   - Assert: `listInvocableSkills('design')` 包含 `frontend-design`
   - Assert: `listInvocableSkills('normal')` **不包含** `frontend-design`
   - Assert: `listInvocableSkills('plan')` **不包含** `frontend-design`

2. **AppendixSelector 测试**
   - Input: `"帮我做一个 portfolio 网站"` → Assert: `gpt-taste` 为推荐项
   - Input: `"基于截图重新设计"` → Assert: `image-to-code` 为推荐项
   - Input: `"redesign 现有网站"` → Assert: `redesign` 为推荐项
   - Input: `"普通的前端 bug 修复"` → Assert: 无推荐项，所有 appendix 按字母排序

3. **TUI Command Registry 测试**
   - Assert: `/frontend-design` 在 design mode 的命令补全列表中
   - Assert: `/frontend-design` **不在** normal mode 的命令补全列表中
   - Assert: `/frontend-design` **不在** plan mode 的命令补全列表中

### 集成测试

1. **Design mode + skill 注入**
   - 启动 design mode session
   - 发送包含 "landing page" 的 prompt
   - Assert: Agent 的 system prompt 中包含 frontend-design skill 内容
   - Assert: Agent 的输出符合 skill 规范（包含 Design Read、Dial 值等）

2. **Command 触发完整流程**
   - 进入 design mode，发送 `/frontend-design`
   - Assert: 流程进入 briefing 状态
   - 模拟用户输入设计需求
   - Assert: 产出 appendix 选择列表，推荐项标记为 "(Recommended)"
   - 选择附录后
   - Assert: 流程进入 designing 状态
   - Assert: 最终产出 `.ody-code/designs/*.md` 文件
   - Assert: 文件包含完整的 Pre-flight Check 章节

### Done Criteria

```bash
# 1. 类型检查通过
pnpm typecheck

# 2. 单元测试通过
pnpm test -- packages/agent-core/src/skill/builtin/frontend-design.test.ts
pnpm test -- packages/agent-core/src/agent/frontend-design/appendix-selector.test.ts

# 3. TUI 命令注册测试
pnpm test -- apps/ody-code/src/tui/commands/registry.test.ts

# 4. 端到端验证（手动）
# - 启动 ody-code，进入 design mode
# - 确认 `/frontend-design` 命令可见
# - 执行命令，验证附录选择 UI
# - 验证 DESIGN.md 产出
```

## Self-Review

### 最昂贵的决策验证

#### 决策 1：AppendixSelector 触发信号匹配算法

**验证工具**: `node -e`（见上文）

| 输入 | 预期输出 | 实际结果 |
|---|---|---|
| "帮我做一个 portfolio 网站" | `gpt-taste` 为推荐项 | ✓ 匹配到 "portfolio" |
| "基于截图重新设计 landing page" | `image-to-code` 和 `redesign` 为推荐项 | ✓ 匹配到 "截图" 和 "重新设计"（中英文信号） |
| "普通的前端 bug 修复" | 无推荐项 | ✓ 无任何匹配 |

**修正**: 初始实现仅包含英文触发信号，Test 2 失败。修正为同时包含中英文信号后通过。

#### 决策 2：SkillRegistry `hiddenInModes` 过滤逻辑修改

**验证工具**: `node -e`（见上文）

| 场景 | 现有逻辑结果 | 修改后逻辑结果 | 是否期望 |
|---|---|---|---|
| `frontend-design` in normal mode | 可见 ❌ | 隐藏 ✓ | ✓ |
| `frontend-design` in plan mode | 隐藏 ✓ | 隐藏 ✓ | ✓ |
| `frontend-design` in design mode | 可见 ✓ | 可见 ✓ | ✓ |
| `receiving-code-review` in normal mode | 可见 ✓ | 可见 ✓ | ✓ |

**结论**: 移除 `sessionMode !== 'normal'` 条件可以实现目标，且不破坏现有行为。

#### 决策 3：DESIGN.md 路径命名规则

| 输入 | 预期输出 |
|---|---|
| 用户 prompt: "设计一个 SaaS landing page" | `.ody-code/designs/2026-06-11-frontend-design-saas-landing.md` |
| 用户 prompt: "portfolio" | `.ody-code/designs/2026-06-11-frontend-design-portfolio.md` |
| 目录不存在 | 自动创建 `.ody-code/designs/` |

---

### 四镜扫描

#### Security
- **检查**: skill 内容是否包含敏感信息（API keys、内部 URL）
- **发现**: 上游 skill 内容包含外部 CDN URL（picsum.photos、cdn.simpleicons.org）和 npm 包名，均为公开资源
- **修复**: 无修复 needed；在 DESIGN.md 中继续使用占位符替换内部标识符（符合 AGENTS.md 规则）

#### Test
- **检查**: 每个行为是否有 must-pass 和 must-reject 测试
- **发现**: 
  - SkillRegistry 过滤测试覆盖了 normal/plan/design 三种 mode ✓
  - AppendixSelector 测试覆盖了匹配/不匹配/多匹配场景 ✓
  - TUI 命令可见性测试覆盖了三种 mode ✓
- **修复**: 无修复 needed

#### Ops
- **检查**: 新增调用的成本/延迟、标识符冲突、并发行为
- **发现**: 
  - 1231 行 skill 内容可能增加 design mode system prompt 长度（风险 #1）
  - 附录文件名与现有内置 skill 命名无冲突（`frontend-design` 为新增唯一名称）
  - Pause 状态存储在 session 内存中，session 重启后丢失（已在 Error Handling 中记录）
- **修复**: 在 Risk Register 中增加 token 预算风险；Error Handling 中记录 Pause 状态丢失

#### Integration
- **检查**: 每个数据源/字段/事件/hook 是否真实存在
- **发现**: 
  - ✅ `hiddenInModes` 存在于 `SkillMetadata`（`packages/agent-core/src/skill/types.ts:9`）
  - ✅ `listInvocableSkills(sessionMode)` 存在于 `SkillRegistry`（`packages/agent-core/src/skill/registry.ts:112-128`）
  - ✅ TUI command `hiddenInModes` 存在于 `BUILTIN_SLASH_COMMANDS`（`apps/ody-code/src/tui/commands/registry.ts:55-72`）
  - ⚠️ `listInvocableSkills` 的 `sessionMode !== 'normal'` 条件导致 normal mode 下不过滤（已在决策 2 中处理）
  - ✅ `design-mode-contract.ts` 提供 design mode injection 框架（`packages/agent-core/src/agent/injection/design-mode-contract.ts`）
  - ✅ `.ody-code/designs/` 目录已存在（已确认）
- **修复**: 更新 Integration 方案，明确需要修改 `listInvocableSkills` 过滤逻辑

#### Scope
- **检查**: 是否仍为单一连贯设计，还是已拆分为多个独立子项目
- **发现**: 本设计涉及 skill 注册、TUI 命令、Agent 流程三个层面，但均为同一能力的不同接入点，属于单一子系统
- **修复**: 无修复 needed；保持单文件设计
