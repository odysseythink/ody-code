use crate::background::types::BackgroundTaskInfo;
use crate::persist::per_id_json_store::PerIdJsonStore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedTaskInfo {
    #[serde(flatten)]
    pub info: BackgroundTaskInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct LegacyBackgroundTaskInfo {
    task_id: String,
    command: String,
    description: String,
    pid: u32,
    started_at: i64,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    status: String,
    timed_out: Option<bool>,
    stop_reason: Option<String>,
    timeout_ms: Option<u64>,
    agent_id: Option<String>,
    subagent_type: Option<String>,
}

pub struct BackgroundTaskPersistence {
    store: PerIdJsonStore<PersistedTaskInfo>,
    session_dir: PathBuf,
}

fn valid_task_id(id: &str) -> bool {
    // Matches TS: /^[a-z0-9]+(?:-[a-z0-9]+)*-[0-9a-z]{8}$/
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() < 2 {
        return false;
    }
    let suffix = parts.last().unwrap();
    if suffix.len() != 8 || !suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
}

impl BackgroundTaskPersistence {
    pub fn new(session_dir: PathBuf) -> Self {
        let store = PerIdJsonStore::new(session_dir.join("tasks"));
        Self { store, session_dir }
    }

    pub async fn write_task(&self, info: &BackgroundTaskInfo) -> std::io::Result<()> {
        let id_str = info.id.to_string();
        self.store
            .write(&id_str, &PersistedTaskInfo { info: info.clone() })
            .await
    }

    pub async fn read_task(&self, id: &str) -> std::io::Result<Option<BackgroundTaskInfo>> {
        match self.store.read(id).await? {
            Some(p) => Ok(Some(p.info)),
            None => {
                // Fallback: try reading raw file as legacy snake_case.
                let path = self.store.base_dir().join(format!("{id}.json"));
                match tokio::fs::read(&path).await {
                    Ok(bytes) => match serde_json::from_slice::<LegacyBackgroundTaskInfo>(&bytes) {
                        Ok(legacy) => Ok(Some(normalize_legacy(legacy))),
                        Err(_) => Ok(None),
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub async fn list_tasks(&self) -> std::io::Result<Vec<BackgroundTaskInfo>> {
        let mut out = Vec::new();
        for p in self.store.list().await? {
            out.push(p.info);
        }
        Ok(out)
    }

    fn task_output_dir(&self, id: &str) -> PathBuf {
        if !valid_task_id(id) {
            panic!("Invalid task id: {id}");
        }
        self.session_dir.join("tasks").join(id)
    }

    pub fn task_output_file(&self, id: &str) -> PathBuf {
        self.task_output_dir(id).join("output.log")
    }

    pub async fn append_task_output(&self, id: &str, chunk: &str) -> std::io::Result<()> {
        let path = self.task_output_file(id);
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?
            .write_all(chunk.as_bytes())
            .await?;
        Ok(())
    }

    pub async fn task_output_size_bytes(&self, id: &str) -> std::io::Result<u64> {
        match tokio::fs::metadata(self.task_output_file(id)).await {
            Ok(m) => Ok(m.len()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e),
        }
    }

    pub async fn task_output_exists(&self, id: &str) -> bool {
        tokio::fs::try_exists(self.task_output_file(id))
            .await
            .unwrap_or(false)
    }

    pub async fn read_task_output_bytes(
        &self,
        id: &str,
        offset: u64,
        max_bytes: u64,
    ) -> std::io::Result<String> {
        let path = self.task_output_file(id);
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(e) => return Err(e),
        };
        let start = std::cmp::min(offset as usize, bytes.len());
        let end = std::cmp::min(start + max_bytes as usize, bytes.len());
        Ok(String::from_utf8_lossy(&bytes[start..end]).to_string())
    }
}

fn normalize_legacy(legacy: LegacyBackgroundTaskInfo) -> BackgroundTaskInfo {
    use crate::background::types::{BackgroundTaskId, BackgroundTaskKind, BackgroundTaskStatus};
    use chrono::{TimeZone, Utc};

    let status = match legacy.status.as_str() {
        "running" => BackgroundTaskStatus::Running,
        "completed" => BackgroundTaskStatus::Completed,
        "failed" if legacy.timed_out == Some(true) => BackgroundTaskStatus::TimedOut,
        "failed" => BackgroundTaskStatus::Failed,
        "killed" => BackgroundTaskStatus::Killed,
        "lost" => BackgroundTaskStatus::Lost,
        _ => BackgroundTaskStatus::Failed,
    };
    let started_at = Utc
        .timestamp_millis_opt(legacy.started_at)
        .single()
        .unwrap_or_else(Utc::now);
    let finished_at = legacy
        .ended_at
        .and_then(|t| Utc.timestamp_millis_opt(t).single());

    let (kind, command, pid, agent_id, subagent_type) = if legacy.task_id.starts_with("agent-") {
        (
            BackgroundTaskKind::Agent,
            None,
            None,
            legacy.agent_id,
            legacy.subagent_type,
        )
    } else {
        (
            BackgroundTaskKind::Process,
            Some(legacy.command),
            Some(legacy.pid),
            None,
            None,
        )
    };

    BackgroundTaskInfo {
        id: BackgroundTaskId::new(legacy.task_id),
        kind,
        description: legacy.description,
        status,
        started_at,
        finished_at,
        stop_reason: legacy.stop_reason.filter(|s| !s.trim().is_empty()),
        command,
        pid,
        exit_code: legacy.exit_code,
        output_snapshot: None,
        question_count: None,
        tool_call_id: None,
        agent_id,
        subagent_type,
        terminal_notification_suppressed: None,
        timeout_ms: legacy.timeout_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::types::{
        BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskKind, BackgroundTaskStatus,
    };
    use chrono::Utc;
    use tempfile::TempDir;

    fn sample_info(id: &str) -> BackgroundTaskInfo {
        BackgroundTaskInfo {
            id: BackgroundTaskId::new(id),
            kind: BackgroundTaskKind::Process,
            description: "echo hi".into(),
            status: BackgroundTaskStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            stop_reason: None,
            command: Some("echo hi".into()),
            pid: Some(1234),
            exit_code: None,
            output_snapshot: None,
            question_count: None,
            tool_call_id: None,
            agent_id: None,
            subagent_type: None,
            terminal_notification_suppressed: None,
            timeout_ms: None,
        }
    }

    #[tokio::test]
    async fn persistence_round_trip_and_output_log() {
        let dir = TempDir::new().unwrap();
        let p = BackgroundTaskPersistence::new(dir.path().to_path_buf());

        p.write_task(&sample_info("bash-12345678")).await.unwrap();
        p.append_task_output("bash-12345678", "line1\n")
            .await
            .unwrap();
        p.append_task_output("bash-12345678", "line2\n")
            .await
            .unwrap();

        let all = p.list_tasks().await.unwrap();
        assert_eq!(all.len(), 1);

        let one = p.read_task("bash-12345678").await.unwrap();
        assert!(one.is_some());
        assert_eq!(one.unwrap().description, "echo hi");

        assert_eq!(p.task_output_size_bytes("bash-12345678").await.unwrap(), 12);
        let tail = p
            .read_task_output_bytes("bash-12345678", 6, 100)
            .await
            .unwrap();
        assert_eq!(tail, "line2\n");
    }
}
