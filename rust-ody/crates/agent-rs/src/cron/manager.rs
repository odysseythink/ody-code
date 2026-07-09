#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::clock::FileClock;
    use crate::cron::task::CronTaskInit;
    use crate::turn::fixture_agent::FixtureAgent;
    use kosong_rs::message::ContentPart;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn make_manager(
        dir: &TempDir,
        clock_path: std::path::PathBuf,
    ) -> (Arc<CronManager>, Arc<FixtureAgent>, Arc<TurnFlow>) {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = Arc::new(TurnFlow::new(agent.clone()));
        let clocks: Arc<dyn ClockSources> =
            Arc::new(FileClock::new(clock_path.to_str().unwrap().to_string()));
        let manager = CronManager::new(
            agent.clone(),
            flow.clone(),
            Some(dir.path().to_path_buf()),
            CronManagerOptions {
                clocks: Some(clocks),
                poll_interval_ms: Some(0), // manual tick
            },
        );
        (manager, agent, flow)
    }

    #[tokio::test]
    async fn add_task_persists_after_flush() {
        let dir = TempDir::new().unwrap();
        let clock = dir.path().join("clock");
        fs::write(&clock, "0").unwrap();
        let (manager, _, _flow) = make_manager(&dir, clock);
        manager.add_task(
            CronTaskInit {
                cron: "0 9 * * *".to_string(),
                prompt: "p".to_string(),
                recurring: Some(true),
            },
            0,
        );
        manager.flush_persist().await;
        let files: Vec<_> = std::fs::read_dir(dir.path().join("cron"))
            .unwrap()
            .collect();
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn fire_steers_and_emits_event() {
        let dir = TempDir::new().unwrap();
        let clock = dir.path().join("clock");
        fs::write(&clock, "0").unwrap();
        let (manager, agent, flow) = make_manager(&dir, clock.clone());
        // Use a known id for deterministic jitter
        manager
            .store
            .lock()
            .unwrap()
            .adopt(crate::cron::task::CronTask {
                id: "00000000".to_string(),
                cron: "* * * * *".to_string(),
                prompt: "ping".to_string(),
                created_at: 0,
                recurring: Some(true),
                last_fired_at: None,
            });
        fs::write(&clock, "60001").unwrap();
        manager.tick();

        // Wait for the steered turn to complete (handle_fire calls steer())
        let _ = flow.wait_for_current_turn(None).await;

        let events = agent.captures.lock().unwrap().events.clone();
        let has_cron_fired = events
            .iter()
            .any(|e| matches!(e, crate::turn::types::AgentEvent::CronFired { .. }));
        assert!(has_cron_fired);

        let inputs = agent.captures.lock().unwrap().context_inputs.clone();
        assert!(
            !inputs.is_empty(),
            "expected at least one context input from cron fire steer"
        );
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
        let (manager, agent, _flow) = make_manager(&dir, clock.clone());
        let now = 8i64 * 24 * 60 * 60 * 1000; // 8 days
        fs::write(&clock, now.to_string()).unwrap();
        manager.add_task(
            CronTaskInit {
                cron: "* * * * *".to_string(),
                prompt: "old".to_string(),
                recurring: Some(true),
            },
            0,
        );
        manager.tick();
        assert!(manager.store.lock().unwrap().list().is_empty());
        let telemetry = agent.captures.lock().unwrap().telemetry_events.clone();
        assert!(telemetry.iter().any(|(n, _)| n == "cron_deleted"));
    }
}

use crate::context::cron_fire_xml::render_cron_fire_xml;
use crate::cron::clock::{resolve_clock_sources, ClockSources};
use crate::cron::persist::CronTaskPersistence;
use crate::cron::scheduler::{CronScheduler, CronSchedulerOptions};
use crate::cron::task::{CronTask, CronTaskInit, SessionCronStore};
use crate::cron::types::CronFireContext;
use crate::turn::types::{AgentEvent, TurnAgent};
use crate::turn::TurnFlow;
use kosong_rs::message::ContentPart;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

use crate::context::types::PromptOrigin;

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
        turn_flow: Arc<TurnFlow>,
        session_dir: Option<PathBuf>,
        opts: CronManagerOptions,
    ) -> Arc<Self> {
        let clocks = opts.clocks.unwrap_or_else(|| resolve_clock_sources(None));
        let persist = session_dir.map(CronTaskPersistence::new);
        let store = Mutex::new(SessionCronStore::new());

        Arc::new_cyclic(|weak: &std::sync::Weak<CronManager>| {
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
        })
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
        let Some(updated) = self.store.lock().unwrap().mark_fired(id, last_fired_at) else {
            return;
        };
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
        let Some(ref persist) = self.persist else {
            return;
        };
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
        self.agent.telemetry().track(
            CRON_SCHEDULED,
            serde_json::json!({
                "recurring": task.recurring != Some(false),
            }),
        );
    }

    pub fn emit_deleted(&self, task_id: &str) {
        self.agent.telemetry().track(
            CRON_DELETED,
            serde_json::json!({
                "task_id": task_id,
            }),
        );
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
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::CronFired {
                origin: origin.clone(),
                prompt: ctx.prompt.clone(),
            });
        self.agent.telemetry().track(
            CRON_FIRED,
            serde_json::json!({
                "recurring": recurring,
                "coalesced_count": ctx.coalesced_count,
                "stale": stale,
                "buffered": turn_id.is_none(),
            }),
        );

        if stale && recurring {
            self.remove_tasks(&[task.id.clone()]);
            self.emit_deleted(&task.id);
        }
    }

    fn persist_enqueue(
        &self,
        id: &str,
        work: impl std::future::Future<Output = ()> + Send + 'static,
    ) {
        if self.persist.is_none() {
            return;
        }
        let id = id.to_string();
        let mut queues = self.persist_queues.lock().unwrap();
        let prev = queues.remove(&id);
        let next = tokio::spawn(async move {
            if let Some(p) = prev {
                let _ = p.await;
            }
            work.await;
        });
        queues.insert(id, next);
    }
}
