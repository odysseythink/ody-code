use chrono::{Datelike, Timelike};

/// Parsed 5-field cron expression. Empty vec = wildcard (all values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCronExpression {
    pub raw: String,
    pub minutes: Vec<u32>,
    pub hours: Vec<u32>,
    pub days_of_month: Vec<u32>,
    pub months: Vec<u32>,
    pub days_of_week: Vec<u32>,
    /// True when days_of_month is wildcard (*)
    pub days_of_month_wildcard: bool,
    /// True when days_of_week is wildcard (*)
    pub days_of_week_wildcard: bool,
}

/// Parse a 5-field cron expression. Returns error with description on invalid input.
pub fn parse_cron_expression(expr: &str) -> Result<ParsedCronExpression, String> {
    let raw = expr.trim().to_string();
    let fields: Vec<&str> = raw.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!("Expected 5 fields, got {}", fields.len()));
    }

    let minutes = parse_field(fields[0], 0, 59)?;
    let hours = parse_field(fields[1], 0, 23)?;
    let days_of_month = parse_field(fields[2], 1, 31)?;
    let months = parse_field(fields[3], 1, 12)?;
    let days_of_week = parse_field_dow(fields[4])?;

    let dom_wildcard = fields[2] == "*";
    let dow_wildcard = fields[4] == "*";

    Ok(ParsedCronExpression {
        raw,
        minutes,
        hours,
        days_of_month,
        months,
        days_of_week,
        days_of_month_wildcard: dom_wildcard,
        days_of_week_wildcard: dow_wildcard,
    })
}

fn parse_field(field: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    if field == "*" {
        return Ok(Vec::new());
    }

    let mut values = Vec::new();
    for part in field.split(',') {
        let part = part.trim();
        if part.contains('/') {
            // Step syntax: */5 or 0-30/5
            let mut split = part.splitn(2, '/');
            let range = split.next().unwrap();
            let step: u32 = split
                .next()
                .unwrap()
                .parse()
                .map_err(|_| format!("Invalid step: {}", part))?;
            let (r_min, r_max) = if range == "*" {
                (min, max)
            } else if range.contains('-') {
                let mut rs = range.splitn(2, '-');
                let lo: u32 = rs
                    .next()
                    .unwrap()
                    .parse()
                    .map_err(|_| format!("Invalid range start: {}", part))?;
                let hi: u32 = rs
                    .next()
                    .unwrap()
                    .parse()
                    .map_err(|_| format!("Invalid range end: {}", part))?;
                (lo, hi)
            } else {
                return Err(format!("Invalid step range: {}", part));
            };
            for v in (r_min..=r_max).step_by(step as usize) {
                values.push(v);
            }
        } else if part.contains('-') {
            let mut rs = part.splitn(2, '-');
            let lo: u32 = rs
                .next()
                .unwrap()
                .parse()
                .map_err(|_| format!("Invalid range: {}", part))?;
            let hi: u32 = rs
                .next()
                .unwrap()
                .parse()
                .map_err(|_| format!("Invalid range: {}", part))?;
            for v in lo..=hi {
                values.push(v);
            }
        } else {
            let v: u32 = part
                .parse()
                .map_err(|_| format!("Invalid value: {}", part))?;
            values.push(v);
        }
    }

    // Validate range
    for &v in &values {
        if v < min || v > max {
            return Err(format!("Value {} out of range [{}, {}]", v, min, max));
        }
    }

    Ok(values)
}

fn parse_field_dow(field: &str) -> Result<Vec<u32>, String> {
    let values = parse_field(field, 0, 7)?;
    // Normalize 7 → 0 (Sunday)
    Ok(values
        .into_iter()
        .map(|v| if v == 7 { 0 } else { v })
        .collect())
}

fn matches_field(value: u32, allowed: &[u32]) -> bool {
    if allowed.is_empty() {
        return true; // wildcard
    }
    allowed.contains(&value)
}

/// Compute the next cron fire time in milliseconds since epoch.
/// Returns None if no fire within 5 years.
pub fn compute_next_cron_run(expr: &ParsedCronExpression, from_ms: u64) -> Option<u64> {
    let five_years_ms: u64 = 5 * 365 * 24 * 3600 * 1000;
    let max_ms = from_ms + five_years_ms;

    // Start from the next minute to avoid matching the current minute
    let mut current = ((from_ms / 60000) + 1) * 60000;

    while current <= max_ms {
        let dt = utc_millis_to_components(current);

        if matches_field(dt.minute, &expr.minutes)
            && matches_field(dt.hour, &expr.hours)
            && matches_field(dt.month, &expr.months)
        {
            // Day matching: OR between days_of_month and days_of_week
            let dom_match =
                expr.days_of_month_wildcard || matches_field(dt.day, &expr.days_of_month);
            let dow_match =
                expr.days_of_week_wildcard || matches_field(dt.day_of_week, &expr.days_of_week);

            // When both are non-wildcard, either match counts (OR semantics)
            let day_ok = if !expr.days_of_month_wildcard && !expr.days_of_week_wildcard {
                dom_match || dow_match
            } else {
                dom_match && dow_match
            };

            if day_ok {
                return Some(current);
            }
        }

        current += 60000; // advance 1 minute
    }

    None
}

#[derive(Debug)]
struct DateTimeComponents {
    minute: u32,
    hour: u32,
    day: u32,
    month: u32,
    day_of_week: u32, // 0=Sun
}

fn utc_millis_to_components(ms: u64) -> DateTimeComponents {
    let total_secs = (ms / 1000) as i64;
    let dt = chrono::DateTime::from_timestamp(total_secs, 0).expect("valid timestamp");
    DateTimeComponents {
        minute: dt.minute(),
        hour: dt.hour(),
        day: dt.day(),
        month: dt.month(),
        day_of_week: dt.weekday().num_days_from_sunday(),
    }
}

/// Check if a parsed expression has at least one fire within `years` from `from_ms`.
pub fn has_fire_within_years(expr: &ParsedCronExpression, years: u32, from_ms: u64) -> bool {
    let max_ms = from_ms + (years as u64) * 365 * 24 * 3600 * 1000;
    compute_next_cron_run(expr, from_ms)
        .map(|next| next <= max_ms)
        .unwrap_or(false)
}

/// Produce a human-readable description of a cron expression.
pub fn cron_to_human(expr: &ParsedCronExpression) -> String {
    // Simple heuristic-based descriptions matching TS behavior
    let parts: Vec<&str> = expr.raw.split_whitespace().collect();
    if parts.len() != 5 {
        return expr.raw.clone();
    }

    let (min, hour, dom, month, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

    // Every minute
    if min == "*" && hour == "*" && dom == "*" && month == "*" && dow == "*" {
        return "every minute".to_string();
    }

    // Every N minutes
    if min.starts_with("*/") && hour == "*" && dom == "*" && month == "*" && dow == "*" {
        let n = &min[2..];
        if n == "1" {
            return "every minute".to_string();
        }
        return format!("every {} minutes", n);
    }

    // Hourly
    if min == "0" && hour == "*" && dom == "*" && month == "*" && dow == "*" {
        return "hourly".to_string();
    }

    // Daily at specific time
    if hour != "*" && dom == "*" && month == "*" && dow == "*" {
        let h: u32 = hour.parse().unwrap_or(0);
        let m: u32 = min.parse().unwrap_or(0);
        let ampm = if h == 0 {
            "12:00 AM".to_string()
        } else if h < 12 {
            format!("{}:{:02} AM", h, m)
        } else if h == 12 {
            format!("12:{:02} PM", m)
        } else {
            format!("{}:{:02} PM", h - 12, m)
        };
        return format!("daily at {}", ampm);
    }

    // Weekdays
    if hour != "*" && dow == "1-5" && dom == "*" && month == "*" {
        let h: u32 = hour.parse().unwrap_or(0);
        let m: u32 = min.parse().unwrap_or(0);
        let ampm = if h < 12 {
            format!("{}:{:02} AM", h, m)
        } else if h == 12 {
            format!("12:{:02} PM", m)
        } else {
            format!("{}:{:02} PM", h - 12, m)
        };
        return format!("weekdays at {}", ampm);
    }

    // Fallback: show the raw expression
    expr.raw.clone()
}
