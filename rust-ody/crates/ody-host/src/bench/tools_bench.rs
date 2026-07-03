use std::time::Instant;

use agent_rs::agent_loop::types::{
    ExecutableTool, ExecutableToolContext, ExecutableToolResult, RunnableToolExecution, ToolExecution,
};
use agent_rs::records::nested::{ExecutableToolOutput, ExecutableToolSuccessResult};
use kosong_rs::provider::AbortSignal;
use serde_json::json;

struct NoOpTool;

#[async_trait::async_trait]
impl ExecutableTool for NoOpTool {
    fn name(&self) -> &str {
        "NoOp"
    }

    fn description(&self) -> &str {
        "No-op tool for latency measurement"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "value": { "type": "string" } },
            "required": ["value"]
        })
    }

    async fn resolve_execution(
        &self,
        _input: serde_json::Value,
    ) -> Result<ToolExecution, anyhow::Error> {
        Ok(ToolExecution::Runnable(RunnableToolExecution {
            is_error: None,
            accesses: None,
            display: None,
            description: None,
            stop_batch_after_this: None,
            approval_rule: "NoOp".into(),
            matches_rule: None,
            execute: Box::new(|_ctx: ExecutableToolContext| {
                Box::pin(async move {
                    Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                        output: ExecutableToolOutput::Text("ok".into()),
                        is_error: None,
                        stop_turn: None,
                        message: None,
                    }))
                })
            }),
        }))
    }
}

fn percentile(sorted_ns: &[u64], p: f64) -> u64 {
    let idx = ((sorted_ns.len() as f64 - 1.0) * p) as usize;
    sorted_ns[idx.min(sorted_ns.len().saturating_sub(1))]
}

#[tokio::main]
async fn main() {
    const ITERATIONS: usize = 1000;
    let tool = NoOpTool;

    let mut latencies_ns = Vec::with_capacity(ITERATIONS);
    for i in 0..ITERATIONS {
        let ctx = ExecutableToolContext {
            turn_id: "0".to_string(),
            tool_call_id: format!("bench-{}", i),
            signal: AbortSignal::new(),
            metadata: None,
            on_update: None,
        };
        let start = Instant::now();
        let exec = tool
            .resolve_execution(json!({"value": "hello"}))
            .await
            .unwrap();
        match exec {
            ToolExecution::Runnable(r) => {
                let _ = (r.execute)(ctx).await.unwrap();
            }
            ToolExecution::Error(e) => panic!("tool returned error: {:?}", e),
        }
        latencies_ns.push(start.elapsed().as_nanos() as u64);
    }

    latencies_ns.sort();
    let p50 = percentile(&latencies_ns, 0.50);
    let p95 = percentile(&latencies_ns, 0.95);
    let p99 = percentile(&latencies_ns, 0.99);
    let total_ns: u64 = latencies_ns.iter().sum();
    let throughput = ITERATIONS as f64 / (total_ns as f64 / 1_000_000_000.0);

    println!("| metric | value |");
    println!("|---|---|");
    println!("| iterations | {} |", ITERATIONS);
    println!("| total | {} ns |", total_ns);
    println!("| p50 | {} ns |", p50);
    println!("| p95 | {} ns |", p95);
    println!("| p99 | {} ns |", p99);
    println!("| throughput | {:.2} ops/s |", throughput);
}
