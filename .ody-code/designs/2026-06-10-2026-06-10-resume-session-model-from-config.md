# 恢复会话统一从 config.toml 加载模型

## 问题

恢复会话时模型从 session records 中恢复，而非从 config.toml 加载。导致用户在 plan/design 模式内临时切换模型后退出，恢复时上次的临时模型"残留"。

diagnostic log 已确认：
- 持久化路径正确：`defaultModel` 不受 plan/design 模式内切换影响
- 新会话正确：`createSession` 用 `config.defaultModel`
- **恢复会话错误**：`refreshSessionRuntimeConfig` 优先用 session records 中的旧 `modelAlias`

## 修改

### Scope In [C:USER]
- `refreshSessionRuntimeConfig` 改为从 config.toml 加载模型

### Scope Out [C:DEFERRED]
- Plan/Design 模式内 `/model` 切换的体验优化
- OAuth 刷新路径对模型的影响

### 文件
- Modify: `packages/agent-core/src/rpc/core-impl.ts:813-847`

### 算法

```
Input: session (Session), config (KimiConfig)
Output: void

refreshSessionRuntimeConfig(session, config):
  1. main = session.requireMainAgent()
  2. currentMode = main.sessionMode.isActive ? main.sessionMode.kind : 'normal'
  3. fromConfig =
       currentMode === 'plan'   → config.modeModels?.plan
       currentMode === 'design' → config.modeModels?.design
       else                     → config.defaultModel
  4. model = fromConfig?.trim()
  5. if !model return          // config 中未配置模型 → 不操作
  6. api = new SessionAPIImpl(session)
  7. api.setModel({ agentId: 'main', model })
  8. session.flushMetadata()
```

替换现有的 try-requested-then-fallback 循环（~30 行代码删除）。

### 数据流

```
config.toml
  ↓ loadRuntimeConfig
KimiConfig (defaultModel, modeModels)
  ↓ refreshSessionRuntimeConfig (修改后)
Session.mainAgent.config.modelAlias
```

不再走 Session records → modelAlias 路径。

### 调用点

文件: `packages/agent-core/src/rpc/core-impl.ts:339`

```
const resumeResult = await session.resume();
warning = resumeResult.warning;
await this.refreshSessionRuntimeConfig(session, config);  // 修改生效处
```

### 错误处理

| 场景 | 行为 |
|---|---|
| config 有对应模型且有效 | 设置为该模型 ✓ |
| config 有对应模型但无效（provider 缺失等） | `setModel` 抛错 → 调用方处理 |
| config 无对应模型 | 不设置 → session 无模型，单一来源无需降级 [C:USER] |

### 设计原则

模型来源**仅限 config.toml** — 不再读取 session records 中的旧模型值。

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `session.requireMainAgent()` 在 `session.resume()` 后始终可用 | High | 若 main agent 未恢复则抛错中断 resume | 已有 resume flow 保证 main agent 始终恢复 |
| 2 | `main.sessionMode.isActive` 在 resume 后正确反映上次退出时的模式 | High | 若模式不对，可能用错 modeModels 字段 | 已有 test/session-mode.test.ts 覆盖 |

## Risk Register

无新增风险。模型来源统一为 config.toml，不存在降级路径。

## Self-Review

### Security
- 无敏感数据变更，仅改变模型来源。Nothing found.

### Test
- 现有 test/agent/resume.test.ts 和 test/harness/model-alias-session.test.ts 覆盖 resume 路径。需要补充验证 resume 后模型来自 config 的断言。
- `session.requireMainAgent()` 和 `sessionMode.kind` 属性已在代码中使用，类型安全。

### Ops
- `session.requireMainAgent()` 和 `new SessionAPIImpl(session)` 均为轻量操作，不引入额外 I/O。
- 删除 try-catch 循环简化了错误路径。

### Integration
- 修改仅影响 `core-impl.ts:refreshSessionRuntimeConfig`，无下游调用方变更。
- 两个已有 caller：`resumeSession` (line 339) 和 `refreshSessionRuntimeConfig` 内部。

### Scope
- 单一函数修改，无需分解。
