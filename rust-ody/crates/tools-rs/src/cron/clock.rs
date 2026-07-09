use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Abstraction over clock sources for testability.
pub trait ClockSources: Send + Sync {
    /// Wall clock time in milliseconds since epoch.
    fn wall_now(&self) -> u64;
    /// Monotonic time in milliseconds (for intervals, not tied to wall clock).
    fn mono_now_ms(&self) -> u64;
}

/// Default system clock implementation.
pub struct SystemClock;

impl ClockSources for SystemClock {
    fn wall_now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn mono_now_ms(&self) -> u64 {
        static BASE: OnceLock<(Instant, u64)> = OnceLock::new();
        let (base_instant, base_wall) =
            BASE.get_or_init(|| (Instant::now(), SystemClock.wall_now()));
        let elapsed = base_instant.elapsed().as_millis() as u64;
        base_wall + elapsed
    }
}
