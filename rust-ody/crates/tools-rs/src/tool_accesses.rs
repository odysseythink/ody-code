use serde::{Deserialize, Serialize};

pub const FILE_READ: &str = "read";
pub const FILE_WRITE: &str = "write";
pub const FILE_READWRITE: &str = "readwrite";
pub const FILE_SEARCH: &str = "search";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ToolResourceAccess {
    #[serde(rename = "file")]
    File {
        operation: String,
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        recursive: Option<bool>,
    },
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolAccesses(pub Vec<ToolResourceAccess>);

impl ToolAccesses {
    pub fn none() -> Self {
        Self(Vec::new())
    }

    pub fn all() -> Self {
        Self(vec![ToolResourceAccess::All])
    }

    pub fn file(operation: &str, path: &str, recursive: Option<bool>) -> Self {
        Self(vec![ToolResourceAccess::File {
            operation: operation.to_owned(),
            path: path.to_owned(),
            recursive,
        }])
    }

    pub fn read_file(path: &str) -> Self {
        Self::file(FILE_READ, path, None)
    }

    pub fn read_tree(path: &str) -> Self {
        Self::file(FILE_READ, path, Some(true))
    }

    pub fn write_file(path: &str) -> Self {
        Self::file(FILE_WRITE, path, None)
    }

    pub fn write_tree(path: &str) -> Self {
        Self::file(FILE_WRITE, path, Some(true))
    }

    pub fn read_write_file(path: &str) -> Self {
        Self::file(FILE_READWRITE, path, None)
    }

    pub fn read_write_tree(path: &str) -> Self {
        Self::file(FILE_READWRITE, path, Some(true))
    }

    pub fn search_tree(path: &str) -> Self {
        Self::file(FILE_SEARCH, path, Some(true))
    }

    pub fn conflict(left: &Self, right: &Self) -> bool {
        left.0
            .iter()
            .any(|l| right.0.iter().any(|r| resource_conflict(l, r)))
    }
}

fn resource_conflict(left: &ToolResourceAccess, right: &ToolResourceAccess) -> bool {
    match (left, right) {
        (ToolResourceAccess::All, _) | (_, ToolResourceAccess::All) => true,
        (
            ToolResourceAccess::File {
                operation: lo,
                path: lp,
                recursive: lr,
            },
            ToolResourceAccess::File {
                operation: ro,
                path: rp,
                recursive: rr,
            },
        ) => file_operations_conflict(lo, ro) && file_accesses_overlap(lp, *lr, rp, *rr),
    }
}

fn file_operations_conflict(left: &str, right: &str) -> bool {
    file_operation_writes(left) || file_operation_writes(right)
}

fn file_operation_writes(operation: &str) -> bool {
    matches!(operation, FILE_WRITE | FILE_READWRITE)
}

fn file_accesses_overlap(
    left: &str,
    left_recursive: Option<bool>,
    right: &str,
    right_recursive: Option<bool>,
) -> bool {
    let lp = normalize_path(left);
    let rp = normalize_path(right);
    if lp == rp {
        return true;
    }
    let lpfx = if lp.ends_with('/') {
        lp.clone()
    } else {
        format!("{}/", lp)
    };
    let rpfx = if rp.ends_with('/') {
        rp.clone()
    } else {
        format!("{}/", rp)
    };
    (left_recursive == Some(true) && rp.starts_with(&lpfx))
        || (right_recursive == Some(true) && lp.starts_with(&rpfx))
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/").replace("//", "/");
    let folded = normalized.to_lowercase();
    if folded.len() > 1 && folded.ends_with('/') {
        folded[..folded.len() - 1].to_string()
    } else {
        folded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_read_same_path_does_not_conflict() {
        let a = ToolAccesses::read_file("/repo/src/main.rs");
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(!ToolAccesses::conflict(&a, &b));
    }

    #[test]
    fn write_read_same_path_conflicts() {
        let a = ToolAccesses::write_file("/repo/src/main.rs");
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(ToolAccesses::conflict(&a, &b));
    }

    #[test]
    fn recursive_write_conflicts_with_descendant_read() {
        let a = ToolAccesses::write_tree("/repo");
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(ToolAccesses::conflict(&a, &b));
    }

    #[test]
    fn all_conflicts_with_everything() {
        let a = ToolAccesses::all();
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(ToolAccesses::conflict(&a, &b));
    }
}
