/// Format a millisecond timestamp as local ISO 8601 with numeric timezone offset.
/// Example: "2026-06-15T17:30:00.000+08:00"
pub fn format_local_iso_with_offset(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let nsecs = ((ms % 1000) * 1_000_000) as u32;
    match chrono::DateTime::from_timestamp(secs, nsecs) {
        Some(utc) => {
            let local = utc.with_timezone(&chrono::Local);
            local.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string()
        }
        None => {
            // Fallback for out-of-range timestamps
            format!("<invalid timestamp {}>", ms)
        }
    }
}
