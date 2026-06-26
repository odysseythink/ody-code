# Design Mode 鲁棒性改造：Batch 2 根本问题修复

> **审计级别**: Deep  
> **日期**: 2026-06-12  
> **状态**: 批准 ✓  
> **来源**: 用户报告 Batch 2 bug + 会话导出问题

---

## 问题陈述与根本原因

### 问题 1：Batch 2 设计被跳过
**现象**：用户下达"完成Batch 2的设计"命令，AI未进行任何设计即退出Design Mode，直接开始写执行计划。

**根本原因**：
1. AI 错误理解"范围定义 = 完整设计"
2. `ExitDesignModeTool` 无完整性验证机制
3. 缺少向 AI 的 System Reminder 来指导完整度检查

### 问题 2：会话导出未捕获 Batch 2
**现象**：`/export-md` 导出的会话文件中缺少 Batch 2 对话内容。

**根本原因**：
1. 导出是手工触发快照，时间点之后的消息不被捕获
2. 缺少实时追加机制
3. 会话持久化在 hot (内存) 和 cold (JSON/Markdown) 之间脱节

### 问题 3：Session 恢复机制不完善
**现象**：设计→Plan 切换时发生 `tool_call_id is not found` 错误。

**根本原因**：
1. 分区切换时 tool call 和 result 可能被分离到不同分区
2. 无完整性检查在发送给 LLM 时进行清理
3. 设计→Plan handoff 中缺少 artifact 隔离

---

## 设计方案概览（6 个 Part）

### Part 1：系统架构与数据分层

**设计完整性检查机制**：
- C1. Scope 章节存在
- C2. Architecture 章节存在
- C3. Data Models 章节存在
- C4. Algorithms 章节存在
- C5. Error Handling 章节存在
- C6. Self-Review 章节存在
- C7. User Final Approval

**事件驱动持久化**：
- 5 个 checkpoint 触发点：
  1. ExitDesignMode (优先级最高)
  2. PartCompleted (通过审计)
  3. TurnCompleted (自然边界)
  4. BatchBoundary (计划执行里程碑)
  5. Manual /checkpoint (用户主动)

**分层存储架构**：
- **Hot 层**：内存中的 `SessionContext`，包含完整消息历史
- **Cold 层 1**：JSON checkpoint (`.ody-code/session-state/session-id.json`)，包含完整序列化状态
- **Cold 层 2**：Markdown 导出 (`.ody-code/session-exports/session-YYYY-MM-DD-HHmmss.md`)，仅追加

**恢复流程**：
1. Session 启动时读取最新 JSON checkpoint
2. 验证完整性（消息计数、JSON 格式、Design Mode 上下文一致性）
3. 若失败则回退到前一个版本
4. JSON 中缺失内容时尝试从 Markdown 重生成

---

### Part 2：数据模型与存储结构

**JSON Checkpoint 结构** (`.ody-code/session-state/session-id.json`):
```json
{
  "sessionID": "...",
  "createdAt": "2026-06-12T10:00:00Z",
  "lastUpdatedAt": "2026-06-12T10:05:00Z",
  "currentMode": "design|plan|normal",
  "messages": [raw message objects...],
  "designModeContext": {
    "sessions": [
      {
        "designSessionID": "...",
        "startedAtMsg": 5,
        "exitedAtMsg": 42,
        "completeness": {...},
        "approvedPath": "..."
      }
    ]
  },
  "toolCallIndex": {
    "callIdToResult": {...}
  }
}
```

**恢复索引** (`.ody-code/session-state/checkpoints.json`):
```json
{
  "versions": [
    {
      "timestamp": "2026-06-12T10:00:00Z",
      "messageCount": 42,
      "valid": true,
      "lastValidParent": null
    }
  ],
  "latest": "..."
}
```

**备份目录** (`.ody-code/session-state/backups/`):
- 保留最近 10 个版本 + 最新版本
- 命名: `session-id-vN.json`

**Markdown 导出** (`.ody-code/session-exports/session-YYYY-MM-DD-HHmmss.md`):
- **仅追加**，永不覆盖
- 每当消息进入内存时实时追加
- 完整转录，不压缩

---

### Part 3：AI 决策逻辑与完整度门控

**ExitDesignModeTool 中的完整度检查**：
```
function canExitDesignMode(designDoc: string): { isComplete: bool, errors: string[] }
  
  checks = [
    C1: hasScope(designDoc),
    C2: hasArchitecture(designDoc),
    C3: hasDataModels(designDoc),
    C4: hasAlgorithms(designDoc),
    C5: hasErrorHandling(designDoc),
    C6: hasSelfReview(designDoc),
    C7: userGaveFinalApproval(designDoc)
  ]
  
  if ANY failed: return (false, ["缺失: Scope", "缺失: Architecture", ...])
  return (true, [])
```

**System Reminder 指导**：
```
在设计模式中，当用户要求完成设计时：
1. 不要接受仅有 Scope 定义的部分设计
2. 必须包含以下 7 个部分才能调用 ExitDesignMode：
   - Scope In/Out
   - Architecture
   - Data Models
   - Algorithms
   - Error Handling
   - Self-Review
   - User Final Approval
3. 如果设计不完整，回到 Design Mode 的适当 Step 继续补充
```

**恢复路径**：
- 若 C1-C6 失败，引导用户回到相应 Part
- 若 C7 失败，提示用户明确批准

---

### Part 4：事件驱动同步与恢复

**实时 Markdown 追加机制**：
1. 每条消息进入内存时触发 `message.created` 事件
2. 立即追加到 Markdown 文件（带文件锁保护）
3. 如果追加失败，记录错误但不阻塞

**JSON Checkpoint 异步保存**：
1. 5 个触发事件中任何一个发生时
2. 读取完整内存状态
3. 序列化为 JSON（包含 Design Mode context）
4. 原子写入（完整替换或回退）
5. 更新 checkpoints.json 索引
6. 维护最多 10 个历史版本

**Batch 2 时间线问题解决**：
- **原因**：`/export-md` 是手工快照，Batch 2 对话发生在导出之后
- **方案**：实时追加 + JSON 状态同步，确保所有消息立即被持久化
- **验证**：消息进入内存 → 立即追加到 Markdown → 立即保存 JSON

**完整性验证**：
```
CheckpointIntegrity = {
  messageCountMatch: jsonMessageCount == memoryMessageCount,
  jsonValid: JSON.parse(checkpoint) succeeds,
  designModeConsistent: designSessionRefs point to valid messages,
  toolCallIndexComplete: allToolResults have matching calls
}

if NOT CheckpointIntegrity:
  skipThisVersion()
  fallbackToPreviousValid()
```

---

### Part 5：错误处理与边界情况

**8 个错误类型与应对**：

| 错误 | 触发条件 | 策略 |
|------|---------|------|
| E1 | JSON 写入失败（磁盘满等） | 异步重试，记录日志，不阻塞 |
| E2 | Checkpoint 损坏（JSON 无效）| 验证失败 → 跳过该版本 → 使用前一有效版本 |
| E3 | Markdown 追加失败 | 记录错误 → 用户恢复时可从 JSON 重生成 |
| E4 | Recovery Index 丢失 | 扫描 backups/ 目录自动重建 |
| E5 | 磁盘满（保存新版本）| 删除最旧版本 → 重试写入 |
| E6 | 并发写入冲突 | 文件锁 (flock) 保护所有写操作 |
| E7 | 恢复后仍然错误 | 提示用户选择：继续当前有效状态 或 回退到前一检查点 |
| E8 | 版本链断裂（中间版本丢失）| 警告但允许恢复 + 建议用户检查磁盘健康 |

---

### Part 6：测试计划

**单元测试** (8 个):
- T1. Completeness Check - C1 (Scope)
- T2. Completeness Check - C2 (Architecture)
- T3. Completeness Check - C3 (Data Models)
- T4. Completeness Check - C4 (Algorithms)
- T5. Completeness Check - C5 (Error Handling)
- T6. Completeness Check - C6 (Self-Review)
- T7. Completeness Check - C7 (User Approval)
- T8. Completeness Check - all pass → ExitDesignMode allowed

**集成测试** (5 个):
- I1. Checkpoint 生命周期 (创建 → 保存 → 读取)
- I2. Checkpoint 版本控制 (保留 10 个 + latest)
- I3. 版本回退 (损坏版本自动跳过)
- I4. Recovery Index 重建 (丢失后扫描恢复)
- I5. 并发写入保护 (flock 生效)

**Markdown 导出测试** (4 个):
- M1. 追加模式 (不覆盖，仅追加)
- M2. 实时同步 (消息进入 → 立即追加)
- M3. 文件锁保护 (并发追加无冲突)
- M4. 完整性验证 (Markdown ↔ JSON 消息计数一致)

**恢复测试** (5 个):
- R1. 正常恢复 (有效 checkpoint)
- R2. 损坏 checkpoint 跳过 (自动降级)
- R3. 多版本降级链 (E2→E3→E4)
- R4. Recovery Index 丢失重建 (E4)
- R5. 磁盘满应急清理 (E5)

**E2E 测试** (4 个):
- E1. Batch 2 再现 (完整设计 → 导出 → 恢复 → 消息完整)
- E2. Design→Plan 切换 (tool_call 不断裂)
- E3. Multi-turn Design (Part 中断→继续 → 完整性保持)
- E4. Session Crash 恢复 (宕机模拟 → 从最新 checkpoint 恢复)

**手工测试清单** (5 个场景):
- 场景 1：完整设计 → ExitDesignMode 成功
- 场景 2：缺少某 Part → ExitDesignMode 被拒 → 回到相应 Step
- 场景 3：Design→Plan 切换 → 设计文件路径正确引用
- 场景 4：执行计划中 Batch 2 → 导出 → 消息完整
- 场景 5：Session 恢复后 → 消息数、Design Context 一致

**自检清单** (设计完整性):
- [ ] Architecture 解决了 Batch 2 时间线问题
- [ ] AI Decision Logic 能阻止不完整设计
- [ ] 持久化机制支持完整恢复
- [ ] 所有 8 种错误都有应对策略
- [ ] 测试覆盖 happy path + 所有 error paths
- [ ] 无遗留风险（并发、磁盘、分区）

---

## 批准状态

✅ **用户批准**: Part 1-6 全部通过  
✅ **方案选择**: C - 分层存储 + 事件驱动（完整改造）  
✅ **下一步**: Plan Mode（详细实施计划）  

---

**设计文件**: `/Users/ranwei/workspace/ody-code/.ody-code/designs/2026-06-12-batch2-robustness-design.md`  
**导出时间**: 2026-06-12 10:00  
**Design Session ID**: batch2-robustness-deep-audit  
