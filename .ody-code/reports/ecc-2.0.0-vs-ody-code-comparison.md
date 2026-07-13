# ECC-2.0.0 vs ody-code 深度对比报告

> 生成时间:2026-07-13
> 对比对象:`/Users/ranwei/Downloads/ECC-2.0.0/`(ECC, "harness-native operator system for agentic work")vs 本仓库 ody-code(TypeScript AI agent CLI)
> 方法:4 个 explore 子代理分别深读 ECC 核心子系统、内容层资产、安全与运维体系,以及 ody-code 全量能力,所有结论基于真实代码/文件证据。

## 0. 定位差异(先对齐预期)

| | ody-code | ECC-2.0.0 |
|---|---|---|
| 本质 | 自包含的 AI agent CLI 产品(TypeScript monorepo) | 跨 harness 的 agent "操作系统":skills/rules/hooks/commands 资产 + 安装器 |
| 服务 harness | 只服务自己(刻意忽略第三方运行时字段) | Claude Code / Codex / Cursor / OpenCode / Gemini / Zed 等 10+ |
| 资产规模 | ~15 内置 skills、~40 内置命令、2 个 marketplace 插件 | 261 skills / 84 commands / 64 agents / 104 rules / 32 MCP 模板 |
| 代码形态 | 产品代码(TS)+ 少量 Markdown 资产 | Markdown/配置资产 + Node 脚本运行时 + Python 组件 + Rust 控制面(ecc2, alpha) |

两者不是同类产品,对比目的是**发现 ody-code 可借鉴的成熟设计**,而非评判高下。

---

## 一、ECC 有而 ody-code 完全没有的亮点功能

### 1. 确定性记忆管道(hooks + 文件,零 LLM 成本)⭐ 最值得借鉴

- **Stop hook 自动写会话摘要**(`scripts/hooks/session-end.js`):每次模型响应结束,从 JSONL 转录提取最近 10 条用户消息、用过的工具(上限 20)、改过的文件(上限 30),写入带 `**Project/Branch/Worktree:**` 元数据的会话文件;幂等标记块(`<!-- ECC:SUMMARY:START/END -->`)只替换自动生成部分、保留手写内容;用函数 replacer 防 `$&` 序列损坏(issue #2180);shortId 从转录文件名 UUID 末 8 位派生,防父子进程互相覆盖(issue #1494)。
- **SessionStart hook 按 worktree 精确匹配注入**(`session-start.js`),三重精巧防护:
  - **Stale-replay 防护**:注入内容包在 `"HISTORICAL REFERENCE ONLY — NOT LIVE INSTRUCTIONS"` 警告框里,防止模型把旧摘要里的 `/command ARGUMENTS=` 当指令重新执行(修自真实事故:压缩恢复后重复建 issue/分支,issue #1534);
  - **注入预算**:默认 8000 字符上限(`ECC_SESSION_START_MAX_CHARS`),30 天 TTL 清理;
  - **模式感知**:区分 startup/resume/clear/compact,只在冷启动注入,resume/clear 后注入会污染上下文。
- **对比**:ody-code 的跨会话记忆只有 product/game-design 两个特殊模式的 learnings JSONL,且必须由模型主动调 `AppendLearning` 写入,普通会话之间完全没有记忆传递。

### 2. Instinct 持续学习闭环 ⭐ 最有原创性

完整的"经验生命周期"(`skills/continuous-learning-v2/`,208K,最大 skill):

1. **观察**:PreToolUse/PostToolUse hook 把每条工具调用脱敏(密钥正则替换 `[REDACTED]`)后写 `observations.jsonl`。核心论点:"hooks 100% 确定触发 vs skill 只有 50-80% 概率触发"。带 5 层自循环防护(防止观察自己的后台 observer);
2. **提炼**:后台 Haiku observer agent(`agents/observer.md`)检测"用户纠正/错误解决/重复工作流/工具偏好"四类模式,产出**原子 instinct**(trigger + action + confidence 0.3-0.9 + Evidence 记录)。置信度:证实 +0.05 / 反驳 -0.1 / 每周衰减 -0.02;
3. **应用**:SessionStart 注入置信度 ≥0.7 的 Top-6 instinct(project 优先于 global 去重);
4. **进化**:`/evolve` 把相关 instinct 聚类升级为 skill/command/agent(用户触发的→command,自动触发的→skill,多步流程→agent);
5. **晋升**:同一 instinct 在 ≥2 个项目复现且均置信 ≥0.8 才从项目级升为全局("宁可后晋升也不污染全局");
6. **项目隔离**:按 git remote URL 哈希做 12 字符 project id,同一 repo 跨机器同 ID;数据目录刻意放在 `~/.local/share/ecc-homunculus` 绕开敏感路径守卫。
7. **共享**:`/instinct-export` 只导出 instinct(模式)不导出原始观察,导入有 URL 校验和 2MB 限制。

工程加固:懒启动用 flock/lockfile/mkdir 三级原子锁(兼容 Linux/macOS);SIGUSR1 通知节流为每 20 条观察一次(#521);会话租约机制(无租约且空闲 1800 秒 observer 自行退出,#1141)。**ody-code 完全没有自动学习机制。**

### 3. 规模治理三件套(资产多了之后的元工具)

- **manifest 驱动的分层安装**:`manifests/` 三层 JSON(components → modules 32 个 → profiles 7 个:minimal/core/developer/security/research/opencode/full);plan/apply 分离(dry-run 同款代码路径);install-state 精确卸载;Ajv 缺失时降级到内置校验器保证裸环境可用;
- **`agent-sort` skill**:以 repo 证据(扩展名、lockfile、CI 配置)把组件分 DAILY/LIBRARY 两桶,每条决策必须引用 grep 证据,6 路并行 subagent 分类——LIBRARY 可搜索但不默认加载;
- **`context-budget` skill**:审计 agents/skills/MCP/rules 的 token 开销,含去重检测。
- **启示**:资产规模大到一定程度,"管理资产的资产"是必需品,全量加载是反模式。

### 4. 多 harness 一份源 + 薄适配边缘

- 核心原则(`docs/architecture/cross-harness.md`):"If a change requires editing three harness copies of the same workflow, the shared source is in the wrong place.";
- 每个 harness 一个 adapter(`scripts/lib/install-targets/`,12 个),统一接口 `supports()/validate()/resolveRoot()/getInstallStatePath()/planOperations()`;如 `gemini-adapt-agents.js` 做工具名映射(`Read→read_file`、`Bash→run_shell_command`);
- **合规记分卡**:每个 harness 的支持状态固化为代码里的 `ADAPTER_RECORDS`(state ∈ Native/Adapter-backed/Instruction-backed/Reference-only,含 verification_commands、risk_notes),渲染回文档矩阵,并有测试(`tests/docs/harness-adapter-compliance.test.js`)保证文档与代码不漂移。
- ody-code 明确不做这个(定位选择),但若未来想让 ody-code 的 skills 资产被其他工具消费,这是成熟范本。

### 5. 安全三层体系

- **AgentShield 扫描器**(外部 npm 包 `ecc-agentshield`,`/security-scan` 调用):扫描 CLAUDE.md/settings.json/mcp.json/hooks 的硬编码密钥、过宽 allow list、prompt injection 模式、`${file}` 插值命令注入、`2>/dev/null` 静默吞错;输出 A–F 等级 + 分数 + `--fix` 自动修复(明文密钥→环境变量引用);`--opus --stream` 模式跑 **Attacker→Defender→Auditor 三 agent 对抗流水线**;
- **CI 守卫**(本仓库自研):硬编码真实供应链投毒 IOC 清单扫 lockfile(`scan-supply-chain-iocs.js`);GitHub Actions workflow 安全审计(`pull_request_target` checkout 不可信 ref、`permissions: write-all` + `persist-credentials` 等);Unicode 零宽/bidi 隐藏字符检查;curl 凭证泄露守卫(凭证不放 argv);
- **运行时 hook**:可选 InsAIts 异常检测(凭证暴露、注入、幻觉链 20+ 类型,exit 2 阻断,审计写 JSONL)。
- **ody-code 现状**:权限系统成熟(3 模式 + 规则 DSL + 15 策略链 + 敏感文件保护),但**密钥泄漏扫描完全缺失**,prompt injection 只有 XML 转义这一层被动防御。

### 6. 对"prompt 治理是否有效"的元检验

- **`skill-comply` skill**:从 skill/rule 的 md 自动生成行为 spec → 生成三档提示词场景(supportive → neutral → **competing**,即 prompt 故意不配合)→ 跑真实会话抓 stream-json 工具轨迹 → LLM 分类判定合规 → 自包含报告。核心理念:"**prompt 不支持时仍遵守,才算真合规**"——对 prompt-based governance 最成熟的检验思路;
- **`gateguard` hook**(`gateguard-fact-force.js`):核心论点——"问 LLM '你确定吗?'永远得到'是',自我评估无效(实验验证)";改为**强制事实产出**:每个文件首次编辑被阻断,要求先 Grep/Read 调查 importers 和 schema 才放行(调查行为本身产生上下文)。三段式 DENY→FORCE→ALLOW,按动作类型分级门避免过度减速,附 A/B 测试数据(+2.25/10),有 `ECC_GATEGUARD=off` 逃生门。

### 7. 反锚定决策机制

- **`council` skill**:四声部决策(Architect 在上下文内 + Skeptic/Pragmatist/Critic 三个全新 subagent),关键设计是**外部声部只拿到压缩问题、不给完整对话记录**,且要求先写下自己的立场再读外部意见——显式防锚定;"When NOT to use"表和"只在决策改变真实事物时才持久化"的规则都很克制;
- **`santa-method` skill**:双上下文隔离 reviewer 用同一 rubric 对抗双审,双 PASS 才放行,**每轮换全新 reviewer 防锚定**,最多 3 轮后升级人类;含失败模式表(rubber stamping → 对抗性 prompt;主观漂移 → 只许客观 pass/fail 标准)和 metrics(first-pass rate、escape rate)。

### 8. 身份三层模型

SOUL.md(17 行人类可读原则宣言)→ agent.yaml(机器可读 gitagent 清单,声明模型偏好 `preferred: claude-opus-4-6` + fallback)→ 仓库原生文件(权威实现),且明确声明"投影 vs 权威"的优先级关系("Native agents, commands, and hooks remain authoritative in the repository until full manifest coverage is added")。

---

## 二、相同功能但 ECC 做得更好的地方

### 1. Hooks:ody-code 事件种类更多(15 种 vs ~8 种),但 ECC 工程纪律更成熟

| ECC 做法 | ody-code 现状 |
|---|---|
| **合并分发器**:一个 Bash hook 挂多个检查(dev-server 拦截/tmux 提醒/commit 前 lint+secret 检测),减少进程 spawn | 每个 hook 独立进程,无合并模式 |
| **批处理设计**:编辑时只累积文件路径,Stop 时一次性跑 format+typecheck(`post:edit:accumulator` + `stop:format-typecheck`) | 无此类模式(需用户自己实现) |
| **profile 门控**:`ECC_HOOK_PROFILE=minimal`、`ECC_DISABLED_HOOKS` 分级禁用 | 只能整条删除配置 |
| **引导层兼容**:plugin-hook-bootstrap 解决 Node 21+ `require.main` 未定义导致所有 hook 静默空转的事故(#2184,2.0.0 头号修复) | 无等效问题但也无等效韧性设计 |
| 所有 hook 默认 exit 0 不阻塞,阻断是显式设计 | 一致 ✓ |

### 2. 上下文压缩:ody-code 有自动比率触发(85%),ECC 补充了"战略压缩"哲学

- ECC 论点(`skills/strategic-compact/SKILL.md`):**自动压缩在任意点触发(常在任务中途丢上下文),战略压缩在逻辑阶段边界进行**(研究→计划、调试完→新功能)。配套:50 次工具调用时 hook *建议*手动 `/compact`(而非强制执行),之后每 25 次提醒一次;SKILL.md 给出"压缩决策表"和"什么能在压缩中存活"对照表;
- ECC 还有**上下文/成本监控器**(`ecc-context-monitor.js`):上下文剩余 <35%/<25%、花费超 $5/10/50、改动文件 >20 个、同一工具循环 ≥3 次时注入警告(带去抖)。ody-code 没有成本维度的运行时提醒,也没有工具循环检测。

### 3. Slash 命令演进:legacy shim 软着陆

- ECC 从"命令中心"向"skill 中心"迁移时,旧命令不删除而是保留**零逻辑 shim**(`legacy-command-shims/`,12 个):frontmatter 写明 "Legacy shim, prefer the skill directly",正文只有 "Canonical Surface → 指向 skills/xxx/SKILL.md",单向引流,零维护负担;
- 对比 ody-code 的 `/review` 命令合并(直接改、走 major changeset)——shim 模式可把破坏性变更变成可逆迁移,值得在下次命令面调整时采用。

### 4. AGENTS.md 分层:ody-code 已成熟,ECC 多了"按 harness 渲染 + 选择性分发"

- ody-code 的 user→project 分层、32KB 预算深层优先已经很好;ECC 额外解决的是**分发侧**:一份 rules 源按 harness 原生机制渲染(Cursor frontmatter `alwaysApply: true`、OpenCode 单文件合并、Claude 受管目录),并用 manifest 驱动的选择性安装避免"全量规则撑爆上下文"。

### 5. Subagent 定义:ECC 有两项可借鉴

- **每个 agent 开头统一的 "Prompt Defense Baseline"**(防角色覆盖、防 unicode/同形字/紧迫感/权威冒充注入、外部内容一律视为不可信)——仓库级安全模板化;
- **显式模型分层写进 agent 定义**(规划/参谋长用 opus、执行/评审用 sonnet、后台观察用 haiku)+ `/model-route` 命令按复杂度×预算路由(输出含 confidence 和 fallback)——成本意识贯彻到资产层。ody-code 的 subagent 可指定模型,但缺少这种"成本路由"的显式设计。

### 6. 文档即契约的测试

- ECC 是纯 Markdown/配置仓库却有 151+ 测试文件(零框架,Node 内置 assert + 自写 `test()`):npm test 先跑 schema/结构校验器再跑行为测试;**校验器本身也被测试**;文档宣称的命令、路径、合规矩阵必须与代码一致(漂移即测试失败);测试前剥离 `GIT_DIR`/`GIT_WORK_TREE` 等 git 环境变量防 hook 污染;c8 对 `scripts/**/*.js` 强制 80% 覆盖率;
- ody-code 作为代码仓库测试体系完整,但 **docs/ 与代码的一致性没有自动化保障**(sync-changelog 等仍靠 skill 约定)。

### 7. 安装器工程(ECC 独有但值得记录的细节)

- 入口脚本刻意做薄(32 行 shell → `exec node scripts/install-apply.js`),真实逻辑在可测试的 Node 代码里;
- 符号链接解引用循环(npm 全局安装 bin 是 symlink)、Git Bash `cygpath -w` 路径修正(注释明确记录 doubling bug);
- `deepMergeJson` 合并而非覆盖用户已有 settings.json;尊重用户禁用的 MCP server。

---

## 三、ody-code 反而更强的地方(保持自信)

| 能力 | ody-code 优势 |
|---|---|
| 结构化工作流模式 | product/game-design 的 Phase 注入 + 硬门禁 + tier 化学习档案 + 第二模型 review,ECC 无等效物 |
| Cron 调度器 | 完整表达式解析 + idle 门控 + coalesce + jitter + 时钟抽象(no-date-now 测试强制),ECC 的 `/loop-start` 简陋得多 |
| 后台任务持久化 | 进程重启后任务状态可恢复(lost 标记)、ring buffer 输出捕获 |
| E2E 测试自动生成 | 变更影响分析 + 多语言(Vitest/Jest/pytest/Go)测试生成 + 结果缓存,作为模型工具暴露 |
| 权限策略链 | 3 模式 + 规则 DSL + 15 策略 + 敏感文件保护,比 ECC 的 hook 阻断更体系化 |
| Skills 类型系统 | prompt/flow/knowledge 三类型 + 触发词自动注入(带 token 预算)+ 4 级 scope |

---

## 四、借鉴优先级建议

1. **会话摘要记忆管道**(Stop/SessionStart hook + worktree 匹配 + stale-replay 防护 + 注入预算)——ody-code 已有 15 种 hook 事件,基础设施现成,投入产出比最高;
2. **成本/上下文运行时监控**(花费阈值 + 工具循环检测警告);
3. **密钥泄漏扫描**(哪怕是 PreToolUse hook 挂 gitleaks 的内置 recipe);
4. **Strategic compact 建议**(工具调用计数 → 阶段边界建议手动 compact,而非仅 85% 比率自动触发);
5. **Instinct 学习闭环**——最有差异化但也最重,建议作为实验 flag 功能(`packages/agent-core/src/flags/registry.ts`)起步;
6. **Prompt Defense Baseline 模板** + subagent 模型分层约定——低成本高收益;
7. **Legacy shim 模式**——下次命令面调整时采用,避免 major bump。

---

## 五、密钥泄漏扫描详细设计

基于 ECC `AgentShield` 与 CI 守卫的思路，结合 ody-code 已有的 HookEngine、权限策略链和配置体系，本节给出可在 ody-code 中落地的最小可用（MVP）密钥泄漏扫描方案。

### 5.1 设计目标

- **检测面**：模型即将执行的 `Bash` 命令、以及所有 `PreToolUse` 工具入参中的字符串值。
- **不新增 LLM 成本**：扫描为纯本地正则 + Shannon 熵计算，零模型调用。
- **默认不阻断**：首次引入时以“记录 + 警告”为主，避免误杀正常开发工作流；`strict` hook profile 或 `blockOnMatch=true` 可显式开启阻断。
- **审计可追溯**：命中记录写入会话级 JSONL，便于事后分析与假阳性调优。

### 5.2 威胁模型

1. **模型被诱导泄露密钥**：用户或恶意 prompt 让模型把 `.env` 中的密钥通过 `Bash`/`WebSearch`/`Read` 等工具外传。
2. **模型主动在命令行使用密钥**：模型为了“方便”把 API key 直接拼进 `curl` 或 `npm config set`。
3. **转录/日志污染**：命中内容进入 wire.jsonl 或审计日志，造成二次泄漏。

针对 #3，审计日志**不保存完整匹配文本**，仅保存 SHA-256 哈希与前 4 字符前缀。

### 5.3 触发点与数据流

```text
用户/模型发起工具调用
       │
       ▼
PreToolUse hook 触发
       │
       ▼
SecretLeakScannerBuiltin.run()
  ├─ Bash 工具：扫描 command 字段
  └─ 其他工具：递归扫描 toolInput 中所有字符串
       │
       ▼
SecretLeakScanner.scan()
  ├─ 默认规则集（AWS/GH PAT/API key/token/JWT）
  ├─ Shannon 熵过滤
  └─ 允许列表过滤
       │
       ├─ 无命中 ──► 放行
       │
       └─ 命中
            ├─ 写入 secret-scan.jsonl（哈希 + 前缀 + 规则 + 动作）
            ├─ blockOnMatch=false：allow + reason 警告
            └─ blockOnMatch=true ：block
```

### 5.4 规则引擎

- **正则规则**：覆盖常见密钥格式（AWS AKIA、GitHub PAT `ghp_...`、JWT、`api_key=` / `secret=` 等高熵值）。
- **熵阈值**：规则可单独指定 `entropyMin`；未指定时回退到全局 `entropyThreshold`（无内置默认值；两者均未设置时跳过熵检查）。低熵字符串（如 `1234567890abcdef`）不命中。
- **允许列表**：内置 `EXAMPLE_KEY`、`YOUR_API_KEY`、`1234567890abcdef`、`example-token` 等占位符；用户可通过 `[secretScan] allowList` 追加。

### 5.5 配置示例

在 `~/.ody-code/config.toml` 中：

```toml
[secretScan]
enabled = true
blockOnMatch = false
maxScanBytes = 8192
entropyThreshold = 4.5
allowList = ["MY_MOCK_KEY", "CI_PLACEHOLDER"]
profiles = ["strict"]
```

同时需开启实验 flag：

```bash
export ODY_CODE_EXPERIMENTAL_SECRET_LEAK_SCAN=1
```

### 5.6 关键文件

| 文件 | 职责 |
|---|---|
| `packages/agent-core-shared/src/flags/registry.ts` | `secret-leak-scan` 实验 flag |
| `packages/agent-core-shared/src/config.ts` | `[secretScan]` 配置 schema |
| `packages/agent-core/src/security/secret-scan/scanner.ts` | 正则 + 熵扫描引擎 |
| `packages/agent-core/src/security/secret-scan/rules.ts` | 默认规则集与 `createDefaultScanner` |
| `packages/agent-core/src/security/secret-scan/allow-list.ts` | 允许列表归一化 |
| `packages/agent-core/src/security/secret-scan/audit.ts` | JSONL 审计日志 |
| `packages/agent-core/src/session/hooks/builtin/secret-leak-scanner.ts` | `SecretLeakScannerBuiltin` |
| `packages/agent-core/src/session/hooks/builtin/registry.ts` | 内置 hook 注册 |
| `packages/agent-core/src/session/index.ts` | `PreToolUse` 默认 hook 注入 |

### 5.7 与 ECC AgentShield 的差异

| 维度 | ECC AgentShield | ody-code MVP 方案 |
|---|---|---|
| 触发时机 | 扫描 CLAUDE.md / settings.json / hooks 等静态资产 | 扫描每次 `PreToolUse` 动态文本 |
| 扫描引擎 | 外部 npm 包 + 可选 LLM 对抗流水线 | 内置纯 TS 正则/熵引擎 |
| 阻断能力 | `--fix` 自动修复 + 可阻断 | 默认警告，配置后可阻断 |
| 部署成本 | 需额外安装/配置 | 实验 flag + 配置即可启用 |

### 5.8 后续扩展

1. **静态资产扫描**：把同一 `SecretLeakScanner` 用于 `AGENTS.md`、MCP `settings.json`、hook 脚本等安装前扫描。
2. **CI 守卫复用**：在 GitHub Actions 中调用 `SecretLeakScanner` 扫描 commit diff 与 lockfile。
3. **LLM 对抗审计**：当成熟度提高后，引入 Attacker→Defender→Auditor 三 agent 流水线，类似 ECC `--opus --stream` 模式。
4. **gitleaks 集成**：保持当前内置引擎作为默认，允许用户配置 `command = "gitleaks detect --no-git"` 作为外部增强规则源。

---

## 附录:ECC 规模数据一览

261 skills / 84 commands(59 核心)/ 64 agents / 104 rules / 32 MCP 模板 / 32 安装模块 / 7 安装 profile / 12 legacy shims / 15 种 hook 事件对应 ~25 个 hook 脚本 / 1 个一等集成(aura,零依赖只读带 THREAT_MODEL)/ 12 个 install adapter / 10+ harness 适配目录 / 151+ Node 测试 + 11 Python 测试 + Rust 测试(ecc2)。

## 附录:关键文件索引(便于后续查阅)

- 记忆管道:`scripts/hooks/session-end.js`、`session-start.js`、`pre-compact.js`、`suggest-compact.js`、`ecc-context-monitor.js`
- 学习闭环:`skills/continuous-learning-v2/SKILL.md`、`hooks/observe.sh`、`agents/observer.md`、`agents/observer-loop.sh`、`scripts/detect-project.sh`、`instinct-cli.py`
- 治理 hooks:`scripts/hooks/gateguard-fact-force.js`、`config-protection.js`、`pre-bash-dispatcher.js`
- 安装器:`scripts/install-apply.js`、`scripts/lib/install-targets/`、`manifests/install-{modules,profiles,components}.json`
- 跨 harness:`docs/architecture/cross-harness.md`、`scripts/lib/harness-adapter-compliance.js`
- 精选 skills:`skills/{gateguard,orch-pipeline,skill-comply,santa-method,council,agent-sort,strategic-compact,context-budget,hermes-imports}/SKILL.md`
