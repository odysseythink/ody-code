use crate::cron::cron_expr::ParsedCronExpression;

/// Configuration for deterministic jitter.
#[derive(Debug, Clone, Copy)]
pub struct JitterConfig {
    /// Maximum fraction of the cron period to shift forward for recurring tasks.
    pub recurring_max_fraction_of_period: f64,
    /// Maximum forward shift in ms for recurring tasks.
    pub recurring_max_ms: u64,
    /// Maximum pull-forward shift in ms for one-shot tasks.
    pub one_shot_max_ms: u64,
}

impl Default for JitterConfig {
    fn default() -> Self {
        Self {
            recurring_max_fraction_of_period: 0.1,
            recurring_max_ms: 15 * 60 * 1000, // 15 minutes
            one_shot_max_ms: 90 * 1000,       // 90 seconds
        }
    }
}

/// Compute a deterministic forward jitter offset for a recurring task.
///
/// The jitter is based on the task's 8-hex ID, converted to a fraction [0, 1).
/// It shifts the ideal fire time FORWARD by up to `min(fraction * period, max_ms)`.
pub fn jittered_next_cron_run_ms(
    expr: &ParsedCronExpression,
    ideal_ms: u64,
    task_id: &str,
    config: &JitterConfig,
) -> u64 {
    let fraction = fraction_from_id(task_id);
    let period_ms = estimate_cron_period_ms(expr);
    let max_jitter = (period_ms as f64 * config.recurring_max_fraction_of_period) as u64;
    let max_jitter = max_jitter.min(config.recurring_max_ms);
    let jitter_ms = (fraction * max_jitter as f64) as u64;
    ideal_ms.saturating_add(jitter_ms)
}

/// Compute deterministic pull-forward jitter for one-shot tasks.
///
/// Only applies when the ideal fire time lands on :00 or :30 of the hour.
/// Shifts EARLIER by a deterministic offset based on the task ID.
pub fn one_shot_jittered_next_cron_run_ms(
    task_id: &str,
    ideal_ms: u64,
    config: &JitterConfig,
) -> u64 {
    let minute = (ideal_ms / 60000) % 60;
    if minute == 0 || minute == 30 {
        let fraction = fraction_from_id(task_id);
        let jitter_ms = (fraction * config.one_shot_max_ms as f64) as u64;
        ideal_ms.saturating_sub(jitter_ms)
    } else {
        ideal_ms
    }
}

/// Convert an 8-hex task ID to a deterministic fraction in [0.0, 1.0).
fn fraction_from_id(id: &str) -> f64 {
    let hex_part: String = id
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect();
    if hex_part.is_empty() {
        return 0.0;
    }
    let val = u32::from_str_radix(&hex_part, 16).unwrap_or(0);
    val as f64 / u32::MAX as f64
}

/// Rough estimate of a cron expression's period in milliseconds.
/// Used to bound the jitter range for recurring tasks.
fn estimate_cron_period_ms(expr: &ParsedCronExpression) -> u64 {
    // Estimate based on the smallest non-wildcard field
    if !expr.minutes.is_empty() {
        let step = min_step(&expr.minutes);
        return (step as u64) * 60 * 1000;
    }
    if !expr.hours.is_empty() {
        let step = min_step(&expr.hours);
        return (step as u64) * 3600 * 1000;
    }
    if !expr.days_of_month.is_empty() || !expr.days_of_week.is_empty() {
        return 24 * 3600 * 1000;
    }
    if !expr.months.is_empty() {
        return 30 * 24 * 3600 * 1000;
    }
    0
}

fn min_step(values: &[u32]) -> u32 {
    if values.len() < 2 {
        return 1;
    }
    let mut sorted: Vec<u32> = values.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut min_diff = u32::MAX;
    for w in sorted.windows(2) {
        let diff = w[1] - w[0];
        if diff < min_diff {
            min_diff = diff;
        }
    }
    if min_diff == u32::MAX {
        1
    } else {
        min_diff
    }
}
