use kaos_rs::kaos::Kaos;
use std::io;
use std::path::Path;

pub const LIST_DIR_ROOT_WIDTH: usize = 30;
pub const LIST_DIR_CHILD_WIDTH: usize = 10;

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    is_dir: bool,
}

async fn collect_entries(
    kaos: &Kaos,
    dir_path: &str,
    max_width: usize,
) -> (Vec<Entry>, usize, bool) {
    let all = match kaos.iterdir(dir_path).await {
        Ok(v) => v,
        Err(_) => return (vec![], 0, false),
    };
    let mut entries: Vec<Entry> = Vec::with_capacity(all.len());
    for full in all {
        let name = Path::new(&full)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut is_dir = false;
        if let Ok(st) = kaos.stat(&full, true).await {
            is_dir = st.is_dir();
        }
        entries.push(Entry { name, is_dir });
    }
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        a.name.cmp(&b.name)
    });
    let total = entries.len();
    entries.truncate(max_width);
    (entries, total, true)
}

pub async fn list_directory(kaos: &Kaos, work_dir: Option<&str>) -> Result<String, io::Error> {
    let work_dir = work_dir
        .map(|s| s.to_string())
        .unwrap_or_else(|| kaos.getcwd());
    let (entries, total, readable) = collect_entries(kaos, &work_dir, LIST_DIR_ROOT_WIDTH).await;
    if !readable {
        return Ok("[not readable]".to_string());
    }
    let remaining = total - entries.len();
    let mut lines: Vec<String> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1 && remaining == 0;
        let connector = if is_last { "└── " } else { "├── " };
        if entry.is_dir {
            lines.push(format!("{}{}/", connector, entry.name));
            let child_prefix = if is_last { "    " } else { "│   " };
            let child_dir = kaos.normpath(&Path::new(&work_dir).join(&entry.name));
            let (child_entries, child_total, child_readable) =
                collect_entries(kaos, &child_dir, LIST_DIR_CHILD_WIDTH).await;
            if !child_readable {
                lines.push(format!("{}└── [not readable]", child_prefix));
                continue;
            }
            let child_remaining = child_total - child_entries.len();
            for (j, ce) in child_entries.iter().enumerate() {
                let c_is_last = j == child_entries.len() - 1 && child_remaining == 0;
                let c_connector = if c_is_last {
                    "└── "
                } else {
                    "├── "
                };
                let suffix = if ce.is_dir { "/" } else { "" };
                lines.push(format!(
                    "{}{}{}{}",
                    child_prefix, c_connector, ce.name, suffix
                ));
            }
            if child_remaining > 0 {
                lines.push(format!(
                    "{}└── ... and {} more",
                    child_prefix, child_remaining
                ));
            }
        } else {
            lines.push(format!("{}{}", connector, entry.name));
        }
    }

    if remaining > 0 {
        lines.push(format!("└── ... and {} more entries", remaining));
    }

    Ok(if lines.is_empty() {
        "(empty directory)".to_string()
    } else {
        lines.join("\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_env() -> kaos_rs::environment::Environment {
        kaos_rs::environment::Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    #[tokio::test]
    async fn empty_directory_placeholder() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        assert_eq!(
            list_directory(&kaos, None).await.unwrap(),
            "(empty directory)"
        );
    }

    #[tokio::test]
    async fn renders_two_level_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        tokio::fs::write(root.join("a.txt"), "").await.unwrap();
        tokio::fs::write(root.join("b.rs"), "").await.unwrap();
        tokio::fs::create_dir(root.join("src")).await.unwrap();
        tokio::fs::write(root.join("src").join("main.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(root.join("src").join("lib.rs"), "")
            .await
            .unwrap();

        let kaos = Kaos::new(dummy_env(), root);
        let out = list_directory(&kaos, None).await.unwrap();
        // Directories come before files, src/ should be before a.txt.
        assert!(out.contains("src/"));
        assert!(out.contains("├── main.rs") || out.contains("└── main.rs"));
        assert!(out.contains("a.txt"));
        assert!(out.contains("b.rs"));
    }

    #[tokio::test]
    async fn root_width_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for i in 0..32 {
            tokio::fs::write(root.join(format!("file{:02}.txt", i)), "")
                .await
                .unwrap();
        }
        let kaos = Kaos::new(dummy_env(), root);
        let out = list_directory(&kaos, None).await.unwrap();
        assert!(out.contains("... and 2 more entries"));
    }

    #[tokio::test]
    async fn child_width_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let sub = root.join("sub");
        tokio::fs::create_dir(&sub).await.unwrap();
        for i in 0..12 {
            tokio::fs::write(sub.join(format!("child{:02}.txt", i)), "")
                .await
                .unwrap();
        }
        let kaos = Kaos::new(dummy_env(), root);
        let out = list_directory(&kaos, None).await.unwrap();
        assert!(out.contains("sub/"));
        assert!(out.contains("... and 2 more"));
    }
}
