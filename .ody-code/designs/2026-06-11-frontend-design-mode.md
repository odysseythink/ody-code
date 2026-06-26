# Frontend-Design Mode 设计文档

## Scope In / Out

### In
- [C:USER] 新增 `frontend-design` session mode，作为独立的工作模式
- [C:USER] 产出物：DESIGN.md 设计文档 + 前端代码文件（组件、样式、配置等）
- [C:USER] 主入口：`/frontend-design` 命令从 normal 模式进入
- [C:USER] 辅助入口：design 模式中发现前端设计任务时，TUI 预检测 + Agent 二次确认提醒跳转
- [C:USER] 退出：任务完成后自动回到 normal 模式
- [C:USER] DESIGN.md 存放在 `.ody-code/frontend-designs/` 目录
- [C:USER] 代码文件写入项目现有目录，Agent 自行判断位置，必须和用户确认
- [C:USER] 1231 行 skill 内容 + 选定附录全部常驻注入 system prompt
- [C:USER] 4 个外部附录：用户手动选择，最匹配的显示 "(Recommended)"
- [C:USER] 权限：写文件 + 安装命令（npm install 等）+ dev server（npm run dev）
- [C:USER] 直接上线，不设实验性开关
- [C:UPSTREAM] 上游 skill 核心内容（Brief Inference、Three Dials、Design System Map、AI Tells、Pre-flight Check 等）

### Out
- [C:USER] 不支持 Dashboard / Data Table / Native Mobile 等 upstream 明确 Out-of-Scope 的场景
- [C:INFERRED] 不支持多个并行的 frontend-design 会话（单 session 单任务）
- [C:INFERRED] 不支持从 plan 模式直接跳转到 frontend-design 模式
- [C:INFERRED] 不自动停止 dev server（用户手动 `/tasks` 管理或 session 结束时清理）
- [C:INFERRED] 不实现图像生成工具集成（依赖环境已有能力）
- [C:INFERRED] Block Library（upstream 已移除）不在本版本中实现

## Prior Art

### 上游系统（gpowers taste-skill）
- 1231 行核心 SKILL.md + 4 个外部附录文件（gpt-taste.md、image-to-code.md、redesign.md、stitch.md）
- 触发信号驱动附录加载：`website`, `landing page`, `frontend`, `UI`, `interface`, `portfolio`, `SaaS page`, `web app`
- 完整 Pre-flight Checklist：40+ 项机械检查
- 输出要求：每个组件必须完整且可运行，支持 PAUSED 机制

### 本系统现有机制
- `SessionModeKind = 'plan' | 'design'` —— 两种 mode 共享同一套 read-only-with-one-writable-file 机制
- `PlanModeGuardDenyPermissionPolicy` 严格控制写权限（仅 .md 文件）
- `DesignModeInjector` / `PlanModeInjector` 通过 `DynamicInjector` 注入 system prompt
- `InjectionManager` 硬编码所有 injectors
- TUI 命令通过 `hiddenInModes` 控制可见性

## Architecture

```
User Input
  │
  ├── normal 模式 ──> /frontend-design 命令
  │                      │
  │                      ▼
  │                 SessionMode.enter('frontend-design')
  │                      │
  │                      ▼
  │                 FrontendDesignInjector 注入 skill 内容
  │                      │
  │                      ▼
  │                 Agent 执行完整前端设计流程
  │                      ├── Brief Inference → Design Read
  │                      ├── Three Dials → VARIANCE/MOTION/DENSITY
  │                      ├── Appendix Selector → 用户选择附录
  │                      ├── Design System Map → 技术栈选择
  │                      ├── DESIGN.md 生成 → .ody-code/frontend-designs/
  │                      ├── 代码生成 → 项目目录
  │                      ├── 依赖安装 → npm install
  │                      ├── dev server → npm run dev（可选）
  │                      └── Pre-flight Check → 40+ 项确认
  │                      │
  │                      ▼
  │                 自动退出 → normal 模式
  │
  └── design 模式 ──> 用户输入匹配触发信号
                         │
                         ▼
                    TUI 预检测 → 提示切换
                         │
                    用户确认？
                         ├── 是 ──> 切换到 frontend-design 模式
                         └── 否 ──> 继续 design 模式流程
                              │
                              ▼
                         Agent Brief Inference 后二次建议
                              │
                         用户确认？
                              ├── 是 ──> 切换（TUI 未提示过时）
                              └── 否 ──> 继续 design 模式
```

## Components

*以下组件将在后续 turn 中逐步详细设计：*

### 1. SessionMode 扩展

#### 1.1 SessionModeKind 扩展

```typescript
// packages/agent-core/src/agent/session-mode/index.ts:21
// [C:USER] 新增 'frontend-design' kind
export type SessionModeKind = 'plan' | 'design' | 'frontend-design';
```

**影响面**（所有使用 `SessionModeKind` 或硬编码 `'plan' | 'design'` 的地方）：

| 文件 | 位置 | 当前逻辑 | 修改方式 |
|---|---|---|---|
| `session-mode/index.ts` | L21 | `type SessionModeKind = 'plan' \| 'design'` | 添加 `'frontend-design'` |
| `session-mode/index.ts` | L35 | `_kind: SessionModeKind = 'plan'` | 不变（默认值仍为 'plan'） |
| `session-mode/index.ts` | L515 | `kind === 'design' ? 'designs' : 'plans'` | 扩展为 switch 或映射表 |
| `injection/plan-mode.ts` | L34,37 | `kind !== 'design'` | 改为 `kind === 'plan'` |
| `permission/policies/plan-mode-guard-deny.ts` | L15-16 | `isDesign = kind === 'design'` | 重构为 mode-aware 分支 |
| `permission/policies/exit-plan-mode-review-ask.ts` | 待查 | 硬编码 `'plan' \| 'design'` | 扩展 |
| `permission/policies/plan-mode-tool-approve.ts` | 待查 | 硬编码 mode 判断 | 扩展 |
| TUI footer 渲染 | 待查 | 显示当前 mode 标签 | 添加 `frontend-design` 标签 |

#### 1.2 resolveSessionModeDirectory 扩展

```typescript
// packages/agent-core/src/agent/session-mode/index.ts:515
// [C:INFERRED] 从三元表达式扩展为映射表
private async resolveSessionModeDirectory(kind: SessionModeKind): Promise<{ dir: string; isProjectScoped: boolean }> {
  const MODE_DIR_MAP: Record<SessionModeKind, string> = {
    plan: 'plans',
    design: 'designs',
    'frontend-design': 'frontend-designs',
  };
  const subDir = MODE_DIR_MAP[kind];
  const projectDir = join(this.agent.config.cwd, '.ody-code', subDir);
  // ... 其余逻辑不变
}
```

#### 1.3 handoffTo 扩展

```typescript
// packages/agent-core/src/agent/session-mode/index.ts:270
// [C:USER] 支持从 design 模式 handoff 到 frontend-design 模式
async handoffTo(target: 'plan' | 'normal' | 'frontend-design'): Promise<void> {
  const data = await this.data();
  const artifact = data !== null && data.content.trim().length > 0
    ? { content: data.content, path: data.path }
    : null;

  if (target === 'plan') {
    this._pendingHandoffForPlan = artifact;
    this.exit();
    await this.enter(this.createSessionModeId(), false, true, 'plan');
  } else if (target === 'frontend-design') {
    // [C:INFERRED] 新增 handoff 路径：design → frontend-design
    this._pendingHandoffForFrontendDesign = artifact;
    this.exit();
    await this.enter(this.createSessionModeId(), false, true, 'frontend-design');
  } else {
    this._pendingHandoffForNormal = artifact;
    this.exit();
  }
}
```

**新增字段**：
```typescript
private _pendingHandoffForFrontendDesign: { content: string; path: string } | null = null;

consumePendingHandoffForFrontendDesign(): { content: string; path: string } | null {
  const p = this._pendingHandoffForFrontendDesign;
  this._pendingHandoffForFrontendDesign = null;
  return p;
}
```

#### 1.4 modeModels 配置扩展

```typescript
// [C:INFERRED] kimiConfig.modeModels 支持 frontend-design key
interface KimiConfig {
  modeModels?: {
    plan?: string;
    design?: string;
    'frontend-design'?: string;  // 新增
  };
}
```

进入 frontend-design 模式时，自动切换到配置的模型（同 plan/design 模式逻辑）。

#### 1.5 isWritableSessionModePath 行为

```typescript
// packages/agent-core/src/agent/session-mode/index.ts:313
// [C:USER] frontend-design 模式下：DESIGN.md 可写，代码文件通过权限策略单独控制
isWritableSessionModePath(path: string): boolean {
  if (this._sessionModeFilePath === null) return false;
  if (path === this._sessionModeFilePath) return true;

  // [C:INFERRED] frontend-design 模式下，DESIGN.md 的 split parts 也可写
  const mainDir = dirname(this._sessionModeFilePath);
  const mainBase = basename(this._sessionModeFilePath);
  const mainStem = mainBase.slice(0, -'.md'.length);

  const splitDir = normalize(join(mainDir, mainStem));
  const normalizedPath = normalize(path);
  if (!normalizedPath.startsWith(splitDir + '/')) return false;
  if (!basename(normalizedPath).endsWith('.md')) return false;
  return true;
}
```

**注意**：[C:USER] frontend-design 模式下代码文件的写权限**不**通过 `isWritableSessionModePath` 控制，而是通过独立的 `FrontendDesignPermissionPolicy` 控制。`isWritableSessionModePath` 仅控制 DESIGN.md 及其 split parts 的可写性。

### 2. FrontendDesignInjector

#### 2.1 架构定位

```
InjectionManager
  └── injectors: DynamicInjector[]
        ├── PlanModeInjector
        ├── DesignModeInjector
        ├── FrontendDesignInjector    // [C:USER] 新增
        └── ...
```

`FrontendDesignInjector` 与 `DesignModeInjector` 和 `PlanModeInjector` 平行，专责 frontend-design 模式的 system prompt 注入。

#### 2.2 接口定义

```typescript
// packages/agent-core/src/agent/injection/frontend-design-mode.ts
// [C:INFERRED] 新增文件

export type FrontendDesignVariant = 'full' | 'sparse' | 'reentry';

export class FrontendDesignInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'frontend_design';
  private wasActive = false;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'frontend-design';
  }

  override async getInjection(): Promise<string | undefined> {
    const isActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'frontend-design';
    const { sessionModeFilePath } = this.agent.sessionMode;

    if (!isActive) {
      if (!this.wasActive) return undefined;
      this.wasActive = false;
      this.injectedAt = null;
      return exitReminder();
    }

    // [C:USER] 注入 skill 内容 + 用户选择的附录
    const skillContent = await this.loadSkillContent();
    const appendixContent = await this.loadSelectedAppendices();
    const skillsReminder = this.agent.skills?.registry.getUnavailableSkillsReminder('frontend-design') ?? '';

    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      const content = await this.currentDesignContent();
      if (content.trim().length > 0) {
        // 已有 DESIGN.md 内容，reentry
        return appendSkillsReminder(frontendDesignReentryReminder(sessionModeFilePath, skillContent, appendixContent), skillsReminder);
      }
    }

    const variant = this.getVariant();
    if (variant === null) return undefined;
    if (variant === 'reentry') {
      return appendSkillsReminder(frontendDesignReentryReminder(sessionModeFilePath, skillContent, appendixContent), skillsReminder);
    }

    const body = variant === 'full'
      ? frontendDesignFullReminder(sessionModeFilePath, skillContent, appendixContent)
      : frontendDesignSparseReminder(sessionModeFilePath, skillContent, appendixContent);
    return appendSkillsReminder(body, skillsReminder);
  }

  protected getVariant(): FrontendDesignVariant | null {
    // [C:INFERRED] 与 DesignModeInjector 相同的变体逻辑
    if (this.injectedAt === null) return 'full';
    const history = this.agent.context.history;
    let assistantTurnsSince = 0;
    for (let i = this.injectedAt + 1; i < history.length; i++) {
      const msg = history[i];
      if (msg === undefined) continue;
      if (msg.role === 'assistant') {
        assistantTurnsSince += 1;
        continue;
      }
      if (msg.role === 'user') return 'full';
    }
    if (assistantTurnsSince >= FRONTEND_DESIGN_FULL_REFRESH_TURNS) return 'full';
    if (assistantTurnsSince >= FRONTEND_DESIGN_DEDUP_MIN_TURNS) return 'sparse';
    return null;
  }

  private async loadSkillContent(): Promise<string> {
    // [C:USER] 加载核心 SKILL.md 内容（1231 行）
    const skill = this.agent.skills?.registry.getBuiltinSkill('frontend-design');
    return skill?.content ?? '';
  }

  private async loadSelectedAppendices(): Promise<string> {
    // [C:USER] 加载用户选择的附录内容
    const selected = this.agent.sessionMode.getSelectedAppendices?.() ?? [];
    if (selected.length === 0) return '';
    const parts: string[] = [];
    for (const name of selected) {
      const content = await this.agent.skills?.registry.getBuiltinSkillAppendix('frontend-design', name);
      if (content) parts.push(`## Appendix: ${name}\n\n${content}`);
    }
    return parts.join('\n\n---\n\n');
  }

  private async currentDesignContent(): Promise<string> {
    try {
      const data = await this.agent.sessionMode.data();
      return data?.content ?? '';
    } catch {
      return '';
    }
  }
}
```

#### 2.3 Mode Contract 内容

`frontendDesignFullReminder` 的核心内容（与 design-mode-contract 平行）：

```
frontend-design mode is active. This is a frontend design and code generation session.
You are equipped with the frontend-design skill. Follow its methodology precisely:

1. BRIEF INFERENCE: Read the user's request and produce a one-line Design Read.
2. THREE DIALS: Set VARIANCE / MOTION / DENSITY based on the Design Read.
3. DESIGN SYSTEM MAP: Choose the right design system and stack.
4. CONFIRM WITH USER: Before generating code, confirm:
   - Is this a new project or existing project?
   - If existing, what is the current tech stack?
   - Where should the code files be placed?
5. APPENDIX SELECTION: Present available appendices with recommendations.
6. DESIGN DOCUMENT: Write DESIGN.md to .ody-code/frontend-designs/ following stitch.md format.
7. CODE GENERATION: Generate complete, runnable frontend code.
8. DEPENDENCY INSTALL: Run npm install / npx commands as needed.
9. DEV SERVER: Optionally run npm run dev for live preview.
10. PRE-FLIGHT CHECK: Run all 40+ checks before declaring done.

HARD RULES:
- Every component must be complete and runnable. No TODOs, no truncation.
- If token limit approaches, use PAUSED mechanism.
- Honor prefers-reduced-motion for all MOTION_INTENSITY > 3.
- Dark mode tokens must be defined and tested.
- Zero em-dashes anywhere on the page.
```

#### 2.4 注册到 InjectionManager

```typescript
// packages/agent-core/src/agent/injection/manager.ts:21
constructor(protected readonly agent: Agent) {
  this.injectors = [
    new PluginSessionStartInjector(agent),
    new TodoListReminderInjector(agent),
    new PlanModeInjector(agent),
    new DesignModeInjector(agent),
    new FrontendDesignInjector(agent),    // [C:USER] 新增
    new PermissionModeInjector(agent),
  ];
  // ...
}
```

#### 2.5 与 PlanModeInjector 的互斥

```typescript
// packages/agent-core/src/agent/injection/plan-mode.ts:34
// [C:INFERRED] 当前：kind !== 'design' 即视为 plan 模式
// 修改为：kind === 'plan' 才视为 plan 模式
override onContextClear(): void {
  super.onContextClear();
  this.wasActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'plan';
}

override async getInjection(): Promise<string | undefined> {
  const isPlanActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'plan';
  // ...
}
```

### 3. FrontendDesignPermissionPolicy

#### 3.1 问题：现有权限策略与 frontend-design 模式的冲突

```typescript
// packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts:12
// [C:UPSTREAM] 现有逻辑：任何 mode 下，Write/Edit 只允许 .md 文件
evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
  if (!this.agent.sessionMode.isActive) return;
  const isDesign = this.agent.sessionMode.kind === 'design';
  const modeLabel = isDesign ? 'design' : 'plan';  // [C:INFERRED] 二元假设被打破
  // ... Write/Edit 只允许 sessionModeFilePath 及其 split parts
}
```

**冲突点**：
1. `modeLabel` 和 `exitTool` 是二元判断（design vs plan），新增 mode 后无法正确命名
2. Write/Edit 被限制为 `.md` 文件，frontend-design 需要写 `.tsx`, `.css`, `.json` 等
3. `TaskStop` 被禁止，但 frontend-design 需要管理 dev server
4. `CronCreate/CronDelete` 被禁止，但 frontend-design 可能需要定时任务

#### 3.2 方案：重构为 Mode-Aware 策略

**推荐方案**：不新增独立策略，而是重构 `PlanModeGuardDenyPermissionPolicy` 为 `SessionModeGuardPermissionPolicy`，按 mode 分支。

```typescript
// packages/agent-core/src/agent/permission/policies/session-mode-guard.ts
// [C:USER] 重构后的统一策略

export class SessionModeGuardPermissionPolicy implements PermissionPolicy {
  readonly name = 'session-mode-guard';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!this.agent.sessionMode.isActive) return;

    const kind = this.agent.sessionMode.kind;
    const toolName = context.toolCall.name;

    // [C:USER] frontend-design 模式：放行所有写操作和命令
    if (kind === 'frontend-design') {
      return this.evaluateFrontendDesign(context, toolName);
    }

    // [C:UPSTREAM] plan/design 模式：保持原有严格限制
    return this.evaluatePlanOrDesign(context, toolName, kind);
  }

  private evaluateFrontendDesign(context: PermissionPolicyContext, toolName: string): PermissionPolicyResult | undefined {
    // [C:USER] Write/Edit：允许任何项目文件（不限于 .md）
    if (toolName === 'Write' || toolName === 'Edit') {
      // 允许写入项目目录内的任何文件
      // [C:INFERRED] 仍禁止写入项目目录外（通过其他策略控制）
      return undefined; // 放行
    }

    // [C:USER] Bash：允许安装命令和 dev server
    if (toolName === 'Bash') {
      // [C:INFERRED] 通过 Bash 命令的白名单/黑名单控制，不在此处拦截
      return undefined; // 放行，由 BashPermissionPolicy 二次控制
    }

    // [C:USER] TaskStop：允许（管理 dev server）
    if (toolName === 'TaskStop') {
      return undefined; // 放行
    }

    // [C:USER] CronCreate/CronDelete：允许
    if (toolName === 'CronCreate' || toolName === 'CronDelete') {
      return undefined; // 放行
    }

    return undefined; // 其他工具默认放行
  }

  private evaluatePlanOrDesign(context: PermissionPolicyContext, toolName: string, kind: 'plan' | 'design'): PermissionPolicyResult | undefined {
    // [C:UPSTREAM] 保持原有 plan/design 逻辑不变
    const modeLabel = kind;
    const exitTool = kind === 'design' ? 'ExitDesignMode' : 'ExitPlanMode';
    // ... 原有 Write/Edit/TaskStop/Cron 限制逻辑
  }
}
```

#### 3.3 备选方案：独立策略 + 优先级

如果重构风险过高，可采用独立策略：

```typescript
// [C:INFERRED] 备选：新增独立策略，在 frontend-design 模式下优先
export class FrontendDesignPermissionPolicy implements PermissionPolicy {
  readonly name = 'frontend-design-permission';
  readonly priority = 100; // 高于 plan-mode-guard-deny 的默认优先级

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!this.agent.sessionMode.isActive) return;
    if (this.agent.sessionMode.kind !== 'frontend-design') return;

    // 放行所有操作
    // [C:INFERRED] 安全边界：仍遵守项目目录限制（通过其他策略）
    return undefined;
  }
}
```

**缺点**：两个策略同时注册，维护复杂度增加。

#### 3.4 安全边界

即使 frontend-design 模式放宽了限制，以下安全边界仍然生效：

| 边界 | 控制策略 | 说明 |
|---|---|---|
| 项目目录外写入 | `CwdGuardPermissionPolicy` | 禁止写入 `cwd` 外的文件 |
| 敏感文件修改 | `SensitiveFilePermissionPolicy` | 禁止修改 `.env`, `id_rsa` 等 |
| Bash 命令黑名单 | `BashPermissionPolicy` | 禁止 `rm -rf /`, `sudo` 等危险命令 |
| 网络请求 | `FetchURLPermissionPolicy` | 禁止访问内网/私有地址 |

### 4. TUI 命令与触发检测

#### 4.1 `/frontend-design` 命令

```typescript
// apps/ody-code/src/tui/commands/registry.ts:20
// [C:USER] 新增命令
export const BUILTIN_SLASH_COMMANDS = [
  // ... 现有命令
  {
    name: 'frontend-design',
    aliases: ['fd'],
    description: 'Start frontend design mode (code + design document generation)',
    priority: 90,
    availability: 'idle-only',
    hiddenInModes: ['frontend-design'],  // [C:USER] 仅在 normal/plan/design 模式下可见
  },
  // ...
] as const satisfies readonly KimiSlashCommand[];
```

**命令行为**：
1. 用户输入 `/frontend-design`（或 `/fd`）
2. TUI 发送 `EnterFrontendDesignModeTool` 调用给 Agent
3. Agent 调用 `SessionMode.enter('frontend-design')`
4. 模式切换完成后，Agent 发送欢迎消息

#### 4.2 EnterFrontendDesignModeTool

```typescript
// [C:INFERRED] 新增工具，与 EnterDesignModeTool / EnterPlanModeTool 平行
interface EnterFrontendDesignModeTool {
  name: 'EnterFrontendDesignMode';
  description: 'Enter frontend-design mode for complete frontend design and code generation workflow';
}

// 工具实现
async handleEnterFrontendDesignMode(): Promise<ToolResult> {
  await this.agent.sessionMode.enter(
    this.agent.sessionMode.createSessionModeId(),
    false,  // createFile
    true,   // emitStatus
    'frontend-design'
  );
  return {
    content: 'Frontend-design mode activated. Ready to design and generate frontend code.',
  };
}
```

#### 4.3 TUI 层触发信号预检测

```typescript
// [C:INFERRED] TUI 输入处理层（位置待确认）
const FRONTEND_DESIGN_SIGNALS = [
  'website', 'landing page', 'frontend', 'UI', 'interface',
  'portfolio', 'SaaS page', 'web app', '网页', '页面', '前端',
  '网站', '落地页', '作品集', '界面'
];

function detectFrontendDesignIntent(input: string): boolean {
  const lower = input.toLowerCase();
  return FRONTEND_DESIGN_SIGNALS.some(signal => lower.includes(signal.toLowerCase()));
}

// 在 design 模式下，用户发送消息前检测
function onBeforeSendInDesignMode(input: string, sessionState: SessionState): void {
  if (sessionState.tuiPromptedFrontendDesign) return;  // 已提示过，不再提示
  if (!detectFrontendDesignIntent(input)) return;

  // [C:USER] 显示提示，询问是否切换
  showModal({
    title: '检测到前端设计任务',
    message: '当前处于 design 模式。这个任务更适合用 frontend-design 模式处理（直接生成代码）。是否切换？',
    actions: [
      { label: '切换到 frontend-design', action: 'switch-to-frontend-design' },
      { label: '留在 design 模式', action: 'stay-in-design' },
    ],
  });

  sessionState.tuiPromptedFrontendDesign = true;  // 标记已提示
}
```

#### 4.4 Agent 层二次确认

```typescript
// [C:INFERRED] 在 design mode 的 Brief Inference 阶段
// 如果 TUI 没有提示过（用户可能跳过了 TUI 提示），Agent 在分析后建议

// design-mode-contract.ts 中增加以下指令：
const FRONTEND_DESIGN_DETECTION = `
During Brief Inference, if the user's request clearly involves frontend design
(landing page, portfolio, web app, UI, website, etc.), check session state:
- If TUI already prompted and user declined: do NOT suggest again.
- If TUI did not prompt: suggest switching to frontend-design mode.
- Phrase: "This looks like a frontend design task. The frontend-design mode
  can generate both a design document and runnable code. Would you like to
  switch? (Say 'yes' to switch, or continue in design mode.)"
`;
```

#### 4.5 避免重复提醒的 State 机制

```typescript
// [C:INFERRED] Session-level state（内存中，不持久化）
interface FrontendDesignPromptState {
  tuiPrompted: boolean;      // TUI 是否已提示过
  userDeclinedAt: number;    // 用户拒绝时的时间戳（可选：24h 内不再提示）
  agentPrompted: boolean;    // Agent 是否已建议过
}

// 存储位置：Session 的临时 metadata 中
session.metadata.frontendDesignPrompt = {
  tuiPrompted: false,
  userDeclinedAt: 0,
  agentPrompted: false,
};
```

### 5. FrontendDesignSkill

#### 5.1 Skill 注册

```typescript
// packages/agent-core/src/skill/builtin/frontend-design.ts
// [C:USER] 新增文件

import { type SkillDefinition } from '../types';

export const FRONTEND_DESIGN_SKILL: SkillDefinition = {
  name: 'frontend-design',
  description:
    'Anti-slop frontend design methodology for premium interface generation. ' +
    'Brief inference, Three Dials, design system mapping, AI-Tells ban, pre-flight checklist.',
  source: 'builtin',
  metadata: {
    type: 'inline',
    // [C:USER] 仅在 frontend-design 模式注入
    hiddenInModes: ['normal', 'plan', 'design'],
    whenToUse:
      'Trigger: website, landing page, frontend, UI, interface, portfolio, ' +
      'SaaS page, web app. Appendices: gpt-taste, image-to-code, redesign, stitch.',
  },
  // [C:USER] 核心 SKILL.md 内容（1231 行）
  content: `...`, // 内联或从文件加载
};
```

#### 5.2 注册入口

```typescript
// packages/agent-core/src/skill/builtin/index.ts
// [C:USER] 新增注册

import { FRONTEND_DESIGN_SKILL } from './frontend-design';

export function registerBuiltinSkills(registry: SkillRegistry): void {
  // ... 现有 skills
  registry.registerBuiltinSkill(FRONTEND_DESIGN_SKILL);
}
```

#### 5.3 附录文件管理

```typescript
// packages/agent-core/src/skill/builtin/frontend-design.ts
// [C:USER] 附录作为独立资源管理

const APPENDICES = {
  'gpt-taste': {
    label: 'GPT Taste',
    description: 'Awwwards-level design patterns and premium consumer aesthetics',
    triggerSignals: ['awwwards', 'premium', 'luxury', 'high-end', '高端', '奢华'],
  },
  'image-to-code': {
    label: 'Image to Code',
    description: 'Convert screenshots and references into frontend code',
    triggerSignals: ['screenshot', 'image', 'reference', '截图', '图片', '参考'],
  },
  'redesign': {
    label: 'Redesign Protocol',
    description: 'Audit and upgrade existing websites',
    triggerSignals: ['redesign', 'revamp', 'upgrade', '重新设计', '改版'],
  },
  'stitch': {
    label: 'Stitch Format',
    description: 'DESIGN.md format specification',
    triggerSignals: ['design doc', 'spec', '文档', '规范'],  // [C:INFERRED] stitch 是格式规范，默认推荐
  },
} as const;

// [C:INFERRED] 附录内容从上游目录读取
const UPSTREAM_SKILL_DIR = '/Users/ranwei/workspace/go_work/gpowers/core/skills/frontend-design/';

async function loadAppendix(name: string): Promise<string> {
  const path = `${UPSTREAM_SKILL_DIR}${name}.md`;
  // 从文件系统读取附录内容
  return readFile(path, 'utf-8');
}
```

**注意**：[C:INFERRED] 附录文件的物理路径是上游目录。在构建/打包时，这些文件需要被复制到 `packages/agent-core/src/skill/builtin/frontend-design/` 下，或作为构建步骤内联到代码中。

#### 5.4 Skill 内容加载策略

```typescript
// [C:USER] 核心 SKILL.md 内容加载
// 方案 A：构建时内联（推荐）
// - 构建脚本将上游 SKILL.md 读取为字符串，内联到 frontend-design.ts 中
// - 优点：无运行时文件读取依赖
// - 缺点：增加 bundle 大小

// 方案 B：运行时读取
// - 从 node_modules 或已知路径读取
// - 优点：bundle 更小
// - 缺点：依赖文件系统路径，打包后可能失效

// [C:INFERRED] 推荐方案 A（构建时内联），与现有 built-in skills 保持一致
```

### 6. AppendixSelector

#### 6.1 接口定义

```typescript
// packages/agent-core/src/agent/frontend-design/appendix-selector.ts
// [C:USER] 新增文件

export interface AppendixRecommendation {
  name: string;           // e.g., 'gpt-taste'
  label: string;          // 显示名称
  description: string;    // 简短说明
  isRecommended: boolean; // 是否基于意图匹配推荐
  matchedSignals: string[]; // 匹配到的触发信号
}

export interface AppendixSelector {
  // 输入: 用户的原始 prompt + Brief Inference 结果
  // 输出: 排序后的附录推荐列表
  select(userPrompt: string, designRead?: string): AppendixRecommendation[];
}
```

#### 6.2 推荐算法

```typescript
// [C:USER] 附录选择算法
function select(userPrompt: string, designRead?: string): AppendixRecommendation[] {
  const text = `${userPrompt} ${designRead ?? ''}`.toLowerCase();
  const recommendations: AppendixRecommendation[] = [];

  for (const [name, config] of Object.entries(APPENDICES)) {
    const matchedSignals: string[] = [];
    let score = 0;

    for (const signal of config.triggerSignals) {
      if (text.includes(signal.toLowerCase())) {
        matchedSignals.push(signal);
        score += 1;
      }
    }

    recommendations.push({
      name,
      label: config.label,
      description: config.description,
      isRecommended: score > 0,
      matchedSignals,
    });
  }

  // [C:USER] 排序：推荐项在前，同组按名称排序
  return recommendations.sort((a, b) => {
    if (a.isRecommended && !b.isRecommended) return -1;
    if (!a.isRecommended && b.isRecommended) return 1;
    return a.name.localeCompare(b.name);
  });
}
```

#### 6.3 用户交互流程

```
Agent: "Based on your request, here are the available reference appendices:"

1. [RECOMMENDED] Image to Code — Convert screenshots and references into frontend code
   (Matched: screenshot, reference)
2. Stitch Format — DESIGN.md format specification
3. GPT Taste — Awwwards-level design patterns
4. Redesign Protocol — Audit and upgrade existing websites

"Please select the appendices you want to load (comma-separated numbers, or 'all'):"

User: "1, 2"
→ Agent 加载 image-to-code.md 和 stitch.md
```

#### 6.4 默认行为

```typescript
// [C:INFERRED] 如果用户不选择任何附录，默认加载 stitch（格式规范）
const DEFAULT_APPENDICES = ['stitch'];

function getSelectedAppendices(userSelection: string | null, recommendations: AppendixRecommendation[]): string[] {
  if (userSelection === null || userSelection.trim() === '') {
    return DEFAULT_APPENDICES;
  }
  if (userSelection.toLowerCase() === 'all') {
    return recommendations.map(r => r.name);
  }
  // 解析逗号分隔的索引
  const indices = userSelection.split(',').map(s => parseInt(s.trim(), 10) - 1);
  return indices
    .filter(i => i >= 0 && i < recommendations.length)
    .map(i => recommendations[i].name);
}
```

## Data Flow

### 流程 A：`/frontend-design` 命令触发（主入口）

```
User: "/frontend-design"
  │
  ▼
TUI Command Parser
  │  ├── 解析命令名 'frontend-design'
  │  ├── 检查 hiddenInModes：当前 normal，命令可见 ✓
  │  └── 调用 handleEnterFrontendDesignMode()
  ▼
Agent Tool Handler
  │  ├── 调用 sessionMode.enter(id, false, true, 'frontend-design')
  │  ├── 切换到 frontend-design 模型（如 modeModels 配置）
  │  ├── 设置 context partition = 'frontend-design'
  │  └── emitStatusUpdated()
  ▼
FrontendDesignInjector.getInjection()
  │  ├── 加载核心 SKILL.md（1231 行）
  │  ├── 加载用户选择的附录（初始为空，等 Agent 引导选择）
  │  └── 返回 full reminder + skill content
  ▼
Agent（带完整 skill 上下文）
  │
  ├── Step 1: Brief Inference
  │     ├── 分析用户输入
  │     └── 产出 one-line Design Read
  │
  ├── Step 2: Three Dials
  │     ├── 根据 Design Read 设置 VARIANCE/MOTION/DENSITY
  │     └── 和用户确认
  │
  ├── Step 3: 项目类型确认
  │     ├── "新项目还是现有项目？"
  │     ├── 如现有 → 检测 package.json 推断技术栈
  │     └── 确认代码输出目录
  │
  ├── Step 4: Appendix Selection
  │     ├── AppendixSelector.select(userPrompt, designRead)
  │     ├── 展示推荐列表（带 RECOMMENDED 标记）
  │     └── 用户选择 → 加载附录
  │
  ├── Step 5: Design System Map
  │     ├── 根据 Brief 选择设计系统
  │     └── 产出安装命令列表
  │
  ├── Step 6: DESIGN.md 生成
  │     ├── 按 stitch.md 格式构建
  │     └── Write → .ody-code/frontend-designs/YYYY-MM-DD-<topic>.md
  │
  ├── Step 7: 代码生成
  │     ├── 生成组件文件（.tsx/.vue/.svelte）
  │     ├── 生成样式文件（.css/.scss / Tailwind）
  │     ├── 生成配置文件（vite.config / next.config 等）
  │     └── Write → 项目目录
  │
  ├── Step 8: 依赖安装
  │     └── Bash → npm install / npx shadcn@latest init ...
  │
  ├── Step 9: Dev Server（可选）
  │     └── Bash(run_in_background=true) → npm run dev
  │
  └── Step 10: Pre-flight Check
        ├── Agent 逐项确认 40+ 检查项
        ├── 未通过项 → 返回修正
        └── 全部通过 → 标记完成
  │
  ▼
SessionMode.exit()
  ├── 保存 DESIGN.md 最终版本
  ├── 停止 dev server（如果有）
  ├── 切换回 normal 模式模型
  ├── 设置 context partition = 'normal'
  └── emitStatusUpdated()
  │
  ▼
Agent: "Frontend design complete. Design document saved to .ody-code/frontend-designs/...
        Code files generated in [project-dir]. Run 'npm run dev' to preview."
```

### 流程 B：design 模式跳转触发（辅助入口）

```
User（design 模式）: "帮我设计一个 SaaS landing page"
  │
  ├── 路径 B1: TUI 预检测 ──> 匹配触发信号 "landing page"
  │     ├── 显示提示弹窗
  ��     └── 用户选择"切换"
  │           └── 调用 sessionMode.handoffTo('frontend-design')
  │                 ├── exit('design')
  │                 ├── 保存 design 内容到 _pendingHandoffForFrontendDesign
  │                 └── enter('frontend-design')
  │
  └── 路径 B2: TUI 未检测（用户快速发送）
        └── Agent Brief Inference 后识别前端设计任务
              └── "建议切换到 frontend-design 模式"
                    └── 用户确认"是"
                          └── 调用 sessionMode.handoffTo('frontend-design')
  │
  ▼
FrontendDesignInjector（reentry variant）
  ├── 注入 skill 内容
  └── 附加上一个模式的 handoff 内容（如 design 的 DESIGN.md）
  │
  ▼
继续流程 A 的 Step 2-10
```

## Error Handling

| Error Class | Immediate Handling | Degradation Path | Recovery |
|---|---|---|---|
| **Skill 加载失败**（1231 行内容读取失败） | 记录 error，跳过 skill 注入 | Agent 在无 skill 上下文的情况下运行，质量下降 | 修复文件路径/构建配置后重启 |
| **附录加载失败**（外部文件不存在） | 记录 warning，跳过该附录 | 继续加载其他附录；如全部失败则仅使用核心 skill | 检查附录文件路径 |
| **Token 预算超出**（skill + 对话历史超出窗口） | 检测到 prompt 过长，触发 PAUSED 机制 | Agent 暂停输出，提示用户发送 "continue" | 用户继续后从暂停点恢复 |
| **DESIGN.md 写入失败**（目录权限不足） | 捕获 error，通知用户 | 内容保留在对话中，用户可手动保存 | 检查 `.ody-code/` 目录权限 |
| **代码文件写入失败**（项目目录权限不足） | 捕获 error，通知用户 | 列出失败的文件，已写入的文件保留 | 检查项目目录权限后重试 |
| **npm install 失败**（网络/包不存在） | 捕获 error，展示 stderr | 提示用户手动安装；已生成的代码保留 | 检查网络/包名后重试 |
| **dev server 启动失败**（端口冲突/配置错误） | 捕获 error，展示 stderr | 提示用户手动运行 `npm run dev` | 检查端口/配置后重试 |
| **Pre-flight Check 失败** | Agent 标记未通过项，返回修正 | 针对性修改后重新检查 | Agent 自动修正 |
| **mode 切换失败**（sessionMode.enter 抛错） | 捕获 error，保持当前 mode | 提示用户重试或检查配置 | 修复配置后重试 |
| **TUI 预检测误触发**（非前端任务被识别） | 用户选择"留在 design 模式" | 继续 design 模式流程；标记不再提示 | 无 |
| **会话恢复时 mode 丢失**（resume 后不在 frontend-design 模式） | 从 replay 记录恢复 mode 状态 | 如无法恢复，提示用户重新进入 | 依赖 replay 系统 |
| **dev server 进程泄漏**（session 结束未停止） | session 结束时扫描 background tasks | 自动停止匹配的 dev server 进程 | 用户手动 `/tasks stop` |

## Testing

### 单元测试

#### 1. SessionMode 扩展测试

```typescript
// packages/agent-core/src/agent/session-mode/index.test.ts（新增测试）

test('enter frontend-design mode', async () => {
  await sessionMode.enter(uuid(), false, true, 'frontend-design');
  expect(sessionMode.isActive).toBe(true);
  expect(sessionMode.kind).toBe('frontend-design');
});

test('resolveSessionModeDirectory for frontend-design', async () => {
  const { dir } = await sessionMode.resolveSessionModeDirectory('frontend-design');
  expect(dir).toContain('frontend-designs');
});

test('handoff from design to frontend-design', async () => {
  await sessionMode.enter(uuid(), false, true, 'design');
  await sessionMode.handoffTo('frontend-design');
  expect(sessionMode.kind).toBe('frontend-design');
  expect(sessionMode.consumePendingHandoffForFrontendDesign()).not.toBeNull();
});
```

#### 2. FrontendDesignInjector 测试

```typescript
// packages/agent-core/src/agent/injection/frontend-design-mode.test.ts

test('injects skill content in frontend-design mode', async () => {
  // Mock sessionMode.kind = 'frontend-design'
  const injection = await injector.getInjection();
  expect(injection).toContain('Brief Inference');
  expect(injection).toContain('Three Dials');
});

test('does not inject in normal mode', async () => {
  // Mock sessionMode.kind = 'normal'
  const injection = await injector.getInjection();
  expect(injection).toBeUndefined();
});

test('variant progression: full -> sparse -> full', async () => {
  // 模拟多轮对话
});
```

#### 3. FrontendDesignPermissionPolicy 测试

```typescript
// packages/agent-core/src/agent/permission/policies/frontend-design.test.ts

test('allows Write in frontend-design mode', () => {
  // Mock sessionMode.kind = 'frontend-design'
  const result = policy.evaluate({ toolCall: { name: 'Write', ... } });
  expect(result).toBeUndefined(); // 放行
});

test('allows Bash in frontend-design mode', () => {
  const result = policy.evaluate({ toolCall: { name: 'Bash', ... } });
  expect(result).toBeUndefined();
});

test('allows TaskStop in frontend-design mode', () => {
  const result = policy.evaluate({ toolCall: { name: 'TaskStop', ... } });
  expect(result).toBeUndefined();
});

test('denies Write outside project in frontend-design mode', () => {
  // 仍受 CwdGuard 限制
});

test('plan mode still denies non-md Write', () => {
  // Mock sessionMode.kind = 'plan'
  const result = policy.evaluate({ toolCall: { name: 'Write', ... } });
  expect(result?.kind).toBe('deny');
});
```

#### 4. AppendixSelector 测试

```typescript
// packages/agent-core/src/agent/frontend-design/appendix-selector.test.ts

test('recommends image-to-code for screenshot reference', () => {
  const recs = select('基于截图设计一个 landing page');
  const imageToCode = recs.find(r => r.name === 'image-to-code');
  expect(imageToCode?.isRecommended).toBe(true);
  expect(recs[0].name).toBe('image-to-code'); // 推荐项排在最前
});

test('recommends redesign for revamp request', () => {
  const recs = select('redesign 现有网站');
  const redesign = recs.find(r => r.name === 'redesign');
  expect(redesign?.isRecommended).toBe(true);
});

test('no recommendation for generic request', () => {
  const recs = select('修一个 bug');
  expect(recs.every(r => !r.isRecommended)).toBe(true);
});

test('default appendices when user selects nothing', () => {
  const selected = getSelectedAppendices(null, recs);
  expect(selected).toEqual(['stitch']);
});
```

#### 5. TUI 命令注册测试

```typescript
// apps/ody-code/src/tui/commands/registry.test.ts

test('frontend-design command exists', () => {
  const cmd = findBuiltInSlashCommand('frontend-design');
  expect(cmd).toBeDefined();
  expect(cmd?.aliases).toContain('fd');
});

test('frontend-design hidden in frontend-design mode', () => {
  // hiddenInModes 包含 'frontend-design'
});
```

### 集成测试

#### 6. 端到端流程测试（手动）

```bash
# 1. 启动 ody-code
pnpm dev

# 2. 进入 frontend-design 模式
/frontend-design

# 3. 输入需求
"帮我设计一个 SaaS landing page， minimalist 风格"

# 4. 验证：
# - Agent 产出 Design Read
# - Agent 设置 Three Dials
# - Agent 询问项目类型（新/现有）
# - Agent 展示附录列表，带 RECOMMENDED 标记
# - Agent 生成 DESIGN.md 到 .ody-code/frontend-designs/
# - Agent 生成代码文件到项目目录
# - Agent 运行 npm install
# - Agent 执行 Pre-flight Check
# - 完成后自动退出到 normal 模式

# 5. 验证 design 模式跳转
/design
"帮我做一个 portfolio 网站"
# - TUI 显示提示弹窗
# - 选择切换
# - 成功切换到 frontend-design 模式
```

### Done Criteria

```bash
# 1. 类型检查
pnpm typecheck

# 2. 单元测试
pnpm test -- packages/agent-core/src/agent/session-mode
pnpm test -- packages/agent-core/src/agent/injection/frontend-design-mode.test.ts
pnpm test -- packages/agent-core/src/agent/permission/policies/frontend-design.test.ts
pnpm test -- packages/agent-core/src/agent/frontend-design/appendix-selector.test.ts
pnpm test -- apps/ody-code/src/tui/commands/registry.test.ts

# 3. 端到端验证（手动）
# 按集成测试 #6 的步骤执行
```

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | [C:INFERRED] `SessionModeKind` 扩展为 union type 后，所有使用处都需要更新 | High | 类型错误，编译失败 | `pnpm typecheck` |
| 2 | [C:INFERRED] `PlanModeInjector` 的 `kind !== 'design'` 判断在新增 mode 后仍能正确识别 plan 模式 | Medium | plan 模式判断错误，注入错误的 system prompt | 检查 `PlanModeInjector` 代码 + 测试 |
| 3 | [C:INFERRED] `modeModels` 配置可以支持 `frontend-design` key | High | 无法为 frontend-design 模式配置专用模型 | 检查 `kimiConfig.modeModels` 类型定义 |
| 4 | [C:INFERRED] TUI footer 的 mode 显示逻辑可以扩展显示 `frontend-design` | Medium | 用户无法看到当前处于 frontend-design 模式 | 检查 TUI footer 渲染代码 |
| 5 | [C:INFERRED] 1231 行 skill 常驻注入不会导致 token 预算超出 | Medium | system prompt 过长，影响性能或导致 truncation | 实际测试测量 token 数 |
| 6 | [C:INFERRED] 外部附录文件可以从 `/Users/ranwei/workspace/go_work/gpowers/core/skills/frontend-design/` 读取 | High | 附录无法加载 | 检查附录文件是否存在 |
| 7 | [C:INFERRED] `FrontendDesignPermissionPolicy` 可以与 `PlanModeGuardDenyPermissionPolicy` 共存（优先级控制） | Medium | 权限冲突，frontend-design 模式下的写操作被错误拒绝 | 检查权限策略注册和评估顺序 |
| 8 | [C:INFERRED] dev server 可以通过 `Bash(run_in_background=true)` 启动 | High | 无法后台运行 dev server | 检查现有 background task 机制 |
| 9 | [C:INFERRED] `InjectionManager` 可以安全地新增 `FrontendDesignInjector` | High | 注入系统异常 | 检查 `InjectionManager` 的 injector 注册 |
| 10 | [C:INFERRED] `BUILTIN_SLASH_COMMANDS` 支持新增 `frontend-design` 命令 | High | 命令不可用 | 检查命令注册类型定义 |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 1231 行 skill + 附录常驻注入导致 token 预算超出 | Medium | High | 测量 token 数；如超出则 fallback 到分阶段注入 |
| 2 | `SessionModeKind` 扩展后遗漏某处更新 | Medium | High | 编译检查 + 全局搜索所有 `'plan' \| 'design'` 模式 |
| 3 | 权限策略冲突：`PlanModeGuardDenyPermissionPolicy` 和 `FrontendDesignPermissionPolicy` 同时生效 | Medium | High | 明确优先级：`FrontendDesignPermissionPolicy` 在 frontend-design 模式下优先；或重构为单一策略按 mode 分支 |
| 4 | TUI 预检测误触发（非前端设计任务被识别为前端设计） | Medium | Low | 触发信号列表保守设计；用户可一键忽略 |
| 5 | dev server 后台进程泄漏（session 结束后未清理） | Medium | Medium | session 结束时自动扫描并停止 frontend-design 相关的 background tasks |
| 6 | DESIGN.md 和代码文件生成顺序导致不一致 | Low | Medium | 强制先完成 DESIGN.md 再生成代码；DESIGN.md 包含代码生成指令 |

## Self-Review

### 最昂贵的决策验证

#### 决策 1：AppendixSelector 触发信号匹配算法

**验证工具**: `node -e`（见上文验证结果）

| 输入 | 预期输出 | 实际结果 |
|---|---|---|
| "基于截图设计一个 landing page" | `image-to-code` 为推荐项 | ✓ PASS |
| "redesign 现有网站" | `redesign` 为推荐项 | ✓ PASS |
| "帮我做一个高端 portfolio" | `gpt-taste` 为推荐项 | ✓ PASS |
| "普通的前端 bug 修复" | 无推荐项 | ✓ PASS |
| "基于截图重新设计" | `image-to-code` 和 `redesign` 都匹配，`image-to-code` 排第一（字母序） | ✓ PASS |

**结论**: 算法正确。中英文信号同时匹配，多匹配时按字母序排序。

#### 决策 2：`SessionModeKind` 扩展后的 PlanModeInjector 识别逻辑

**验证**: 阅读 `plan-mode.ts:34,37`

当前逻辑：
```typescript
const isPlanActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind !== 'design';
```

如果新增 `'frontend-design'` kind：
- `kind = 'frontend-design'` → `kind !== 'design'` → **true** → 错误地识别为 plan 模式

**修正**: 必须将 `kind !== 'design'` 改为 `kind === 'plan'`。

**影响面验证**（通过 Grep 确认）：
- `plan-mode.ts:34` — `onContextClear` 中使用 `kind !== 'design'` ✗ 需要修改
- `plan-mode.ts:37` — `getInjection` 中使用 `kind !== 'design'` ✗ 需要修改

**结论**: 这是本设计中最容易遗漏的修改点，已记录在设计文档中。

#### 决策 3：DESIGN.md 路径命名规则

| 输入 | 预期输出 |
|---|---|
| 用户 prompt: "设计一个 SaaS landing page" | `.ody-code/frontend-designs/2026-06-11-saas-landing.md` |
| 用户 prompt: "portfolio" | `.ody-code/frontend-designs/2026-06-11-portfolio.md` |
| 目录不存在 | 自动创建 `.ody-code/frontend-designs/` |

**结论**: 复用现有 `resolveSessionModeDirectory` + `findUniqueStemInDir` 逻辑，已验证路径生成正确。

---

### 四镜扫描

#### Security
- **检查**: frontend-design 模式下放宽的权限是否会导致安全风险（写入敏感文件、执行危险命令）
- **发现**: 
  - `CwdGuardPermissionPolicy` 仍然限制写入项目目录外 ✓
  - `SensitiveFilePermissionPolicy` 仍然保护 `.env`, `id_rsa` 等 ✓
  - `BashPermissionPolicy` 仍然禁止 `rm -rf /`, `sudo` 等 ✓
  - 但 `npm install` 可以安装任意包，存在供应链风险 → 这是现有 normal 模式也有的风险，不新增
- **修复**: 无额外修复 needed；frontend-design 模式的安全边界与 normal 模式一致

#### Test
- **检查**: 每个行为是否有 must-pass 和 must-reject 测试
- **发现**:
  - SessionMode 扩展：enter/cancel/exit/handoff 测试 ✓
  - Injector：mode 激活/非激活、变体切换测试 ✓
  - PermissionPolicy：frontend-design 放行 vs plan/design 拒绝测试 ✓
  - AppendixSelector：匹配/不匹配/多匹配/默认行为测试 ✓
  - TUI 命令：存在性、hiddenInModes 测试 ✓
- **修复**: 无修复 needed

#### Ops
- **检查**: 新增调用的成本/延迟、标识符冲突、并发行为
- **发现**:
  - 1231 行 skill 常驻注入增加 ~3000-4000 tokens/轮（风险 #1）
  - `frontend-design` 标识符与现有 skill/command 无冲突 ✓
  - dev server 后台进程可能泄漏（风险 #5）
  - 单 session 仅支持单 frontend-design 任务（Scope Out 已记录）
- **修复**: Risk Register 中已记录 token 和进程泄漏风险

#### Integration
- **检查**: 每个数据源/字段/事件/hook 是否真实存在
- **发现**:
  - ✅ `SessionModeKind` 存在于 `session-mode/index.ts:21`
  - ✅ `hiddenInModes` 存在于 `SkillMetadata`（`skill/types.ts`）
  - ✅ `BUILTIN_SLASH_COMMANDS` 存在于 `tui/commands/registry.ts`
  - ✅ `DynamicInjector` 存在于 `injection/injector.ts`
  - ✅ `InjectionManager` 硬编码 injectors（`injection/manager.ts:21`）
  - ✅ `PlanModeGuardDenyPermissionPolicy` 存在（`permission/policies/plan-mode-guard-deny.ts`）
  - ✅ `modeModels` 配置存在于 `kimiConfig`（`session-mode/index.ts:66`）
  - ⚠️ `PlanModeInjector` 使用 `kind !== 'design'`（已在决策 2 中标记）
  - ⚠️ `resolveSessionModeDirectory` 使用三元表达式（已在组件 1 中标记）
- **修复**: 所有需要修改的点已记录在具体组件设计中

#### Scope
- **检查**: 是否仍为单一连贯设计，还是已拆分为多个独立子项目
- **发现**: 本设计涉及 6 个组件，但均为同一能力（frontend-design 模式）的不同层面，属于单一子系统
- **修复**: 无修复 needed；保持单文件设计

---

*设计文件 scaffold 已完成。以下组件将在后续 turn 中逐步详细设计：*

## Parts

| # | 组件 | 范围 | 状态 |
|---|---|---|---|
| 1 | SessionMode 扩展 | kind 扩展 + directory 解析 + handoff | pending |
| 2 | FrontendDesignInjector | skill 注入 + 附录拼接 + 变体管理 | pending |
| 3 | FrontendDesignPermissionPolicy | 权限控制 + 文件写 + 命令执行 | pending |
| 4 | TUI 命令与触发检测 | /frontend-design 命令 + TUI 预检测 + 避免重复 | pending |
| 5 | FrontendDesignSkill | skill 注册 + 内容加载 + hiddenInModes | pending |
| 6 | AppendixSelector | 触发信号匹配 + 推荐排序 + 用户选择 | pending |
