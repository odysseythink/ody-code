# ody-code 综合实施路线图（Master Implementation Roadmap）

**Document Type**: Master Implementation Roadmap (synthesis)
**Last Updated**: 2026-06-18 (A1/A2/A3 ✅ 已完成)
**Status**: DRAFT (awaiting approval)
**Synthesizes**:
- `.ody-code/roadmaps/openhands-inspired-roadmap.md`（OpenHands 启发）
- `.ody-code/roadmaps/ponytail-inspired-roadmap.md`（Ponytail 启发）
- `.ody-code/roadmaps/design-mode: internal reuse-discovery step + Reuse Analysis gate.md`（设计模式复用门）
- `.ody-code/roadmaps/e2e-testing-automation-roadmap.md`（已落地基础 / 依赖）
**Epic Owner**: TBD

---

## 📋 执行摘要

**目标**：把三份独立路线图（OpenHands 启发、Ponytail 启发、设计模式复用门）综合为**一份总的实施路线图**——按工程实践主题归并为 **5 个 Epic**，给出**跨 Epic 的单一发布时间线**、依赖图与共享基础设施复用，作为排期与协调主控；各源路线图保留为对应 Epic 的"明细来源"。

**核心洞察**：三份路线图存在强主题重叠，其中价值最高、协同最强、共享基础设施最多的是 **Epic A —— 代码质量（不重复造 + 不过度建）**，作为旗舰：
- `design-mode` 的 **C8 复用门**（设计期，反重复）、OpenHands **T1-A 微代理复用记忆**（运行期，反重复）、Ponytail **P1-A 简约 skill**（反过度建）本质是同一目标的不同层次，过去分散在三份文档里无法统一排期。归并后形成完整闭环：**先找再造**（复用）→ **先问需不需要**（简约）→ **审查过度设计** → **把简化记成可追踪债务**。

**已落地基线（不在本路线图 TODO 内，作为依赖）**：e2e 测试 Phase 1（`packages/agent-core/src/e2e-testing/`）、设计/计划模式、会话 checkpoint、compaction、MCP、sub-agents、ripgrep 搜索、**仓库知识微代理 A2/T1-A（`packages/agent-core/src/agent/injection/knowledge-microagent.ts`，默认由 `repo-knowledge` flag 门控）**。

**单一时间线总览**：Release 1（快赢）→ Release 2（代码质量核心，旗舰）→ Release 3（闭环+安全）→ Release 4（度量+多语言）→ Backlog（探索性）。Tier 1 级工作量约 15–22 工程周（单人，指示性）。

**Non-Goals（综合）**：OpenHands 的企业/SaaS 多租户与托管运行时；Ponytail 的跨 13+ 宿主适配器分发。ody-code 是**单宿主本地 CLI**，二者均不做，只取其工程实践思路原生实现。

---

## 🗺️ Epic 总览

| Epic | 来源条目（合并） | 核心价值 | 共享基础设施 | 优先级 |
|------|----------------|---------|-------------|--------|
| **A 代码质量：不重复造 + 不过度建（旗舰）** | design-mode C8 + OH T1-A + PT P1-A/P1-B/P1-C | 抑制重复与过度设计，直接提升生成代码质量、降本 | skill 注册解析、注入、ripgrep | 高 |
| **B 工程闭环：环境引导与提交验证** | OH T1-B + e2e（已落地）+ e2e Phase 2 | 会话冷启动自动就绪、提交前自动验证，闭环反馈 | permission、Bash、e2e | 高 |
| **C 可靠性与安全** | OH T1-C + OH T1-D | 打断卡死循环省 token；风险分级减少误授权 | loop 控制器、permission、kaos | 高/中 |
| **D 质量度量与可观测** | PT P2-A + PT P2-B + OH T2-B | 量化 Epic A 收益、守护 i18n 一致、可复现调试 | e2e、agent/records、i18n | 中 |
| **E 增量 / 探索性（Backlog）** | OH T2-A/T2-C/T3-A/T3-B/T3-C + PT P3-A/P3-B/P3-C | 锦上添花与长线探索 | 视项而定 | 探索 |

> 缩写：OH = OpenHands 启发路线图；PT = Ponytail 启发路线图。

---

## 🧩 Epic 详述

### 🥇 Epic A — 代码质量：不重复造 + 不过度建（旗舰）

**统一叙事**：把分散在三份路线图里的"代码质量"条目合并成一条完整链路：
**先找再造**（复用：设计期 C8 门 + 运行期微代理记忆）→ **先问需不需要**（简约阶梯）→ **审查过度设计**（review/audit）→ **把简化记成可追踪债务**（债务台账）。

**Epic 内条目与排序（含依赖）**：

| 编号 | 条目 | 来源 | 集成路径 | 依赖 |
|------|------|------|---------|------|
| **A0** | 设计模式 Step 0.6 内部复用扫描 + C8 完整性门 | design-mode 路线图 | `agent/injection/design-mode-contract.ts`、`tools/builtin/planning/exit-design-mode.ts` 的 `findMissingDesignSections()` | 无（**改动已具体到行，最 ready**） |
| **A1** | 简约阶梯 skill（lite/full/ultra 单文件多档过滤） | PT P1-A | **✅ 已完成** — `packages/agent-core/src/skill/builtin/simplicity-first.ts` 实现 lite/full/ultra 三档过滤；`simplicity-first.md` 技能文本含 LEVEL 标签；`skill/registry.ts` 集成 `filterSimplicityLevels`/`parseSimplicityLevel`；`builtin/index.ts` 注册；`simplicity-first.test.ts` 覆盖纯函数与集成 | 无 |
| **A2** | 仓库知识微代理 / reuse-conventions（关键词触发的运行期复用记忆） | OH T1-A | **✅ 已完成** — `skill/parser.ts` 识别 `type: knowledge` + `triggers`；`skill/scanner.ts` 从 `.ody-code/microagents/` 加载；`agent/injection/knowledge-microagent.ts` 实现匹配、预算、去重、注入；TUI `/microagent` 命令提供创建向导与 `reuse-conventions` 模板 | 复用 A0/A1 同套基础设施；**当前由 `repo-knowledge` flag 门控，默认未启用** |
| **A3** | 反过度设计 review / audit（结构化标签 `delete:/stdlib:/native:/yagni:/shrink:`） | PT P1-B | 新增 review skill/工具，复用 ripgrep | 无 |
| **A4** | 简化债务台账（`ody:` 标记收割 + rot 标记） | PT P1-C | 新增 builtin 工具/skill，复用 ripgrep | **依赖 A1**（agent 先学会留标记）；与 A3 共用 ripgrep |

**关键设计要点**：
- A0 与 A2 是反重复的两层：A0 在**设计期**强制扫描既有组件并按候选问用户 reuse/extend/new（硬门 C8 阻断 `ExitDesignMode`）；A2 在**运行期**按关键词注入"这些组件已存在、如何复用"的事实。二者 + Backlog 的 T3-A 语义搜索三层强化（见下文依赖视图）。
- A1 的"绝不简化掉"硬约束（信任边界校验/错误处理/安全/可访问性/标定）由 Epic D 的 P2-A 行为门控兜底。
- A1 应教 agent 在做简化时主动留 `ody:` 标记，A4 负责收割——故 **A1 先于 A4**。
- A3 命中过度设计时可建议补 `ody:` 债务标记，衔接 A4。

**成功标准**：设计含相似组件时被 C8 门强制做复用分析；运行期输入"加组件/页面"自动浮现复用约定；可被标准库/原生/一行覆盖的任务优先取最简方案（**✅ 已达成** — `simplicity-first` skill 加载后 agent 自动走简约阶梯）；`ody:` 标记可被收割成台账并标出 rot 风险。

---

### 🥈 Epic B — 工程闭环：环境引导与提交验证

**解决的问题**：agent 每次会话"冷启动"（依赖未装、环境未备），且可能提交连项目自身检查都过不了的代码。

| 编号 | 条目 | 来源 | 集成路径 | 依赖 |
|------|------|------|---------|------|
| **B1** | `.ody-code/setup.sh` 会话启动权限门控运行 | OH T1-B | 会话启动钩子 + `agent/permission/`（manual 提示一次，auto/yolo 自动） | 无 |
| **B2** | 提交前验证钩子（`.ody-code/verify.sh` 或检测到的 pre-commit） | OH T1-B | 复用 Bash 工具 + git-cwd 写权限策略；**钩子内调用已落地 `RunE2ETests`**（`packages/agent-core/src/e2e-testing/`） | 复用 e2e Phase 1 |
| **B3** | 失败反馈回路（把失败输出回喂 agent 修复 + 重试预算） | OH T1-B | 验证钩子失败路径 | 依赖 B2 |
| **B4** | e2e Phase 2 多语言生成器（Python/Pytest、Node/Jest）+ 递归影响分析 | e2e 路线图 Phase 2 | `e2e-testing/generators/*`、增强 `impact-analyzer.ts` | 复用 e2e Phase 1 框架 |

**成功标准**：带 `.ody-code/setup.sh` 的仓库首会话自动就绪；破坏 lint/test 的提交被拦截并把失败回喂 agent 修复；多语言项目可自动生成并运行 E2E。

---

### 🥉 Epic C — 可靠性与安全

| 编号 | 条目 | 来源 | 集成路径 | 依赖 |
|------|------|------|---------|------|
| **C1** | 卡死/循环检测（滚动窗口签名 + 纠偏注入 + 升级打断） | OH T1-C | `packages/agent-core/src/loop/`（利用 `AfterStepHook`），telemetry 复用 `agent/records/` | 无（**独立、高 ROI**） |
| **C2** | 规则版风险分级确认（`ConfirmRisky`） | OH T1-D | `agent/permission/policies/`，复用 `path-access.ts`/`sensitive.ts` 信号 | 无 |
| **C3** | 可选 LLM 风险打分（flag 后） | OH T1-D | 同上，flag 门控 | 依赖 C2 |
| **C4** | 可选 kaos 容器沙箱（高 blast-radius 任务隔离执行） | OH T1-D | `packages/kaos` 执行后端，opt-in | 推迟；与 Backlog T3-C 网络隔离配对 |

**成功标准**：诱发的重复编辑循环在窗口内被检出并纠偏，不再撞到 step 上限；`auto` 模式下 `rm -rf` 仓库外等破坏性命令仍触发确认，而仓库内常规编辑不打扰。

---

### Epic D — 质量度量与可观测

| 编号 | 条目 | 来源 | 说明 | 依赖 |
|------|------|------|------|------|
| **P2-A** | 行为门控基准 / Agent 评测 harness | PT P2-A | 不只测正确性，还测 LOC/简约度/完整性；**用来量化 Epic A（A1 简约）的真实收益**，以本仓数据取代 Ponytail 自报数字 | **依赖 A1 已落地**；复用 e2e 基础 |
| **P2-B** | i18n / skill 翻译漂移 canary（承重短语校验） | PT P2-B | 守护中文优先 i18n（`packages/agent-core/src/i18n/`）副本不语义漂移 | 无（**独立、低成本**） |
| **T2-B** | 事件流回放 / 可复现调试 | OH T2-B | 扩展已有 append-only `agent/records/` 走向确定性回放，写回归 fixture | 无 |

---

### Epic E — 增量 / 探索性（Backlog）

| 编号 | 条目 | 来源 | 备注 |
|------|------|------|------|
| **T2-A** | GitHub Issue→PR 解析器 / `CreatePR` 工具 | OH T2-A | 本地 CLI 走 `gh` CLI，不重造 GraphQL |
| **T2-C** | 更丰富的 condenser 策略（LLM 摘要式） | OH T2-C | 复用 `agent/compaction/`，低成本 |
| **T3-A** | 语义代码搜索 / repo-map | OH T3-A | **放大 Epic A 的 find-before-build**（让 agent 真能找到既有组件） |
| **T3-B** | 多 Agent 监督 / 并行协调 | OH T3-B | 扩展现有单次 spawn `Agent` 工具 |
| **T3-C** | 网络隔离（egress 白名单） | OH T3-C | 与 C4 沙箱配对 |
| **P3-A** | 当前模式/简约强度状态栏徽章 | PT P3-A | 单宿主下读会话内模式即可，无需 flag 文件 |
| **P3-B** | config 三级级联默认强度（env > config.toml > 内置） | PT P3-B | 接入 `config/schema.ts` |
| **P3-C** | before/after 示例作为 A1 配套文档 | PT P3-C | 随 Release 2 A1 一起落 |

---

## 📅 单一发布时间线（跨 Epic）

排序原则：**低风险高 ROI 先行 + 依赖驱动**。

| Release | 内容 | 粗略工期（单人，指示性） | 说明 |
|---------|------|------------------------|------|
| **R1 快赢** | **A0**（C8 复用门）+ **C1**（循环检测）+ **P2-B**（i18n canary） | ~3–4 周 | 三者基本独立、风险最低；A0 改动已具体到行，C1 限于 `loop/`，P2-B 纯测试 |
| **R2 代码质量核心（旗舰）** | **A1** 简约 → **A2** 微代理 → **A3** review/audit → **A4** 债务台账；随 A1 落 **P3-C** 示例 | ~5–7 周 | Epic A 主体，按内部依赖串行；**A1 ✅、A2 ✅ 已完成**，A3 ✅ 已完成，A4 待实现；A1 必须先于 A4 |
| **R3 闭环 + 安全** | **B1–B3** 引导/验证钩子（复用已落地 e2e）+ **C2** 风险分级确认 | ~4–6 周 | B 引入脚本执行，受益于 C2 风险门已设计 |
| **R4 度量 + 多语言** | **P2-A** 行为基准（A1 落地后可量化）+ **B4** e2e Phase 2 + **T2-B** 回放 | ~3–4 周 | P2-A 依赖 R2 已交付 A1 |
| **Backlog 探索性** | Epic E 全部 + **C3/C4** 沙箱 + **T3-A** 语义搜索 | 持续 | 视 R1–R4 成效与 telemetry 再评估 |

**依赖图（ASCII）**：

```
R1 ── A0 设计期复用门 ─┐
      C1 循环检测      │ (独立)
      P2-B i18n canary │ (独立)
                       │
R2 ── A1 简约 skill ✅ ──┼──► A4 债务台账 (A1 教 agent 留 `ody:` 标记)
      A2 微代理 ✅ ──────┤    (运行期复用记忆, 强化 A0)
      A3 review/audit ✅ ─┘──► 命中 → 建议补债务标记 → A4
      P3-C 示例 ◄─ 随 A1
                       │
R3 ── B1 setup.sh ─────┤ (permission 门控)
      B2 verify 钩子 ──┼──► 调用 e2e Phase 1 (已落地)
      B3 反馈回路 ◄─ B2 │
      C2 风险分级确认 ──┘ (B 的脚本执行受益于此)
                       │
R4 ── P2-A 行为基准 ◄── 依赖 A1；量化简约收益；复用 e2e
      B4 e2e Phase 2 ── 复用 e2e Phase 1 框架
      T2-B 事件回放 ─── 扩展 agent/records
                       │
Backlog ── T3-A 语义搜索 ──► 放大 A0/A2 的 find-before-build
           T2-A / T2-C / T3-B / T3-C / C3 / C4 / P3-A / P3-B
```

---

## 🔗 依赖与共享基础设施（综合视图）

**反重复三层强化**（Epic A 的核心协同）：
- **设计期**：A0 C8 门——提案前强制扫描并按候选问用户 reuse/extend/new。
- **运行期**：A2 微代理——按关键词注入"已存在、如何复用"的事实记忆。
- **检索底座**：Backlog T3-A 语义搜索/repo-map——让前两者"真的能找到"既有组件。

**共享基础设施一次性梳理**：

| 基础设施 | 被哪些条目复用 |
|---------|---------------|
| skill 注册/解析 `skill/registry.ts` + 注入 `agent/injection/` | A0、A1、A2 |
| ripgrep 搜索 | A3、A4、T3-A |
| 已落地 e2e `packages/agent-core/src/e2e-testing/` | B2、B4、P2-A |
| 权限系统 `agent/permission/` | B1、C2 |
| `agent/records/` / telemetry | C1、T2-B |
| `agent/compaction/` | T2-C |
| `packages/kaos` | C4 |

**已落地基线（依赖，不在 TODO 内）**：e2e Phase 1（49 测试通过）、设计/计划模式、checkpoint、compaction、MCP、sub-agents、ripgrep。

---

## ⚠️ 风险与 Non-Goals（综合）

**Non-Goals（汇总各源路线图）**：
- OpenHands 企业/SaaS 多租户（org 路由、webhook、Redis 缓存）、托管/云运行时、app-server↔agent-server REST 拆分。
- Ponytail 的跨 13+ 宿主适配器分发（`.cursor/`、`.windsurf/`、`.opencode/` 等）与多宿主 flag 路由。
- 理由：ody-code 是单宿主本地 CLI，只取工程实践思路原生实现。

**风险表（汇总各源 + 跨 Epic 新增）**：

| 风险 | 缓解 |
|------|------|
| 简约强度过激破坏正确性 | A1 保留"懒但不破"硬约束；P2-A 行为门控兜底 |
| 微代理/复用扫描注入膨胀 token | A2 关键词门控注入、按会话去重、设上限（与 A0/A1 注入共享预算需统一管理） |
| **跨 Epic 新增：Epic A 多条目并行的注入预算冲突** | 统一注入预算管理：A0 设计契约、A1 简约 skill、A2 微代理共用一套 token 预算与优先级（project > user > builtin） |
| setup.sh/verify 脚本执行未审查代码 | B1 经 permission 门控，manual 模式提示一次，绝不静默运行 |
| 循环检测误报中断合法迭代 | C1 保守默认阈值、先纠偏后打断、可 opt-out |
| 风险分级过度打扰/漏判 | C2 先规则版按模式调参，C3 LLM 打分置于 flag 后 |
| 债务标记被滥用为逃避 review | A4 强制写升级触发条件，无触发即标 rot |
| i18n 副本悄悄漂移 | P2-B 承重短语 canary 测试 |
| 范围蔓延到多宿主/SaaS | 明确列入 Non-Goals |

---

## 🚀 下一步

1. 审批/裁剪本总路线图（可按 Release 粒度取舍）。
2. 从 **Release 1** 起步：
   - **A0** 直接进入实现——其源路线图（`design-mode: internal reuse-discovery step + Reuse Analysis gate.md`）已有**行级改动与验证命令**，可直接执行。
   - **C1**、**P2-B** 并行（彼此独立）。
3. **Release 2** 旗舰按 A1→A2→A3→A4 串行推进；对 A1 单独跑 `/plan` 产出实现计划。
4. 各源路线图保留为对应 Epic 的"明细来源"，本总图作为排期与协调主控；每个 Release 结束回看 telemetry 再决策下一档。

---

## 📖 相关文档

**源路线图（Epic 明细来源）**：
- `.ody-code/roadmaps/openhands-inspired-roadmap.md` —— Epic A(A2)、B、C、D(T2-B)、E 多数条目。
- `.ody-code/roadmaps/ponytail-inspired-roadmap.md` —— Epic A(A1/A3/A4)、D(P2-A/P2-B)、E(P3-*)。
- `.ody-code/roadmaps/design-mode: internal reuse-discovery step + Reuse Analysis gate.md` —— Epic A(A0)，含行级改动。
- `.ody-code/roadmaps/e2e-testing-automation-roadmap.md` —— 已落地基线（Phase 1）+ B4（Phase 2）。

**已落地实现 / 设计 / 计划文档**：
- e2e 实现：`packages/agent-core/src/e2e-testing/`；设计 `.ody-code/designs/2026-06-16-e2e-testing-automation-phase-1.md`；计划 `.ody-code/plans/2026-06-16-e2e-testing-automation-phase-1.md`。

---

**Version**: 1.0
**Status**: DRAFT (awaiting approval)
