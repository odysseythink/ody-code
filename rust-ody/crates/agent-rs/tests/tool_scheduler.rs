use agent_rs::agent_loop::tool_access::ToolAccesses;
use agent_rs::agent_loop::tool_scheduler::{ToolCallTask, ToolScheduler};
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread")]
async fn scheduler_runs_non_conflicting_tasks_in_parallel() {
    let mut scheduler = ToolScheduler::new();
    let start = Instant::now();

    let t1 = scheduler
        .add(ToolCallTask {
            accesses: ToolAccesses::read_file("/tmp/a.txt"),
            start: Box::new(|| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok("a".to_string()) as Result<String, anyhow::Error>
                })
            }),
        })
        .await
        .unwrap();

    let t2 = scheduler
        .add(ToolCallTask {
            accesses: ToolAccesses::read_file("/tmp/b.txt"),
            start: Box::new(|| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok("b".to_string())
                })
            }),
        })
        .await
        .unwrap();

    let (r1, r2) = tokio::join!(t1, t2);
    assert_eq!(r1.unwrap().unwrap(), "a");
    assert_eq!(r2.unwrap().unwrap(), "b");
    assert!(
        start.elapsed() < Duration::from_millis(90),
        "parallel expected"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduler_serializes_conflicting_writes() {
    let mut scheduler = ToolScheduler::new();
    let start = Instant::now();

    let t1 = scheduler
        .add(ToolCallTask {
            accesses: ToolAccesses::write_file("/tmp/x.txt"),
            start: Box::new(|| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok("first".to_string()) as Result<String, anyhow::Error>
                })
            }),
        })
        .await
        .unwrap();

    let t2 = scheduler
        .add(ToolCallTask {
            accesses: ToolAccesses::write_file("/tmp/x.txt"),
            start: Box::new(|| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok("second".to_string())
                })
            }),
        })
        .await
        .unwrap();

    let (r1, r2) = tokio::join!(t1, t2);
    assert_eq!(r1.unwrap().unwrap(), "first");
    assert_eq!(r2.unwrap().unwrap(), "second");
    assert!(
        start.elapsed() >= Duration::from_millis(95),
        "serial expected"
    );
}
