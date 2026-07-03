use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type SessionId = String;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_prompt: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub custom: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_records_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_state: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: SessionId,
    pub work_dir: PathBuf,
    pub session_dir: PathBuf,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub title: Option<String>,
    pub last_prompt: Option<String>,
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct SessionStoreAdapter {
    home_dir: PathBuf,
}

#[derive(Debug)]
pub enum SessionError {
    AlreadyExists {
        session_id: SessionId,
    },
    NotFound {
        session_id: SessionId,
    },
    InvalidId {
        session_id: SessionId,
    },
    Io {
        source: std::io::Error,
        path: PathBuf,
    },
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::AlreadyExists { session_id } => {
                write!(f, r#"Session "{session_id}" already exists"#)
            }
            SessionError::NotFound { session_id } => {
                write!(f, r#"Session "{session_id}" was not found"#)
            }
            SessionError::InvalidId { session_id } => write!(
                f,
                r#"Session id "{session_id}" contains unsupported path characters"#
            ),
            SessionError::Io { source, path } => {
                write!(f, "io error at {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for SessionError {}

impl SessionStoreAdapter {
    pub fn new(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.home_dir.join("sessions")
    }

    pub fn session_dir_for(&self, id: &str, work_dir: &Path) -> Result<PathBuf, SessionError> {
        assert_safe_session_id(id)?;
        let work_dir = normalize_work_dir(work_dir);
        let wd_str = work_dir.to_str().unwrap_or("");
        Ok(self
            .sessions_dir()
            .join(encode_work_dir_key(wd_str))
            .join(id))
    }

    pub fn append_index(&self, entry: IndexEntry) -> Result<(), SessionError> {
        let path = self.home_dir.join("session_index.jsonl");
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|e| SessionError::Io {
                source: e,
                path: path.clone(),
            })?;
        writeln!(file, "{}", serde_json::to_string(&entry).unwrap())
            .map_err(|e| SessionError::Io { source: e, path })?;
        Ok(())
    }

    pub fn read_index(&self) -> Result<HashMap<SessionId, IndexEntry>, SessionError> {
        let path = self.home_dir.join("session_index.jsonl");
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(e) => return Err(SessionError::Io { source: e, path }),
        };
        let mut map = HashMap::new();
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: IndexEntry = serde_json::from_str(line).map_err(|e| SessionError::Io {
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
                path: path.clone(),
            })?;
            map.insert(entry.session_id.clone(), entry);
        }
        Ok(map)
    }

    pub fn summary_from_dir(
        &self,
        id: SessionId,
        dir: &Path,
        work_dir: &Path,
    ) -> Result<SessionSummary, SessionError> {
        let dir_stat = fs::metadata(dir).map_err(|e| SessionError::Io {
            source: e,
            path: dir.to_path_buf(),
        })?;
        let state = read_state_json(dir).map_err(|e| SessionError::Io {
            source: e,
            path: dir.to_path_buf(),
        })?;
        let state_mtime = mtime_ms(dir.join("state.json"));
        let wire_mtime = mtime_ms(dir.join("wire.jsonl"));
        let updated_at = [
            dir_stat.modified().ok().map(ts_to_ms),
            state_mtime,
            wire_mtime,
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
        let created_at = dir_stat
            .created()
            .ok()
            .map(ts_to_ms)
            .unwrap_or_else(|| ts_to_ms(std::time::SystemTime::now()));
        let title = state.as_ref().and_then(title_from_state);
        let last_prompt = state.as_ref().and_then(|s| s.last_prompt.clone());
        let metadata = state
            .as_ref()
            .map(|s| s.custom.clone().into_iter().collect());
        Ok(SessionSummary {
            id,
            work_dir: work_dir.to_path_buf(),
            session_dir: dir.to_path_buf(),
            created_at_ms: created_at,
            updated_at_ms: updated_at,
            title,
            last_prompt,
            metadata,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexEntry {
    pub session_id: SessionId,
    pub session_dir: PathBuf,
    pub work_dir: PathBuf,
}

pub fn normalize_work_dir(work_dir: &Path) -> PathBuf {
    work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf())
}

pub fn encode_work_dir_key(work_dir: &str) -> String {
    use std::path::Path as StdPath;
    let normalized = StdPath::new(work_dir)
        .canonicalize()
        .unwrap_or_else(|_| StdPath::new(work_dir).to_path_buf());
    let name = normalized
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let slug = slugify_work_dir_name(name);
    let hash = format!(
        "{:x}",
        Sha256::digest(normalized.to_string_lossy().as_bytes())
    );
    format!("wd_{slug}_{}", &hash[..12.min(hash.len())])
}

fn slugify_work_dir_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.len() > 40 { &slug[..40] } else { slug };
    let slug = slug.trim_matches('-');
    if slug.is_empty() || slug == "." || slug == ".." {
        "workspace".to_string()
    } else {
        slug.to_string()
    }
}

fn assert_safe_session_id(id: &str) -> Result<(), SessionError> {
    if id == "."
        || id == ".."
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(SessionError::InvalidId {
            session_id: id.to_string(),
        });
    }
    Ok(())
}

pub fn read_state_json(dir: &Path) -> Result<Option<SessionState>, std::io::Error> {
    let path = dir.join("state.json");
    match fs::read_to_string(&path) {
        Ok(s) => Ok(serde_json::from_str(&s).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn write_state_json(dir: &Path, state: &SessionState) -> Result<(), std::io::Error> {
    fs::create_dir_all(dir)?;
    let path = dir.join("state.json");
    let mut file = fs::File::create(&path)?;
    file.write_all(serde_json::to_string_pretty(state).unwrap().as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn mtime_ms(path: PathBuf) -> Option<u64> {
    fs::metadata(&path).ok()?.modified().ok().map(ts_to_ms)
}

fn ts_to_ms(t: std::time::SystemTime) -> u64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn title_from_state(state: &SessionState) -> Option<String> {
    state.title.clone().filter(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_work_dir_key_like_ts() {
        let key = encode_work_dir_key("/Users/ranwei/workspace/ody-code");
        assert!(key.starts_with("wd_ody-code_"));
        assert_eq!(key.len(), "wd_ody-code_".len() + 12);
    }

    #[test]
    fn slugifies_special_characters() {
        let key = encode_work_dir_key("/tmp/foo bar!baz");
        assert!(key.starts_with("wd_foo-bar-baz_"));
    }

    #[test]
    fn empty_slug_becomes_workspace() {
        let key = encode_work_dir_key("/");
        assert!(key.starts_with("wd_workspace_"));
    }

    #[test]
    fn state_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let state = SessionState {
            title: Some("hello".to_string()),
            last_prompt: Some("hi".to_string()),
            custom: [("k".to_string(), serde_json::json!(1))]
                .into_iter()
                .collect(),
            model: None,
            thinking: None,
            permission: None,
            provider_id: None,
            agent_records_path: None,
            resume_state: None,
        };
        write_state_json(dir.path(), &state).unwrap();
        let restored = read_state_json(dir.path()).unwrap().unwrap();
        assert_eq!(restored.title, state.title);
        assert_eq!(restored.last_prompt, state.last_prompt);
    }

    #[test]
    fn state_json_roundtrip_with_provider_id() {
        let dir = tempfile::tempdir().unwrap();
        let state = SessionState {
            title: None,
            last_prompt: None,
            custom: HashMap::new(),
            model: Some("gpt-4o".into()),
            thinking: None,
            permission: None,
            provider_id: Some("openai".into()),
            agent_records_path: None,
            resume_state: None,
        };
        write_state_json(dir.path(), &state).unwrap();
        let restored = read_state_json(dir.path()).unwrap().unwrap();
        assert_eq!(restored.provider_id, Some("openai".into()));
    }
}
