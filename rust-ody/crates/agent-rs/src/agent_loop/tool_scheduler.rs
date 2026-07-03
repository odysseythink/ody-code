use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use crate::agent_loop::tool_access::ToolAccesses;

pub struct ToolCallTask<R> {
    pub accesses: ToolAccesses,
    pub start: Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = R> + Send>> + Send>,
}

struct ScheduledTask<R> {
    id: u64,
    accesses: ToolAccesses,
    result_tx: Option<oneshot::Sender<R>>,
    start: Option<Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = R> + Send>> + Send>>,
}

struct ToolSchedulerInner<R> {
    next_id: u64,
    active: Vec<ScheduledTask<R>>,
    queued: Vec<ScheduledTask<R>>,
}

pub struct ToolScheduler<R> {
    inner: Arc<Mutex<ToolSchedulerInner<R>>>,
}

impl<R: Send + 'static> ToolScheduler<R> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ToolSchedulerInner {
                next_id: 1,
                active: Vec::new(),
                queued: Vec::new(),
            })),
        }
    }

    pub async fn add(
        &mut self,
        task: ToolCallTask<R>,
    ) -> Result<oneshot::Receiver<R>, anyhow::Error> {
        let (tx, rx) = oneshot::channel();

        let scheduled = {
            let mut guard = self.inner.lock().unwrap();
            let id = guard.next_id;
            guard.next_id += 1;
            let scheduled = ScheduledTask {
                id,
                accesses: task.accesses,
                result_tx: Some(tx),
                start: Some(task.start),
            };

            if is_blocked(&scheduled, &guard.active, &guard.queued) {
                guard.queued.push(scheduled);
                return Ok(rx);
            }
            scheduled
        };

        start_task(&self.inner, scheduled);
        Ok(rx)
    }
}

fn is_blocked<R>(
    task: &ScheduledTask<R>,
    active: &[ScheduledTask<R>],
    queued_before: &[ScheduledTask<R>],
) -> bool {
    conflicts_with_any(task, active) || conflicts_with_any(task, queued_before)
}

fn conflicts_with_any<R>(task: &ScheduledTask<R>, candidates: &[ScheduledTask<R>]) -> bool {
    candidates
        .iter()
        .any(|c| ToolAccesses::conflict(&task.accesses, &c.accesses))
}

fn start_task<R: Send + 'static>(
    inner: &Arc<Mutex<ToolSchedulerInner<R>>>,
    mut task: ScheduledTask<R>,
) {
    let id = task.id;
    let start = task.start.take().unwrap();
    let tx = task.result_tx.take();

    {
        let mut guard = inner.lock().unwrap();
        guard.active.push(task);
    }

    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        let result = start().await;
        if let Some(tx) = tx {
            let _ = tx.send(result);
        }
        finish(inner, id);
    });
}

fn finish<R: Send + 'static>(inner: Arc<Mutex<ToolSchedulerInner<R>>>, id: u64) {
    let mut guard = inner.lock().unwrap();
    guard.active.retain(|t| t.id != id);

    let mut still_queued: Vec<ScheduledTask<R>> = Vec::new();
    while let Some(task) = guard.queued.first() {
        if is_blocked(task, &guard.active, &still_queued) {
            still_queued.push(guard.queued.remove(0));
        } else {
            let task = guard.queued.remove(0);
            // start_task will take its own lock; release ours first to avoid deadlock.
            drop(guard);
            start_task(&inner, task);
            guard = inner.lock().unwrap();
        }
    }
    guard.queued = still_queued;
}
