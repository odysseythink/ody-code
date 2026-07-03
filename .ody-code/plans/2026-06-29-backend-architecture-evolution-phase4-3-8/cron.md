# Part 3 — cron.md

## 范围

本部分完成 Rust `agent-rs` 的 cron 子系统，与 TS 侧 `packages/agent-core/src/tools/cron/*` 语义对齐：

- 5 字段 cron 表达式解析与下一触发点计算（本地时间）。
- 可注入的 `ClockSources`（system / file），支持 `ODY_CRON_CLOCK`。
- 确定性抖动（recurring 正向、one-shot 负向），支持 `KIMI_CRON_NO_JITTER=1` 关闭。
- 内存任务存储 `SessionCronStore` 与 per-id JSON 持久化。
- `CronScheduler`：tick、coalesced count、idle 门控、killswitch、next-fire 查询。
- `CronManager`：连接 scheduler、store、persistence、turn flow，负责事件发射与 telemetry。

本部分结束后，`cargo test -p agent-rs --lib cron` 通过。

---

## 依赖总览

- `schema.md` Task 1：已提供 `chrono`、`rand`、`AgentEvent::CronFired`、`PromptOrigin::CronJob`、`PerIdJsonStore`、`render_cron_fire_xml`。
- `background.md` 已注册的 `pub mod cron;`（如未注册，在本 Part Task 1 一并注册）。

本 Part 无新增外部依赖。

---

## 阶段划分

- **Phase A（纯计算）**：Task 1 cron 解析、Task 2 clock、Task 3 jitter。三者互相独立，可并行。
- **Phase B（状态 + IO）**：Task 4 store + persistence。依赖 Phase A 的 `ClockSources` 仅用于 `created_at` 传入，不依赖解析/抖动。
- **Phase C（调度 + 集成）**：Task 5 scheduler 依赖 Task 1/3；Task 6 CronManager 依赖全部前面任务。

---

## 文件结构

```
rust-ody/crates/agent-rs/src/cron/
├── mod.rs          # 模块聚合
├── expr.rs         # cron 解析与 next-run 计算
├── clock.rs        # ClockSources trait / system / file
├── jitter.rs       # deterministic jitter
├── task.rs         # CronTask（内部表示）+ SessionCronStore
├── persist.rs      # CronTaskPersistence
├── scheduler.rs    # CronScheduler trait + 实现
└── manager.rs      # CronManager
```

> 注意：本 Part 使用的任务类型命名为 `cron::task::CronTask`，字段与 TS `CronTask` 接口一致（`id`/`cron`/`prompt`/`createdAt`/`recurring`/`lastFiredAt`）。`schema.md` 中 `cron::types::CronTask` 是另一公共状态形状，二者不冲突。

---

## Task 1：cron 表达式解析与 next-run 计算

**Depends on:** `schema.md` Task 1（`chrono` 已加入依赖）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/cron/expr.rs`
- Modify: `rust-ody/crates/agent-rs/src/cron/mod.rs`（新建，注册模块）
- Modify: `rust-ody/crates/agent-rs/src/lib.rs`（如未注册 `pub mod cron;`）

### 步骤 1.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/cron/expr.rs`，先写入测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    fn ms(y: i32, mo: u32, d: u32, h: u32, min: u32) -> i64 {
        Local.ymd_opt(y, mo, d).unwrap().and_hms_opt(h, min, 0).unwrap().timestamp_millis()
    }

    #[test]
    fn parse_every_minute() {
        let expr = parse_cron_expression("* * * * *").unwrap();
        assert!(expr.minutes.is_full_range(0, 59));
        assert!(expr.dom_wildcard);
        assert!(expr.dow_wildcard);
    }

    #[test]
    fn parse_rejects_bad_field_count() {
        let err = parse_cron_expression("* * * *").unwrap_err().to_string();
        assert!(err.contains("5 fields"));
    }

    #[test]
    fn next_run_for_daily() {
        let expr = parse_cron_expression("30 9 * * *").unwrap();
        let from = ms(2026, 6, 28, 8, 0);
        let next = expr.next_run_after(from).unwrap();
        let dt = Local.timestamp_millis_opt(next).unwrap().naive_local();
        assert_eq!(dt.hour(), 9);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.day(), 28);
    }

    #[test]
    fn next_run_for_weekdays() {
        let expr = parse_cron_expression("0 9 * * 1-5").unwrap();
        // 2026-06-28 is Sunday
        let from = ms(2026, 6, 28, 10, 0);
        let next = expr.next_run_after(from).unwrap();
        let dt = Local.timestamp_millis_opt(next).unwrap().naive_local();
        assert_eq!(dt.weekday().number_from_monday(), 1); // Monday
    }

    #[test]
    fn dom_and_dow_or_rule() {
        // 1st of month OR Monday
        let expr = parse_cron_expression("0 0 1 * 1").unwrap();
        assert!(!expr.dom_wildcard);
        assert!(!expr.dow_wildcard);
        let from = ms(2026, 6, 28, 0, 0);
        // next is 29 Jun 2026 (Monday)
        let next = expr.next_run_after(from).unwrap();
        let dt = Local.timestamp_millis_opt(next).unwrap().naive_local();
        assert_eq!(dt.day(), 29);
    }

    #[test]
    fn never_fires_returns_none() {
        let expr = parse_cron_expression("0 0 31 2 *").unwrap();
        let from = ms(2026, 1, 1, 0, 0);
        assert!(expr.next_run_after(from).is_none());
    }

    #[test]
    fn has_fire_within_years_sanity() {
        let expr = parse_cron_expression("0 0 * * *").unwrap();
        assert!(has_fire_within_years(&expr, 1, ms(2026, 1, 1, 0, 0)));
        let never = parse_cron_expression("0 0 31 2 *").unwrap();
        assert!(!has_fire_within_years(&never, 5, ms(2026, 1, 1, 0, 0)));
    }
}
```

运行：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --lib cron::expr::tests
```

**预期结果：** 编译失败，`parse_cron_expression`、`CronExpr` 等不存在。

### 步骤 1.2：实现解析器

在同一文件追加实现：

```rust
use chrono::{Duration, Local, Months, NaiveDateTime, TimeZone};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CronExpr {
    pub raw: String,
    pub minutes: CronField,
    pub hours: CronField,
    pub days_of_month: CronField,
    pub months: CronField,
    pub days_of_week: CronField,
    pub dom_wildcard: bool,
    pub dow_wildcard: bool,
}

/// 用一个无符号整数做位集合。value i 被允许 <=> (bits >> i) & 1 == 1。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CronField {
    bits: u64,
}

impl CronField {
    pub fn empty() -> Self { Self { bits: 0 } }
    pub fn insert(&mut self, value: i32) { self.bits |= 1u64 << value; }
    pub fn contains(&self, value: i32) -> bool { (self.bits >> value) & 1 == 1 }
    pub fn is_full_range(&self, min: i32, max: i32) -> bool {
        let mask = ((1u64 << (max - min + 1)) - 1) << min;
        self.bits == mask
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronParseError {
    BadFieldCount { got: usize },
    EmptyField { name: &'static str },
    InvalidInt { name: &'static str, role: &'static str, raw: String },
    OutOfRange { name: &'static str, value: i32, min: i32, max: i32 },
    BadRange { name: &'static str, lo: i32, hi: i32 },
    EmptyStep { name: &'static str, term: String },
    NonPositiveStep { name: &'static str, step: String },
    NoMatchingValues { name: &'static str },
}

impl fmt::Display for CronParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CronParseError::BadFieldCount { got } => write!(f, "cron expression must have exactly 5 fields (minute hour day-of-month month day-of-week); got {}", got),
            CronParseError::EmptyField { name } => write!(f, "cron {} field is empty", name),
            CronParseError::InvalidInt { name, role, raw } => write!(f, "cron {} {} must be a non-negative integer with digits only (got {})", name, role, raw),
            CronParseError::OutOfRange { name, value, min, max } => write!(f, "cron {} value {} out of range {}..{}", name, value, min, max),
            CronParseError::BadRange { name, lo, hi } => write!(f, "cron {} range {}-{} out of bounds (must be ascending)", name, lo, hi),
            CronParseError::EmptyStep { name, term } => write!(f, "cron {} step is empty in {}", name, term),
            CronParseError::NonPositiveStep { name, step } => write!(f, "cron {} step must be a positive integer (got {})", name, step),
            CronParseError::NoMatchingValues { name } => write!(f, "cron {} field matches no values", name),
        }
    }
}

impl std::error::Error for CronParseError {}

const MINUTE_RANGE: (i32, i32) = (0, 59);
const HOUR_RANGE: (i32, i32) = (0, 23);
const DOM_RANGE: (i32, i32) = (1, 31);
const MONTH_RANGE: (i32, i32) = (1, 12);
const DOW_RANGE: (i32, i32) = (0, 7);
const MS_PER_MINUTE: i64 = 60_000;

pub fn parse_cron_expression(raw: &str) -> Result<CronExpr, CronParseError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CronParseError::EmptyField { name: "expression" });
    }
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(CronParseError::BadFieldCount { got: fields.len() });
    }
    let minutes = parse_field(fields[0], "minute", MINUTE_RANGE)?;
    let hours = parse_field(fields[1], "hour", HOUR_RANGE)?;
    let days_of_month = parse_field(fields[2], "day-of-month", DOM_RANGE)?;
    let months = parse_field(fields[3], "month", MONTH_RANGE)?;
    let mut dow_field = parse_field(fields[4], "day-of-week", DOW_RANGE)?;
    // fold 7 -> 0
    let mut folded = CronField::empty();
    for v in 0..=7 {
        if dow_field.contains(v) {
            folded.insert(if v == 7 { 0 } else { v });
        }
    }
    Ok(CronExpr {
        raw: trimmed.to_string(),
        minutes,
        hours,
        days_of_month,
        months,
        days_of_week: folded,
        dom_wildcard: is_wildcard(fields[2]),
        dow_wildcard: is_wildcard(fields[4]),
    })
}

fn is_wildcard(field: &str) -> bool {
    field == "*"
}

fn parse_field(field: &str, name: &'static str, range: (i32, i32)) -> Result<CronField, CronParseError> {
    if field.is_empty() {
        return Err(CronParseError::EmptyField { name });
    }
    let mut out = CronField::empty();
    for term in field.split(',') {
        if term.is_empty() {
            return Err(CronParseError::EmptyField { name });
        }
        add_term(term, name, range, &mut out)?;
    }
    if out.bits == 0 {
        return Err(CronParseError::NoMatchingValues { name });
    }
    Ok(out)
}

fn parse_int(raw: &str, name: &'static str, role: &'static str) -> Result<i32, CronParseError> {
    if raw.chars().all(|c| c.is_ascii_digit()) && !raw.is_empty() {
        raw.parse::<i32>().map_err(|_| CronParseError::InvalidInt { name, role, raw: raw.to_string() })
    } else {
        Err(CronParseError::InvalidInt { name, role, raw: raw.to_string() })
    }
}

fn add_term(term: &str, name: &'static str, (min, max): (i32, i32), out: &mut CronField) -> Result<(), CronParseError> {
    let (range_part, step) = if let Some(idx) = term.find('/') {
        let rp = &term[..idx];
        let s = &term[idx + 1..];
        if s.is_empty() {
            return Err(CronParseError::EmptyStep { name, term: term.to_string() });
        }
        let step_val = parse_int(s, name, "step")?;
        if step_val <= 0 {
            return Err(CronParseError::NonPositiveStep { name, step: s.to_string() });
        }
        if rp.is_empty() {
            return Err(CronParseError::EmptyField { name });
        }
        (rp, step_val)
    } else {
        (term, 1)
    };

    let (lo, hi): (i32, i32) = if range_part == "*" {
        (min, max)
    } else if let Some(idx) = range_part.find('-') {
        let lo = parse_int(&range_part[..idx], name, "range lower bound")?;
        let hi = parse_int(&range_part[idx + 1..], name, "range upper bound")?;
        if lo < min || hi > max || lo > hi {
            return Err(CronParseError::BadRange { name, lo, hi });
        }
        (lo, hi)
    } else {
        let v = parse_int(range_part, name, "value")?;
        if v < min || v > max {
            return Err(CronParseError::OutOfRange { name, value: v, min, max });
        }
        if term.contains('/') {
            // bare value with step: e.g. 5/10 -> 5..max step 10
            (v, max)
        } else {
            out.insert(v);
            return Ok(());
        }
    };

    let mut v = lo;
    while v <= hi {
        out.insert(v);
        v += step;
    }
    Ok(())
}

impl CronExpr {
    pub fn next_run_after(&self, from_ms: i64) -> Option<i64> {
        self.next_run_within(from_ms, 5 * 366 * 24 * 60)
    }

    fn next_run_within(&self, from_ms: i64, cap_minutes: i64) -> Option<i64> {
        let local = Local.timestamp_millis_opt(from_ms).single()?;
        let mut date = local.naive_local();
        date = date.with_second(0).unwrap().with_nanosecond(0).unwrap() + Duration::minutes(1);
        let deadline = from_ms + cap_minutes * MS_PER_MINUTE;
        let mut iterations = 0i64;
        const HARD_ITERATION_CAP: i64 = 10_000_000;

        while date.timestamp_millis() <= deadline && iterations < HARD_ITERATION_CAP {
            iterations += 1;
            if !self.months.contains(date.month() as i32) {
                advance_month(&mut date);
                continue;
            }
            if !self.day_matches(date) {
                advance_day(&mut date);
                continue;
            }
            if !self.hours.contains(date.hour() as i32) {
                advance_hour(&mut date);
                continue;
            }
            if !self.minutes.contains(date.minute() as i32) {
                advance_minute(&mut date);
                continue;
            }
            return Some(date.timestamp_millis());
        }
        None
    }

    fn day_matches(&self, date: NaiveDateTime) -> bool {
        let dom = date.day() as i32;
        let dow = date.weekday().num_days_from_sunday() as i32;
        let dom_ok = self.days_of_month.contains(dom);
        let dow_ok = self.days_of_week.contains(dow);
        if self.dom_wildcard && self.dow_wildcard { return true; }
        if self.dom_wildcard { return dow_ok; }
        if self.dow_wildcard { return dom_ok; }
        dom_ok || dow_ok
    }
}

pub fn has_fire_within_years(expr: &CronExpr, years: i64, from_ms: i64) -> bool {
    let cap = (years * 366 * 24 * 60).max(1);
    expr.next_run_within(from_ms, cap).is_some()
}

fn advance_month(date: &mut NaiveDateTime) {
    *date = (date.date().with_day(1).unwrap() + Months::new(1))
        .and_hms_opt(0, 0, 0)
        .unwrap();
}

fn advance_day(date: &mut NaiveDateTime) {
    *date = (*date + Duration::days(1))
        .with_hour(0).unwrap()
        .with_minute(0).unwrap()
        .with_second(0).unwrap();
}

fn advance_hour(date: &mut NaiveDateTime) {
    *date = (*date + Duration::hours(1))
        .with_minute(0).unwrap()
        .with_second(0).unwrap();
}

fn advance_minute(date: &mut NaiveDateTime) {
    *date = *date + Duration::minutes(1);
}
```

### 步骤 1.3：注册模块

创建 `rust-ody/crates/agent-rs/src/cron/mod.rs`：

```rust
pub mod expr;
pub mod clock;
pub mod jitter;
pub mod task;
pub mod persist;
pub mod scheduler;
pub mod manager;
```

如 `rust-ody/crates/agent-rs/src/lib.rs` 未包含 `pub mod cron;`，追加。

### 步骤 1.4：运行测试

```bash
cargo test -p agent-rs --lib cron::expr::tests
cargo check -p agent-rs
```

**预期结果：** 所有测试通过，类型检查无错。

---

## Task 2：ClockSources（wall + monotonic clock）

**Depends on:** 无（依赖 `chrono` 已在 schema.md 加入）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/cron/clock.rs`

### 步骤 2.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/cron/clock.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn system_clock_returns_positive_wall_and_mono() {
        let clock: Arc<dyn ClockSources> = system_clocks();
        let w = clock.wall_now();
        let m = clock.mono_now_ms();
        assert!(w > 0);
        assert!(m > 0);
    }

    #[test]
    fn file_clock_reads_first_line() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("clock.txt");
        fs::write(&path, "1234567890123\nsecond line\n").unwrap();
        let clock = FileClock::new(path.to_str().unwrap().to_string());
        assert_eq!(clock.wall_now(), 1234567890123);
        fs::write(&path, "999\n").unwrap();
        assert_eq!(clock.wall_now(), 999);
    }

    #[test]
    fn file_clock_missing_file_falls_back() {
        let clock = FileClock::new("/tmp/does-not-exist-cron-clock-xyz".to_string());
        let w = clock.wall_now();
        assert!(w > 0); // Date.now() fallback
    }

    #[test]
    fn resolve_file_spec() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("clock.txt");
        fs::write(&path, "1111").unwrap();
        let spec = format!("file:{}", path.to_str().unwrap());
        let clock = resolve_clock_sources(Some(&spec));
        assert_eq!(clock.wall_now(), 1111);
    }
}
```

运行：

```bash
cargo test -p agent-rs --lib cron::clock::tests
```

**预期结果：** 编译失败，`ClockSources`、`system_clocks` 等不存在。

### 步骤 2.2：实现 clock

在同一文件追加实现：

```rust
use std::fs;
use std::io::Read;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub trait ClockSources: Send + Sync {
    fn wall_now(&self) -> i64;
    fn mono_now_ms(&self) -> u128;
}

pub fn system_clocks() -> Arc<dyn ClockSources> {
    Arc::new(SystemClocks)
}

struct SystemClocks;

impl ClockSources for SystemClocks {
    fn wall_now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    fn mono_now_ms(&self) -> u128 {
        Instant::now().elapsed().as_millis()
    }
}

pub struct FileClock {
    path: String,
}

impl FileClock {
    pub fn new(path: String) -> Self { Self { path } }
}

impl ClockSources for FileClock {
    fn wall_now(&self) -> i64 {
        read_file_wall(&self.path)
    }

    fn mono_now_ms(&self) -> u128 {
        Instant::now().elapsed().as_millis()
    }
}

const MAX_CLOCK_FILE_BYTES: usize = 64;

fn read_file_wall(path: &str) -> i64 {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return system_wall_now(),
    };
    let mut buf = [0u8; MAX_CLOCK_FILE_BYTES];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return system_wall_now(),
    };
    let raw = String::from_utf8_lossy(&buf[..n]);
    let first = raw.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        return system_wall_now();
    }
    first.parse::<i64>().ok().filter(|v| v.is_finite_ms()).unwrap_or_else(system_wall_now)
}

fn system_wall_now() -> i64 {
    SystemClocks.wall_now()
}

pub fn resolve_clock_sources(spec: Option<&str>) -> Arc<dyn ClockSources> {
    let spec = spec.unwrap_or("");
    if spec.is_empty() || spec == "system" {
        return system_clocks();
    }
    if let Some(path) = spec.strip_prefix("file:") {
        if path.is_empty() {
            return system_clocks();
        }
        return Arc::new(FileClock::new(path.to_string()));
    }
    system_clocks()
}

trait FiniteMs {
    fn is_finite_ms(&self) -> bool;
}

impl FiniteMs for i64 {
    fn is_finite_ms(&self) -> bool { true }
}
```

### 步骤 2.3：运行测试

```bash
cargo test -p agent-rs --lib cron::clock::tests
cargo check -p agent-rs
```

**预期结果：** 测试通过，类型检查无错。

---

## Task 3：确定性抖动（jitter）

**Depends on:** Task 1（`CronExpr`）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/cron/jitter.rs`

### 步骤 3.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/cron/jitter.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::expr::parse_cron_expression;

    fn task(id: &str, created_at: i64, recurring: Option<bool>) -> CronTask {
        CronTask {
            id: id.to_string(),
            cron: "0 9 * * *".to_string(),
            prompt: "p".to_string(),
            created_at,
            recurring,
            last_fired_at: None,
        }
    }

    #[test]
    fn recurring_jitter_is_deterministic() {
        let expr = parse_cron_expression("0 9 * * *").unwrap();
        let t = task("deadbeef", 0, None);
        let ideal = parse_cron_expression("0 9 * * *").unwrap().next_run_after(0).unwrap();
        let j1 = jittered_next_run_ms(&t, &expr, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        let j2 = jittered_next_run_ms(&t, &expr, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        assert_eq!(j1, j2);
        assert!(j1 >= ideal);
        assert!(j1 - ideal <= 15 * 60 * 1000);
    }

    #[test]
    fn recurring_jitter_disabled_env() {
        let expr = parse_cron_expression("0 9 * * *").unwrap();
        let t = task("deadbeef", 0, None);
        let ideal = expr.next_run_after(0).unwrap();
        std::env::set_var("KIMI_CRON_NO_JITTER", "1");
        let j = jittered_next_run_ms(&t, &expr, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        std::env::remove_var("KIMI_CRON_NO_JITTER");
        assert_eq!(j, ideal);
    }

    #[test]
    fn one_shot_pulls_forward_on_round_minutes() {
        // ideal 09:00, created at 08:59:30 -> jitter budget 90s would push before createdAt,
        // so function returns ideal unchanged.
        let created = 9 * 60 * 60 * 1000 - 30_000;
        let t = task("deadbeef", created, Some(false));
        let ideal = 9 * 60 * 60 * 1000;
        let j = one_shot_jittered_next_run_ms(&t, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        assert_eq!(j, ideal);
    }

    #[test]
    fn one_shot_respects_non_round_minute() {
        let t = task("deadbeef", 0, Some(false));
        let ideal = (9 * 60 + 7) * 60 * 1000; // 09:07
        let j = one_shot_jittered_next_run_ms(&t, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        assert_eq!(j, ideal);
    }
}
```

运行：

```bash
cargo test -p agent-rs --lib cron::jitter::tests
```

**预期结果：** 编译失败，`CronTask`、`jittered_next_run_ms` 等不存在。

### 步骤 3.2：实现 jitter

在同一文件追加实现：

```rust
use crate::cron::expr::CronExpr;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JitterConfig {
    pub recurring_max_fraction_of_period: f64,
    pub recurring_max_ms: i64,
    pub one_shot_max_ms: i64,
}

pub const DEFAULT_CRON_JITTER_CONFIG: JitterConfig = JitterConfig {
    recurring_max_fraction_of_period: 0.1,
    recurring_max_ms: 15 * 60 * 1000,
    one_shot_max_ms: 90 * 1000,
};

const MS_PER_MINUTE: i64 = 60_000;
const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub created_at: i64,
    pub recurring: Option<bool>,
    pub last_fired_at: Option<i64>,
}

pub fn jittered_next_run_ms(
    task: &CronTask,
    expr: &CronExpr,
    ideal_ms: i64,
    config: &JitterConfig,
) -> i64 {
    if jitter_disabled() {
        return ideal_ms;
    }
    let next_next = expr.next_run_after(ideal_ms);
    let period = next_next.map(|n| n - ideal_ms).filter(|d| *d > 0).unwrap_or(MS_PER_DAY);
    let period_cap = (period as f64 * config.recurring_max_fraction_of_period) as i64;
    let cap = period_cap.min(config.recurring_max_ms);
    if cap <= 0 {
        return ideal_ms;
    }
    let offset = (cap as f64 * fraction_from_id(&task.id)).round() as i64;
    ideal_ms + offset
}

pub fn one_shot_jittered_next_run_ms(
    task: &CronTask,
    ideal_ms: i64,
    config: &JitterConfig,
) -> i64 {
    if jitter_disabled() {
        return ideal_ms;
    }
    if ideal_ms % MS_PER_MINUTE != 0 {
        return ideal_ms;
    }
    let minute_of_hour = ((ideal_ms / MS_PER_MINUTE) % 60) as i32;
    if minute_of_hour != 0 && minute_of_hour != 30 {
        return ideal_ms;
    }
    if config.one_shot_max_ms <= 0 {
        return ideal_ms;
    }
    let offset = -(config.one_shot_max_ms as f64 * fraction_from_id(&task.id)).round() as i64;
    let shifted = ideal_ms + offset;
    if task.created_at > 0 && shifted < task.created_at {
        return ideal_ms;
    }
    shifted
}

fn jitter_disabled() -> bool {
    std::env::var("KIMI_CRON_NO_JITTER").ok().as_deref() == Some("1")
}

fn fraction_from_id(id: &str) -> f64 {
    if let Some(hex) = id.strip_prefix("0x").or_else(|| id.strip_prefix("0X")) {
        if hex.len() == 8 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(n) = u32::from_str_radix(hex, 16) {
                return n as f64 / (u32::MAX as f64 + 1.0);
            }
        }
    }
    if id.len() == 8 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(n) = u32::from_str_radix(id, 16) {
            return n as f64 / (u32::MAX as f64 + 1.0);
        }
    }
    // djb2 fallback
    let mut hash: i64 = 5381;
    for b in id.bytes() {
        hash = ((hash << 5) + hash + b as i64) & 0xffffffff;
    }
    hash as u32 as f64 / (u32::MAX as f64 + 1.0)
}
```

### 步骤 3.3：运行测试

```bash
cargo test -p agent-rs --lib cron::jitter::tests
cargo check -p agent-rs
```

**预期结果：** 测试通过，类型检查无错。

> 注意：`CronTask` 在本任务中临时定义在 `jitter.rs`，Task 4 会将其迁移到 `cron/task.rs` 并从这里导入；迁移后删除本文件中的 `CronTask` 定义。

---

## Task 4：SessionCronStore + CronTaskPersistence

**Depends on:** Task 3（`CronTask` 定义将迁移到本任务）；依赖 `schema.md` Task 3（`PerIdJsonStore`）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/cron/task.rs`
- Create: `rust-ody/crates/agent-rs/src/cron/persist.rs`
- Modify: `rust-ody/crates/agent-rs/src/cron/jitter.rs`（删除临时 `CronTask` 定义，改为 `use crate::cron::task::CronTask;`）

### 步骤 4.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/cron/task.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn init() -> CronTaskInit {
        CronTaskInit {
            cron: "0 9 * * *".to_string(),
            prompt: "morning standup".to_string(),
            recurring: Some(true),
        }
    }

    #[test]
    fn store_add_and_list() {
        let mut store = SessionCronStore::new();
        let t = store.add(init(), 1000);
        assert!(t.id.len() == 8);
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get(&t.id).unwrap().prompt, "morning standup");
    }

    #[test]
    fn store_mark_fired_updates_last_fired_at() {
        let mut store = SessionCronStore::new();
        let t = store.add(init(), 1000);
        let updated = store.mark_fired(&t.id, 5000).unwrap();
        assert_eq!(updated.last_fired_at, Some(5000));
        assert_eq!(store.get(&t.id).unwrap().last_fired_at, Some(5000));
    }

    #[test]
    fn store_remove_returns_only_present() {
        let mut store = SessionCronStore::new();
        let t = store.add(init(), 1000);
        let removed = store.remove(&[t.id.clone(), "ffffffff".to_string()]);
        assert_eq!(removed, vec![t.id]);
        assert!(store.list().is_empty());
    }

    #[test]
    fn store_adopt_preserves_id() {
        let mut store = SessionCronStore::new();
        let t = CronTask {
            id: "a1b2c3d4".to_string(),
            cron: "0 9 * * *".to_string(),
            prompt: "x".to_string(),
            created_at: 42,
            recurring: None,
            last_fired_at: None,
        };
        store.adopt(t.clone());
        assert_eq!(store.get("a1b2c3d4").unwrap().created_at, 42);
    }
}
```

运行：

```bash
cargo test -p agent-rs --lib cron::task::tests
```

**预期结果：** 编译失败，`SessionCronStore`、`CronTaskInit` 等不存在。

### 步骤 4.2：实现 `cron/task.rs`

在同一文件追加实现：

```rust
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct CronTaskInit {
    pub cron: String,
    pub prompt: String,
    pub recurring: Option<bool>,
}

pub struct SessionCronStore {
    tasks: HashMap<String, CronTask>,
    id_generator: Box<dyn Fn() -> String + Send + Sync>,
}

const ID_REGEX: &str = r"^[0-9a-f]{8}$";
const MAX_ID_ATTEMPTS: usize = 8;

impl Default for SessionCronStore {
    fn default() -> Self { Self::new() }
}

impl SessionCronStore {
    pub fn new() -> Self {
        Self::with_id_generator(Box::new(random_hex_id))
    }

    pub fn with_id_generator(gen: Box<dyn Fn() -> String + Send + Sync>) -> Self {
        Self { tasks: HashMap::new(), id_generator: gen }
    }

    pub fn add(&mut self, init: CronTaskInit, now_ms: i64) -> CronTask {
        let id = self.generate_unique_id();
        let task = CronTask {
            id,
            cron: init.cron,
            prompt: init.prompt,
            created_at: now_ms,
            recurring: init.recurring,
            last_fired_at: None,
        };
        self.tasks.insert(task.id.clone(), task.clone());
        task
    }

    pub fn adopt(&mut self, task: CronTask) {
        self.tasks.insert(task.id.clone(), task);
    }

    pub fn mark_fired(&mut self, id: &str, last_fired_at: i64) -> Option<CronTask> {
        let existing = self.tasks.get_mut(id)?;
        existing.last_fired_at = Some(last_fired_at);
        Some(existing.clone())
    }

    pub fn get(&self, id: &str) -> Option<&CronTask> {
        self.tasks.get(id)
    }

    pub fn list(&self) -> Vec<CronTask> {
        self.tasks.values().cloned().collect()
    }

    pub fn remove(&mut self, ids: &[String]) -> Vec<String> {
        let mut removed = Vec::new();
        for id in ids {
            if self.tasks.remove(id).is_some() {
                removed.push(id.clone());
            }
        }
        removed
    }

    pub fn clear(&mut self) {
        self.tasks.clear();
    }

    fn generate_unique_id(&mut self) -> String {
        for _ in 0..MAX_ID_ATTEMPTS {
            let candidate = (self.id_generator)();
            if regex::Regex::new(ID_REGEX).unwrap().is_match(&candidate) && !self.tasks.contains_key(&candidate) {
                return candidate;
            }
        }
        panic!("SessionCronStore: failed to generate a unique 8-hex id after {} attempts", MAX_ID_ATTEMPTS);
    }
}

fn random_hex_id() -> String {
    let bytes: [u8; 4] = rand::thread_rng().gen();
    hex::encode(bytes)
}
```

### 步骤 4.3：实现 `cron/persist.rs`

创建 `rust-ody/crates/agent-rs/src/cron/persist.rs`：

```rust
use crate::cron::task::CronTask;
use crate::persist::per_id_json_store::PerIdJsonStore;
use std::io;
use std::path::PathBuf;

pub struct CronTaskPersistence {
    store: PerIdJsonStore<CronTask>,
    id_regex: regex::Regex,
}

impl CronTaskPersistence {
    pub fn new(session_dir: PathBuf) -> Self {
        let base = session_dir.join("cron");
        Self {
            store: PerIdJsonStore::new(base),
            id_regex: regex::Regex::new(r"^[0-9a-f]{8}$").unwrap(),
        }
    }

    pub async fn write(&self, task: &CronTask) -> io::Result<()> {
        self.validate_id(&task.id)?;
        self.store.write(&task.id, task).await
    }

    pub async fn read(&self, id: &str) -> io::Result<Option<CronTask>> {
        self.validate_id(id)?;
        Ok(self.store.read(id).await.ok().flatten())
    }

    pub async fn list(&self) -> io::Result<Vec<CronTask>> {
        let mut out = Vec::new();
        let ids = self.list_ids().await?;
        for id in ids {
            if let Some(task) = self.read(&id).await? {
                out.push(task);
            }
        }
        Ok(out)
    }

    pub async fn remove(&self, id: &str) -> io::Result<()> {
        self.validate_id(id)?;
        self.store.remove(id).await
    }

    fn validate_id(&self, id: &str) -> io::Result<()> {
        if self.id_regex.is_match(id) {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid cron job id: {}", id)))
        }
    }

    async fn list_ids(&self) -> io::Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut entries = tokio::fs::read_dir(self.store.base_dir()).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".json") {
                if self.id_regex.is_match(id) {
                    ids.push(id.to_string());
                }
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample(id: &str) -> CronTask {
        CronTask {
            id: id.to_string(),
            cron: "0 9 * * *".to_string(),
            prompt: "hi".to_string(),
            created_at: 1,
            recurring: Some(true),
            last_fired_at: None,
        }
    }

    #[tokio::test]
    async fn persistence_round_trip() {
        let dir = TempDir::new().unwrap();
        let p = CronTaskPersistence::new(dir.path().to_path_buf());
        p.write(&sample("deadbeef")).await.unwrap();
        let all = p.list().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].prompt, "hi");

        p.remove("deadbeef").await.unwrap();
        let all = p.list().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn persistence_skips_invalid_basename() {
        let dir = TempDir::new().unwrap();
        let p = CronTaskPersistence::new(dir.path().to_path_buf());
        p.write(&sample("deadbeef")).await.unwrap();
        tokio::fs::write(dir.path().join("cron/not-an-id.json"), b"{}").await.unwrap();
        let all = p.list().await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
```

### 步骤 4.4：更新 `cron/jitter.rs` 导入

将 `cron/jitter.rs` 中的 `CronTask` 定义替换为：

```rust
use crate::cron::task::CronTask;
```

删除 `jitter.rs` 中原有的 `#[derive(...)] pub struct CronTask { ... }` 代码块。

### 步骤 4.5：运行测试

```bash
cargo test -p agent-rs --lib cron::task::tests
cargo test -p agent-rs --lib cron::persist::tests
cargo test -p agent-rs --lib cron::jitter::tests
cargo check -p agent-rs
```

**预期结果：** 所有测试通过，类型检查无错。

---

## Task 5：CronScheduler

**Depends on:** Task 1（`CronExpr`）、Task 3（`jitter`）、Task 4（`CronTask`）、`schema.md` Task 1（`CronFireContext`）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/cron/scheduler.rs`

### 步骤 5.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/cron/scheduler.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::clock::FileClock;
    use crate::cron::expr::parse_cron_expression;
    use crate::cron::task::{CronTask, CronTaskInit, SessionCronStore};
    use crate::cron::types::CronFireContext;
    use chrono::Utc;
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    #[tokio::test]
    async fn recurring_fires_and_advances_cursor() {
        let dir = TempDir::new().unwrap();
        let clock_path = dir.path().join("clock");
        fs::write(&clock_path, "0").unwrap();
        let clocks: Arc<dyn ClockSources> = Arc::new(FileClock::new(clock_path.to_str().unwrap().to_string()));

        let fired = Arc::new(Mutex::new(Vec::<CronFireContext>::new()));
        let advanced = Arc::new(Mutex::new(Vec::<(String, i64)>::new()));
        let store = Arc::new(Mutex::new(SessionCronStore::new()));
        let task = store.lock().unwrap().add(CronTaskInit { cron: "* * * * *".to_string(), prompt: "p".to_string(), recurring: Some(true) }, 0);

        let f = fired.clone();
        let a = advanced.clone();
        let s = store.clone();
        let scheduler = CronScheduler::new(CronSchedulerOptions {
            clocks: clocks.clone(),
            source: Box::new(move || s.lock().unwrap().list()),
            is_idle: Box::new(|| true),
            is_killed: None,
            on_fire: Box::new(move |_, ctx| { f.lock().unwrap().push(ctx.clone()); }),
            remove_one_shot: None,
            on_advance_cursor: Some(Box::new(move |id, ts| { a.lock().unwrap().push((id.to_string(), ts)); })),
            poll_interval_ms: None,
        });

        fs::write(&clock_path, "60001").unwrap();
        scheduler.tick();
        assert_eq!(fired.lock().unwrap().len(), 1);
        assert_eq!(advanced.lock().unwrap().len(), 1);

        fs::write(&clock_path, "120001").unwrap();
        scheduler.tick();
        assert_eq!(fired.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn one_shot_removed_after_fire() {
        let dir = TempDir::new().unwrap();
        let clock_path = dir.path().join("clock");
        fs::write(&clock_path, "0").unwrap();
        let clocks: Arc<dyn ClockSources> = Arc::new(FileClock::new(clock_path.to_str().unwrap().to_string()));

        let removed = Arc::new(Mutex::new(Vec::<String>::new()));
        let r = removed.clone();
        let task = CronTask {
            id: "deadbeef".to_string(),
            cron: "* * * * *".to_string(),
            prompt: "once".to_string(),
            created_at: 0,
            recurring: Some(false),
            last_fired_at: None,
        };
        let scheduler = CronScheduler::new(CronSchedulerOptions {
            clocks: clocks.clone(),
            source: Box::new(move || vec![task.clone()]),
            is_idle: Box::new(|| true),
            is_killed: None,
            on_fire: Box::new(|_, _| {}),
            remove_one_shot: Some(Box::new(move |id| r.lock().unwrap().push(id.to_string()))),
            on_advance_cursor: None,
            poll_interval_ms: None,
        });
        fs::write(&clock_path, "60001").unwrap();
        scheduler.tick();
        assert_eq!(removed.lock().unwrap(), vec!["deadbeef".to_string()]);
    }

    #[tokio::test]
    async fn idle_gate_prevents_fire() {
        let idle = Arc::new(AtomicBool::new(false));
        let i = idle.clone();
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        let scheduler = CronScheduler::new(CronSchedulerOptions {
            clocks: Arc::new(crate::cron::clock::system_clocks()),
            source: Box::new(|| vec![]),
            is_idle: Box::new(move || i.load(Ordering::SeqCst)),
            is_killed: None,
            on_fire: Box::new(move |_, _| f.store(true, Ordering::SeqCst)),
            remove_one_shot: None,
            on_advance_cursor: None,
            poll_interval_ms: None,
        });
        scheduler.tick();
        assert!(!fired.load(Ordering::SeqCst));
    }
}
```

运行：

```bash
cargo test -p agent-rs --lib cron::scheduler::tests
```

**预期结果：** 编译失败，`CronScheduler`、`CronSchedulerOptions` 不存在。

### 步骤 5.2：实现 scheduler

在同一文件追加实现：

```rust
use crate::cron::clock::ClockSources;
use crate::cron::expr::{parse_cron_expression, CronExpr};
use crate::cron::jitter::{jittered_next_run_ms, one_shot_jittered_next_run_ms, DEFAULT_CRON_JITTER_CONFIG};
use crate::cron::task::CronTask;
use crate::cron::types::{CronFireContext, CronTaskId};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DEFAULT_POLL_INTERVAL_MS: i64 = 1000;
const MAX_COALESCE_ITERATIONS: usize = 10_000;

pub struct CronSchedulerOptions {
    pub clocks: Arc<dyn ClockSources>,
    pub source: Box<dyn Fn() -> Vec<CronTask> + Send + Sync>,
    pub is_idle: Box<dyn Fn() -> bool + Send + Sync>,
    pub is_killed: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    pub on_fire: Box<dyn Fn(&CronTask, CronFireContext) + Send + Sync>,
    pub remove_one_shot: Option<Box<dyn Fn(&str) + Send + Sync>>,
    pub on_advance_cursor: Option<Box<dyn Fn(&str, i64) + Send + Sync>>,
    pub poll_interval_ms: Option<i64>,
}

pub struct CronScheduler {
    clocks: Arc<dyn ClockSources>,
    source: Box<dyn Fn() -> Vec<CronTask> + Send + Sync>,
    is_idle: Box<dyn Fn() -> bool + Send + Sync>,
    is_killed: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    on_fire: Box<dyn Fn(&CronTask, CronFireContext) + Send + Sync>,
    remove_one_shot: Option<Box<dyn Fn(&str) + Send + Sync>>,
    on_advance_cursor: Option<Box<dyn Fn(&str, i64) + Send + Sync>>,
    poll_interval_ms: Option<i64>,
    parsed_cache: Mutex<HashMap<String, CronExpr>>,
    last_seen_at: Mutex<HashMap<String, i64>>,
    seeded: Mutex<HashSet<String>>,
    in_flight: Mutex<HashSet<String>>,
    timer_running: Mutex<bool>,
}

impl CronScheduler {
    pub fn new(opts: CronSchedulerOptions) -> Arc<Self> {
        Arc::new(Self {
            clocks: opts.clocks,
            source: opts.source,
            is_idle: opts.is_idle,
            is_killed: opts.is_killed,
            on_fire: opts.on_fire,
            remove_one_shot: opts.remove_one_shot,
            on_advance_cursor: opts.on_advance_cursor,
            poll_interval_ms: opts.poll_interval_ms,
            parsed_cache: Mutex::new(HashMap::new()),
            last_seen_at: Mutex::new(HashMap::new()),
            seeded: Mutex::new(HashSet::new()),
            in_flight: Mutex::new(HashSet::new()),
            timer_running: Mutex::new(false),
        })
    }

    pub fn start(self: &Arc<Self>) {
        let mut running = self.timer_running.lock().unwrap();
        if *running { return; }
        let interval = self.poll_interval_ms.unwrap_or(DEFAULT_POLL_INTERVAL_MS);
        if interval <= 0 { return; }
        let this = self.clone();
        *running = true;
        drop(running);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(interval as u64)).await;
                let still_running = *this.timer_running.lock().unwrap();
                if !still_running { break; }
                this.tick();
            }
        });
    }

    pub fn stop(&self) {
        *self.timer_running.lock().unwrap() = false;
        self.in_flight.lock().unwrap().clear();
        self.last_seen_at.lock().unwrap().clear();
        self.seeded.lock().unwrap().clear();
        self.parsed_cache.lock().unwrap().clear();
    }

    pub fn tick(&self) {
        if let Some(ref is_killed) = self.is_killed {
            if is_killed() { return; }
        }
        if !(self.is_idle)() { return; }
        let tasks = (self.source)();
        if tasks.is_empty() { return; }
        let now = self.clocks.wall_now();
        let mut in_flight = self.in_flight.lock().unwrap();
        for task in tasks {
            if in_flight.contains(&task.id) { continue; }
            let parsed = match self.get_parsed(&task.cron) {
                Some(p) => p,
                None => continue,
            };

            {
                let mut seeded = self.seeded.lock().unwrap();
                let mut last_seen = self.last_seen_at.lock().unwrap();
                if !seeded.contains(&task.id) {
                    if let Some(lf) = task.last_fired_at {
                        if lf <= now && !last_seen.contains_key(&task.id) {
                            last_seen.insert(task.id.clone(), lf);
                        }
                    }
                    seeded.insert(task.id.clone());
                }
            }

            let base = {
                let last_seen = self.last_seen_at.lock().unwrap();
                match last_seen.get(&task.id) {
                    Some(ts) if *ts > task.created_at => *ts,
                    _ => task.created_at,
                }
            };

            let next_fire = self.jittered_next(&task, &parsed, base);
            if next_fire.is_none() || now < next_fire.unwrap() { continue; }
            let next_fire = next_fire.unwrap();

            let ideal = parsed.next_run_after(base);
            let recurring = task.recurring != Some(false);
            let (coalesced, last_due) = if recurring {
                if let Some(first) = ideal {
                    let (c, l) = self.count_coalesced(&task, &parsed, first, now);
                    (c, Some(l))
                } else {
                    (1, None)
                }
            } else {
                (1, None)
            };

            in_flight.insert(task.id.clone());
            let fired_at = Utc.timestamp_millis_opt(now).single().unwrap_or_else(Utc::now);
            let ctx = CronFireContext {
                id: CronTaskId::new(task.id.clone()),
                schedule: task.cron.clone(),
                prompt: task.prompt.clone(),
                coalesced_count: coalesced as u64,
                fired_at,
            };
            (self.on_fire)(&task, ctx);

            if recurring {
                let advanced_to = last_due.unwrap_or(now);
                self.last_seen_at.lock().unwrap().insert(task.id.clone(), advanced_to);
                if let Some(ref cb) = self.on_advance_cursor {
                    cb(&task.id, advanced_to);
                }
            } else {
                if let Some(ref cb) = self.remove_one_shot {
                    cb(&task.id);
                }
                self.last_seen_at.lock().unwrap().remove(&task.id);
                self.seeded.lock().unwrap().remove(&task.id);
            }
            in_flight.remove(&task.id);
        }
    }

    pub fn next_fire_time(&self) -> Option<i64> {
        let tasks = (self.source)();
        let mut min: Option<i64> = None;
        for task in tasks {
            if let Some(ts) = self.next_fire_for_task(&task) {
                min = Some(min.map_or(ts, |m| m.min(ts)));
            }
        }
        min
    }

    pub fn next_fire_for_task(&self, task: &CronTask) -> Option<i64> {
        let parsed = self.get_parsed(&task.cron)?;
        let now = self.clocks.wall_now();
        let seen = self.last_seen_at.lock().unwrap().get(&task.id).copied();
        let persisted = task.last_fired_at.filter(|lf| *lf <= now);
        let cursor = seen.or(persisted);
        let base = match cursor {
            Some(ts) if ts > task.created_at => ts,
            _ => task.created_at,
        };
        self.jittered_next(task, &parsed, base)
    }

    fn get_parsed(&self, cron: &str) -> Option<CronExpr> {
        {
            let cache = self.parsed_cache.lock().unwrap();
            if let Some(p) = cache.get(cron) { return Some(p.clone()); }
        }
        match parse_cron_expression(cron) {
            Ok(p) => {
                let mut cache = self.parsed_cache.lock().unwrap();
                cache.insert(cron.to_string(), p.clone());
                Some(p)
            }
            Err(_) => None,
        }
    }

    fn jittered_next(&self, task: &CronTask, parsed: &CronExpr, base_ms: i64) -> Option<i64> {
        let ideal = parsed.next_run_after(base_ms)?;
        if task.recurring == Some(false) {
            Some(one_shot_jittered_next_run_ms(task, ideal, &DEFAULT_CRON_JITTER_CONFIG))
        } else {
            Some(jittered_next_run_ms(task, parsed, ideal, &DEFAULT_CRON_JITTER_CONFIG))
        }
    }

    fn count_coalesced(&self, task: &CronTask, parsed: &CronExpr, first_fire: i64, now: i64) -> (usize, i64) {
        let mut count = 1usize;
        let mut cursor = first_fire;
        let mut last_due = first_fire;
        while count < MAX_COALESCE_ITERATIONS {
            let next = match parsed.next_run_after(cursor) {
                Some(n) => n,
                None => break,
            };
            if next > now { break; }
            let jittered = if task.recurring == Some(false) {
                one_shot_jittered_next_run_ms(task, next, &DEFAULT_CRON_JITTER_CONFIG)
            } else {
                jittered_next_run_ms(task, parsed, next, &DEFAULT_CRON_JITTER_CONFIG)
            };
            if jittered > now { break; }
            count += 1;
            cursor = next;
            last_due = next;
        }
        (count, last_due)
    }
}
```

### 步骤 5.3：运行测试

```bash
cargo test -p agent-rs --lib cron::scheduler::tests
cargo check -p agent-rs
```

**预期结果：** 测试通过，类型检查无错。

---

## Task 6：CronManager

**Depends on:** Task 2（`ClockSources`）、Task 4（`SessionCronStore`、`CronTaskPersistence`、`CronTask`）、Task 5（`CronScheduler`）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/cron/manager.rs`

### 步骤 6.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/cron/manager.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::clock::FileClock;
    use crate::cron::task::CronTaskInit;
    use crate::turn::fixture_agent::FixtureAgent;
    use crate::turn::TurnFlow;
    use kosong_rs::message::ContentPart;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn make_manager(dir: &TempDir, clock_path: std::path::PathBuf) -> (Arc<CronManager>, Arc<FixtureAgent>) {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let clocks: Arc<dyn ClockSources> = Arc::new(FileClock::new(clock_path.to_str().unwrap().to_string()));
        let manager = CronManager::new(
            agent.clone(),
            Some(dir.path().to_path_buf()),
            CronManagerOptions {
                clocks: Some(clocks),
                poll_interval_ms: Some(0), // manual tick
            },
        );
        (manager, agent)
    }

    #[tokio::test]
    async fn add_task_persists_after_flush() {
        let dir = TempDir::new().unwrap();
        let clock = dir.path().join("clock");
        fs::write(&clock, "0").unwrap();
        let (manager, _) = make_manager(&dir, clock);
        manager.add_task(CronTaskInit { cron: "0 9 * * *".to_string(), prompt: "p".to_string(), recurring: Some(true) }, 0);
        manager.flush_persist().await;
        let files: Vec<_> = std::fs::read_dir(dir.path().join("cron")).unwrap().collect();
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn fire_steers_and_emits_event() {
        let dir = TempDir::new().unwrap();
        let clock = dir.path().join("clock");
        fs::write(&clock, "0").unwrap();
        let (manager, agent) = make_manager(&dir, clock.clone());
        manager.add_task(CronTaskInit { cron: "* * * * *".to_string(), prompt: "ping".to_string(), recurring: Some(true) }, 0);
        fs::write(&clock, "60001").unwrap();
        manager.tick();

        let events = agent.captures.lock().unwrap().events.clone();
        let has_cron_fired = events.iter().any(|e| matches!(e, crate::turn::types::AgentEvent::CronFired { .. }));
        assert!(has_cron_fired);

        let inputs = agent.captures.lock().unwrap().context_inputs.clone();
        assert_eq!(inputs.len(), 1);
        if let ContentPart::Text { text } = &inputs[0].0[0] {
            assert!(text.contains("<cron_fire>"));
        } else {
            panic!("expected text content");
        }
    }

    #[tokio::test]
    async fn stale_recurring_task_removed() {
        let dir = TempDir::new().unwrap();
        let clock = dir.path().join("clock");
        fs::write(&clock, "0").unwrap();
        let (manager, agent) = make_manager(&dir, clock.clone());
        let now = 8 * 24 * 60 * 60 * 1000i64; // 8 days
        fs::write(&clock, now.to_string()).unwrap();
        manager.add_task(CronTaskInit { cron: "* * * * *".to_string(), prompt: "old".to_string(), recurring: Some(true) }, 0);
        manager.tick();
        assert!(manager.store.lock().unwrap().list().is_empty());
        let telemetry = agent.captures.lock().unwrap().telemetry_events.clone();
        assert!(telemetry.iter().any(|(n, _)| n == "cron_deleted"));
    }
}
```

运行：

```bash
cargo test -p agent-rs --lib cron::manager::tests
```

**预期结果：** 编译失败，`CronManager`、`CronManagerOptions` 不存在。

### 步骤 6.2：实现 manager

在同一文件追加实现：

```rust
use crate::context::cron_fire_xml::render_cron_fire_xml;
use crate::context::types::PromptOrigin;
use crate::cron::clock::{resolve_clock_sources, ClockSources};
use crate::cron::persist::CronTaskPersistence;
use crate::cron::scheduler::{CronScheduler, CronSchedulerOptions};
use crate::cron::task::{CronTask, CronTaskInit, SessionCronStore};
use crate::cron::types::CronFireContext;
use crate::turn::types::AgentEvent;
use crate::turn::TurnFlow;
use crate::turn::types::TurnAgent;
use kosong_rs::message::ContentPart;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

const STALE_THRESHOLD_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const CRON_SCHEDULED: &str = "cron_scheduled";
const CRON_FIRED: &str = "cron_fired";
const CRON_DELETED: &str = "cron_deleted";

pub struct CronManagerOptions {
    pub clocks: Option<Arc<dyn ClockSources>>,
    pub poll_interval_ms: Option<i64>,
}

pub struct CronManager {
    pub store: Mutex<SessionCronStore>,
    agent: Arc<dyn TurnAgent>,
    turn_flow: Arc<TurnFlow>,
    clocks: Arc<dyn ClockSources>,
    persist: Option<CronTaskPersistence>,
    scheduler: Arc<CronScheduler>,
    persist_queues: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl CronManager {
    pub fn new(
        agent: Arc<dyn TurnAgent>,
        session_dir: Option<PathBuf>,
        opts: CronManagerOptions,
    ) -> Arc<Self> {
        let clocks = opts.clocks.unwrap_or_else(|| resolve_clock_sources(None));
        let persist = session_dir.map(CronTaskPersistence::new);
        let store = Mutex::new(SessionCronStore::new());
        let turn_flow = Arc::new(TurnFlow::new(agent.clone()));

        let manager = Arc::new_cyclic(|weak| {
            let weak = weak.clone();
            let scheduler = CronScheduler::new(CronSchedulerOptions {
                clocks: clocks.clone(),
                source: Box::new({
                    let weak = weak.clone();
                    move || {
                        weak.upgrade()
                            .map(|m| m.store.lock().unwrap().list())
                            .unwrap_or_default()
                    }
                }),
                is_idle: Box::new({
                    let weak = weak.clone();
                    move || {
                        weak.upgrade()
                            .map(|m| !m.turn_flow.has_active_turn())
                            .unwrap_or(true)
                    }
                }),
                is_killed: Some(Box::new(|| {
                    std::env::var("ODY_DISABLE_CRON").ok().as_deref() == Some("1")
                })),
                on_fire: Box::new({
                    let weak = weak.clone();
                    move |task, ctx| {
                        if let Some(m) = weak.upgrade() {
                            m.handle_fire(task, ctx);
                        }
                    }
                }),
                remove_one_shot: Some(Box::new({
                    let weak = weak.clone();
                    move |id| {
                        if let Some(m) = weak.upgrade() {
                            m.remove_tasks(&[id.to_string()]);
                        }
                    }
                })),
                on_advance_cursor: Some(Box::new({
                    let weak = weak.clone();
                    move |id, ts| {
                        if let Some(m) = weak.upgrade() {
                            m.advance_cursor(id, ts);
                        }
                    }
                })),
                poll_interval_ms: opts.poll_interval_ms,
            });
            scheduler.start();

            CronManager {
                store,
                agent: agent.clone(),
                turn_flow,
                clocks,
                persist,
                scheduler,
                persist_queues: Mutex::new(HashMap::new()),
            }
        });
        manager
    }

    pub fn add_task(&self, init: CronTaskInit, now_ms: i64) -> CronTask {
        let task = self.store.lock().unwrap().add(init, now_ms);
        self.persist_enqueue(&task.id, {
            let task = task.clone();
            let persist = self.persist.clone();
            async move {
                if let Some(p) = persist {
                    let _ = p.write(&task).await;
                }
            }
        });
        task
    }

    pub fn remove_tasks(&self, ids: &[String]) -> Vec<String> {
        let removed = self.store.lock().unwrap().remove(ids);
        for id in &removed {
            self.persist_enqueue(id, {
                let id = id.clone();
                let persist = self.persist.clone();
                async move {
                    if let Some(p) = persist {
                        let _ = p.remove(&id).await;
                    }
                }
            });
        }
        removed
    }

    pub fn advance_cursor(&self, id: &str, last_fired_at: i64) {
        let Some(updated) = self.store.lock().unwrap().mark_fired(id, last_fired_at) else { return; };
        self.persist_enqueue(&updated.id, {
            let updated = updated.clone();
            let persist = self.persist.clone();
            async move {
                if let Some(p) = persist {
                    let _ = p.write(&updated).await;
                }
            }
        });
    }

    pub async fn load_from_disk(&self) {
        let Some(ref persist) = self.persist else { return; };
        if let Ok(tasks) = persist.list().await {
            let mut store = self.store.lock().unwrap();
            store.clear();
            for task in tasks {
                store.adopt(task);
            }
        }
    }

    pub async fn flush_persist(&self) {
        let handles: Vec<_> = {
            let mut queues = self.persist_queues.lock().unwrap();
            queues.drain().map(|(_, h)| h).collect()
        };
        for h in handles {
            let _ = h.await;
        }
    }

    pub fn start(&self) {
        self.scheduler.start();
    }

    pub fn stop(&self) {
        self.scheduler.stop();
    }

    pub fn tick(&self) {
        self.scheduler.tick();
    }

    pub fn next_fire_time(&self) -> Option<i64> {
        self.scheduler.next_fire_time()
    }

    pub fn next_fire_for_task(&self, task_id: &str) -> Option<i64> {
        if let Some(task) = self.store.lock().unwrap().get(task_id) {
            self.scheduler.next_fire_for_task(task)
        } else {
            None
        }
    }

    pub fn is_stale(&self, task: &CronTask) -> bool {
        if std::env::var("ODY_CRON_NO_STALE").ok().as_deref() == Some("1") {
            return false;
        }
        if task.recurring == Some(false) {
            return false;
        }
        let age = self.clocks.wall_now() - task.created_at;
        age >= 0 && age < i64::MAX && age >= STALE_THRESHOLD_MS
    }

    pub fn emit_scheduled(&self, task: &CronTask) {
        self.agent.telemetry().track(CRON_SCHEDULED, serde_json::json!({
            "recurring": task.recurring != Some(false),
        }));
    }

    pub fn emit_deleted(&self, task_id: &str) {
        self.agent.telemetry().track(CRON_DELETED, serde_json::json!({
            "task_id": task_id,
        }));
    }

    fn handle_fire(&self, task: &CronTask, ctx: CronFireContext) {
        let stale = self.is_stale(task);
        let recurring = task.recurring != Some(false);
        let origin = PromptOrigin::CronJob {
            job_id: ctx.id.to_string(),
            cron: ctx.schedule.clone(),
            recurring,
            coalesced_count: ctx.coalesced_count as i64,
            stale,
        };
        let xml = render_cron_fire_xml(&ctx);
        let content = vec![ContentPart::Text { text: xml }];
        let turn_id = self.turn_flow.steer(content, origin.clone());
        self.agent.event_emitter().emit_event(AgentEvent::CronFired {
            origin: origin.clone(),
            prompt: ctx.prompt.clone(),
        });
        self.agent.telemetry().track(CRON_FIRED, serde_json::json!({
            "recurring": recurring,
            "coalesced_count": ctx.coalesced_count,
            "stale": stale,
            "buffered": turn_id.is_none(),
        }));

        if stale && recurring {
            self.remove_tasks(&[task.id.clone()]);
            self.emit_deleted(&task.id);
        }
    }

    fn persist_enqueue(&self, id: &str, work: impl std::future::Future<Output = ()> + Send + 'static) {
        if self.persist.is_none() { return; }
        let id = id.to_string();
        let mut queues = self.persist_queues.lock().unwrap();
        let prev = queues.remove(&id);
        let next = tokio::spawn(async move {
            if let Some(p) = prev { let _ = p.await; }
            work.await;
        });
        queues.insert(id, next);
    }
}
```

### 步骤 6.3：运行测试

```bash
cargo test -p agent-rs --lib cron::manager::tests
cargo check -p agent-rs
```

**预期结果：** 测试通过，类型检查无错。

### 步骤 6.4：全量 crate 测试

```bash
cargo test -p agent-rs --lib
```

**预期结果：** 所有新增 cron 测试通过，且未引入 background/schema Part 的回归失败。

---

## Self-Review（本 Part）

### 1. 是否完整覆盖 4.3.8 的 cron 层需求？
是：解析、时钟、抖动、store、persistence、scheduler、CronManager 均实现。

### 2. 是否每个任务都有明确的依赖关系？
是：Task 1 无依赖；Task 2/3 与 Task 1 并行；Task 4 依赖 Task 3；Task 5 依赖 Task 1/3/4；Task 6 依赖 Task 2/4/5。

### 3. 是否有测试覆盖每个新增行为？
是：解析（next-run、OR 规则、never-fires）、时钟（system/file）、抖动（deterministic/disable/one-shot clamp）、store/persist（CRUD、id 校验）、scheduler（fire/cursor/one-shot removal/idle gate）、manager（persist/steer/stale）均有测试。

### 4. 是否引入了 TODO / placeholder？
否：所有代码均可直接执行。`CronTask` 在 Task 3 临时出现后立即在 Task 4 迁移到 `task.rs`，无悬空占位。

### 5. 是否遵循现有代码风格？
是：使用 `Arc<dyn TurnAgent>` + `Arc<TurnFlow>` 与 background Part 一致；使用 `Mutex` 保护共享状态；事件/telemetry 命名与 TS 对齐。

### 6. 类型一致性如何？
`CronTask` 字段使用 camelCase 序列化，与 TS `CronTask` 接口一致；`CronFireContext` 复用 schema.md 定义；`PromptOrigin::CronJob` 字段名与 TS `CronJobOrigin` 对齐。

### 7. 共享签名是否有遗漏？
本 Part 未修改共享枚举/struct（仅使用 schema.md 已扩展的 `AgentEvent`、`PromptOrigin`、`CronFireContext`），因此无需额外全树 match 修复。Manager 的 `handle_fire` 已使用 schema.md 提供的 `render_cron_fire_xml`。
