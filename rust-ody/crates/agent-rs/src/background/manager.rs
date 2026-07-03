use crate::background::persistence::BackgroundTaskPersistence;
use crate::background::types::{
    is_background_task_terminal, BackgroundTask, BackgroundTaskId, BackgroundTaskInfo,
    BackgroundTaskKind, BackgroundTaskSink, BackgroundTaskStatus, SinkState,
};
use crate::context::types::PromptOrigin;
use crate::turn::types::{AgentEvent, TurnAgent};
use crate::turn::TurnFlow;
use chrono::Utc;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

const SIGTERM_GRACE_MS: u64 = 5_000;
const NOTIFICATION_TAIL_BYTES: u64 = 3_000;

pub type IdGenerator = Arc<dyn Fn(&str) -> String + Send + Sync>;

struct OutputLogger {
    _handle: tokio::task::JoinHandle<()>,
    tx: mpsc::UnboundedSender<String>,
}

impl OutputLogger {
    fn new(persistence: Arc<BackgroundTaskPersistence>, task_id: String) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let handle = tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let _ = persistence.append_task_output(&task_id, &chunk).await;
            }
        });
        Self {
            _handle: handle,
            tx,
        }
    }

    fn append(&self, chunk: String) {
        let _ = self.tx.send(chunk);
    }
}

struct ManagerSink {
    inner: Arc<SinkState>,
    logger: Option<OutputLogger>,
}

impl BackgroundTaskSink for ManagerSink {
    fn append_output(&self, chunk: &str) {
        self.inner.append_output(chunk);
        if let Some(ref logger) = self.logger {
            logger.append(chunk.to_string());
        }
    }

    fn snapshot(&self) -> String {
        self.inner.snapshot()
    }
}

struct ManagedTask {
    task_id: String,
    #[allow(dead_code)]
    base_id_prefix: String,
    description: String,
    kind: BackgroundTaskKind,
    status: Mutex<BackgroundTaskStatus>,
    started_at: chrono::DateTime<Utc>,
    finished_at: Mutex<Option<chrono::DateTime<Utc>>>,
    stop_reason: Mutex<Option<String>>,
    terminal_notification_suppressed: Mutex<bool>,
    sink: Arc<SinkState>,
    #[allow(dead_code)]
    output_logger: Option<OutputLogger>,
    stop_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    terminal_fired: Mutex<bool>,
    waiters: Mutex<Vec<tokio::sync::oneshot::Sender<()>>>,
}

pub struct BackgroundManager {
    agent: Arc<dyn TurnAgent>,
    turn_flow: Arc<TurnFlow>,
    persistence: Option<Arc<BackgroundTaskPersistence>>,
    tasks: Mutex<std::collections::HashMap<String, Arc<ManagedTask>>>,
    ghosts: Mutex<std::collections::HashMap<String, BackgroundTaskInfo>>,
    scheduled_notifications: Mutex<HashSet<String>>,
    delivered_notifications: Mutex<HashSet<String>>,
    id_generator: Mutex<IdGenerator>,
    persist_tx: Option<tokio::sync::mpsc::UnboundedSender<BackgroundTaskInfo>>,
}

impl BackgroundManager {
    pub fn new(
        agent: Arc<dyn TurnAgent>,
        turn_flow: Arc<TurnFlow>,
        persistence: Option<BackgroundTaskPersistence>,
    ) -> Self {
        let persistence = persistence.map(Arc::new);
        let mut manager = Self {
            agent,
            turn_flow,
            persistence: persistence.clone(),
            tasks: Mutex::new(std::collections::HashMap::new()),
            ghosts: Mutex::new(std::collections::HashMap::new()),
            scheduled_notifications: Mutex::new(HashSet::new()),
            delivered_notifications: Mutex::new(HashSet::new()),
            id_generator: Mutex::new(Arc::new(default_id_generator)),
            persist_tx: None,
        };
        manager.spawn_persist_worker(persistence);
        manager
    }

    pub fn set_id_generator(&mut self, gen: IdGenerator) {
        *self.id_generator.lock().unwrap() = gen;
    }

    fn spawn_persist_worker(&mut self, persistence: Option<Arc<BackgroundTaskPersistence>>) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<BackgroundTaskInfo>();
        tokio::spawn(async move {
            if let Some(p) = persistence {
                while let Some(info) = rx.recv().await {
                    let _ = p.write_task(&info).await;
                }
            }
        });
        self.persist_tx = Some(tx);
    }

    fn generate_id(&self, prefix: &str) -> String {
        (self.id_generator.lock().unwrap())(prefix)
    }

    pub fn register_task(&self, mut task: Box<dyn BackgroundTask>) -> String {
        let prefix = match task.base().kind {
            BackgroundTaskKind::Process => "bash",
            BackgroundTaskKind::Agent => "agent",
            BackgroundTaskKind::Question => "question",
        };
        let task_id = self.generate_id(prefix);
        task.set_id(BackgroundTaskId::new(task_id.clone()));
        let started_at = Utc::now();
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let inner_sink = Arc::new(SinkState::default());
        let output_logger = self
            .persistence
            .as_ref()
            .map(|p| OutputLogger::new(p.clone(), task_id.clone()));
        let manager_sink: Arc<dyn BackgroundTaskSink> = Arc::new(ManagerSink {
            inner: inner_sink.clone(),
            logger: output_logger,
        });

        let entry = Arc::new(ManagedTask {
            task_id: task_id.clone(),
            base_id_prefix: prefix.into(),
            description: task.base().description.clone(),
            kind: task.base().kind,
            status: Mutex::new(BackgroundTaskStatus::Running),
            started_at,
            finished_at: Mutex::new(None),
            stop_reason: Mutex::new(None),
            terminal_notification_suppressed: Mutex::new(false),
            sink: inner_sink,
            stop_tx: Mutex::new(Some(stop_tx)),
            terminal_fired: Mutex::new(false),
            waiters: Mutex::new(Vec::new()),
            output_logger: None,
        });

        self.tasks
            .lock()
            .unwrap()
            .insert(task_id.clone(), entry.clone());

        let entry_for_worker = entry.clone();
        let manager = Arc::new(self.clone_shallow());
        tokio::spawn(async move {
            let settlement = task.run(manager_sink, stop_rx).await;
            manager.settle_task(&entry_for_worker, settlement).await;
        });

        self.persist_snapshot(&entry);
        self.emit_task_started(self.to_info(&entry));
        task_id
    }

    pub fn get_task(&self, task_id: &str) -> Option<BackgroundTaskInfo> {
        self.tasks
            .lock()
            .unwrap()
            .get(task_id)
            .map(|e| self.to_info(e))
    }

    pub fn list(&self, active_only: bool, limit: Option<usize>) -> Vec<BackgroundTaskInfo> {
        let mut out = Vec::new();
        for entry in self.tasks.lock().unwrap().values() {
            let info = self.to_info(entry);
            if active_only && is_background_task_terminal(info.status) {
                continue;
            }
            out.push(info);
            if let Some(l) = limit {
                if out.len() >= l {
                    return out;
                }
            }
        }
        if !active_only {
            for info in self.ghosts.lock().unwrap().values() {
                out.push(info.clone());
                if let Some(l) = limit {
                    if out.len() >= l {
                        return out;
                    }
                }
            }
        }
        out
    }

    pub async fn stop(&self, task_id: &str, reason: Option<String>) -> Option<BackgroundTaskInfo> {
        let entry = self.tasks.lock().unwrap().get(task_id)?.clone();
        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return Some(self.to_info(&entry));
        }
        let trimmed = reason.as_ref().and_then(|r| {
            let t = r.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        });
        *entry.stop_reason.lock().unwrap() = trimmed;
        if let Some(tx) = entry.stop_tx.lock().unwrap().take() {
            let _ = tx.send(true);
        }

        let graceful = tokio::time::timeout(
            Duration::from_millis(SIGTERM_GRACE_MS),
            self.wait_internal(&entry),
        )
        .await
        .is_ok();

        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return Some(self.to_info(&entry));
        }
        if !graceful {
            // No per-task forceStop in current trait; rely on process-level kill already sent.
        }
        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return Some(self.to_info(&entry));
        }
        self.settle_task(
            &entry,
            crate::background::types::BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Killed,
                stop_reason: entry.stop_reason.lock().unwrap().clone(),
            },
        )
        .await;
        Some(self.to_info(&entry))
    }

    pub async fn stop_all(&self, reason: Option<String>) -> Vec<BackgroundTaskInfo> {
        let ids: Vec<String> = self.tasks.lock().unwrap().keys().cloned().collect();
        let mut out = Vec::new();
        for id in ids {
            if let Some(info) = self.stop(&id, reason.clone()).await {
                out.push(info);
            }
        }
        out
    }

    pub async fn wait(&self, task_id: &str, timeout: Duration) -> Option<BackgroundTaskInfo> {
        let entry = self.tasks.lock().unwrap().get(task_id)?.clone();
        let _ = tokio::time::timeout(timeout, self.wait_internal(&entry)).await;
        Some(self.to_info(&entry))
    }

    async fn wait_internal(&self, entry: &Arc<ManagedTask>) {
        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        entry.waiters.lock().unwrap().push(tx);
        let _ = rx.await;
    }

    async fn settle_task(
        &self,
        entry: &Arc<ManagedTask>,
        settlement: crate::background::types::BackgroundTaskSettlement,
    ) {
        let mut status = entry.status.lock().unwrap();
        if is_background_task_terminal(*status) {
            return;
        }
        *status = settlement.status;
        *entry.finished_at.lock().unwrap() = Some(Utc::now());
        if settlement.stop_reason.is_some() {
            *entry.stop_reason.lock().unwrap() = settlement.stop_reason;
        }
        drop(status);
        self.persist_snapshot(entry);
        self.fire_terminal_effects(entry);
        let waiters: Vec<_> = entry.waiters.lock().unwrap().drain(..).collect();
        for w in waiters {
            let _ = w.send(());
        }
    }

    fn fire_terminal_effects(&self, entry: &ManagedTask) {
        let mut fired = entry.terminal_fired.lock().unwrap();
        if *fired {
            return;
        }
        *fired = true;
        let info = self.to_info(entry);
        let _ = self.notify_background_task(info.clone());
        self.emit_task_terminated(info);
    }

    fn emit_task_started(&self, info: BackgroundTaskInfo) {
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::BackgroundTaskStarted { info: info.clone() });
        self.agent.telemetry().track(
            "background_task_created",
            serde_json::json!({ "kind": match info.kind {
                BackgroundTaskKind::Process => serde_json::Value::String("bash".into()),
                _ => serde_json::to_value(&info.kind).unwrap(),
            } }),
        );
    }

    fn emit_task_terminated(&self, info: BackgroundTaskInfo) {
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::BackgroundTaskTerminated { info: info.clone() });
        let duration = info
            .finished_at
            .map(|f| f.timestamp_millis() - info.started_at.timestamp_millis());
        self.agent.telemetry().track(
            "background_task_completed",
            serde_json::json!({ "kind": info.kind, "duration": duration, "status": info.status }),
        );
    }

    fn persist_snapshot(&self, entry: &ManagedTask) {
        if let Some(ref tx) = self.persist_tx {
            let _ = tx.send(self.to_info(entry));
        }
    }

    pub async fn load_from_disk(&self) {
        let Some(ref p) = self.persistence else {
            return;
        };
        let mut ghosts = self.ghosts.lock().unwrap();
        ghosts.clear();
        match p.list_tasks().await {
            Ok(tasks) => {
                for info in tasks {
                    if self
                        .tasks
                        .lock()
                        .unwrap()
                        .contains_key(&info.id.to_string())
                    {
                        continue;
                    }
                    ghosts.insert(info.id.to_string(), info);
                }
            }
            Err(_) => {}
        }
    }

    pub async fn reconcile(&self) {
        let mut lost = Vec::new();
        {
            let ghosts = self.ghosts.lock().unwrap();
            for (id, info) in ghosts.iter() {
                if is_background_task_terminal(info.status) {
                    continue;
                }
                let mut updated = info.clone();
                updated.status = BackgroundTaskStatus::Lost;
                updated.finished_at = Some(updated.finished_at.unwrap_or_else(Utc::now));
                lost.push((id.clone(), updated));
            }
        }
        for (id, info) in lost {
            self.ghosts.lock().unwrap().insert(id, info.clone());
            if let Some(ref p) = self.persistence {
                let _ = p.write_task(&info).await;
            }
            self.emit_task_terminated(info);
        }
        for info in self.list(false, None) {
            if is_background_task_terminal(info.status) {
                let _ = self.restore_background_task_notification(info);
            }
        }
    }

    pub fn mark_delivered_notification(&self, origin: &PromptOrigin) {
        if let PromptOrigin::BackgroundTask {
            notification_id, ..
        } = origin
        {
            self.delivered_notifications
                .lock()
                .unwrap()
                .insert(notification_id.clone());
        }
    }

    fn to_info(&self, entry: &ManagedTask) -> BackgroundTaskInfo {
        let status = *entry.status.lock().unwrap();
        let snapshot = entry.sink.snapshot();
        BackgroundTaskInfo {
            id: BackgroundTaskId::new(entry.task_id.clone()),
            kind: entry.kind,
            description: entry.description.clone(),
            status,
            started_at: entry.started_at,
            finished_at: *entry.finished_at.lock().unwrap(),
            stop_reason: entry.stop_reason.lock().unwrap().clone(),
            command: (entry.kind == BackgroundTaskKind::Process).then(|| entry.description.clone()),
            pid: None,
            exit_code: None,
            output_snapshot: Some(snapshot.clone()).filter(|s| !s.is_empty()),
            question_count: None,
            tool_call_id: None,
            agent_id: (entry.kind == BackgroundTaskKind::Agent).then(|| entry.task_id.clone()),
            subagent_type: None,
            terminal_notification_suppressed: Some(
                *entry.terminal_notification_suppressed.lock().unwrap(),
            ),
            timeout_ms: None,
        }
    }

    fn notify_background_task(&self, info: BackgroundTaskInfo) -> Option<()> {
        let context = self.build_notification_context(info)?;
        self.turn_flow.steer(context.content, context.origin);
        self.fire_notification_hook(context.notification);
        Some(())
    }

    fn restore_background_task_notification(&self, info: BackgroundTaskInfo) -> Option<()> {
        let context = self.build_notification_context(info)?;
        self.agent
            .context()
            .append_user_message(context.content, context.origin);
        self.fire_notification_hook(context.notification);
        Some(())
    }

    fn build_notification_context(&self, info: BackgroundTaskInfo) -> Option<NotificationContext> {
        if self.is_terminal_notification_suppressed(&info.id.to_string()) {
            return None;
        }
        let status_str = serde_json::to_value(&info.status)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let notification_id = format!(
            "task:{}:{}",
            info.id,
            serde_json::to_value(&info.status).unwrap()
        );
        let origin = PromptOrigin::BackgroundTask {
            task_id: info.id.to_string(),
            status: status_str,
            notification_id: notification_id.clone(),
        };
        let key = notification_key(&origin);
        if self.scheduled_notifications.lock().unwrap().contains(&key) {
            return None;
        }
        if self.delivered_notifications.lock().unwrap().contains(&key) {
            return None;
        }
        self.scheduled_notifications.lock().unwrap().insert(key);
        if self.is_terminal_notification_suppressed(&info.id.to_string()) {
            return None;
        }

        let snapshot = self.get_output_snapshot_sync(&info.id.to_string(), NOTIFICATION_TAIL_BYTES);
        let severity = if info.status == BackgroundTaskStatus::Completed {
            "info"
        } else {
            "warning"
        };
        let body = build_notification_body(&info);
        let notification = MapBuilder::new()
            .insert("id", notification_id.clone())
            .insert("category", "task")
            .insert(
                "type",
                format!("task.{}", serde_json::to_value(&info.status).unwrap()),
            )
            .insert("source_kind", "background_task")
            .insert("source_id", info.id.to_string())
            .insert_opt("agent_id", info.agent_id.clone())
            .insert(
                "title",
                format!(
                    "Background {} {}",
                    serde_json::to_value(&info.kind).unwrap(),
                    serde_json::to_value(&info.status).unwrap()
                ),
            )
            .insert("severity", severity)
            .insert("body", body)
            .insert("tail_output", snapshot.preview)
            .build();
        let xml = crate::context::notification_xml::render_notification_xml(&notification);
        Some(NotificationContext {
            content: vec![kosong_rs::message::ContentPart::Text { text: xml }],
            origin,
            notification,
        })
    }

    fn fire_notification_hook(&self, notification: Map<String, Value>) {
        let input = serde_json::json!({
            "sink": "context",
            "notificationType": notification.get("type").and_then(|v| v.as_str()).unwrap_or(""),
            "title": notification.get("title").and_then(|v| v.as_str()).unwrap_or(""),
            "body": notification.get("body").and_then(|v| v.as_str()).unwrap_or(""),
            "severity": notification.get("severity").and_then(|v| v.as_str()).unwrap_or(""),
            "sourceKind": notification.get("source_kind").and_then(|v| v.as_str()).unwrap_or(""),
            "sourceId": notification.get("source_id").and_then(|v| v.as_str()).unwrap_or(""),
        });
        if let Some(hooks) = self.agent.hooks() {
            hooks.fire_and_forget_trigger("Notification", input);
        }
    }

    fn is_terminal_notification_suppressed(&self, task_id: &str) -> bool {
        self.tasks
            .lock()
            .unwrap()
            .get(task_id)
            .map(|e| *e.terminal_notification_suppressed.lock().unwrap())
            .unwrap_or_else(|| {
                self.ghosts
                    .lock()
                    .unwrap()
                    .get(task_id)
                    .and_then(|g| g.terminal_notification_suppressed)
                    .unwrap_or(false)
            })
    }

    pub async fn get_output_snapshot(
        &self,
        task_id: &str,
        max_preview_bytes: u64,
    ) -> BackgroundTaskOutputSnapshot {
        if self.get_task(task_id).is_none() {
            return empty_output_snapshot();
        }
        self.get_output_snapshot_sync(task_id, max_preview_bytes)
    }

    fn get_output_snapshot_sync(
        &self,
        task_id: &str,
        max_preview_bytes: u64,
    ) -> BackgroundTaskOutputSnapshot {
        let preview_limit = std::cmp::max(0, max_preview_bytes as usize);
        if let Some(ref p) = self.persistence {
            let path = p.task_output_file(task_id);
            if path.exists() {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let preview_offset =
                    std::cmp::max(0, size.saturating_sub(preview_limit as u64)) as usize;
                let preview_bytes = (size as usize) - preview_offset;
                let bytes = std::fs::read(&path).unwrap_or_default();
                let preview =
                    String::from_utf8_lossy(&bytes[preview_offset..preview_offset + preview_bytes])
                        .to_string();
                return BackgroundTaskOutputSnapshot {
                    output_path: Some(path),
                    output_size_bytes: size as usize,
                    preview_bytes,
                    truncated: preview_offset > 0,
                    full_output_available: true,
                    preview,
                };
            }
        }
        if let Some(entry) = self.tasks.lock().unwrap().get(task_id) {
            let available = entry.sink.snapshot();
            let bytes = available.as_bytes();
            let preview_bytes = std::cmp::min(preview_limit, bytes.len());
            let preview_offset = bytes.len() - preview_bytes;
            return BackgroundTaskOutputSnapshot {
                output_path: None,
                output_size_bytes: bytes.len(),
                preview_bytes,
                truncated: entry.sink.snapshot().as_bytes().len() > preview_bytes,
                full_output_available: false,
                preview: String::from_utf8_lossy(&bytes[preview_offset..]).to_string(),
            };
        }
        empty_output_snapshot()
    }

    pub async fn suppress_terminal_notification(&self, task_id: &str) {
        if let Some(entry) = self.tasks.lock().unwrap().get(task_id) {
            *entry.terminal_notification_suppressed.lock().unwrap() = true;
            self.persist_snapshot(entry);
        }
    }

    fn clone_shallow(&self) -> Self {
        Self {
            agent: self.agent.clone(),
            turn_flow: self.turn_flow.clone(),
            persistence: self.persistence.clone(),
            tasks: Mutex::new(std::collections::HashMap::new()),
            ghosts: Mutex::new(std::collections::HashMap::new()),
            scheduled_notifications: Mutex::new(HashSet::new()),
            delivered_notifications: Mutex::new(HashSet::new()),
            id_generator: Mutex::new(Arc::new(default_id_generator)),
            persist_tx: self.persist_tx.clone(),
        }
    }
}

fn default_id_generator(prefix: &str) -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    let suffix: String = (0..8)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect();
    format!("{prefix}-{suffix}")
}

fn notification_key(origin: &PromptOrigin) -> String {
    match origin {
        PromptOrigin::BackgroundTask {
            task_id,
            status,
            notification_id,
        } => format!("{}\0{}\0{}", task_id, status, notification_id),
        _ => String::new(),
    }
}

fn build_notification_body(info: &BackgroundTaskInfo) -> String {
    let status_str = serde_json::to_value(&info.status)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let base = if status_str == "timed_out" {
        format!("{} timed out.", info.description)
    } else if let Some(ref reason) = info.stop_reason {
        let verb = if status_str == "killed" {
            "was killed"
        } else {
            &status_str
        };
        format!("{} {}: {}.", info.description, verb, reason)
    } else {
        format!("{} {}.", info.description, status_str)
    };
    if info.kind != BackgroundTaskKind::Agent || status_str == "completed" {
        return base;
    }
    let id_str = info.id.to_string();
    let agent_id = info.agent_id.as_deref().unwrap_or(&id_str);
    if agent_id == id_str {
        return base;
    }
    format!(
        "{}\n\nTo recover or continue this subagent, call Agent(resume=\"{}\", prompt=\"Pick up where you left off; redo the last tool call if its result was never observed.\").\nUse agent_id (\"{}\"), NOT source_id / task_id (\"{}\") — the two look alike but only agent_id is accepted by the resume parameter.",
        base, agent_id, agent_id, info.id
    )
}

struct NotificationContext {
    content: Vec<kosong_rs::message::ContentPart>,
    origin: PromptOrigin,
    notification: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct BackgroundTaskOutputSnapshot {
    pub output_path: Option<PathBuf>,
    pub output_size_bytes: usize,
    pub preview_bytes: usize,
    pub truncated: bool,
    pub full_output_available: bool,
    pub preview: String,
}

fn empty_output_snapshot() -> BackgroundTaskOutputSnapshot {
    BackgroundTaskOutputSnapshot {
        output_path: None,
        output_size_bytes: 0,
        preview_bytes: 0,
        truncated: false,
        full_output_available: false,
        preview: String::new(),
    }
}

struct MapBuilder(Map<String, Value>);

impl MapBuilder {
    fn new() -> Self {
        Self(Map::new())
    }
    fn insert(mut self, k: &str, v: impl Into<Value>) -> Self {
        self.0.insert(k.into(), v.into());
        self
    }
    fn insert_opt(mut self, k: &str, v: Option<impl Into<Value>>) -> Self {
        if let Some(v) = v {
            self.0.insert(k.into(), v.into());
        }
        self
    }
    fn build(self) -> Map<String, Value> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::tasks::ProcessBackgroundTask;
    use crate::background::types::BackgroundTaskStatus;
    use crate::turn::fixture_agent::FixtureAgent;
    use crate::turn::TurnFlow;
    use kaos_rs::environment::Environment;
    use kaos_rs::kaos::Kaos;
    use std::sync::Arc;
    use std::time::Duration;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        }
    }

    fn fixture_setup() -> (Arc<FixtureAgent>, Arc<TurnFlow>) {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = Arc::new(TurnFlow::new(agent.clone()));
        (agent, flow)
    }

    #[tokio::test]
    async fn manager_registers_process_and_emits_events() {
        let (agent, flow) = fixture_setup();
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let mut manager = BackgroundManager::new(agent.clone(), flow.clone(), None);
        manager.set_id_generator(Arc::new(|_| "bash-12345678".to_string()));

        let task = Box::new(
            ProcessBackgroundTask::new(kaos, vec!["/bin/echo", "-n", "hi"]).with_id(
                crate::background::types::BackgroundTaskId::new("bash-12345678"),
            ),
        );
        let id = manager.register_task(task);
        assert_eq!(id, "bash-12345678");

        let info = manager.wait(&id, Duration::from_secs(5)).await.unwrap();
        assert_eq!(info.status, BackgroundTaskStatus::Completed);

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events.iter().any(|e| matches!(
            e,
            crate::turn::types::AgentEvent::BackgroundTaskStarted { .. }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            crate::turn::types::AgentEvent::BackgroundTaskTerminated { .. }
        )));
    }

    #[tokio::test]
    async fn manager_stop_kills_long_running_task() {
        let (agent, flow) = fixture_setup();
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let mut manager = BackgroundManager::new(agent.clone(), flow.clone(), None);
        manager.set_id_generator(Arc::new(|_| "bash-87654321".to_string()));

        let task = Box::new(
            ProcessBackgroundTask::new(kaos, vec!["/bin/sleep", "30"]).with_id(
                crate::background::types::BackgroundTaskId::new("bash-87654321"),
            ),
        );
        let id = manager.register_task(task);

        tokio::time::sleep(Duration::from_millis(50)).await;
        let info = manager
            .stop(&id, Some("user request".into()))
            .await
            .unwrap();
        assert!(matches!(
            info.status,
            BackgroundTaskStatus::Killed | BackgroundTaskStatus::Completed
        ));
    }
}
