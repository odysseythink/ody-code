# In-Context Design Mockup Evaluation

## Problem Statement

ody-code 的 design mode 当前通过 `ShowDesignMockup` 工具把设计候选渲染成**独立的 HTML 文件**，再打开浏览器让用户查看。这个流程的痛点是：用户只能看到组件在真空里的样子，无法直接判断它放进**自己的真实项目**后是否和谐、是否破坏现有布局、是否与主题/响应式冲突。每次都需要用户在大脑里做一次“上下文投射”。[C:USER]

具体表现为：
- 独立 mockup 的字体、颜色、间距与目标项目脱节，导致 approvals 时反复修改。
- 用户需要手动把方案搬运到代码里才能验证最终效果，设计 → 实现 的反馈环太长。
- 当需要比较多个视觉方案时，独立页面无法同时呈现“项目原有界面 + 新组件”的叠加关系。[C:INFERRED]

## What Makes This Cool

终极形态不是“在浏览器里看一个更真的预览”，而是 **AI 能自动判断组件在用户项目里的最佳落位点**：哪里插入不破坏布局、如何继承现有主题、在不同断点下表现如何。用户看到的是“我的页面，但那个位置已经长出了新组件”，并且可以一次性对比 2–3 个落位/样式变体。[C:USER]

这个体验的“wow 时刻”是：用户只说了“在 landing page 上加一个 pricing section”，AI 不但画出来，还直接把它放到用户本地项目的真实页面里，且风格一致。[C:USER]

## Constraints

- **不改动 design mode 的核心工作流**：仍然先写设计文档、经用户批准后再进入实现；mockup 只是辅助决策的“视觉同伴”。[C:USER]
- **必须复用现有基础设施**：优先复用 2026-06-08 浏览器原生控制设计（`BrowserConnectionManager` / CDP / Puppeteer）和已有的 `ShowDesignMockup` 机制，不另起炉灶。[C:USER]
- **最小权限与可降级**：如果用户环境没有 Chrome、没有运行中的 dev server、或无法连接 CDP，必须能 gracefully 回退到独立 HTML mockup。[C:INFERRED]
- **仅在 design mode 下使用**：该工具仍是设计阶段的视觉辅助，不跨越到自动修改用户源代码。[C:USER]

## Premises

| ID | Premise | Confidence | Verification |
|---|---|---|---|
| P1 | 用户更需要在真实项目上下文里预览 mockup，而非独立的 HTML 文件 | Medium | 来自 idea evaluator 报告 [V:STATED]；需后续 design mode 使用日志或访谈确认 |
| P2 | 用 Chrome DevTools / CDP 注入真实 DOM 是达成“最真效果”的最快路径 | Medium | 2026-06-08 浏览器控制设计已规划原生能力 [C:INFERRED]；技术上可行但依赖实现进度 |
| P3 | AI 能可靠判断组件在用户项目里的最佳位置 | Low | 这是终极愿景，尚未验证；静态重建方案可先验证 placement 逻辑 [C:INFERRED] |
| P4 | 开源社区 / 技术博主会被“AI 把组件放进真实项目”的 demo 打动 | Low | 用户主观判断 [V:STATED]；需本周通过 demo 视频验证 |
| P5 | 静态源码重建上下文（不依赖运行项目）是成本最低的 MVP 路径 | Medium | 用户选择 [C:USER]；需验证是否能保留足够多的真实上下文 |

## Approaches

### A. 浏览器 CDP 注入真实 DOM

**What it looks like**
通过 `chrome-devtools-mcp` 或原生 `BrowserConnectionManager` 连接到用户正在浏览的本地 dev server 页面，用 `BrowserEvaluate` 把组件 HTML/CSS 注入到真实 DOM 树的指定位置，并返回截图/页面状态。

**What has to be true**
- 用户本地有 Chrome 且项目已 `npm run dev`。
- Agent 能解析页面 DOM 结构并安全地注入样式隔离（Shadow DOM / scoped CSS）以避免污染。
- CDP 连接稳定（2026-06-08 设计已识别 macOS 连接风险）。[C:INFERRED]

**Biggest risk**
环境依赖重：用户可能没开浏览器、没启动 dev server、或页面是动态渲染（SSR/CSR），导致 demo 成功率低，影响“wow”体验。

### B. 页面截图 + 叠加层

**What it looks like**
用现有浏览器工具截取目标页面，生成一个自包含 HTML：底层是页面截图，上层用绝对定位把 mockup 半透明叠加在 AI 建议的位置。用户看到的是“看起来像真实上下文”的静态合成图。

**What has to be true**
- 能获取到目标页面截图（CDP 或本地截图）。
- AI 能根据截图推断可放置区域。
- 用户对“静态合成”的保真度能接受。

**Biggest risk**
不是真正的 WYSIWYG，只能解决“大致看看”的问题；对于主题继承、响应式、交互状态等无法验证。

### C. 静态源码重建上下文（Recommended First Version）

**What it looks like**
读取用户项目的源码（入口 HTML、全局 CSS、主题 token、布局框架），在一个隔离的 iframe 中重建一个**足够真实的页面上下文**，再把 mockup 注入到 AI 选定的位置。整个预览是单个自包含 HTML 文件，可被 `ShowDesignMockup` 直接打开。

**What has to be true**
- 项目使用常见前端框架（React/Vue/Svelte/纯 HTML）且源码结构可被解析。
- 全局样式和主题 token 能被抽取并应用到 iframe 中。
- AI 的 placement 逻辑在重建上下文中有效。

**Biggest risk**
重建的上下文可能丢失动态数据、运行时样式、JS 交互，导致预览与真实页面仍有差距；但它是验证 P3 的最便宜实验台。

## Recommended Approach

**第一阶段：静态源码重建上下文（Approach C）**

理由：
1. 用户已明确选择它作为“第一版”路径 [C:USER]。
2. 不依赖运行中的 dev server 或 Chrome CDP，demo 成功率高，适合本周做传播验证（P4 的 falsification test）。
3. 能在一个可控环境里优先验证“AI 自动 placement”逻辑（P3），为后续真实 DOM 注入积累启发式规则。

**第二阶段：CDP 真实 DOM 注入（Approach A）**

当第一阶段验证了 placement 规则、且 2026-06-08 浏览器原生控制实现落地后，再迁移到真实页面注入，实现真正的 WYSIWYG。

**第三阶段：A/B 对比 + 行为数据（10x 愿景）**

在真实注入能力稳定后，支持同时渲染多个 placement/样式变体，并收集用户反馈，形成闭环优化。

## Open Questions

1. **静态重建的精度边界**：对于使用 CSS-in-JS、Tailwind 运行时生成、或 SSR 的项目，重建上下文能保留多少真实样式？[C:INFERRED]
2. **placement 规则来源**：AI 是仅基于页面 DOM + 主题推断位置，还是需要用户先指定“大概区域”？[C:INFERRED]
3. **与现有 `ShowDesignMockup` 的关系**：是扩展该工具（增加 `contextUrl` / `projectPath` 参数），还是新增独立工具 `ShowInContextMockup`？[C:INFERRED]
4. **权限与沙箱**：重建上下文时读取用户项目源码是否触及新的权限策略？是否需要对敏感文件跳过？[C:INFERRED]

## Success Criteria

1. **MVP（第 1–2 周）**：在 3 个不同的示例项目（纯 HTML、React + Tailwind、Vue）上，能自动生成包含真实上下文 + mockup 的预览 HTML，且视觉风格与项目一致。
2. **Placement 验证（第 2–3 周）**：AI 建议的组件位置在 5 组真实页面任务中，有 ≥3 组被用户认为“合理或接近合理”（无需手动大调）。
3. **传播信号（本周）**：demo 视频在 X/Reddit/HN 上获得 ≥3 个真实用户表达“想试用”或询问实现方式。若未达标，则 P4 不成立，需重新评估目标受众。
4. **工程稳定性**：在 CDP 不可用时，能在 1 秒内回退到独立 HTML mockup，且不影响 design mode 流程。

## Distribution Plan

**本周验证动作**
- 用最小可运行原型（基于现有 `ShowDesignMockup` + 静态源码重建），录制 30 秒屏，展示“AI 把组件放进用户真实项目上下文”的效果。
- 发布到 X、r/webdev、Hacker News Show 等渠道。
- 指标：真实用户表达试用意愿的数量 ≥3。

**后续分发**
- 若验证有效，在 ody-code 的 design mode 中默认启用该能力，并通过 release note / changelog 告知用户。
- 对开源社区：提供可独立运行的 demo 仓库，降低外部贡献者参与门槛。

## Next Steps

1. **今天**：确认 Open Questions #3（扩展 `ShowDesignMockup` vs 新增工具）和 #4（权限边界）。
2. **本周内**：实现最小静态源码重建原型，跑通 1 个示例项目。
3. **本周内**：录制 demo 视频并发布，执行 P4 的 falsification test。
4. **下周**：根据反馈决定是继续打磨静态方案，还是切换到 CDP 注入路径。
5. **持续**：跟踪 2026-06-08 浏览器原生控制的实现进度，评估复用时机。

## What I Noticed

1. **bolt.diy 其实仍然活跃**。通过 GitHub API 查看，其最新 commit 在 2026-02-07（feat: add Cerebras and Fireworks providers；feat: add web URL content fetcher），项目 README 也列出了 Electron App、MCP、Diff View、Supabase 等大量近期新增能力。所以“这类产品 UX 不好”并不是它活跃度低的原因——它活跃度并不低。[C:USER，基于 GitHub API 观察]

2. **OpenUI 确实趋于停滞**。其最新 commit 在 2025-09-15，距今约 9 个月。可能原因包括：W&B 内部把它当原型/实验（README 明确说“a tool we're using at W&B to test and prototype”）、商业化路径不清晰、或生成式 UI 的泛化难度超预期。不能简单归因于 UX 差，更可能是**从 demo 到日常工具鸿沟太大**。[C:INFERRED，基于 GitHub API 观察]

3. **真正的坑是“上下文保真度”**。独立生成 UI 相对容易；但把它无违和地放进用户已经写了几百行的真实项目，需要处理主题、布局、响应式、动态数据、私有组件——这才是最脏的活。 bolt.diy 选择绕过这个问题（在 WebContainer 里从零生成完整应用），而你想解决的问题比它更难：不是“生成一个新项目”，而是“改造一个已有项目”。[C:INFERRED]

4. **现有资产很丰富**：`ShowDesignMockup` 工具、design mode contract、2026-06-08 浏览器控制设计、frontend-design mode 都可以被复用。你不用从零造轮子，但需要把它们串成一条闭环。[C:INFERRED]

## Assumptions

| ID | Assumption | Confidence | Impact if Wrong | How to Verify |
|---|---|---|---|---|
| A1 | ody-code 用户在 design mode 下会频繁使用视觉 mockup | Medium | 功能使用率低于预期 | 查看现有 `ShowDesignMockup` 调用日志 |
| A2 | 静态源码重建能保留足够让 AI 做出合理 placement 的上下文 | Medium | placement 质量差，用户不信任 | 用 5 个真实项目做盲测 |
| A3 | 2026-06-08 浏览器原生控制设计会按期实现并暴露稳定接口 | Medium | 第二阶段 CDP 注入延期 | 跟踪实现进度 |
| A4 | 用户愿意让 Agent 读取项目源码以生成上下文预览 | Medium | 隐私/权限顾虑导致功能被关闭 | 在权限策略中提供明确授权提示 |
| A5 | 开源社区对“AI 注入真实项目”的 demo 反应可作为有效产品信号 | Low | 被 demo 吸引的人不等于真实用户 | 同步追踪 demo 后实际试用/贡献转化率 |
