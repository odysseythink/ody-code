use crate::turn::types::TurnAgent;
use kosong_rs::provider::AbortSignal;
use std::sync::Mutex;

pub const DEFAULT_SPLIT_PLAN_COMPACTION_RATIO: f64 = 0.5;

#[derive(Debug, Clone, Default)]
pub struct ManifestCounts {
    pub done: usize,
    pub pending: usize,
}

pub fn count_manifest_rows(content: &str) -> Option<ManifestCounts> {
    let rows = scan_manifest_rows(content);
    if rows.is_empty() {
        return None;
    }
    let mut done = 0usize;
    let mut pending = 0usize;
    for row in rows {
        match row.status.as_str() {
            "done" => done += 1,
            "pending" => pending += 1,
            _ => {}
        }
    }
    Some(ManifestCounts { done, pending })
}

#[derive(Debug, Clone)]
struct ManifestRow {
    file: String,
    scope: String,
    status: String,
}

fn scan_manifest_rows(content: &str) -> Vec<ManifestRow> {
    let mut rows = Vec::new();
    for line in content.lines() {
        let cells: Vec<String> = line.split('|').map(|c| c.trim().to_string()).collect();
        let cells: Vec<&str> = cells.iter().map(|c| c.as_str()).collect();
        // Skip empty first cell (leading pipe) and empty last cell (trailing pipe)
        let start = if cells.first().map_or(false, |c| c.is_empty()) {
            1
        } else {
            0
        };
        let end = cells.len()
            - if cells.last().map_or(false, |c| c.is_empty()) {
                1
            } else {
                0
            };
        if end <= start + 3 {
            continue;
        }
        let status = cells[end - 1].to_lowercase();
        if status != "pending" && status != "done" {
            continue;
        }
        let file = cells[start + 1].replace('`', "").trim().to_string();
        if !file.to_lowercase().ends_with(".md") {
            continue;
        }
        let scope = cells[end - 2].to_string();
        rows.push(ManifestRow {
            file,
            scope,
            status,
        });
    }
    rows
}

pub struct SplitPlanCheckpoint {
    last_done_count: Mutex<Option<usize>>,
    last_file_path: Mutex<Option<String>>,
}

impl SplitPlanCheckpoint {
    pub fn new() -> Self {
        Self {
            last_done_count: Mutex::new(None),
            last_file_path: Mutex::new(None),
        }
    }

    pub fn reset(&self) {
        *self.last_done_count.lock().unwrap() = None;
        *self.last_file_path.lock().unwrap() = None;
    }

    pub async fn before_step(
        &self,
        agent: std::sync::Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        let ratio = agent
            .config()
            .loop_control()
            .and_then(|c| c.split_plan_compaction_ratio)
            .unwrap_or(DEFAULT_SPLIT_PLAN_COMPACTION_RATIO);
        let session_mode = agent.session_mode();
        if ratio <= 0.0 || !session_mode.is_active() {
            self.reset();
            return Ok(());
        }

        let file_path = session_mode.file_path();
        {
            let mut last_file_path = self.last_file_path.lock().unwrap();
            if file_path.as_deref() != last_file_path.as_deref() {
                *self.last_done_count.lock().unwrap() = None;
                *last_file_path = file_path.clone();
            }
        }

        let content = match session_mode.data().await {
            Some(data) => data,
            None => return Ok(()),
        };

        let counts = match count_manifest_rows(&content) {
            Some(c) => c,
            None => {
                *self.last_done_count.lock().unwrap() = None;
                return Ok(());
            }
        };

        let crossed_boundary = self
            .last_done_count
            .lock()
            .unwrap()
            .map(|last| counts.done > last)
            .unwrap_or(false);
        let more_pending = counts.pending > 0;
        *self.last_done_count.lock().unwrap() = Some(counts.done);

        if !crossed_boundary || !more_pending {
            return Ok(());
        }

        let max_context_tokens = agent.config().model_capabilities().max_context_tokens;
        if max_context_tokens <= 0 {
            return Ok(());
        }
        if agent.context().token_count_with_pending() as f64 >= max_context_tokens as f64 * ratio {
            let agent_clone = agent.clone();
            agent
                .full_compaction()
                .compact_checkpoint(agent_clone, signal)
                .await?;
        }
        Ok(())
    }
}

impl Default for SplitPlanCheckpoint {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_manifest_rows() {
        let content = "# Plan\n\n## Parts\n\n| # | File | Scope | Status |\n|---|---|---|---|\n| 1 | core.md | models | done |\n| 2 | api.md | endpoints | pending |\n";
        let counts = count_manifest_rows(content).unwrap();
        assert_eq!(counts.done, 1);
        assert_eq!(counts.pending, 1);
    }

    #[test]
    fn no_manifest_returns_none() {
        assert!(count_manifest_rows("# Just a plan\n\nSome text").is_none());
    }

    #[test]
    fn ignores_header_and_separator() {
        let content = "| # | File | Scope | Status |\n|---|---|---|---|\n| 1 | a.md | x | done |\n| 2 | b.md | y | pending |";
        let counts = count_manifest_rows(content).unwrap();
        assert_eq!(counts.done, 1);
        assert_eq!(counts.pending, 1);
    }

    #[test]
    fn file_cell_may_be_backtick_quoted() {
        let content = "| 1 | `core.md` | models | done |\n";
        let counts = count_manifest_rows(content).unwrap();
        assert_eq!(counts.done, 1);
    }
}
