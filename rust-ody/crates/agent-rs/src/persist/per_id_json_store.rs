#[cfg(test)]
use tempfile::TempDir;

use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct PerIdJsonStore<T> {
    base_dir: PathBuf,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> PerIdJsonStore<T> {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            _phantom: std::marker::PhantomData,
        }
    }

    fn path_for(&self, id: &str) -> PathBuf {
        let safe = id.replace(['/', '\\'], "_");
        self.base_dir.join(format!("{safe}.json"))
    }
}

impl<T: Serialize + DeserializeOwned + Send + Sync> PerIdJsonStore<T> {
    pub async fn write(&self, id: &str, value: &T) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.path_for(id);
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(value)?;
        {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        tokio::fs::rename(tmp, path).await?;
        Ok(())
    }

    pub async fn read(&self, id: &str) -> std::io::Result<Option<T>> {
        let path = self.path_for(id);
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let value = serde_json::from_slice(&bytes)?;
                Ok(Some(value))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn remove(&self, id: &str) -> std::io::Result<()> {
        let path = self.path_for(id);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn list(&self) -> std::io::Result<Vec<T>> {
        let mut values = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let bytes = tokio::fs::read(&path).await?;
                let value = serde_json::from_slice(&bytes)?;
                values.push(value);
            }
        }
        Ok(values)
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Task {
        id: String,
        value: i32,
    }

    #[tokio::test]
    async fn per_id_json_store_round_trip() {
        let dir = TempDir::new().unwrap();
        let store = PerIdJsonStore::<Task>::new(dir.path().to_path_buf());

        store
            .write(
                "task-1",
                &Task {
                    id: "task-1".into(),
                    value: 42,
                },
            )
            .await
            .unwrap();
        store
            .write(
                "task-2",
                &Task {
                    id: "task-2".into(),
                    value: 7,
                },
            )
            .await
            .unwrap();

        let all = store.list().await.unwrap();
        assert_eq!(all.len(), 2);

        let one = store.read("task-1").await.unwrap();
        assert!(one.is_some());
        assert_eq!(one.unwrap().value, 42);

        store.remove("task-1").await.unwrap();
        let one = store.read("task-1").await.unwrap();
        assert!(one.is_none());

        let all = store.list().await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
