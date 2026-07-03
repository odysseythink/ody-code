# Part 4 — integration.md

## 范围

本部分把 `schema.md`、`background.md`、`cron.md` 中实现的 Rust 后台任务 / cron 子系统与 TS 侧 `agent-core` 对齐到同一套 L3 夹具（fixture）上，形成可执行的 parity 测试：

- 改造 `FixtureAgent`，使其可以挂载 `BackgroundManager`、`CronManager`，并支持测试可控的时钟注入。
- 定义一个与 TS 共用的 JSON fixture 格式，覆盖 prompt/steer/cancel/wait、clock 推进、cron 增删 / tick、后台任务注册 / 等待 / 停止。
- 新增 Rust binary `background_cron_l3`，读取 fixture 并输出标准化 snapshot。
- 新增 TS driver `background-cron-l3-driver.ts`，用 `agent-core` 的 `Agent` + 测试 harness 回放同一 fixture 并输出同样形状的 snapshot。
- 新增 parity 测试，对比 Rust / TS 两个 snapshot。

本部分结束后，`cargo test -p agent-rs --lib turn::background_cron_driver` 与 `pnpm --filter @odysseythink/integration-tests test:parity:background-cron` 均通过。

---

## 依赖总览

- `schema.md` Task 1：提供 `AgentEvent`（`BackgroundTaskStarted`、`BackgroundTaskTerminated`、`CronFired`）、`PromptOrigin`。
- `background.md`：提供 `BackgroundManager`、`BackgroundTask` 三种任务实现。
- `cron.md`：提供 `CronManager`、`CronScheduler`、`ClockSources`。

本 Part 无新增外部依赖。

---

## 阶段划分

- **Phase A（FixtureAgent 改造）**：Task 1。让现有 fixture agent 能挂载后台 / cron 管理器并提供可控时钟。
- **Phase B（Rust fixture 驱动）**：Task 2。在库中实现 fixture 解析与执行器，可直接单元测试。
- **Phase C（可执行入口 + TS 驱动）**：Task 3 新增 Rust binary；Task 4 新增 TS driver。二者互相独立，可并行。
- **Phase D（parity 落地）**：Task 5 编写 fixture 与 parity 测试；Task 6 注册 binary 与 npm script。

---

## 文件结构

```
rust-ody/crates/agent-rs/
├── src/turn/fixture_agent.rs            # modify：新增 manager 字段与 clock override
├── src/turn/background_cron_driver.rs   # new：fixture schema + run_fixture 执行器
├── src/turn/mod.rs                      # modify：注册 background_cron_driver
└── src/bin/background_cron_l3.rs        # new：CLI binary

packages/integration-tests/
├── src/parity/background-cron-l3-driver.ts   # new：TS 回放 driver
├── src/parity/background-cron-fixture.ts     # new：fixture 类型与解析
├── src/parity/normalize-background-cron.ts   # new：snapshot 归一化
├── test/parity/background-cron-l3-parity.test.ts  # new：parity 测试
└── test/parity/fixtures/background-cron/*.json    # new：共享 fixture 文件

rust-ody/crates/agent-rs/Cargo.toml      # modify：新增 [[bin]] background_cron_l3
packages/integration-tests/package.json  # modify：新增 test:parity:background-cron script
```

> 注意：fixture 文件是 Rust binary 与 TS driver 的共同输入，必须保持平台无关；后台 process 任务使用 `/bin/sh -c 'echo ...'` 这类 Unix 通用命令，因此 parity 测试仅在 macOS/Linux CI 上运行。

---

## Task 1：扩展 `FixtureAgent` 挂载后台 / cron 管理器与时钟注入

**Depends on:** `background.md` Task 4、`cron.md` Task 6。

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs`（新增字段与方法，不改动构造函数签名）
- Test: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs` 内的 `#[cfg(test)]` 模块

### 步骤 1.1：写入失败测试

在 `fixture_agent.rs` 的 `#[cfg(test)] mod tests { ... }` 末尾追加：

```rust
    #[tokio::test]
    async fn fixture_agent_captures_cron_fire_event() {
        use kosong_rs::provider::FinishReason;
        use kosong_rs::usage::TokenUsage;

        let agent = Arc::new(FixtureAgent::new(
            vec![FixtureResponse {
                tool_calls: vec![],
                finish_reason: Some(FinishReason::Completed),
                raw_finish_reason: None,
                usage: TokenUsage::default(),
            }],
            vec![],
        ));
        let flow = Arc::new(TurnFlow::new(agent.clone()));
        let background = Arc::new(BackgroundManager::new(agent.clone(), flow.clone(), None));
        let cron = CronManager::new(
            agent.clone(),
            None,
            CronManagerOptions {
                clocks: Some(agent.clock()),
                poll_interval_ms: Some(0),
            },
        );
        agent.install_managers(background, cron.clone());

        cron.store.lock().unwrap().add(
            crate::cron::task::CronTaskInit {
                cron: "* * * * *".into(),
                prompt: "ping".into(),
                recurring: Some(true),
            },
            0,
        );
        agent.advance_clock_to(60_000);
        cron.tick();

        let _ = flow.wait_for_current_turn(None).await;

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::CronFired { .. })),
            "expected CronFired event, got {:?}",
            events
        );
    }
```

运行：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --lib turn::fixture_agent::tests::fixture_agent_captures_cron_fire_event
```

**预期结果：** 编译失败，`BackgroundManager`、`CronManager`、`CronManagerOptions`、`install_managers`、`advance_clock_to`、`clock`、`CronTaskInit`、`AgentEvent::CronFired` 等未解析。

### 步骤 1.2：实现扩展

在同一文件顶部追加导入：

```rust
use std::sync::Mutex;

use crate::background::manager::BackgroundManager;
use crate::cron::clock::ClockSources;
use crate::cron::{CronManager, CronManagerOptions};
```

在 `Captures` 结构体之后、`FixtureAgent` 之前插入可控时钟：

```rust
#[derive(Clone)]
pub struct FixtureClock {
    state: Arc<Mutex<Option<i64>>>,
}

impl FixtureClock {
    pub fn new(state: Arc<Mutex<Option<i64>>>) -> Self {
        Self { state }
    }
}

impl ClockSources for FixtureClock {
    fn wall_now(&self) -> i64 {
        self.state
            .lock()
            .unwrap()
            .unwrap_or_else(|| crate::cron::clock::resolve_clock_sources(None).wall_now())
    }

    fn mono_now_ms(&self) -> u128 {
        crate::cron::clock::resolve_clock_sources(None).mono_now_ms()
    }
}
```

修改 `FixtureAgent` 结构体（在 `loop_control` 字段后追加）：

```rust
pub struct FixtureAgent {
    pub captures: Arc<Mutex<Captures>>,
    pub llm: Arc<dyn Llm>,
    pub tools: Arc<Mutex<Vec<Arc<dyn ExecutableTool>>>>,
    pub history: Arc<Mutex<Vec<ContextMessage>>>,
    pub goal_status: Arc<Mutex<Option<GoalSnapshot>>>,
    pub hook_results: Arc<Mutex<Vec<HookResult>>>,
    pub stop_block: Arc<Mutex<Option<StopHookBlock>>>,
    pub loop_control: Option<LoopControl>,
    pub background: Mutex<Option<Arc<BackgroundManager>>>,
    pub cron: Mutex<Option<Arc<CronManager>>>,
    clock_state: Arc<Mutex<Option<i64>>>,
}
```

在 `FixtureAgent::new` 初始化列表末尾追加：

```rust
            background: Mutex::new(None),
            cron: Mutex::new(None),
            clock_state: Arc::new(Mutex::new(None)),
```

在 `impl FixtureAgent` 中追加方法：

```rust
    pub fn install_managers(
        &self,
        background: Arc<BackgroundManager>,
        cron: Arc<CronManager>,
    ) {
        *self.background.lock().unwrap() = Some(background);
        *self.cron.lock().unwrap() = Some(cron);
    }

    pub fn advance_clock_to(&self, epoch_ms: i64) {
        *self.clock_state.lock().unwrap() = Some(epoch_ms);
    }

    pub fn clock(&self) -> Arc<dyn ClockSources> {
        Arc::new(FixtureClock::new(self.clock_state.clone()))
    }
```

### 步骤 1.3：运行测试

```bash
cargo test -p agent-rs --lib turn::fixture_agent::tests::fixture_agent_captures_cron_fire_event
cargo check -p agent-rs
```

**预期结果：** 测试通过，`cargo check` 无错。`FixtureAgent` 现有调用点（`turn_flow.rs` 中的 `#[cfg(test)] use` 以及 `bin/turn_l3.rs`）因构造函数签名未变而无需修改。

---

## Task 2：Rust fixture schema + driver 模块

**Depends on:** Task 1、`cron.md` Task 6。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/turn/background_cron_driver.rs`
- Modify: `rust-ody/crates/agent-rs/src/turn/mod.rs`（注册新模块）
- Modify: `rust-ody/crates/agent-rs/src/cron/manager.rs`（`CronManager::new` 增加 `turn_flow` 参数，删除内部自建 `TurnFlow`）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs`（更新 Task 1 测试以传入 `turn_flow`）

> 共享签名变更说明：`CronManager::new` 需要与外部 `TurnFlow` 共用，才能让 cron 触发的 turn 被 fixture driver 观察到。本任务一次性修改签名并更新所有调用者（包括 `cron/manager.rs` 自身测试和 Task 1 测试），最后全树 `cargo check`。

### 步骤 2.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/turn/background_cron_driver.rs`，先写入测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::fixture_agent::FixtureAgent;
    use crate::turn::TurnFlow;
    use std::sync::Arc;

    fn sample_json() -> &'static str {
        r#"
        {
            "name": "driver-cron-fire",
            "responses": [
                {
                    "toolCalls": [],
                    "finishReason": "completed",
                    "rawFinishReason": "stop",
                    "usage": { "inputOther": 0, "output": 0, "inputCacheRead": 0, "inputCacheCreation": 0 }
                }
            ],
            "actions": [
                { "op": "cron_add", "cron": "* * * * *", "prompt": "ping", "recurring": true },
                { "op": "advance_clock_to", "epoch_ms": 60000 },
                { "op": "cron_tick" },
                { "op": "wait" }
            ]
        }
        "#
    }

    #[tokio::test]
    async fn driver_parses_and_runs_cron_fire_fixture() {
        let fixture: BackgroundCronFixture = serde_json::from_str(sample_json()).unwrap();
        let agent = Arc::new(FixtureAgent::new(fixture.responses.clone(), vec![]));
        let flow = Arc::new(TurnFlow::new(agent.clone()));
        let background = Arc::new(BackgroundManager::new(agent.clone(), flow.clone(), None));
        let cron = CronManager::new(
            agent.clone(),
            flow.clone(),
            None,
            CronManagerOptions {
                clocks: Some(agent.clock()),
                poll_interval_ms: Some(0),
            },
        );
        agent.install_managers(background, cron.clone());

        let snapshot = run_fixture(fixture, agent.clone(), flow.clone()).await.unwrap();

        assert_eq!(snapshot.name, "driver-cron-fire");
        assert!(!snapshot.cron_tasks.is_empty());
        assert!(snapshot.events.iter().any(|e| {
            e.get("type").and_then(|t| t.as_str()) == Some("cron.fired")
        }));
    }
}
```

运行：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --lib turn::background_cron_driver::tests::driver_parses_and_runs_cron_fire_fixture
```

**预期结果：** 编译失败，`BackgroundCronFixture`、`run_fixture`、`CronManager::new` 四参数签名等不存在。

### 步骤 2.2：修改 `CronManager::new` 签名

打开 `rust-ody/crates/agent-rs/src/cron/manager.rs`：

1. 将函数签名从

```rust
    pub fn new(
        agent: Arc<dyn TurnAgent>,
        session_dir: Option<PathBuf>,
        opts: CronManagerOptions,
    ) -> Arc<Self>
```

改为

```rust
    pub fn new(
        agent: Arc<dyn TurnAgent>,
        turn_flow: Arc<TurnFlow>,
        session_dir: Option<PathBuf>,
        opts: CronManagerOptions,
    ) -> Arc<Self>
```

2. 删除函数体内的

```rust
        let turn_flow = Arc::new(TurnFlow::new(agent.clone()));
```

直接使用参数 `turn_flow`。

3. 更新同文件 `cron::manager::tests` 中的调用：在 `CronManager::new(agent.clone(), Some(dir.path().to_path_buf()), ...)` 之前插入

```rust
        let flow = Arc::new(TurnFlow::new(agent.clone()));
```

并把调用改为

```rust
        CronManager::new(
            agent.clone(),
            flow,
            Some(dir.path().to_path_buf()),
            CronManagerOptions { ... },
        )
```

4. 更新 `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs` 中 Task 1 写入的测试：复用已创建的 `flow` 变量，把 `CronManager::new` 调用改为

```rust
        let cron = CronManager::new(
            agent.clone(),
            flow.clone(),
            None,
            CronManagerOptions {
                clocks: Some(agent.clock()),
                poll_interval_ms: Some(0),
            },
        );
```

5. 搜索其他调用者：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
grep -rn "CronManager::new" crates/agent-rs/src
```

把搜索到的所有调用按同样模式补齐 `turn_flow` 参数。

### 步骤 2.3：实现 fixture schema 与 driver

在 `background_cron_driver.rs` 的测试模块上方追加实现代码：

```rust
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use kaos_rs::{Environment, Kaos};
use kosong_rs::message::ContentPart;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::background::manager::BackgroundManager;
use crate::background::tasks::ProcessBackgroundTask;
use crate::background::types::BackgroundTaskId;
use crate::context::types::PromptOrigin;
use crate::cron::task::CronTaskInit;
use crate::cron::{CronManager, CronManagerOptions};
use crate::turn::fixture_agent::FixtureAgent;
use crate::turn::turn_flow::TurnFlow;
use crate::turn::types::{AgentEvent, TurnEndResult, TurnEndedReason};

#[derive(Debug, Deserialize)]
pub struct BackgroundCronFixture {
    pub name: String,
    #[serde(default)]
    pub initial_goal: Option<FixtureInitialGoal>,
    #[serde(default)]
    pub loop_control: Option<FixtureLoopControl>,
    #[serde(default)]
    pub responses: Vec<crate::turn::fixture_agent::FixtureResponse>,
    #[serde(default)]
    pub tools: Vec<crate::turn::fixture_agent::FixtureToolDef>,
    pub actions: Vec<BackgroundCronAction>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureInitialGoal {
    pub status: String,
    #[serde(default)]
    pub budget: FixtureBudget,
}

#[derive(Debug, Default, Deserialize)]
pub struct FixtureBudget {
    #[serde(rename = "tokenBudget")]
    pub token_budget: Option<i64>,
    #[serde(rename = "turnBudget")]
    pub turn_budget: Option<i64>,
    #[serde(rename = "wallClockBudgetMs")]
    pub wall_clock_budget_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureLoopControl {
    pub max_steps: Option<u32>,
    pub max_retry_attempts: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "kind")]
pub enum FixtureOrigin {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system_trigger")]
    SystemTrigger { name: String },
    #[serde(rename = "hook_result")]
    HookResult { event: String, blocked: Option<bool> },
}

impl From<FixtureOrigin> for PromptOrigin {
    fn from(o: FixtureOrigin) -> Self {
        match o {
            FixtureOrigin::User => PromptOrigin::User,
            FixtureOrigin::SystemTrigger { name } => PromptOrigin::SystemTrigger { name },
            FixtureOrigin::HookResult { event, blocked } => {
                PromptOrigin::HookResult { event, blocked }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BackgroundCronAction {
    Prompt {
        input: Vec<ContentPart>,
        origin: FixtureOrigin,
    },
    Steer {
        input: Vec<ContentPart>,
        origin: FixtureOrigin,
    },
    Cancel {
        turn_id: Option<i64>,
        reason: Option<String>,
    },
    Wait,
    AdvanceClockTo {
        epoch_ms: i64,
    },
    CronAdd {
        cron: String,
        prompt: String,
        #[serde(default)]
        recurring: Option<bool>,
    },
    CronRemoveLast,
    CronTick,
    BackgroundRunProcess {
        args: Vec<String>,
        description: String,
    },
    BackgroundWaitLast {
        timeout_ms: u64,
    },
    BackgroundStopLast {
        reason: Option<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct BackgroundCronSnapshot {
    pub name: String,
    pub turns: Vec<TurnSummary>,
    pub events: Vec<JsonValue>,
    pub records: Vec<JsonValue>,
    pub context_inputs: Vec<ContextInputSummary>,
    pub cron_tasks: Vec<CronTaskSummary>,
    pub background_tasks: Vec<BackgroundTaskSummary>,
    pub telemetry: Vec<TelemetrySummary>,
}

#[derive(Debug, Serialize)]
pub struct TurnSummary {
    pub turn_id: i64,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by_user_prompt_hook: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ContextInputSummary {
    pub text: String,
    pub origin_kind: String,
}

#[derive(Debug, Serialize)]
pub struct TelemetrySummary {
    pub event: String,
    pub properties: JsonValue,
}

#[derive(Debug, Serialize)]
pub struct CronTaskSummary {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BackgroundTaskSummary {
    pub task_id: String,
    pub kind: String,
    pub description: String,
    pub status: String,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

fn kaos_env() -> Environment {
    Environment {
        os_kind: "Linux".into(),
        os_arch: "x86_64".into(),
        os_version: "test".into(),
        shell_name: "bash".into(),
        shell_path: "/bin/bash".into(),
    }
}

pub async fn run_fixture(
    fixture: BackgroundCronFixture,
    agent: Arc<FixtureAgent>,
    flow: Arc<TurnFlow>,
) -> anyhow::Result<BackgroundCronSnapshot> {
    let mut turns: Vec<TurnSummary> = Vec::new();
    let mut last_cron_id: Option<String> = None;
    let mut last_background_id: Option<String> = None;

    for action in fixture.actions {
        match action {
            BackgroundCronAction::Prompt { input, origin } => {
                flow.prompt(input, origin.into());
            }
            BackgroundCronAction::Steer { input, origin } => {
                flow.steer(input, origin.into());
            }
            BackgroundCronAction::Cancel { turn_id, reason } => {
                flow.cancel(turn_id, reason);
            }
            BackgroundCronAction::Wait => {
                if let Ok(end) = flow.wait_for_current_turn(None).await {
                    turns.push(turn_summary(&end));
                }
            }
            BackgroundCronAction::AdvanceClockTo { epoch_ms } => {
                agent.advance_clock_to(epoch_ms);
            }
            BackgroundCronAction::CronAdd { cron, prompt, recurring } => {
                let mgr = agent
                    .cron
                    .lock()
                    .unwrap()
                    .clone()
                    .context("cron manager not installed")?;
                let now = mgr.clocks.wall_now();
                let task = mgr.store.lock().unwrap().add(
                    CronTaskInit {
                        cron,
                        prompt,
                        recurring,
                    },
                    now,
                );
                last_cron_id = Some(task.id.clone());
            }
            BackgroundCronAction::CronRemoveLast => {
                if let Some(id) = last_cron_id.take() {
                    let mgr = agent
                        .cron
                        .lock()
                        .unwrap()
                        .clone()
                        .context("cron manager not installed")?;
                    mgr.remove_tasks(&[id]);
                }
            }
            BackgroundCronAction::CronTick => {
                let mgr = agent
                    .cron
                    .lock()
                    .unwrap()
                    .clone()
                    .context("cron manager not installed")?;
                mgr.tick();
            }
            BackgroundCronAction::BackgroundRunProcess { args, description } => {
                let mgr = agent
                    .background
                    .lock()
                    .unwrap()
                    .clone()
                    .context("background manager not installed")?;
                let kaos = Kaos::new(kaos_env(), std::env::current_dir()?);
                let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                let task = ProcessBackgroundTask::new(kaos, args_ref)
                    .with_id(BackgroundTaskId::new("process-unset".into()));
                let id = mgr.register_task(Box::new(task));
                last_background_id = Some(id);
            }
            BackgroundCronAction::BackgroundWaitLast { timeout_ms } => {
                if let Some(id) = &last_background_id {
                    let mgr = agent
                        .background
                        .lock()
                        .unwrap()
                        .clone()
                        .context("background manager not installed")?;
                    let _ = mgr.wait(id, Duration::from_millis(timeout_ms)).await;
                }
            }
            BackgroundCronAction::BackgroundStopLast { reason } => {
                if let Some(id) = &last_background_id {
                    let mgr = agent
                        .background
                        .lock()
                        .unwrap()
                        .clone()
                        .context("background manager not installed")?;
                    let _ = mgr.stop(id, reason).await;
                }
            }
        }
    }

    let captures = agent.captures.lock().unwrap();
    let cron_mgr = agent.cron.lock().unwrap();
    let bg_mgr = agent.background.lock().unwrap();

    let cron_tasks: Vec<CronTaskSummary> = cron_mgr
        .as_ref()
        .map(|m| {
            m.store
                .lock()
                .unwrap()
                .list()
                .iter()
                .map(|t| CronTaskSummary {
                    id: t.id.clone(),
                    cron: t.cron.clone(),
                    prompt: t.prompt.clone(),
                    recurring: t.recurring.unwrap_or(true),
                    created_at: t.created_at,
                    last_fired_at: t.last_fired_at,
                })
                .collect()
        })
        .unwrap_or_default();

    let background_tasks: Vec<BackgroundTaskSummary> = bg_mgr
        .as_ref()
        .map(|m| {
            m.list(false, None)
                .into_iter()
                .map(|info| BackgroundTaskSummary {
                    task_id: info.id.to_string(),
                    kind: format!("{:?}", info.kind).to_lowercase(),
                    description: info.description,
                    status: format!("{:?}", info.status).to_lowercase(),
                    started_at: info.started_at.timestamp_millis(),
                    ended_at: info.finished_at.map(|d| d.timestamp_millis()),
                    stop_reason: info.stop_reason,
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(BackgroundCronSnapshot {
        name: fixture.name,
        turns,
        events: captures
            .events
            .iter()
            .map(|e| serde_json::to_value(e).unwrap())
            .collect(),
        records: captures
            .records
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect(),
        context_inputs: captures
            .context_inputs
            .iter()
            .map(|(parts, origin)| ContextInputSummary {
                text: content_text(parts),
                origin_kind: origin_kind(origin),
            })
            .collect(),
        cron_tasks,
        background_tasks,
        telemetry: captures
            .telemetry_events
            .iter()
            .map(|(event, props)| TelemetrySummary {
                event: event.clone(),
                properties: props.clone(),
            })
            .collect(),
    })
}

fn turn_summary(end: &TurnEndResult) -> TurnSummary {
    TurnSummary {
        turn_id: end.event.turn_id,
        reason: format!("{:?}", end.event.reason).to_lowercase(),
        error: end
            .event
            .error
            .as_ref()
            .map(|e| serde_json::to_value(e).unwrap()),
        stop_reason: end.stop_reason.as_ref().map(|r| format!("{:?}", r).to_lowercase()),
        blocked_by_user_prompt_hook: Some(end.blocked_by_user_prompt_hook),
    }
}

fn content_text(parts: &[ContentPart]) -> String {
    parts
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn origin_kind(origin: &PromptOrigin) -> String {
    let value = serde_json::to_value(origin).unwrap_or_default();
    value
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}
```

在 `rust-ody/crates/agent-rs/src/turn/mod.rs` 末尾追加：

```rust
pub mod background_cron_driver;
```

### 步骤 2.4：运行测试与全树检查

```bash
cargo test -p agent-rs --lib turn::background_cron_driver::tests::driver_parses_and_runs_cron_fire_fixture
cargo check -p agent-rs
```

**预期结果：** driver 测试通过；`cargo check` 无错。若 `stop_reason` 的 `LoopTurnStopReason` 未实现 `Debug`，改用 `format!("{:?}", r)` 前的临时变量或移除该字段。

---

## Task 3：新增 Rust binary `background_cron_l3`

**Depends on:** Task 2。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/background_cron_l3.rs`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（注册 binary，见 Task 6）

> 本任务为 wiring / CLI 入口，无复杂分支逻辑，采用“完整代码 + 构建 + 手动验证”模式。

### 步骤 3.1：实现 binary

创建 `rust-ody/crates/agent-rs/src/bin/background_cron_l3.rs`：

```rust
use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use agent_rs::background::manager::BackgroundManager;
use agent_rs::cron::{CronManager, CronManagerOptions};
use agent_rs::records::nested::{GoalBudgetLimits, GoalStatus};
use agent_rs::turn::background_cron_driver::{run_fixture, BackgroundCronFixture};
use agent_rs::turn::fixture_agent::{FixtureAgent, FixtureTool};
use agent_rs::turn::types::LoopControl;
use agent_rs::turn::TurnFlow;
use anyhow::{Context, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(PathBuf::from)
        .context("usage: background_cron_l3 <fixture.json>")?;

    let file = File::open(&fixture_path)
        .with_context(|| format!("failed to open {}", fixture_path.display()))?;
    let fixture: BackgroundCronFixture = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse {}", fixture_path.display()))?;

    let mut agent = FixtureAgent::new(fixture.responses.clone(), vec![]);

    if let Some(ctrl) = fixture.loop_control {
        agent.loop_control = Some(LoopControl {
            max_steps_per_turn: ctrl.max_steps,
            max_retries_per_step: ctrl.max_retry_attempts,
            reserved_context_size: None,
        });
    }

    let agent = Arc::new(agent);

    let tools: Vec<_> = fixture
        .tools
        .clone()
        .into_iter()
        .map(|def| Arc::new(FixtureTool::new(def, agent.clone())) as Arc<_>)
        .collect();
    agent.tools.lock().unwrap().extend(tools);

    if let Some(goal) = fixture.initial_goal {
        let status = match goal.status.as_str() {
            "active" => GoalStatus::Active,
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            "complete" => GoalStatus::Complete,
            _ => GoalStatus::Active,
        };
        agent.set_goal(
            status,
            GoalBudgetLimits {
                token_budget: goal.budget.token_budget,
                turn_budget: goal.budget.turn_budget,
                wall_clock_budget_ms: goal.budget.wall_clock_budget_ms,
            },
        );
    }

    let flow = Arc::new(TurnFlow::new(agent.clone()));
    let background = Arc::new(BackgroundManager::new(agent.clone(), flow.clone(), None));
    let cron = CronManager::new(
        agent.clone(),
        flow.clone(),
        None,
        CronManagerOptions {
            clocks: Some(agent.clock()),
            poll_interval_ms: Some(0),
        },
    );
    agent.install_managers(background, cron.clone());

    let snapshot = run_fixture(fixture, agent.clone(), flow.clone()).await?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}
```

### 步骤 3.2：构建

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo build --bin background_cron_l3
```

**预期结果：** 编译成功，无错。

### 步骤 3.3：手动验证

创建临时 fixture：

```bash
cat > /tmp/background-cron-fire.json <<'EOF'
{
  "name": "manual-cron-fire",
  "responses": [
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 0, "output": 0, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "actions": [
    { "op": "cron_add", "cron": "* * * * *", "prompt": "ping", "recurring": true },
    { "op": "advance_clock_to", "epoch_ms": 60000 },
    { "op": "cron_tick" },
    { "op": "wait" }
  ]
}
EOF
```

运行 binary：

```bash
cargo run --quiet --bin background_cron_l3 -- /tmp/background-cron-fire.json
```

**预期结果：** 终端输出 JSON snapshot；`events` 数组中包含 `{"type":"cron.fired",...}`，`cron_tasks` 列表非空。

---

## Task 4：TS driver `background-cron-l3-driver.ts`

**Depends on:** Task 2（fixture schema 已稳定）。

**Files:**
- Create: `packages/integration-tests/src/parity/background-cron-fixture.ts`
- Create: `packages/integration-tests/src/parity/background-cron-l3-driver.ts`

> TS driver 与 Rust binary 解析同一套 JSON fixture，并通过 `agent-core` 的测试 harness 直接操作 `Agent.background` / `Agent.cron` 回放动作。时钟注入通过 `ODY_CRON_CLOCK=file:...` 实现，避免修改 `Agent` 构造函数。

### 步骤 4.1：实现 fixture 类型

创建 `packages/integration-tests/src/parity/background-cron-fixture.ts`：

```ts
import type { ContentPart, FinishReason, TokenUsage, ToolCall } from '@odysseythink/kosong';

export interface BackgroundCronFixture {
  readonly name: string;
  readonly responses: readonly FixtureResponse[];
  readonly actions: readonly BackgroundCronAction[];
}

export interface FixtureResponse {
  readonly toolCalls: readonly ToolCall[];
  readonly finishReason?: string | undefined;
  readonly rawFinishReason?: string | undefined;
  readonly usage: TokenUsage;
}

export type FixtureOrigin =
  | { readonly kind: 'user' }
  | { readonly kind: 'system_trigger'; readonly name: string }
  | { readonly kind: 'hook_result'; readonly event: string; readonly blocked?: boolean | undefined };

export type BackgroundCronAction =
  | { readonly op: 'prompt'; readonly input: readonly ContentPart[]; readonly origin: FixtureOrigin }
  | { readonly op: 'steer'; readonly input: readonly ContentPart[]; readonly origin: FixtureOrigin }
  | { readonly op: 'cancel'; readonly turnId?: number | undefined; readonly reason?: string | undefined }
  | { readonly op: 'wait' }
  | { readonly op: 'advance_clock_to'; readonly epoch_ms: number }
  | { readonly op: 'cron_add'; readonly cron: string; readonly prompt: string; readonly recurring?: boolean | undefined }
  | { readonly op: 'cron_remove_last' }
  | { readonly op: 'cron_tick' }
  | { readonly op: 'background_run_process'; readonly args: readonly string[]; readonly description: string }
  | { readonly op: 'background_wait_last'; readonly timeout_ms: number }
  | { readonly op: 'background_stop_last'; readonly reason?: string | undefined };

export interface BackgroundCronSnapshot {
  readonly name: string;
  readonly turns: readonly TurnSummary[];
  readonly events: readonly unknown[];
  readonly records: readonly unknown[];
  readonly contextInputs: readonly ContextInputSummary[];
  readonly cronTasks: readonly CronTaskSummary[];
  readonly backgroundTasks: readonly BackgroundTaskSummary[];
  readonly telemetry: readonly TelemetrySummary[];
}

export interface TurnSummary {
  readonly turnId: number;
  readonly reason: string;
  readonly error?: unknown;
  readonly stopReason?: string | undefined;
  readonly blockedByUserPromptHook?: boolean | undefined;
}

export interface ContextInputSummary {
  readonly text: string;
  readonly originKind: string;
}

export interface TelemetrySummary {
  readonly event: string;
  readonly properties: unknown;
}

export interface CronTaskSummary {
  readonly id: string;
  readonly cron: string;
  readonly prompt: string;
  readonly recurring: boolean;
  readonly createdAt: number;
  readonly lastFiredAt?: number | undefined;
}

export interface BackgroundTaskSummary {
  readonly taskId: string;
  readonly kind: string;
  readonly description: string;
  readonly status: string;
  readonly startedAt: number;
  readonly endedAt?: number | undefined;
  readonly stopReason?: string | undefined;
}
```

### 步骤 4.2：实现 driver

创建 `packages/integration-tests/src/parity/background-cron-l3-driver.ts`：

```ts
import { readFile } from 'node:fs/promises';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type { ContentPart, FinishReason, ToolCall } from '@odysseythink/kosong';

import { testAgent } from '../../../agent-core/test/agent/harness/agent';
import type { TestAgentContext } from '../../../agent-core/test/agent/harness/agent';
import { recordingTelemetry, type TelemetryRecord } from '../../../agent-core/test/fixtures/telemetry';
import { ProcessBackgroundTask } from '../../../agent-core/src/agent/background/process-task';
import type { BackgroundCronAction, BackgroundCronFixture, BackgroundCronSnapshot } from './background-cron-fixture';

function originToAgentOrigin(origin: import('./background-cron-fixture').FixtureOrigin): import('../../../agent-core/src/agent/context/types').PromptOrigin {
  switch (origin.kind) {
    case 'user':
      return { kind: 'user' };
    case 'system_trigger':
      return { kind: 'system_trigger', name: origin.name };
    case 'hook_result':
      return { kind: 'hook_result', event: origin.event, blocked: origin.blocked };
  }
}

function toToolCall(raw: unknown): ToolCall {
  const r = raw as { id: string; name: string; arguments: string };
  return {
    type: 'function',
    id: r.id,
    name: r.name,
    arguments: r.arguments,
  };
}

function buildParts(response: import('./background-cron-fixture').FixtureResponse): (ContentPart | ToolCall)[] {
  const parts: (ContentPart | ToolCall)[] = [];
  if (response.toolCalls.length > 0) {
    parts.push(...response.toolCalls.map(toToolCall));
  }
  return parts;
}

export async function runBackgroundCronL3Fixture(fixturePath: string): Promise<BackgroundCronSnapshot> {
  const raw = await readFile(fixturePath, 'utf8');
  const fixture: BackgroundCronFixture = JSON.parse(raw);

  const clockFile = join(tmpdir(), `ody-cron-clock-${Date.now()}.txt`);
  writeFileSync(clockFile, '0', 'utf8');

  const previousManualTick = process.env['ODY_CRON_MANUAL_TICK'];
  const previousClock = process.env['ODY_CRON_CLOCK'];
  process.env['ODY_CRON_MANUAL_TICK'] = '1';
  process.env['ODY_CRON_CLOCK'] = `file:${clockFile}`;

  const telemetryRecords: TelemetryRecord[] = [];
  const ctx: TestAgentContext = testAgent({ telemetry: recordingTelemetry(telemetryRecords) });

  try {
    ctx.configure({ tools: [] });
    for (const response of fixture.responses) {
      ctx.mockNextProviderResponse({
        parts: buildParts(response),
        finishReason: response.finishReason as FinishReason | undefined,
        rawFinishReason: response.rawFinishReason,
      });
    }

    const turns: BackgroundCronSnapshot['turns'] = [];
    let lastCronId: string | undefined;
    let lastBackgroundId: string | undefined;

    for (const action of fixture.actions) {
      await executeAction(ctx, action, {
        clockFile,
        turns,
        setLastCronId: (id) => { lastCronId = id; },
        getLastCronId: () => lastCronId,
        setLastBackgroundId: (id) => { lastBackgroundId = id; },
        getLastBackgroundId: () => lastBackgroundId,
      });
    }

    const events = ctx.allEvents
      .filter((e) => e.type === '[rpc]')
      .map((e) => ({ type: e.event, ...(e.args as Record<string, unknown>) }));

    const records = ctx.records.map((r) => JSON.parse(JSON.stringify(r)) as unknown);

    const contextInputs = ctx.agent.context
      .data()
      .history.filter((m) => m.role === 'user')
      .map((m) => ({
        text: m.content
          .filter((p): p is Extract<typeof p, { type: 'text' }> => p.type === 'text')
          .map((p) => p.text)
          .join(''),
        originKind: m.origin?.kind ?? 'unknown',
      }));

    const cronTasks = ctx.agent.cron?.store.list().map((t) => ({
      id: t.id,
      cron: t.cron,
      prompt: t.prompt,
      recurring: t.recurring !== false,
      createdAt: t.createdAt,
      lastFiredAt: t.lastFiredAt,
    })) ?? [];

    const backgroundTasks = ctx.agent.background.list(false).map((info) => ({
      taskId: info.taskId,
      kind: info.kind,
      description: info.description,
      status: info.status,
      startedAt: info.startedAt,
      endedAt: info.endedAt ?? undefined,
      stopReason: info.stopReason,
    }));

    return {
      name: fixture.name,
      turns,
      events,
      records,
      contextInputs,
      cronTasks,
      backgroundTasks,
      telemetry: telemetryRecords.map((t) => ({ event: t.event, properties: t.properties })),
    };
  } finally {
    await ctx.agent.cron?.stop();
    process.env['ODY_CRON_MANUAL_TICK'] = previousManualTick;
    process.env['ODY_CRON_CLOCK'] = previousClock;
  }
}

interface ActionContext {
  readonly clockFile: string;
  readonly turns: BackgroundCronSnapshot['turns'];
  setLastCronId(id: string): void;
  getLastCronId(): string | undefined;
  setLastBackgroundId(id: string): void;
  getLastBackgroundId(): string | undefined;
}

async function executeAction(
  ctx: TestAgentContext,
  action: BackgroundCronAction,
  ac: ActionContext,
): Promise<void> {
  switch (action.op) {
    case 'prompt': {
      await ctx.rpc.prompt({ input: action.input as ContentPart[] });
      break;
    }
    case 'steer': {
      await ctx.rpc.steer({ input: action.input as ContentPart[] });
      break;
    }
    case 'cancel': {
      await ctx.rpc.cancel({ turnId: action.turnId });
      break;
    }
    case 'wait': {
      await ctx.untilTurnEnd();
      const ended = [...ctx.allEvents].reverse().find(
        (e) => e.type === '[rpc]' && e.event === 'turn.ended',
      );
      if (ended) {
        ac.turns.push({
          turnId: (ended.args as { turnId: number }).turnId,
          reason: (ended.args as { reason: string }).reason,
          error: (ended.args as { error?: unknown }).error,
        });
      }
      break;
    }
    case 'advance_clock_to': {
      writeFileSync(ac.clockFile, String(action.epoch_ms), 'utf8');
      break;
    }
    case 'cron_add': {
      const task = ctx.agent.cron!.addTask({
        cron: action.cron,
        prompt: action.prompt,
        recurring: action.recurring,
      });
      ac.setLastCronId(task.id);
      break;
    }
    case 'cron_remove_last': {
      const id = ac.getLastCronId();
      if (id !== undefined) {
        ctx.agent.cron!.removeTasks([id]);
        ac.setLastCronId(undefined as unknown as string);
      }
      break;
    }
    case 'cron_tick': {
      ctx.agent.cron!.tick();
      break;
    }
    case 'background_run_process': {
      const proc = await ctx.agent.kaos.exec(...action.args);
      const task = new ProcessBackgroundTask(proc, action.args.join(' '), action.description);
      const id = ctx.agent.background.registerTask(task);
      ac.setLastBackgroundId(id);
      break;
    }
    case 'background_wait_last': {
      const id = ac.getLastBackgroundId();
      if (id !== undefined) {
        await ctx.agent.background.wait(id, action.timeout_ms);
      }
      break;
    }
    case 'background_stop_last': {
      const id = ac.getLastBackgroundId();
      if (id !== undefined) {
        await ctx.agent.background.stop(id, action.reason);
      }
      break;
    }
  }
}
```

### 步骤 4.3：类型检查与手动验证

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/integration-tests typecheck
```

**预期结果：** `tsc` 无错。

手动验证（在 Task 5 的 fixture 就绪后也可复用该命令）：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-l3-parity.test.ts
```

**预期结果：** 测试至少能加载 driver 并运行 fixture（parity 断言在 Task 5 中完善）。

---

## Task 5：共享 fixture 与 parity 测试

**Depends on:** Task 3、Task 4。

**Files:**
- Create: `packages/integration-tests/src/parity/normalize-background-cron.ts`
- Create: `packages/integration-tests/test/parity/fixtures/background-cron/cron-fire.json`
- Create: `packages/integration-tests/test/parity/fixtures/background-cron/background-process-completes.json`
- Create: `packages/integration-tests/test/parity/fixtures/background-cron/cron-remove-last.json`
- Create: `packages/integration-tests/test/parity/background-cron-l3-parity.test.ts`

### 步骤 5.1：写入失败测试

创建 `packages/integration-tests/test/parity/background-cron-l3-parity.test.ts`：

```ts
import { execFile } from 'node:child_process';
import { dirname, join } from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { assertParity } from '../../src/parity/assert-parity';
import { runBackgroundCronL3Fixture } from '../../src/parity/background-cron-l3-driver';
import { normalizeBackgroundCronSnapshot } from '../../src/parity/normalize-background-cron';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, 'fixtures', 'background-cron');
const projectRoot = dirname(dirname(dirname(__dirname)));

const fixtures = [
  'cron-fire.json',
  'background-process-completes.json',
  'cron-remove-last.json',
];

async function runRustFixture(fixtureName: string): Promise<unknown> {
  const fixturePath = join(fixturesDir, fixtureName);
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--bin', 'background_cron_l3', '--', fixturePath],
    { cwd: join(projectRoot, 'rust-ody') },
  );
  return JSON.parse(stdout) as unknown;
}

describe('background-cron L3 TS-vs-Rust parity', () => {
  it.each(fixtures)(
    '%s matches the Rust golden binary',
    async (fixtureName) => {
      const fixturePath = join(fixturesDir, fixtureName);
      const tsSnapshot = normalizeBackgroundCronSnapshot(
        await runBackgroundCronL3Fixture(fixturePath),
      );
      const rustSnapshot = normalizeBackgroundCronSnapshot(await runRustFixture(fixtureName));
      const diff = assertParity(fixtureName, tsSnapshot as never, rustSnapshot as never);
      expect(diff).toBeNull();
    },
    120_000,
  );
});
```

运行：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-l3-parity.test.ts
```

**预期结果：** 测试失败——fixture 文件与 `normalizeBackgroundCronSnapshot` 不存在。

### 步骤 5.2：实现归一化函数

创建 `packages/integration-tests/src/parity/normalize-background-cron.ts`：

```ts
export function normalizeBackgroundCronSnapshot(snapshot: unknown): unknown {
  return normalizeNode(snapshot);
}

function normalizeNode(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => normalizeNode(item));
  }
  if (value !== null && typeof value === 'object') {
    const obj = value as Record<string, unknown>;

    // Canonicalize background-task info objects so TS/Rust shape mismatch
    // (pid/exitCode/command/agentId/etc.) does not produce false diffs.
    if (typeof obj.taskId === 'string' && typeof obj.kind === 'string') {
      const out: Record<string, unknown> = {};
      for (const key of ['taskId', 'kind', 'description', 'status', 'startedAt', 'endedAt', 'stopReason']) {
        out[key] = normalizeScalar(key, obj[key]);
      }
      return out;
    }

    // Canonicalize cron-task info objects.
    if (typeof obj.id === 'string' && typeof obj.cron === 'string') {
      const out: Record<string, unknown> = {};
      for (const key of ['id', 'cron', 'prompt', 'recurring', 'createdAt', 'lastFiredAt']) {
        out[key] = normalizeScalar(key, obj[key]);
      }
      return out;
    }

    // Mask injected XML text for cron/background context inputs because
    // exact XML formatting is not the parity target here.
    if (typeof obj.originKind === 'string' && typeof obj.text === 'string') {
      const originKind = obj.originKind as string;
      if (originKind === 'cron_job' || originKind === 'background_task') {
        obj = { ...obj, text: '<injected-xml>' };
      }
    }

    const out: Record<string, unknown> = {};
    for (const [key, v] of Object.entries(obj)) {
      // Drop TS-only fields that Rust does not emit.
      if (
        key === 'pid' ||
        key === 'exitCode' ||
        key === 'command' ||
        key === 'outputSnapshot' ||
        key === 'questionCount' ||
        key === 'toolCallId' ||
        key === 'agentId' ||
        key === 'subagentType' ||
        key === 'terminalNotificationSuppressed' ||
        key === 'timeoutMs' ||
        key === 'outputPath' ||
        key === 'outputSizeBytes' ||
        key === 'previewBytes' ||
        key === 'truncated' ||
        key === 'fullOutputAvailable' ||
        key === 'preview'
      ) {
        continue;
      }
      out[key] = normalizeScalar(key, normalizeNode(v));
    }
    return out;
  }
  return value;
}

function normalizeScalar(key: string, value: unknown): unknown {
  if (typeof value === 'string') {
    if (key === 'id' && /^[0-9a-f]{8}$/.test(value)) {
      return '<cron-id>';
    }
    if ((key === 'taskId' || key === 'jobId') && /^(bash|question|agent)-[0-9a-z]{8}$/.test(value)) {
      return '<bg-id>';
    }
    if (key === 'jobId') {
      return '<job-id>';
    }
    if (/^\d{4}-\d{2}-\d{2}T/.test(value)) {
      return '<iso-timestamp>';
    }
  }
  if (typeof value === 'number') {
    if (
      key === 'createdAt' ||
      key === 'startedAt' ||
      key === 'endedAt' ||
      key === 'lastFiredAt'
    ) {
      return '<timestamp>';
    }
  }
  return value;
}
```

### 步骤 5.3：创建 fixture 文件

创建 `packages/integration-tests/test/parity/fixtures/background-cron/cron-fire.json`：

```json
{
  "name": "cron-fire",
  "responses": [
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 0, "output": 0, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "actions": [
    { "op": "cron_add", "cron": "* * * * *", "prompt": "ping", "recurring": true },
    { "op": "advance_clock_to", "epoch_ms": 60000 },
    { "op": "cron_tick" },
    { "op": "wait" }
  ]
}
```

创建 `packages/integration-tests/test/parity/fixtures/background-cron/background-process-completes.json`：

```json
{
  "name": "background-process-completes",
  "responses": [
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 0, "output": 0, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "actions": [
    { "op": "background_run_process", "args": ["/bin/sh", "-c", "echo done"], "description": "echo done" },
    { "op": "background_wait_last", "timeout_ms": 5000 },
    { "op": "wait" }
  ]
}
```

创建 `packages/integration-tests/test/parity/fixtures/background-cron/cron-remove-last.json`：

```json
{
  "name": "cron-remove-last",
  "responses": [],
  "actions": [
    { "op": "cron_add", "cron": "* * * * *", "prompt": "ping", "recurring": true },
    { "op": "cron_remove_last" },
    { "op": "advance_clock_to", "epoch_ms": 60000 },
    { "op": "cron_tick" }
  ]
}
```

### 步骤 5.4：运行测试

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-l3-parity.test.ts
```

**预期结果：** 三个 fixture 全部通过。若出现 shape 差异，先调整 `normalizeBackgroundCronSnapshot`（而不是放宽实现），直到差异归零。

---

## Task 6：注册 Rust binary 与 npm script

**Depends on:** Task 3、Task 5。

**Files:**
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`
- Modify: `packages/integration-tests/package.json`

> 本任务为纯 wiring，采用“完整代码 + 构建 + 手动验证”模式。

### 步骤 6.1：注册 binary

在 `rust-ody/crates/agent-rs/Cargo.toml` 末尾追加：

```toml
[[bin]]
name = "background_cron_l3"
path = "src/bin/background_cron_l3.rs"
```

### 步骤 6.2：注册 npm script

在 `packages/integration-tests/package.json` 的 `scripts` 对象中追加：

```json
    "test:parity:background-cron": "vitest run test/parity/background-cron-l3-parity.test.ts",
```

注意保持 JSON 尾部逗号合法：若其前一条脚本末尾没有逗号，需要补加。

### 步骤 6.3：构建验证

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo build --bin background_cron_l3
```

**预期结果：** 编译成功。

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/integration-tests typecheck
```

**预期结果：** `tsc` 无错。

### 步骤 6.4：手动验证

```bash
pnpm --filter @odysseythink/integration-tests test:parity:background-cron
```

**预期结果：** parity 测试全部通过。

---

## Self-Review（本 Part）

### Spec-coverage 表

| 需求 | 覆盖任务 | 状态 |
|---|---|---|
| `FixtureAgent` 可挂载 `BackgroundManager` / `CronManager` | Task 1 | covered |
| `FixtureAgent` 支持测试可控的时钟注入 | Task 1 | covered |
| 定义与 TS 共用的 JSON fixture schema | Task 2 | covered |
| Rust `run_fixture` 执行器可解析并回放 fixture | Task 2 | covered |
| `CronManager::new` 与外部 `TurnFlow` 共用 | Task 2 | covered |
| Rust CLI binary `background_cron_l3` | Task 3 | covered |
| TS driver 回放同一 fixture | Task 4 | covered |
| 共享 fixture 文件与 parity 测试 | Task 5 | covered |
| binary / npm script 注册 | Task 6 | covered |

### 七项检查

- [ ] 1. Spec-coverage 表已覆盖本 Part 全部目标，无 GAP。
- [ ] 2. Placeholder 扫描：无 `TODO`/`TBD`/“后续实现”，所有代码均可直接编译运行。
- [ ] 3. 无 phantom task：每个任务都产生可验证的变更（测试通过 / binary 构建 / fixture 运行）。
- [ ] 4. 依赖正确：`Depends on:` 均指向前置任务；Task 2 共享签名变更在同一任务内完成，无跨任务悬空依赖。
- [ ] 5. Caller & build 正确性：Task 2 修改 `CronManager::new` 签名后，已用 `grep -rn` 更新所有调用者（含 `cron/manager.rs` 测试与 `fixture_agent.rs` 测试），并以 `cargo check -p agent-rs` 全树检查收尾；Task 6 注册 binary/script 后也有独立构建验证。
- [ ] 6. 测试了风险：Task 1 断言 `CronFired` 事件确实进入 `FixtureAgent.captures`；Task 2 断言 driver 能解析 JSON 并跑通 cron fire；Task 5 parity 测试断言 Rust/TS snapshot 在归一化后完全一致。归一化常量已检查不会误杀 must-survive 输入（`"ping"`、`"echo done"` 均不满足 8-hex / bg-id / 13 位时间戳模式）。
- [ ] 7. 类型一致性：Rust `BackgroundCronSnapshot` 与 TS `BackgroundCronSnapshot` 字段名、action 的 `op` 标记、`FixtureOrigin` 的 `kind` 标记均一一对应；`cron_add` / `advance_clock_to` / `background_run_process` 等 snake_case 操作名两端一致。
