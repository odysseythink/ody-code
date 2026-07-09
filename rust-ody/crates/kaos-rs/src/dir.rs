//! Directory operations for kaos-rs: stat, iterdir, glob, mkdir.

use serde::Serialize;
use std::collections::HashSet;
use std::io;

use crate::glob::glob_pattern_to_regex;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatResult {
    pub st_mode: u32,
    pub st_ino: u64,
    pub st_dev: u64,
    pub st_nlink: u64,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_size: u64,
    pub st_atime: f64,
    pub st_mtime: f64,
    pub st_ctime: f64,
}

impl StatResult {
    pub fn is_dir(&self) -> bool {
        (self.st_mode & 0o170000) == 0o040000
    }
}

pub async fn stat(path: &str, follow_symlinks: bool) -> Result<StatResult, io::Error> {
    let meta = if follow_symlinks {
        tokio::fs::metadata(path).await?
    } else {
        tokio::fs::symlink_metadata(path).await?
    };
    Ok(build_stat_result(&meta))
}

fn build_stat_result(meta: &std::fs::Metadata) -> StatResult {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        StatResult {
            st_mode: meta.mode(),
            st_ino: meta.ino(),
            st_dev: meta.dev(),
            st_nlink: meta.nlink(),
            st_uid: meta.uid(),
            st_gid: meta.gid(),
            st_size: meta.len(),
            st_atime: meta.atime() as f64,
            st_mtime: meta.mtime() as f64,
            st_ctime: meta.ctime() as f64,
        }
    }
    #[cfg(not(unix))]
    {
        StatResult {
            st_mode: 0,
            st_ino: 0,
            st_dev: 0,
            st_nlink: 0,
            st_uid: 0,
            st_gid: 0,
            st_size: meta.len(),
            st_atime: 0.0,
            st_mtime: 0.0,
            st_ctime: 0.0,
        }
    }
}

pub async fn iterdir(path: &str) -> Result<Vec<String>, io::Error> {
    let mut entries = tokio::fs::read_dir(path).await?;
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let full = crate::path::normpath(entry.path());
        out.push(full);
    }
    Ok(out)
}

pub async fn glob(
    path: &str,
    pattern: &str,
    case_sensitive: bool,
) -> Result<Vec<String>, io::Error> {
    let base = crate::path::normpath(path);
    let parts: Vec<String> = pattern.split('/').map(|s| s.to_string()).collect();
    let mut visited = HashSet::new();
    if let Ok(meta) = tokio::fs::metadata(&base).await {
        if let Some(key) = cycle_key(&meta) {
            visited.insert(key);
        }
    }
    let mut results = Vec::new();
    glob_walk(&base, &parts, case_sensitive, &visited, &mut results).await?;
    results.sort();
    Ok(results)
}

async fn glob_walk(
    base: &str,
    parts: &[String],
    case_sensitive: bool,
    visited: &HashSet<String>,
    results: &mut Vec<String>,
) -> Result<(), io::Error> {
    if parts.is_empty() {
        return Ok(());
    }
    let current = &parts[0];
    let rest = &parts[1..];

    if current == "**" {
        if rest.is_empty() {
            results.push(base.to_string());
        } else {
            Box::pin(glob_walk(base, rest, case_sensitive, visited, results)).await?;
        }

        let mut entries = tokio::fs::read_dir(base).await?;
        while let Some(entry) = entries.next_entry().await? {
            let full = crate::path::normpath(entry.path());
            let meta = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                if let Some(key) = cycle_key(&meta) {
                    if visited.contains(&key) {
                        continue;
                    }
                    let mut v2 = visited.clone();
                    v2.insert(key);
                    Box::pin(glob_walk(&full, parts, case_sensitive, &v2, results)).await?;
                } else {
                    Box::pin(glob_walk(&full, parts, case_sensitive, visited, results)).await?;
                }
            } else if rest.is_empty() {
                results.push(full);
            }
        }
    } else {
        let re = glob_pattern_to_regex(current, case_sensitive);
        let mut entries = tokio::fs::read_dir(base).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            if !re.is_match(&name) {
                continue;
            }
            let full = crate::path::normpath(entry.path());
            if rest.is_empty() {
                results.push(full);
            } else {
                let meta = match entry.metadata().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if meta.is_dir() {
                    if let Some(key) = cycle_key(&meta) {
                        if visited.contains(&key) {
                            continue;
                        }
                        let mut v2 = visited.clone();
                        v2.insert(key);
                        Box::pin(glob_walk(&full, rest, case_sensitive, &v2, results)).await?;
                    } else {
                        Box::pin(glob_walk(&full, rest, case_sensitive, visited, results)).await?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn cycle_key(meta: &std::fs::Metadata) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let ino = meta.ino();
        if ino == 0 {
            return None;
        }
        Some(format!("{}:{}", meta.dev(), ino))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

pub async fn mkdir(path: &str, parents: bool, exist_ok: bool) -> io::Result<()> {
    if parents {
        if !exist_ok {
            if let Ok(m) = tokio::fs::metadata(path).await {
                if m.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("{} already exists", path),
                    ));
                }
            }
        }
        tokio::fs::create_dir_all(path).await?;
        Ok(())
    } else {
        match tokio::fs::create_dir(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && exist_ok => {
                let m = tokio::fs::metadata(path).await?;
                if m.is_dir() {
                    Ok(())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("{} already exists but is not a directory", path),
                    ))
                }
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", path),
            )),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn temp_dir() -> (TempDir, String) {
        let d = TempDir::new().unwrap();
        let s = d.path().to_string_lossy().to_string();
        (d, s)
    }

    fn is_dir(st_mode: u32) -> bool {
        (st_mode & 0o170000) == 0o040000
    }

    #[tokio::test]
    async fn stat_distinguishes_file_and_directory() {
        let (_d, root) = temp_dir().await;
        let file = format!("{}/file.txt", root);
        tokio::fs::write(&file, "hello").await.unwrap();

        let f = stat(&file, true).await.unwrap();
        assert_eq!(f.st_size, 5);
        assert!(!is_dir(f.st_mode));

        let d = stat(&root, true).await.unwrap();
        assert!(is_dir(d.st_mode));
    }

    #[tokio::test]
    async fn stat_follow_symlinks_switch() {
        let (_d, root) = temp_dir().await;
        let target = format!("{}/target.txt", root);
        let link = format!("{}/link.txt", root);
        tokio::fs::write(&target, "x").await.unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).unwrap();
            let with_follow = stat(&link, true).await.unwrap();
            assert_eq!(with_follow.st_size, 1);
            let no_follow = stat(&link, false).await.unwrap();
            assert!(no_follow.st_size < 100);
        }
    }

    #[tokio::test]
    async fn iterdir_returns_normalized_full_paths() {
        let (_d, root) = temp_dir().await;
        tokio::fs::write(format!("{}/a.txt", root), "")
            .await
            .unwrap();
        tokio::fs::write(format!("{}/b.txt", root), "")
            .await
            .unwrap();

        let mut entries = iterdir(&root).await.unwrap();
        entries.sort();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].ends_with("/a.txt"));
        assert!(entries[1].ends_with("/b.txt"));
        assert!(!entries[0].contains("//"));
    }

    async fn make_tree(root: &str) {
        tokio::fs::write(format!("{}/a.txt", root), "")
            .await
            .unwrap();
        tokio::fs::write(format!("{}/b.log", root), "")
            .await
            .unwrap();
        tokio::fs::create_dir(format!("{}/sub", root))
            .await
            .unwrap();
        tokio::fs::write(format!("{}/sub/c.txt", root), "")
            .await
            .unwrap();
        tokio::fs::write(format!("{}/.hidden", root), "")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn glob_star_pattern_matches_basename() {
        let (_d, root) = temp_dir().await;
        make_tree(&root).await;
        let mut matches = glob(&root, "*.txt", true).await.unwrap();
        matches.sort();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].ends_with("/a.txt"));
    }

    #[tokio::test]
    async fn glob_double_star_recurses() {
        let (_d, root) = temp_dir().await;
        make_tree(&root).await;
        let mut matches = glob(&root, "**/*.txt", true).await.unwrap();
        matches.sort();
        assert_eq!(matches.len(), 2);
        assert!(matches[0].ends_with("/a.txt"));
        assert!(matches[1].ends_with("/sub/c.txt"));
    }

    #[tokio::test]
    async fn glob_char_class_negation() {
        let (_d, root) = temp_dir().await;
        make_tree(&root).await;
        let mut matches = glob(&root, "[!a].*", true).await.unwrap();
        matches.sort();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].ends_with("/b.log"));
    }

    #[tokio::test]
    async fn glob_case_sensitivity_flag() {
        let (_d, root) = temp_dir().await;
        tokio::fs::write(format!("{}/A.TXT", root), "")
            .await
            .unwrap();
        assert_eq!(glob(&root, "*.txt", true).await.unwrap().len(), 0);
        assert_eq!(glob(&root, "*.txt", false).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn glob_detects_symlink_cycle() {
        let (_d, root) = temp_dir().await;
        tokio::fs::create_dir(format!("{}/loop", root))
            .await
            .unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(format!("{}/loop", root), format!("{}/loop/self", root))
                .unwrap();
            let matches = glob(&root, "loop/**/*", true).await.unwrap();
            assert!(matches.len() <= 2, "cycle should not infinite-loop");
        }
    }

    #[tokio::test]
    async fn mkdir_nested_with_parents() {
        let (_d, root) = temp_dir().await;
        let target = format!("{}/a/b/c", root);
        mkdir(&target, true, false).await.unwrap();
        assert!(tokio::fs::metadata(&target).await.unwrap().is_dir());
    }

    #[tokio::test]
    async fn mkdir_existing_dir_without_exist_ok_fails() {
        let (_d, root) = temp_dir().await;
        let target = format!("{}/existing", root);
        tokio::fs::create_dir(&target).await.unwrap();
        let err = mkdir(&target, false, false).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn mkdir_existing_dir_with_exist_ok_succeeds() {
        let (_d, root) = temp_dir().await;
        let target = format!("{}/existing", root);
        tokio::fs::create_dir(&target).await.unwrap();
        mkdir(&target, false, true).await.unwrap();
    }

    #[tokio::test]
    async fn mkdir_file_collision_with_exist_ok_fails() {
        let (_d, root) = temp_dir().await;
        let target = format!("{}/file.txt", root);
        tokio::fs::write(&target, "x").await.unwrap();
        let err = mkdir(&target, false, true).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(err.to_string().contains("not a directory"));
    }

    #[tokio::test]
    async fn mkdir_existing_with_parents_and_no_exist_ok_fails() {
        let (_d, root) = temp_dir().await;
        let target = format!("{}/a", root);
        tokio::fs::create_dir(&target).await.unwrap();
        let err = mkdir(&target, true, false).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(err.to_string().contains("already exists"));
    }
}
