# Ponytail 启发的工程实践路线图（ody-code）

**Document Type**: Product Roadmap (research-derived)
**Last Updated**: 2026-06-17
**Status**: DRAFT (awaiting approval)
**Source Study**: ponytail 4.7.0（`~/Downloads/ponytail-4.7.0`）
**Epic Owner**: TBD

---

## 📋 执行摘要

**目标**：从 ponytail 4.7.0 中提炼对**软件工程实践**有切实帮助的功能与设计思路，迁移进 ody-code（本地 TypeScript 编程 Agent CLI），并整理为可执行的分层路线图。

**核心论点**：ody-code 已经有“**不要重复造**”的能力——设计模式下的 `reuse-discovery`（先扫描既有组件再决定复用/扩展/新建，见 `.ody-code/roadmaps/design-mode: internal reuse-discovery step + Reuse Analysis gate.md`）。但 ody-code **缺少“不要过度建”的能力**。Ponytail 的全部身份恰恰是后者——“懒惰资深工程师”：在写任何代码前强制走一条**简约阶梯**：

1. 这东西到底需不需要存在？（YAGNI）
2. 标准库能不能做？
3. 平台/语言原生能力能不能覆盖？
4. 已安装的依赖能不能解决？
5. 能不能一行搞定？
6. 以上都不行，才写**最小可用**实现。

二者互补：reuse-discovery 管横向去重，simplicity 管纵向减负。把 ponytail 的思路引入后，ody-code 生成的代码会更短、更简、更省 token、更易维护。

**价值主张**：
- ✅ 抑制过度设计——生成代码前强制走简约阶梯，砍掉重造标准库 / 投机抽象 / 死灵活性。
- ✅ 降本提速——更少代码意味着更低 token 成本与更快产出（ponytail 自报基准：代码量 ↓80–94%、成本 ↓47–77%、速度 ↑3–6×；见下文“证据”一节，需在 ody-code 上自测复核）。
- ✅ 让“偷懒”可追踪——把刻意的简化变成带天花板、带升级触发条件的工程债务台账，避免烂掉。
- ✅ 质量不打折——简约的同时，信任边界校验 / 错误处理 / 安全 / 可访问性 / 硬件标定**绝不被简化掉**。

**过滤标准**：ody-code 是**单宿主本地 CLI**。Ponytail 的核心工程量其实在“跨 13+ 宿主的适配器分发”（Claude Code / Codex / Cursor / Windsurf / OpenCode …），这部分对单宿主的 ody-code **不适用**（见 Non-Goals）。我们只取其**规则内容与工程实践模式**，用 ody-code 既有的 skill / injection / permission 基础设施**原生实现**，不照搬其多宿主机制。

**时间线**：Tier 1 约 5–8 工程周（单人，指示性）；Tier 2/3 持续推进。
**优先级**：Tier 1 = 高，Tier 2 = 中，Tier 3 = 探索性。

---

## 🔬 方法论与来源

Ponytail 在结构上可分为两层，只有其中一层对 ody-code 可迁移：

| 层 | 在 checkout 中的位置 | 对 ody-code 的相关性 |
|----|---------------------|---------------------|
| **多宿主分发层** | `.cursor/`、`.windsurf/`、`.opencode/`、`.codex-plugin/`、`.claude-plugin/`、`hooks/*-runtime.js` 等适配器 | **基本不相关**——解决“一套规则分发到 13 个宿主且不漂移”的问题，单宿主 ody-code 没有这个问题。 |
| **规则内容 + 工程实践模式** | `skills/*/SKILL.md`、`AGENTS.md`、`benchmarks/`、`scripts/check-rule-copies.js`、`examples/*.md` | **直接相关**——简约阶梯、反过度设计 review/audit、债务台账、行为门控基准、漂移检测 canary，可 1:1 映射到 ody-code 的 skill/注入/测试体系。 |

**证据路径（均位于 `~/Downloads/ponytail-4.7.0/`，已核实存在）**：
- 简约阶梯 skill 与三档强度（lite/full/ultra）：`skills/ponytail/SKILL.md`；单文件多档**运行时过滤**：`hooks/ponytail-instructions.js`。
- 反过度设计 review（diff 范围）：`skills/ponytail-review/SKILL.md`；audit（全仓库范围）：`skills/ponytail-audit/SKILL.md`。结构化标签 `delete:/stdlib:/native:/yagni:/shrink:`。
- 简化债务台账：`skills/ponytail-debt/SKILL.md`（约定 `ponytail:` 标记 = 天花板 + 升级触发条件，无触发条件即标记 rot 风险）。
- 行为门控基准：`benchmarks/behavior.js`（不只测代码能否跑通，还测 LOC / 简约度 / 完整性，防止“terse but broken”刷分）。
- 单一事实源 + 漂移检测：`scripts/check-rule-copies.js`（用“承重短语” invariants 做 canary 校验副本不漂移）；规则单源：`AGENTS.md`。
- “懒但不破”的硬约束与 before/after 示例：`AGENTS.md`、`examples/{caching,api-endpoint,date-picker,email-validation}.md`。

**ody-code 现状（gap 框定，均已核实存在）**：
- 已有：builtin skill 系统（`packages/agent-core/src/skill/builtin/`）、skill 注册/解析（`packages/agent-core/src/skill/registry.ts`）、动态上下文注入（`packages/agent-core/src/agent/injection/`）、设计/计划模式、reuse-discovery、E2E 测试（`packages/agent-core/src/e2e-testing/`）、权限系统（`packages/agent-core/src/agent/permission/`）、中文优先 i18n（`packages/agent-core/src/i18n/`）。
- **确认缺失**：简约阶梯 skill、反过度设计的 review/audit、简化债务台账、行为门控基准、i18n/skill 翻译漂移检测。

> 关于基准数字：ponytail 的 80–94% / 47–77% / 3–6× 来自其自身 `benchmarks/`（5 任务 × 3 方法 × 3 模型，10 次中位）。本路线图引用这些数字仅作动机佐证；P2-A 落地后应在 ody-code 上自测，以本仓数据为准。

---

## 📊 候选功能编目（按工程价值排名）

| 编号 | 候选 | ponytail 证据 | ody-code gap（集成路径） | 工程价值 | 迁移难度 |
|------|------|--------------|------------------------|---------|---------|
| **P1-A** | **简约阶梯 Skill**（lite/full/ultra 强度档） | `skills/ponytail/SKILL.md`、`hooks/ponytail-instructions.js` | 无简约 skill → 新增于 `skill/builtin/` + `agent/injection/` | ★★★★★ | 低 |
| **P1-B** | **反过度设计 Review / Audit** | `skills/ponytail-review/SKILL.md`、`skills/ponytail-audit/SKILL.md` | 无“简化视角”审查 → 新增 review skill/工具，复用 ripgrep | ★★★★☆ | 低–中 |
| **P1-C** | **简化债务台账**（`ody:` 标记 + 升级触发器） | `skills/ponytail-debt/SKILL.md` | 完全缺失 → 新增 builtin 工具/skill，复用 ripgrep | ★★★★☆ | 低–中 |
| **P2-A** | **行为门控 Agent 评测/基准 harness** | `benchmarks/`、`benchmarks/behavior.js` | 有 E2E 但无 agent 输出质量评测 → 与 `e2e-testing/` 协同 | ★★★☆☆ | 中–高 |
| **P2-B** | **i18n / Skill 翻译漂移检测 canary** | `scripts/check-rule-copies.js` | 中文优先 i18n 副本可能漂移 → 新增 canary 测试 | ★★★☆☆ | 低 |
| **P3-A** | **当前模式/强度状态栏徽章** | `hooks/ponytail-statusline.sh`、flag 文件 | TUI 无强度徽章 → TUI 显示 | ★★☆☆☆ | 低 |
| **P3-B** | **config 驱动默认强度**（env > config.toml > 内置三级级联） | `hooks/ponytail-config.js` | 无简约强度配置 → 接入 config schema | ★★☆☆☆ | 低 |
| **P3-C** | **before/after 示例作为 skill 配套文档** | `examples/*.md` | skill 缺配套对照示例 → 文档化 | ★★☆☆☆ | 低 |

★ = 相对工程实践影响，非工作量。

---

## 🥇 Tier 1 — 详细分层设计

### P1-A — 简约阶梯 Skill（核心）

**解决的问题**：ody-code 缺少在“写代码前”强制做减法的机制。Agent 容易重造标准库、堆叠投机抽象、为不存在的未来需求预留灵活性。

**设计**：
- 在 `packages/agent-core/src/skill/builtin/` 新增一个 procedural skill（建议命名 `simplicity-first` 或 `lazy-senior`），编码 ponytail 简约阶梯（YAGNI → 标准库 → 平台原生 → 已有依赖 → 一行 → 最小可用），以及输出纪律：**代码先行，最多 3 行说明（跳过了什么、何时再加），解释长于代码即失败**。
- **复用既有基础设施**：
  - skill 解析/注册：`packages/agent-core/src/skill/registry.ts`（已支持带 frontmatter 的 `.md` skill）。
  - 动态注入：`packages/agent-core/src/agent/injection/`（已是 mode-aware 上下文注入的既定机制）。
- **强度档（lite/full/ultra）**：借鉴 ponytail 的**单文件多档过滤**（`hooks/ponytail-instructions.js`）——同一个 SKILL.md 用表格行/示例标签标注 lite/full/ultra，运行时按当前强度过滤出对应内容。**避免维护三份文件、避免档位间漂移**。
  - lite：照常实现，附带提一句更懒的替代方案。
  - full（默认）：强制走阶梯。
  - ultra：YAGNI 极端派，敢于质疑需求本身。
- **绝不简化掉的硬约束**（照搬 `AGENTS.md` 的“懒但不破”原则）：信任边界的输入校验、错误处理、安全、可访问性、硬件标定。“没有配套检查的偷懒代码 = 未完成”。
- **与 reuse-discovery 的分工**：reuse-discovery（设计模式）= 横向去重（先找既有组件）；simplicity skill = 纵向减负（先问需不需要、能否更简）。文档需明确二者协同，避免概念重叠。

**分阶段**：

| 阶段 | 范围 |
|------|------|
| A.1 | 撰写 skill 内容（阶梯 + 输出纪律 + 硬约束），注册进 builtin。 |
| A.2 | lite/full/ultra 单文件多档过滤逻辑，接入注入机制。 |
| A.3 | 与 design/plan 模式联动（设计阶段即提示简约取舍）。 |
| A.4 | 随附 starter 对照示例（见 P3-C）；文档说明与 reuse-discovery 的分工。 |

**成功标准**：当任务可被标准库/原生能力/一行代码覆盖时，agent 优先采用最简方案并给出简短取舍说明；强度档切换可改变行为且不维护重复文案；硬约束项在任何强度下都不被省略。

---

### P1-B — 反过度设计 Review / Audit

**解决的问题**：ody-code 现有 code-review 偏正确性视角，缺少专门“猎杀过度设计”的审查视角；builtin skill 中未发现简约审查（故为**新增**而非改造）。

**设计**：
- 两种粒度（对应 ponytail 的 review 与 audit）：
  - **review**：diff 范围。只查过度设计——重造标准库、无谓依赖、投机抽象、死灵活性。
  - **audit**：全仓库范围，按“可削减代码量”从大到小排名，结尾给出 `净计：-N 行，-M 依赖 可省`。
- **结构化输出标签**（照搬 ponytail）：`delete:`（整段删）、`stdlib:`（标准库替代）、`native:`（平台原生替代）、`yagni:`（用不上）、`shrink:`（可缩短）。格式 `L<行>: <标签> <现状>。<替代>。`
- **只报告、不自动改**：尊重用户决定删什么（与 ponytail 一致，也与 ody-code review skill 的现有约定一致）。
- **复用**：ripgrep 搜索基础设施用于 audit 的全仓库扫描；与现有 code-review skill 并列为“简化视角”补充。

**分阶段**：

| 阶段 | 范围 |
|------|------|
| B.1 | review skill（diff 范围 + 结构化标签输出）。 |
| B.2 | audit 模式（全仓库扫描 + 按削减量排名 + 净计汇总）。 |
| B.3 | 与 P1-A 联动：review 命中可建议补 `ody:` 债务标记（衔接 P1-C）。 |

**成功标准**：对一段过度设计的 diff，review 能逐行给出带标签的删减建议且不擅自改动；audit 能产出按削减量排名的全仓库报告。

---

### P1-C — 简化债务台账（亮点）

**解决的问题**：刻意的简化（“先用全局锁，以后再说”）若无记录，会悄悄烂成永久技术债，且无人知道何时该升级。ody-code 当前完全没有这类追踪能力。

**设计**：
- **约定标记**：`// ody: <天花板>, <升级触发条件>`
  - 例：`// ody: 全局锁，吞吐 > 100 rps 时改为按账户锁`
  - 标记必须同时写明**天花板**（这个简化的能力上限）和**升级触发条件**（什么时候该升级）。
- **新增 builtin 工具/skill**：全仓库收割所有 `ody:` 标记，汇总成债务台账，输出 `<文件>:<行> — <被简化了什么>。天花板：<上限>。升级：<触发条件>。`
- **rot 风险标记**：对**没有升级触发条件**的条目（光偷懒不写何时升级）显式标红，防止变成永久债务。
- **复用**：ripgrep 搜索基础设施（与 P1-B 的 audit 共用扫描能力）。
- **与 P1-A 的依赖**：P1-A 的简约 skill 应教 agent 在做简化时**主动留下 `ody:` 标记**；P1-C 负责收割。故 **A 先于 C**。

**分阶段**：

| 阶段 | 范围 |
|------|------|
| C.1 | `ody:` 标记约定 + 全仓库收割工具/skill。 |
| C.2 | 台账输出格式 + rot 条目（缺升级触发）标记。 |
| C.3 | 与 P1-A 联动：简约 skill 在简化处自动建议写 `ody:` 标记。 |

**成功标准**：仓库中所有 `ody:` 标记被汇总为一张有序台账；缺升级触发条件的条目被显式标为 rot 风险。

---

## 🥈 Tier 2 — 方向性

### P2-A — 行为门控 Agent 评测 / 基准 harness

**解决的问题**：ody-code 有 E2E 测试（验证功能正确），但没有评测**自身 agent 输出质量**（是否过度设计、是否简约）的手段。

**设计**：
- 借鉴 ponytail benchmark 结构（多任务 × 多方法 × 多模型，取中位）与 `benchmarks/behavior.js` 的**行为门控**理念：
  - 不只测“代码能否跑通”，还测 **LOC（代码行数）/ 简约度 / 完整性**（如要求的说明没被截断、硬约束项没被省略）。
  - 防止“terse but broken”——又短又错的代码不应在简约指标上得高分。
- 用途：在引入 P1-A 后，量化简约 skill 的真实收益（代码量、成本、正确性是否退化），以**本仓数据**取代 ponytail 的自报数字。
- **复用/协同**：现有 E2E 测试基础设施（`packages/agent-core/src/e2e-testing/`）提供正确性门，本项补充“简约/完整性门”。

### P2-B — i18n / Skill 翻译漂移检测 canary

**解决的问题**：ody-code 是**中文优先**且带 i18n（`packages/agent-core/src/i18n/`）。skill 文案与 i18n 副本在多语言维护中容易发生**语义漂移**（中文改了英文没跟上，或反之），导致行为不一致。

**设计**：
- 借鉴 `scripts/check-rule-copies.js`：定义一组**承重短语（load-bearing invariants）**——那些必须在各语言副本中都保留语义的关键句（如硬约束、阶梯顺序）。
- 写一个测试 canary，校验这些承重短语在中英文 skill / i18n 副本中都存在（不要求逐字相等，只抓漂移），任一副本漂移即测试失败。
- 成本低、收益高，直接守护 ody-code 多语言一致性。

---

## 🥉 Tier 3 — 探索性

- **P3-A 状态栏徽章**：在 TUI 显示当前简约强度（如 `[简约:ULTRA]`），让用户随时看到当前模式。借鉴 ponytail 的持久 flag 文件 + `hooks/ponytail-statusline.sh`（ody-code 单宿主下无需 flag 文件，直接读会话内模式状态即可）。
- **P3-B config 驱动默认强度**：借鉴 `hooks/ponytail-config.js` 的三级级联——环境变量（`ODY_*`）> `config.toml` > 内置默认（`full`）。让用户全局设一次默认强度。
- **P3-C before/after 示例作为 skill 配套文档**：借鉴 `examples/{date-picker,email-validation,caching,api-endpoint}.md` 的“无简约 vs 有简约”对照，给 P1-A 的 skill 配一组中文对照示例（如：日期选择用 `<input type="date">` 而非引库；缓存先问“需不需要缓存”再用 stdlib LRU）。提升 skill 的可理解性与说服力。

---

## 🧭 排序与依赖

```
P1-A 简约阶梯 Skill ───┐ (核心；教 agent 简约并留 `ody:` 标记；最低风险)
                       ├─► P3-C 对照示例为 A 配套
                       ├─► P3-A/P3-B 徽章与默认强度配置围绕 A
P1-B 反过度设计 Review ─┤ (独立可先行；命中可建议补债务标记 → 衔接 C)
                       └─► P1-C 债务台账（收割 A 留下的标记）
P2-A 行为门控基准 ──────► 量化 A 的收益；依赖 A 已落地
P2-B i18n 漂移 canary ──► 完全独立、低成本，可随时插入
```

**推荐顺序**：**P1-A → P1-B → P1-C**。
- A 先：直接补齐“反过度设计”这一缺失能力，最低 blast radius，复用 skill/注入基础设施。
- B 次：审查视角独立，可与 A 并行；其命中可引导补债务标记。
- C 随后：依赖 A 让 agent 留下 `ody:` 标记后，收割才有素材。
- P2-B 独立低成本，任意时点可插入；P2-A 与 Tier 3 视 Tier 1 成效再评估。

**粗略工程周（单人，指示性）**：

| 项 | 规模 |
|----|------|
| P1-A (A.1–A.4) | 2–3 周 |
| P1-B (B.1–B.3) | 1.5–2.5 周 |
| P1-C (C.1–C.3) | 1.5–2 周 |
| P2-B | 0.5–1 周 |
| P2-A | 2–4 周 |

---

## ⚠️ 风险与 Non-Goals

**Non-Goals（对单宿主本地 CLI 明确不做）**：
- Ponytail 的**跨 13+ 宿主适配器分发**：`.cursor/`、`.windsurf/`、`.clinerules/`、`.opencode/`、`.codex-plugin/`、`.claude-plugin/`、Copilot/Gemini/Kiro 适配器等。ody-code 是单宿主，不需要把同一套规则路由到多个宿主。
- 多宿主 flag 文件路由与 `hooks/*-runtime.js` 的宿主检测逻辑。
- 我们**只取规则内容与工程实践思路**，用 ody-code 既有基础设施原生实现。

**风险与缓解**：

| 风险 | 缓解 |
|------|------|
| 简约强度过激破坏正确性（又短又错） | 照搬 ponytail“懒但不破”硬约束（校验/错误处理/安全/可访问性/标定绝不省）；用 P2-A 行为门控兜底 |
| Review/Audit 误报、过度删减 | 只报告不自动改，删除决定权留给用户（P1-B） |
| 债务标记被滥用为逃避 review 的借口 | 强制写明升级触发条件，缺触发条件即标 rot（P1-C） |
| 强度档维护成多份文案、相互漂移 | 单文件多档运行时过滤，单一事实源（P1-A，借鉴 `ponytail-instructions.js`） |
| i18n 副本悄悄漂移 | 承重短语 canary 测试（P2-B） |
| 范围蔓延到多宿主分发 | 明确列为 Non-Goal，只做规则内容迁移 |

---

## ❓ FAQ

**Q：简约 skill 和 reuse-discovery 有什么区别？**
A：reuse-discovery 管“**不要重复造**”（横向：先找既有组件再决定复用/扩展/新建）；简约 skill 管“**不要过度建**”（纵向：先问需不需要、能否用标准库/原生/一行解决）。二者互补，可共用同一套 `.md` + frontmatter 解析。

**Q：要不要照搬 ponytail 的多宿主分发机制？**
A：不。那是为“一套规则供 13 个宿主用且不漂移”设计的，单宿主 ody-code 没有这个问题。我们只取规则内容，用既有 skill/注入/权限基础设施原生实现。

**Q：最高 ROI 的一项是哪个？**
A：**P1-A（简约阶梯 Skill）**——直接补齐 ody-code 缺失的“反过度设计”能力，blast radius 最小，复用既有基础设施。

**Q：ponytail 自报的 80–94% 代码缩减可信吗？**
A：来自其自身 benchmark，作动机佐证可以；落地 ody-code 后须用 P2-A 在本仓自测，以本仓数据为准。

---

## 🚀 下一步

1. 审批本路线图（或裁剪 tier 集合）。
2. 对 **P1-A** 跑 `/plan` 产出实现计划（skill 内容 → 多档过滤 → 注入联动 → 对照示例）。
3. **P1-B → P1-C** 依次跟进；**P2-B** 可随时插入。
4. P1-A 落地后用 **P2-A** 量化收益，再评估 Tier 2/3 其余项。

---

## 📖 相关文档

- `.ody-code/roadmaps/openhands-inspired-roadmap.md` —— reuse-discovery（不要重复造）与本路线图的 simplicity（不要过度建）互补。
- `.ody-code/roadmaps/design-mode: internal reuse-discovery step + Reuse Analysis gate.md` —— reuse-discovery 的设计来源。
- `.ody-code/roadmaps/e2e-testing-automation-roadmap.md` —— P2-A 行为门控基准与 E2E 测试基础设施协同。
- 来源研究：`~/Downloads/ponytail-4.7.0/skills/`、`/benchmarks/`、`/scripts/check-rule-copies.js`、`/AGENTS.md`、`/examples/`。

---

**Version**: 1.0
**Status**: DRAFT (awaiting approval)
