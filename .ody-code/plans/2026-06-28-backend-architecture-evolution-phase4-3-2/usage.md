# Part 2: UsageRecorder

本部分迁移 `packages/agent-core/src/agent/usage/index.ts`，把 Agent 的用量记录面抽象成可独立测试的 Rust 模块。与 `ConfigState` 一致，采用 `UsageRecorderContext` trait 隔离对 Agent 其余子系统的依赖。

---

### Task 1: 实现 `UsageStatus` 与 `UsageRecorder`

**Depends on:** 4.3.0 records 层（`AgentRecord::UsageRecord`、`UsageRecordScope` 已定义）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/usage/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/usage/recorder.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs`
- Test: `rust-ody/crates/agent-rs/tests/usage_recorder.rs`

**目标：** 定义 `UsageStatus`、作用域枚举、`UsageRecorderContext` trait 与 `UsageRecorder`，行为与 TS 侧 `UsageRecorder` 逐字段对齐。

- [ ] 新建 `rust-ody/crates/agent-rs/src/usage/mod.rs`：

```rust
pub mod recorder;
pub use recorder::{UsageRecorder, UsageRecorderContext, UsageRecordScope, UsageStatus};
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/usage/recorder.rs`：

```rust
use std::collections::HashMap;

use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};

pub use crate::records::nested::UsageRecordScope;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_model: Option<HashMap<String, TokenUsage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_turn: Option<TokenUsage>,
}

fn copy_usage(usage: &TokenUsage) -> TokenUsage {
    TokenUsage {
        input_other: usage.input_other,
        output: usage.output,
        input_cache_read: usage.input_cache_read,
        input_cache_creation: usage.input_cache_creation,
    }
}

fn add_usage(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input_other: a.input_other + b.input_other,
        output: a.output + b.output,
        input_cache_read: a.input_cache_read + b.input_cache_read,
        input_cache_creation: a.input_cache_creation + b.input_cache_creation,
    }
}

fn total_usage(by_model: &HashMap<String, TokenUsage>) -> Option<TokenUsage> {
    let mut total: Option<TokenUsage> = None;
    for usage in by_model.values() {
        total = Some(match total {
            Some(t) => add_usage(t, copy_usage(usage)),
            None => copy_usage(usage),
        });
    }
    total
}

/// Minimal Agent surface required by `UsageRecorder`.
pub trait UsageRecorderContext {
    fn log_record(&mut self, record: crate::records::AgentRecord);
    fn emit_status_updated(&mut self);
}

pub struct UsageRecorder<C: UsageRecorderContext> {
    context: C,
    by_model: HashMap<String, TokenUsage>,
    current_turn: Option<TokenUsage>,
}

impl<C: UsageRecorderContext> UsageRecorder<C> {
    pub fn new(context: C) -> Self {
        Self {
            context,
            by_model: HashMap::new(),
            current_turn: None,
        }
    }

    pub fn begin_turn(&mut self) {
        self.current_turn = None;
    }

    pub fn end_turn(&mut self) {
        self.current_turn = None;
    }

    pub fn record(&mut self, model: &str, usage: TokenUsage, scope: UsageRecordScope) {
        self.context.log_record(crate::records::AgentRecord::UsageRecord {
            time: None,
            model: model.to_owned(),
            usage,
            usage_scope: Some(scope),
        });

        let current = self.by_model.get(model).cloned();
        self.by_model.insert(
            model.to_owned(),
            match current {
                Some(c) => add_usage(c, usage),
                None => copy_usage(&usage),
            },
        );

        if scope == UsageRecordScope::Turn {
            self.current_turn = Some(match self.current_turn {
                Some(c) => add_usage(c, usage),
                None => copy_usage(&usage),
            });
        }

        self.context.emit_status_updated();
    }

    pub fn data(&self) -> UsageStatus {
        let by_model = self.by_model_snapshot();
        let has_by_model = !by_model.is_empty();
        let total = if has_by_model {
            total_usage(&by_model)
        } else {
            None
        };
        UsageStatus {
            by_model: if has_by_model { Some(by_model) } else { None },
            total,
            current_turn: self.current_turn.as_ref().map(copy_usage),
        }
    }

    pub fn status(&self) -> Option<UsageStatus> {
        let status = self.data();
        if status.by_model.is_none()
            && status.total.is_none()
            && status.current_turn.is_none()
        {
            return None;
        }
        Some(status)
    }

    pub fn into_inner(self) -> C {
        self.context
    }

    fn by_model_snapshot(&self) -> HashMap<String, TokenUsage> {
        self.by_model
            .iter()
            .map(|(k, v)| (k.clone(), copy_usage(v)))
            .collect()
    }
}
```

- [ ] 修改 `rust-ody/crates/agent-rs/src/lib.rs`，加入 `usage` 模块导出：

```rust
pub mod config;
pub mod records;
pub mod usage;

pub use records::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/tests/usage_recorder.rs`（先写失败测试）：

```rust
use std::cell::RefCell;

use agent_rs::records::AgentRecord;
use agent_rs::usage::{UsageRecordScope, UsageRecorder, UsageRecorderContext};
use kosong_rs::usage::TokenUsage;

#[derive(Debug, Default)]
struct MockContext {
    records: RefCell<Vec<AgentRecord>>,
    status_emits: RefCell<usize>,
}

impl UsageRecorderContext for MockContext {
    fn log_record(&mut self, record: AgentRecord) {
        self.records.borrow_mut().push(record);
    }

    fn emit_status_updated(&mut self) {
        *self.status_emits.borrow_mut() += 1;
    }
}

#[test]
fn empty_recorder_returns_no_status() {
    let recorder = UsageRecorder::new(MockContext::default());
    assert_eq!(recorder.status(), None);
    let data = recorder.data();
    assert!(data.by_model.is_none());
    assert!(data.total.is_none());
    assert!(data.current_turn.is_none());
}

#[test]
fn session_record_accumulates_by_model_and_logs() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    let usage = TokenUsage {
        input_other: 10,
        output: 5,
        input_cache_read: 2,
        input_cache_creation: 1,
    };
    recorder.record("kimi-k2", usage, UsageRecordScope::Session);

    let data = recorder.data();
    let by_model = data.by_model.unwrap();
    let model_usage = by_model.get("kimi-k2").unwrap();
    assert_eq!(model_usage.input_other, 10);
    assert_eq!(model_usage.output, 5);
    assert_eq!(data.total.unwrap().grand_total(), 18);

    let ctx = recorder.into_inner();
    assert_eq!(ctx.records.borrow().len(), 1);
    assert_eq!(*ctx.status_emits.borrow(), 1);
}

#[test]
fn turn_record_accumulates_current_turn() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 4,
            output: 2,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 1,
            output: 1,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );

    let data = recorder.data();
    assert_eq!(data.current_turn.unwrap().input_other, 5);
    assert_eq!(data.current_turn.unwrap().output, 3);
    let by_model = data.by_model.unwrap();
    assert_eq!(by_model.get("kimi-k2").unwrap().input_other, 5);
}

#[test]
fn begin_turn_resets_current_turn_but_keeps_session_total() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 1,
            output: 1,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );
    assert!(recorder.data().current_turn.is_some());
    recorder.begin_turn();
    assert!(recorder.data().current_turn.is_none());
    assert!(recorder.data().by_model.is_some());
}

#[test]
fn multiple_models_tracked_separately() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 1,
            output: 0,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Session,
    );
    recorder.record(
        "gpt-4o",
        TokenUsage {
            input_other: 0,
            output: 2,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Session,
    );
    let data = recorder.data();
    assert_eq!(data.by_model.as_ref().unwrap().len(), 2);
    assert_eq!(data.total.unwrap().output, 2);
}
```

- [ ] 运行测试，确认失败（类型或符号未找到）：

```bash
cd rust-ody && cargo test -p agent-rs --test usage_recorder
```

预期失败：`error[E0433]: failed to resolve: use of undeclared crate or module 'usage'`（因为 `lib.rs` 尚未导出）。

- [ ] 完成实现并再次运行：

```bash
cd rust-ody && cargo test -p agent-rs --test usage_recorder
```

预期输出：`test result: ok. 5 passed; 0 failed`。

- [ ] 运行整 crate 类型检查：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：无错误，`Finished dev [unoptimized + debuginfo] target(s)`。

- [ ] Commit：`feat(agent-rs): implement UsageRecorder with UsageRecorderContext trait`

---

### Task 2: 生成 `UsageRecorder` fixture 与 round-trip 对照

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/generate_usage_fixture.rs`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/usage-rust.json`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`
- Test: `rust-ody/crates/agent-rs/tests/usage_fixture_parity.rs`

**目标：** 让 Rust 生成一份 `UsageStatus` JSON fixture，供后续 `parity.md` 做 TS↔Rust 字段对照；Rust 侧先做 round-trip 自检。

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` 末尾新增 bin：

```toml
[[bin]]
name = "generate-usage-fixture"
path = "src/bin/generate_usage_fixture.rs"
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/bin/generate_usage_fixture.rs`：

```rust
use std::{env, fs, path::PathBuf};

use agent_rs::records::AgentRecord;
use agent_rs::usage::{UsageRecordScope, UsageRecorder, UsageRecorderContext};
use kosong_rs::usage::TokenUsage;

struct NoopCtx;

impl UsageRecorderContext for NoopCtx {
    fn log_record(&mut self, _record: AgentRecord) {}
    fn emit_status_updated(&mut self) {}
}

fn main() {
    let mut recorder = UsageRecorder::new(NoopCtx);
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 10,
            output: 5,
            input_cache_read: 2,
            input_cache_creation: 1,
        },
        UsageRecordScope::Session,
    );
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 3,
            output: 2,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );
    let status = recorder.data();

    let out_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap()
        .join("tests/fixtures");
    fs::create_dir_all(&out_dir).unwrap();
    fs::write(
        out_dir.join("usage-rust.json"),
        serde_json::to_string_pretty(&status).unwrap(),
    )
    .unwrap();
}
```

- [ ] 生成 fixture：

```bash
cd rust-ody && cargo run -p agent-rs --bin generate-usage-fixture
```

预期输出：`tests/fixtures/usage-rust.json` 被创建，JSON 中 `byModel.kimi-k2.inputOther` 为 13、`currentTurn.output` 为 2。

- [ ] 新建 `rust-ody/crates/agent-rs/tests/usage_fixture_parity.rs`：

```rust
use agent_rs::usage::UsageStatus;

#[test]
fn rust_usage_fixture_round_trips() {
    let json = include_str!("fixtures/usage-rust.json");
    let status: UsageStatus = serde_json::from_str(json).unwrap();

    let by_model = status.by_model.as_ref().unwrap();
    let kimi = by_model.get("kimi-k2").unwrap();
    assert_eq!(kimi.input_other, 13);
    assert_eq!(kimi.output, 7);
    assert_eq!(kimi.input_cache_read, 2);
    assert_eq!(kimi.input_cache_creation, 1);
    assert_eq!(status.current_turn.as_ref().unwrap().output, 2);

    let re = serde_json::to_string_pretty(&status).unwrap();
    let status2 = serde_json::from_str(&re).unwrap();
    assert_eq!(status, status2);
}
```

- [ ] 运行 fixture 测试：

```bash
cd rust-ody && cargo test -p agent-rs --test usage_fixture_parity
```

预期输出：`test result: ok. 1 passed; 0 failed`。

- [ ] Commit：`test(agent-rs): add UsageRecorder L1 fixture for TS parity`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.2.2（`UsageRecorder`）。
- [ ] 2. Placeholder扫描：无 TODO/TBD；所有未完成的外部依赖均通过 `UsageRecorderContext` trait 抽象，不引入死代码。
- [ ] 3. No phantom tasks：Task 1 产出 `UsageRecorder` 与行为测试；Task 2 产出 fixture 生成器与 round-trip 测试。
- [ ] 4. Dependency soundness：Task 2 依赖 Task 1；仅依赖 4.3.0 records 层，无反向依赖。
- [ ] 5. Caller & build soundness：`lib.rs` 新增 `pub mod usage`，无其他 crate 调用方；`Cargo.toml` 新增 bin 以 `cargo check -p agent-rs --workspace --tests` 验证。
- [ ] 6. Test-the-risk：`record` 测试断言按 model 累加、currentTurn 仅对 `Turn` 作用域累加、`begin_turn` 重置当前 turn、每次记录写 WAL 并 emit status；fixture 测试断言具体数值与实现常量一致。
- [ ] 7. Type一致性：`UsageStatus` 字段名（`byModel`/`total`/`currentTurn`）与 TS `UsageStatus` 一致；`UsageRecordScope` 复用 4.3.0 records 层定义；`TokenUsage` 复用 `kosong-rs` 定义。
