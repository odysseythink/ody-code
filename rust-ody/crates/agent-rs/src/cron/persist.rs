use crate::cron::task::CronTask;
use crate::persist::per_id_json_store::PerIdJsonStore;
use std::io;
use std::path::PathBuf;

#[derive(Clone)]
pub struct CronTaskPersistence {
    store: PerIdJsonStore<CronTask>,
    id_regex: regex::Regex,
}

impl CronTaskPersistence {
    pub fn new(session_dir: PathBuf) -> Self {
        let base = session_dir.join("cron");
        Self {
            store: PerIdJsonStore::new(base),
            id_regex: regex::Regex::new(r"^[0-9a-f]{8}$").unwrap(),
        }
    }

    pub async fn write(&self, task: &CronTask) -> io::Result<()> {
        self.validate_id(&task.id)?;
        self.store.write(&task.id, task).await
    }

    pub async fn read(&self, id: &str) -> io::Result<Option<CronTask>> {
        self.validate_id(id)?;
        Ok(self.store.read(id).await.ok().flatten())
    }

    pub async fn list(&self) -> io::Result<Vec<CronTask>> {
        let mut out = Vec::new();
        let ids = self.list_ids().await?;
        for id in ids {
            if let Some(task) = self.read(&id).await? {
                out.push(task);
            }
        }
        Ok(out)
    }

    pub async fn remove(&self, id: &str) -> io::Result<()> {
        self.validate_id(id)?;
        self.store.remove(id).await
    }

    fn validate_id(&self, id: &str) -> io::Result<()> {
        if self.id_regex.is_match(id) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid cron job id: {}", id),
            ))
        }
    }

    async fn list_ids(&self) -> io::Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut entries = tokio::fs::read_dir(self.store.base_dir()).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".json") {
                if self.id_regex.is_match(id) {
                    ids.push(id.to_string());
                }
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample(id: &str) -> CronTask {
        CronTask {
            id: id.to_string(),
            cron: "0 9 * * *".to_string(),
            prompt: "hi".to_string(),
            created_at: 1,
            recurring: Some(true),
            last_fired_at: None,
        }
    }

    #[tokio::test]
    async fn persistence_round_trip() {
        let dir = TempDir::new().unwrap();
        let p = CronTaskPersistence::new(dir.path().to_path_buf());
        p.write(&sample("deadbeef")).await.unwrap();
        let all = p.list().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].prompt, "hi");

        p.remove("deadbeef").await.unwrap();
        let all = p.list().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn persistence_skips_invalid_basename() {
        let dir = TempDir::new().unwrap();
        let p = CronTaskPersistence::new(dir.path().to_path_buf());
        p.write(&sample("deadbeef")).await.unwrap();
        tokio::fs::write(dir.path().join("cron/not-an-id.json"), b"{}")
            .await
            .unwrap();
        let all = p.list().await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
