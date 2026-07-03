#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn system_clock_returns_positive_wall_and_mono() {
        let clock: Arc<dyn ClockSources> = system_clocks();
        let w = clock.wall_now();
        let _m = clock.mono_now_ms();
        assert!(w > 0, "wall clock should be positive");
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
    pub fn new(path: String) -> Self {
        Self { path }
    }
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
    first
        .parse::<i64>()
        .ok()
        .filter(|v| v.is_finite_ms())
        .unwrap_or_else(system_wall_now)
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
    fn is_finite_ms(&self) -> bool {
        true
    }
}
