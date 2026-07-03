use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub workspace_dir: String,
    pub additional_dirs: Vec<String>,
}

impl WorkspaceConfig {
    pub fn new(workspace_dir: impl Into<String>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
            additional_dirs: Vec::new(),
        }
    }
}
