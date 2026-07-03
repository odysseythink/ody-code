#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::clock::FileClock;
    use crate::cron::task::{CronTask, SessionCronStore};
    use crate::cron::types::CronFireContext;
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    #[tokio::test]
    async fn recurring_fires_and_advances_cursor() {
        let dir = TempDir::new().unwrap();
        let clock_path = dir.path().join("clock");
        fs::write(&clock_path, "0").unwrap();
        let clocks: Arc<dyn ClockSources> =
            Arc::new(FileClock::new(clock_path.to_str().unwrap().to_string()));

        let fired = Arc::new(Mutex::new(Vec::<CronFireContext>::new()));
        let advanced = Arc::new(Mutex::new(Vec::<(String, i64)>::new()));
        let store = Arc::new(Mutex::new(SessionCronStore::new()));
        // Use a known id to make jitter deterministic
        let _task = store.lock().unwrap().adopt(CronTask {
            id: "00000000".to_string(),
            cron: "* * * * *".to_string(),
            prompt: "p".to_string(),
            created_at: 0,
            recurring: Some(true),
            last_fired_at: None,
        });

        let f = fired.clone();
        let a = advanced.clone();
        let s = store.clone();
        let scheduler = CronScheduler::new(CronSchedulerOptions {
            clocks: clocks.clone(),
            source: Box::new(move || s.lock().unwrap().list()),
            is_idle: Box::new(|| true),
            is_killed: None,
            on_fire: Box::new(move |_, ctx| {
                f.lock().unwrap().push(ctx.clone());
            }),
            remove_one_shot: None,
            on_advance_cursor: Some(Box::new(move |id, ts| {
                a.lock().unwrap().push((id.to_string(), ts));
            })),
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
        let clocks: Arc<dyn ClockSources> =
            Arc::new(FileClock::new(clock_path.to_str().unwrap().to_string()));

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
            remove_one_shot: Some(Box::new(move |id| {
                r.lock().unwrap().push(id.to_string());
            })),
            on_advance_cursor: None,
            poll_interval_ms: None,
        });
        fs::write(&clock_path, "60001").unwrap();
        scheduler.tick();
        assert_eq!(*removed.lock().unwrap(), vec!["deadbeef".to_string()]);
    }

    #[tokio::test]
    async fn idle_gate_prevents_fire() {
        let idle = Arc::new(AtomicBool::new(false));
        let i = idle.clone();
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        let scheduler = CronScheduler::new(CronSchedulerOptions {
            clocks: crate::cron::clock::system_clocks(),
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

use crate::cron::clock::ClockSources;
use crate::cron::expr::{parse_cron_expression, CronExpr};
use crate::cron::jitter::{
    jittered_next_run_ms, one_shot_jittered_next_run_ms, DEFAULT_CRON_JITTER_CONFIG,
};
use crate::cron::task::CronTask;
use crate::cron::types::{CronFireContext, CronTaskId};
use chrono::{TimeZone, Utc};
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
        if *running {
            return;
        }
        let interval = self.poll_interval_ms.unwrap_or(DEFAULT_POLL_INTERVAL_MS);
        if interval <= 0 {
            return;
        }
        let this = self.clone();
        *running = true;
        drop(running);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(interval as u64)).await;
                let still_running = *this.timer_running.lock().unwrap();
                if !still_running {
                    break;
                }
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
            if is_killed() {
                return;
            }
        }
        if !(self.is_idle)() {
            return;
        }
        let tasks = (self.source)();
        if tasks.is_empty() {
            return;
        }
        let now = self.clocks.wall_now();
        let mut in_flight = self.in_flight.lock().unwrap();
        for task in tasks {
            if in_flight.contains(&task.id) {
                continue;
            }
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
            if next_fire.is_none() || now < next_fire.unwrap() {
                continue;
            }
            let _next_fire = next_fire.unwrap();

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
            let fired_at = Utc
                .timestamp_millis_opt(now)
                .single()
                .unwrap_or_else(Utc::now);
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
                self.last_seen_at
                    .lock()
                    .unwrap()
                    .insert(task.id.clone(), advanced_to);
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
            if let Some(p) = cache.get(cron) {
                return Some(p.clone());
            }
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
            Some(one_shot_jittered_next_run_ms(
                task,
                ideal,
                &DEFAULT_CRON_JITTER_CONFIG,
            ))
        } else {
            Some(jittered_next_run_ms(
                task,
                parsed,
                ideal,
                &DEFAULT_CRON_JITTER_CONFIG,
            ))
        }
    }

    fn count_coalesced(
        &self,
        task: &CronTask,
        parsed: &CronExpr,
        first_fire: i64,
        now: i64,
    ) -> (usize, i64) {
        let mut count = 1usize;
        let mut cursor = first_fire;
        let mut last_due = first_fire;
        while count < MAX_COALESCE_ITERATIONS {
            let next = match parsed.next_run_after(cursor) {
                Some(n) => n,
                None => break,
            };
            if next > now {
                break;
            }
            let jittered = if task.recurring == Some(false) {
                one_shot_jittered_next_run_ms(task, next, &DEFAULT_CRON_JITTER_CONFIG)
            } else {
                jittered_next_run_ms(task, parsed, next, &DEFAULT_CRON_JITTER_CONFIG)
            };
            if jittered > now {
                break;
            }
            count += 1;
            cursor = next;
            last_due = next;
        }
        (count, last_due)
    }
}
