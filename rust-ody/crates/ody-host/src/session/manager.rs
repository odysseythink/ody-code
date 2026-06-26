use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

fn new_id() -> String {
    Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
}

use crate::session::store::{IndexEntry, SessionError, SessionState, SessionStoreAdapter, SessionSummary};

#[derive(Debug, Default, Clone)]
pub struct SessionFilter {
    pub work_dir: Option<String>,
    pub session_id: Option<String>,
}

pub struct SessionManager {
    store: SessionStoreAdapter,
    active: RwLock<HashMap<String, Arc<Session>>>,
}

#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub work_dir: std::path::PathBuf,
    pub dir: std::path::PathBuf,
    state: tokio::sync::Mutex<SessionState>,
}

impl Session {
    pub async fn model(&self) -> Option<String> {
        self.state.lock().await.model.clone()
    }

    pub async fn thinking(&self) -> Option<String> {
        self.state.lock().await.thinking.clone()
    }

    pub async fn permission(&self) -> Option<String> {
        self.state.lock().await.permission.clone()
    }

    pub async fn set_model(&self, model: Option<String>) {
        self.state.lock().await.model = model;
    }

    pub async fn set_thinking(&self, thinking: Option<String>) {
        self.state.lock().await.thinking = thinking;
    }

    pub async fn set_permission(&self, permission: Option<String>) {
        self.state.lock().await.permission = permission;
    }

    pub async fn persist_state(&self) -> Result<(), SessionError> {
        let state = self.state.lock().await.clone();
        crate::session::store::write_state_json(&self.dir, &state)
            .map_err(|e| SessionError::Io { source: e, path: self.dir.clone() })
    }
}

impl SessionManager {
    pub fn new(store: SessionStoreAdapter) -> Self {
        Self { store, active: RwLock::new(HashMap::new()) }
    }

    pub async fn create(&self, work_dir: &Path, title: Option<&str>) -> Result<SessionSummary, SessionError> {
        let id = new_id();
        self.create_with_id(&id, work_dir, title).await
    }

    pub async fn create_with_id(&self, id: &str, work_dir: &Path, title: Option<&str>) -> Result<SessionSummary, SessionError> {
        let dir = self.store.session_dir_for(id, work_dir)?;
        if dir.exists() {
            return Err(SessionError::AlreadyExists { session_id: id.to_string() });
        }
        let index = self.store.read_index()?;
        if index.contains_key(id) {
            return Err(SessionError::AlreadyExists { session_id: id.to_string() });
        }
        std::fs::create_dir_all(&dir).map_err(|e| SessionError::Io { source: e, path: dir.clone() })?;
        let state = SessionState {
            title: title.map(|s| s.to_string()),
            last_prompt: None,
            custom: HashMap::new(),
            model: None,
            thinking: None,
            permission: None,
        };
        crate::session::store::write_state_json(&dir, &state)
            .map_err(|e| SessionError::Io { source: e, path: dir.clone() })?;
        let normalized = crate::session::store::normalize_work_dir(work_dir);
        self.store.append_index(IndexEntry {
            session_id: id.to_string(),
            session_dir: dir.clone(),
            work_dir: normalized.clone(),
        })?;
        let summary = self.store.summary_from_dir(id.to_string(), &dir, &normalized)?;
        let session = Arc::new(Session {
            id: id.to_string(),
            work_dir: normalized,
            dir: dir.clone(),
            state: tokio::sync::Mutex::new(state),
        });
        self.active.write().await.insert(id.to_string(), session);
        Ok(summary)
    }

    pub async fn list(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, SessionError> {
        let index = self.store.read_index()?;
        let mut summaries = Vec::new();
        for (id, entry) in index {
            if let Some(wd) = &filter.work_dir {
                if entry.work_dir != crate::session::store::normalize_work_dir(Path::new(wd)) {
                    continue;
                }
            }
            if let Some(sid) = &filter.session_id {
                if &id != sid { continue; }
            }
            if !entry.session_dir.exists() { continue; }
            summaries.push(self.store.summary_from_dir(id, &entry.session_dir, &entry.work_dir)?);
        }
        summaries.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        Ok(summaries)
    }

    pub async fn get(&self, id: String) -> Result<Arc<Session>, SessionError> {
        {
            let active = self.active.read().await;
            if let Some(s) = active.get(&id) {
                return Ok(Arc::clone(s));
            }
        }
        let index = self.store.read_index()?;
        let entry = index.get(&id).cloned().ok_or_else(|| SessionError::NotFound { session_id: id.clone() })?;
        if !entry.session_dir.exists() {
            return Err(SessionError::NotFound { session_id: id });
        }
        let state = crate::session::store::read_state_json(&entry.session_dir)
            .map_err(|e| SessionError::Io { source: e, path: entry.session_dir.clone() })?
            .unwrap_or_default();
        let session = Arc::new(Session {
            id: id.clone(),
            work_dir: entry.work_dir.clone(),
            dir: entry.session_dir.clone(),
            state: tokio::sync::Mutex::new(state),
        });
        self.active.write().await.insert(id, Arc::clone(&session));
        Ok(session)
    }

    pub async fn close(&self, id: String) -> Result<(), SessionError> {
        self.active.write().await.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn create_then_list_returns_session() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(SessionStoreAdapter::new(tmp.path().to_path_buf()));
        let summary = manager.create(Path::new("/tmp/wd"), Some("t")).await.unwrap();
        let list = manager.list(SessionFilter::default()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, summary.id);
        assert_eq!(list[0].title, Some("t".to_string()));
    }

    #[tokio::test]
    async fn duplicate_id_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(SessionStoreAdapter::new(tmp.path().to_path_buf()));
        let summary = manager.create(Path::new("/tmp/wd"), None).await.unwrap();
        let err = manager.create_with_id(&summary.id, Path::new("/tmp/wd"), None).await.unwrap_err();
        assert!(matches!(err, SessionError::AlreadyExists { .. }));
    }

    #[tokio::test]
    async fn close_removes_active_but_keeps_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(SessionStoreAdapter::new(tmp.path().to_path_buf()));
        let summary = manager.create(Path::new("/tmp/wd"), None).await.unwrap();
        manager.close(summary.id.clone()).await.unwrap();
        // Session dir stays on disk after close
        assert!(summary.session_dir.exists());
        // get() reloads from disk since close only affects the active cache
        let reloaded = manager.get(summary.id.clone()).await.unwrap();
        assert_eq!(reloaded.id, summary.id);
    }
}
