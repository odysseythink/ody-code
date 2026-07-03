use tools_rs::cron::clock::{ClockSources, SystemClock};
use tools_rs::cron::cron_expr::{compute_next_cron_run, cron_to_human, parse_cron_expression};
use tools_rs::cron::jitter::{
    jittered_next_cron_run_ms, one_shot_jittered_next_cron_run_ms, JitterConfig,
};
use tools_rs::cron::time_format::format_local_iso_with_offset;

// === PARSING ===

#[test]
fn test_parse_simple_wildcard() {
    let p = parse_cron_expression("* * * * *").unwrap();
    assert!(p.minutes.is_empty()); // wildcard → empty vec
    assert!(p.hours.is_empty());
}

#[test]
fn test_parse_specific_values() {
    let p = parse_cron_expression("30 14 28 2 *").unwrap();
    assert_eq!(p.minutes, vec![30]);
    assert_eq!(p.hours, vec![14]);
    assert_eq!(p.days_of_month, vec![28]);
    assert_eq!(p.months, vec![2]);
}

#[test]
fn test_parse_ranges() {
    let p = parse_cron_expression("0-30 9-17 1-5 * 1-5").unwrap();
    assert_eq!(p.minutes, (0..=30).collect::<Vec<u32>>());
    assert_eq!(p.hours, (9..=17).collect::<Vec<u32>>());
    assert_eq!(p.days_of_week, (1..=5).collect::<Vec<u32>>());
}

#[test]
fn test_parse_steps() {
    let p = parse_cron_expression("*/15 */2 * * *").unwrap();
    assert_eq!(p.minutes, vec![0, 15, 30, 45]);
    assert_eq!(p.hours, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22]);
}

#[test]
fn test_parse_lists() {
    let p = parse_cron_expression("0,15,30,45 8,12,16 * * *").unwrap();
    assert_eq!(p.minutes, vec![0, 15, 30, 45]);
    assert_eq!(p.hours, vec![8, 12, 16]);
}

#[test]
fn test_parse_day_of_week_sunday() {
    let p = parse_cron_expression("0 9 * * 0").unwrap();
    assert!(p.days_of_week.contains(&0));
    let p7 = parse_cron_expression("0 9 * * 7").unwrap();
    assert!(p7.days_of_week.contains(&0)); // 7 → 0
}

#[test]
fn test_parse_reject_invalid() {
    assert!(parse_cron_expression("60 * * * *").is_err()); // minute 60
    assert!(parse_cron_expression("* 24 * * *").is_err()); // hour 24
    assert!(parse_cron_expression("* * 32 * *").is_err()); // dom 32
    assert!(parse_cron_expression("* * * 13 *").is_err()); // month 13
    assert!(parse_cron_expression("* * * * 8").is_err()); // dow 8
    assert!(parse_cron_expression("").is_err());
    assert!(parse_cron_expression("a b c d e").is_err());
}

#[test]
fn test_parse_normalize_whitespace() {
    let p = parse_cron_expression("  0   9   *   *   *  ").unwrap();
    assert_eq!(p.minutes, vec![0]);
    assert_eq!(p.hours, vec![9]);
}

// === COMPUTE NEXT RUN ===

#[test]
fn test_next_run_every_minute() {
    let p = parse_cron_expression("* * * * *").unwrap();
    let from = 1000 * 60 * 60 * 9; // 9:00 AM UTC in ms
    let next = compute_next_cron_run(&p, from).unwrap();
    assert_eq!(next, from + 60_000); // next minute
}

#[test]
fn test_next_run_specific_time() {
    let p = parse_cron_expression("30 14 * * *").unwrap();
    // 2026-01-01 12:00 UTC in ms: 1735732800000
    let from: u64 = 1735732800000;
    let next = compute_next_cron_run(&p, from).unwrap();
    // Should be 2026-01-01 14:30 UTC
    let expected = from + (2 * 3600 + 30 * 60) * 1000;
    assert_eq!(next, expected);
}

#[test]
fn test_next_run_first_of_month() {
    let p = parse_cron_expression("0 0 1 * *").unwrap();
    // 2026-01-15 12:00 UTC in ms
    let from: u64 = 1736942400000;
    let next = compute_next_cron_run(&p, from).unwrap();
    // Should be 2026-02-01 00:00
    let feb_first: u64 = 1738368000000;
    assert_eq!(next, feb_first);
}

// === CRON TO HUMAN ===

#[test]
fn test_cron_to_human_every_minute() {
    let p = parse_cron_expression("* * * * *").unwrap();
    assert_eq!(cron_to_human(&p), "every minute");
}

#[test]
fn test_cron_to_human_daily() {
    let p = parse_cron_expression("0 9 * * *").unwrap();
    assert!(cron_to_human(&p).contains("9:00"));
}

#[test]
fn test_cron_to_human_every_5_minutes() {
    let p = parse_cron_expression("*/5 * * * *").unwrap();
    assert_eq!(cron_to_human(&p), "every 5 minutes");
}

#[test]
fn test_cron_to_human_hourly() {
    let p = parse_cron_expression("0 * * * *").unwrap();
    assert_eq!(cron_to_human(&p), "hourly");
}

#[test]
fn test_cron_to_human_weekdays() {
    let p = parse_cron_expression("0 9 * * 1-5").unwrap();
    assert!(cron_to_human(&p).contains("weekdays"));
}

// === JITTER ===

#[test]
fn test_jitter_recurring_forward_shift() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000, // 15 min
        one_shot_max_ms: 90 * 1000,
    };
    let expr = parse_cron_expression("0 9 * * *").unwrap();
    // ideal fire at 9:00
    let ideal: u64 = 100000000;
    let jittered = jittered_next_cron_run_ms(&expr, ideal, "aabbccdd", &config);
    // Should be >= ideal (forward shift only for recurring)
    assert!(jittered >= ideal);
    // Should not exceed ideal + 15 minutes for a daily cron
    assert!(jittered < ideal + 15 * 60 * 1000);
}

#[test]
fn test_jitter_deterministic_same_id() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    let expr = parse_cron_expression("0 9 * * *").unwrap();
    let ideal: u64 = 100000000;
    let a = jittered_next_cron_run_ms(&expr, ideal, "deadbeef", &config);
    let b = jittered_next_cron_run_ms(&expr, ideal, "deadbeef", &config);
    assert_eq!(a, b, "same id must produce same jitter");
}

#[test]
fn test_jitter_different_ids() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    let expr = parse_cron_expression("0 9 * * *").unwrap();
    let ideal: u64 = 100000000;
    let a = jittered_next_cron_run_ms(&expr, ideal, "aaaaaaaa", &config);
    let b = jittered_next_cron_run_ms(&expr, ideal, "bbbbbbbb", &config);
    // Different IDs should (almost certainly) produce different jitter
    assert_ne!(a, b);
}

#[test]
fn test_one_shot_jitter_pull_forward() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    // Ideal is at :00 — should be pulled earlier
    let ideal: u64 = 100000000;
    let jittered = one_shot_jittered_next_cron_run_ms("abcdef01", ideal, &config);
    assert!(jittered <= ideal, "one-shot jitter pulls earlier");
    assert!(jittered >= ideal - 90_000, "max 90s pull-forward");
    // For :30, also applies
    let ideal30: u64 = 100000000 + 30 * 60 * 1000;
    let jittered30 = one_shot_jittered_next_cron_run_ms("abcdef01", ideal30, &config);
    assert!(jittered30 <= ideal30);
    assert!(jittered30 >= ideal30 - 90_000);
}

#[test]
fn test_one_shot_no_jitter_on_non_round_minute() {
    let config = JitterConfig {
        recurring_max_fraction_of_period: 0.1,
        recurring_max_ms: 15 * 60 * 1000,
        one_shot_max_ms: 90 * 1000,
    };
    // Not :00 or :30 — should pass through unchanged
    let ideal: u64 = 100000000 + 7 * 60 * 1000; // :07
    let jittered = one_shot_jittered_next_cron_run_ms("abcdef01", ideal, &config);
    assert_eq!(jittered, ideal, "non-round minute passes through unchanged");
}

// === TIME FORMAT ===

#[test]
fn test_format_local_iso_with_offset() {
    // Use a known UTC timestamp: 2026-06-15T09:30:00Z = 1774549800000 ms
    let ms: u64 = 1774549800000;
    let formatted = format_local_iso_with_offset(ms);
    // Format should be ISO 8601 with timezone offset
    assert!(
        formatted.contains("T"),
        "expected ISO 8601 format, got: {}",
        formatted
    );
    // Should contain timezone offset like +08:00 or -04:00 or Z
    assert!(
        formatted.contains('+') || formatted.contains('-') || formatted.ends_with('Z'),
        "expected timezone offset in: {}",
        formatted
    );
    // Should have millisecond precision
    assert!(
        formatted.contains('.'),
        "expected millisecond precision in: {}",
        formatted
    );
}

// === CLOCK ===

#[test]
fn test_system_clock_wall_now() {
    let clock = SystemClock;
    let now = clock.wall_now();
    // Should be a reasonable epoch millis (after 2020)
    assert!(now > 1577836800000); // 2020-01-01
}

#[test]
fn test_system_clock_mono_now() {
    let clock = SystemClock;
    let a = clock.mono_now_ms();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let b = clock.mono_now_ms();
    assert!(b > a, "monotonic clock should advance");
}
