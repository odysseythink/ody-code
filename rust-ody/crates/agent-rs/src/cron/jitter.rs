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
        let ideal = parse_cron_expression("0 9 * * *")
            .unwrap()
            .next_run_after(0)
            .unwrap();
        let j1 = jittered_next_run_ms(&t, &expr, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        let j2 = jittered_next_run_ms(&t, &expr, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        assert_eq!(j1, j2);
        assert!(j1 >= ideal);
        assert!(j1 - ideal <= 15 * 60 * 1000);
    }

    #[test]
    fn recurring_jitter_disabled_by_config() {
        let expr = parse_cron_expression("0 9 * * *").unwrap();
        let t = task("deadbeef", 0, None);
        let ideal = expr.next_run_after(0).unwrap();
        let config = JitterConfig {
            recurring_max_fraction_of_period: 0.0,
            recurring_max_ms: 0,
            one_shot_max_ms: 0,
        };
        let j = jittered_next_run_ms(&t, &expr, ideal, &config);
        assert_eq!(j, ideal);
    }

    #[test]
    fn recurring_jitter_disabled_env() {
        let expr = parse_cron_expression("0 9 * * *").unwrap();
        let t = task("deadbeef", 0, None);
        let ideal = expr.next_run_after(0).unwrap();
        // Simulate env-var check by temporarily setting test mode
        TEST_DISABLE_JITTER.store(true, std::sync::atomic::Ordering::SeqCst);
        let j = jittered_next_run_ms(&t, &expr, ideal, &DEFAULT_CRON_JITTER_CONFIG);
        TEST_DISABLE_JITTER.store(false, std::sync::atomic::Ordering::SeqCst);
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

use crate::cron::expr::CronExpr;
use crate::cron::task::CronTask;
use std::sync::atomic::{AtomicBool, Ordering};

static TEST_DISABLE_JITTER: AtomicBool = AtomicBool::new(false);

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
    let period = next_next
        .map(|n| n - ideal_ms)
        .filter(|d| *d > 0)
        .unwrap_or(MS_PER_DAY);
    let period_cap = (period as f64 * config.recurring_max_fraction_of_period) as i64;
    let cap = period_cap.min(config.recurring_max_ms);
    if cap <= 0 {
        return ideal_ms;
    }
    let offset = (cap as f64 * fraction_from_id(&task.id)).round() as i64;
    ideal_ms + offset
}

pub fn one_shot_jittered_next_run_ms(task: &CronTask, ideal_ms: i64, config: &JitterConfig) -> i64 {
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
    if TEST_DISABLE_JITTER.load(Ordering::Relaxed) {
        return true;
    }
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
        hash = ((hash << 5).wrapping_add(hash).wrapping_add(b as i64)) & 0xffffffff;
    }
    hash as u32 as f64 / (u32::MAX as f64 + 1.0)
}
