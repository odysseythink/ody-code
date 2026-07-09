#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    fn ms(y: i32, mo: u32, d: u32, h: u32, min: u32) -> i64 {
        Local
            .with_ymd_and_hms(y, mo, d, h, min, 0)
            .unwrap()
            .timestamp_millis()
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

use chrono::{Datelike, Duration, Local, Months, NaiveDateTime, TimeZone, Timelike};
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
    pub fn empty() -> Self {
        Self { bits: 0 }
    }
    pub fn insert(&mut self, value: i32) {
        self.bits |= 1u64 << value;
    }
    pub fn contains(&self, value: i32) -> bool {
        (self.bits >> value) & 1 == 1
    }
    pub fn is_full_range(&self, min: i32, max: i32) -> bool {
        let mask = ((1u64 << (max - min + 1)) - 1) << min;
        self.bits == mask
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronParseError {
    BadFieldCount {
        got: usize,
    },
    EmptyField {
        name: &'static str,
    },
    InvalidInt {
        name: &'static str,
        role: &'static str,
        raw: String,
    },
    OutOfRange {
        name: &'static str,
        value: i32,
        min: i32,
        max: i32,
    },
    BadRange {
        name: &'static str,
        lo: i32,
        hi: i32,
    },
    EmptyStep {
        name: &'static str,
        term: String,
    },
    NonPositiveStep {
        name: &'static str,
        step: String,
    },
    NoMatchingValues {
        name: &'static str,
    },
}

impl fmt::Display for CronParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CronParseError::BadFieldCount { got } => write!(
                f,
                "cron expression must have exactly 5 fields (minute hour day-of-month month day-of-week); got {}",
                got
            ),
            CronParseError::EmptyField { name } => write!(f, "cron {} field is empty", name),
            CronParseError::InvalidInt { name, role, raw } => write!(
                f,
                "cron {} {} must be a non-negative integer with digits only (got {})",
                name, role, raw
            ),
            CronParseError::OutOfRange {
                name,
                value,
                min,
                max,
            } => write!(
                f,
                "cron {} value {} out of range {}..{}",
                name, value, min, max
            ),
            CronParseError::BadRange { name, lo, hi } => write!(
                f,
                "cron {} range {}-{} out of bounds (must be ascending)",
                name, lo, hi
            ),
            CronParseError::EmptyStep { name, term } => {
                write!(f, "cron {} step is empty in {}", name, term)
            }
            CronParseError::NonPositiveStep { name, step } => write!(
                f,
                "cron {} step must be a positive integer (got {})",
                name, step
            ),
            CronParseError::NoMatchingValues { name } => {
                write!(f, "cron {} field matches no values", name)
            }
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
    let dow_field = parse_field(fields[4], "day-of-week", DOW_RANGE)?;
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

fn parse_field(
    field: &str,
    name: &'static str,
    range: (i32, i32),
) -> Result<CronField, CronParseError> {
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
        raw.parse::<i32>().map_err(|_| CronParseError::InvalidInt {
            name,
            role,
            raw: raw.to_string(),
        })
    } else {
        Err(CronParseError::InvalidInt {
            name,
            role,
            raw: raw.to_string(),
        })
    }
}

fn add_term(
    term: &str,
    name: &'static str,
    (min, max): (i32, i32),
    out: &mut CronField,
) -> Result<(), CronParseError> {
    let (range_part, step) = if let Some(idx) = term.find('/') {
        let rp = &term[..idx];
        let s = &term[idx + 1..];
        if s.is_empty() {
            return Err(CronParseError::EmptyStep {
                name,
                term: term.to_string(),
            });
        }
        let step_val = parse_int(s, name, "step")?;
        if step_val <= 0 {
            return Err(CronParseError::NonPositiveStep {
                name,
                step: s.to_string(),
            });
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
            return Err(CronParseError::OutOfRange {
                name,
                value: v,
                min,
                max,
            });
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

        while date.and_utc().timestamp_millis() <= deadline && iterations < HARD_ITERATION_CAP {
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
            let local_dt = Local.from_local_datetime(&date).single()?;
            return Some(local_dt.timestamp_millis());
        }
        None
    }

    fn day_matches(&self, date: NaiveDateTime) -> bool {
        let dom = date.day() as i32;
        let dow = date.weekday().num_days_from_sunday() as i32;
        let dom_ok = self.days_of_month.contains(dom);
        let dow_ok = self.days_of_week.contains(dow);
        if self.dom_wildcard && self.dow_wildcard {
            return true;
        }
        if self.dom_wildcard {
            return dow_ok;
        }
        if self.dow_wildcard {
            return dom_ok;
        }
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
        .with_hour(0)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap();
}

fn advance_hour(date: &mut NaiveDateTime) {
    *date = (*date + Duration::hours(1))
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap();
}

fn advance_minute(date: &mut NaiveDateTime) {
    *date = *date + Duration::minutes(1);
}
