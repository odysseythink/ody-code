use std::io;
use std::path::{Path, PathBuf};

use crate::environment::Environment;
use crate::file::{self, KaosIoError};
use crate::path;
use crate::text::ErrorMode;

/// A KAOS implementation that directly interacts with the local filesystem.
///
/// `Kaos` maintains its own per-instance working directory (`cwd`) rather than
/// mutating the process current directory. This lets multiple `Kaos` instances
/// coexist with independent cwds.
#[derive(Debug, Clone)]
pub struct Kaos {
    name: String,
    env: Environment,
    cwd: PathBuf,
}

impl Kaos {
    /// Construct a fresh `Kaos` with the given environment and cwd.
    pub fn new(env: Environment, cwd: impl AsRef<Path>) -> Self {
        Self {
            name: "local".to_string(),
            env,
            cwd: PathBuf::from(path::normpath(cwd)),
        }
    }

    /// Return a new `Kaos` with the given `cwd`.
    pub fn with_cwd(&self, cwd: impl AsRef<Path>) -> Self {
        Self {
            name: self.name.clone(),
            env: self.env.clone(),
            cwd: PathBuf::from(path::normpath(cwd)),
        }
    }

    /// Change the working directory of this instance.
    ///
    /// Validates that the target exists and is a directory before updating the
    /// internal cwd. Relative paths are resolved against the current cwd.
    pub async fn chdir(&mut self, cwd: impl AsRef<Path>) -> io::Result<()> {
        let target = if Path::new(cwd.as_ref()).is_absolute() {
            path::normpath(&cwd)
        } else {
            path::normpath(self.cwd.join(cwd.as_ref()))
        };
        let meta = tokio::fs::metadata(&target).await?;
        if !meta.is_dir() {
            return Err(io::Error::other(format!("Not a directory: {}", target)));
        }
        self.cwd = PathBuf::from(target);
        Ok(())
    }

    /// Human-readable name for this environment.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// OS / shell probe describing the target environment.
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Return the path style used by this environment.
    pub fn path_class(&self) -> &'static str {
        path::path_class()
    }

    /// Normalize the given path string (resolve `.` / `..` segments).
    pub fn normpath(&self, path: impl AsRef<Path>) -> String {
        path::normpath(path)
    }

    /// Return the home directory of the current user.
    pub fn gethome(&self) -> Option<PathBuf> {
        path::gethome()
    }

    /// Return the current working directory of this instance.
    pub fn getcwd(&self) -> String {
        path::normpath(&self.cwd)
    }

    /// Resolve a path relative to this instance's cwd.
    fn resolve_path(&self, path_str: &str) -> String {
        let p = Path::new(path_str);
        if p.is_absolute() {
            path::normpath(p)
        } else {
            path::normpath(self.cwd.join(p))
        }
    }

    // ── File I/O ───────────────────────────────────────────────────────

    /// Read up to `n` bytes from `path` (all bytes if `n` is None).
    pub async fn read_bytes(&self, path_str: &str, n: Option<u64>) -> Result<Vec<u8>, io::Error> {
        let resolved = self.resolve_path(path_str);
        file::read_bytes(&resolved, n).await
    }

    /// Read the file at `path` as a string with encoding and error mode control.
    pub async fn read_text(
        &self,
        path_str: &str,
        encoding: Option<&str>,
        errors: Option<ErrorMode>,
    ) -> Result<String, KaosIoError> {
        let resolved = self.resolve_path(path_str);
        file::read_text(&resolved, encoding, errors).await
    }

    /// Yield lines from `path`. Lines preserve trailing newlines except the last.
    pub async fn read_lines(
        &self,
        path_str: &str,
        encoding: Option<&str>,
        errors: Option<ErrorMode>,
    ) -> Result<Vec<String>, KaosIoError> {
        let resolved = self.resolve_path(path_str);
        file::read_lines(&resolved, encoding, errors).await
    }

    /// Write raw bytes to `path`, returning the number of bytes written.
    pub async fn write_bytes(&self, path_str: &str, data: &[u8]) -> Result<u64, io::Error> {
        let resolved = self.resolve_path(path_str);
        file::write_bytes(&resolved, data).await
    }

    /// Write text to `path`, returning the number of characters written.
    /// `mode`: "w" (truncate) or "a" (append).
    pub async fn write_text(
        &self,
        path_str: &str,
        data: &str,
        mode: Option<&str>,
        encoding: Option<&str>,
    ) -> Result<usize, io::Error> {
        let resolved = self.resolve_path(path_str);
        file::write_text(&resolved, data, mode, encoding).await
    }

    // ── Directory I/O ────────────────────────────────────────────────────

    /// Return stat metadata for `path`.
    pub async fn stat(
        &self,
        path_str: &str,
        follow_symlinks: bool,
    ) -> Result<crate::dir::StatResult, std::io::Error> {
        let resolved = self.resolve_path(path_str);
        crate::dir::stat(&resolved, follow_symlinks).await
    }

    /// Yield entry names in the directory at `path` as normalized full paths.
    pub async fn iterdir(&self, path_str: &str) -> Result<Vec<String>, std::io::Error> {
        let resolved = self.resolve_path(path_str);
        crate::dir::iterdir(&resolved).await
    }

    /// Yield paths matching `pattern` under `path`.
    pub async fn glob(
        &self,
        path_str: &str,
        pattern: &str,
        case_sensitive: bool,
    ) -> Result<Vec<String>, std::io::Error> {
        let resolved = self.resolve_path(path_str);
        crate::dir::glob(&resolved, pattern, case_sensitive).await
    }

    /// Create a directory at `path`.
    pub async fn mkdir(&self, path_str: &str, parents: bool, exist_ok: bool) -> io::Result<()> {
        let resolved = self.resolve_path(path_str);
        crate::dir::mkdir(&resolved, parents, exist_ok).await
    }

    // ── Process execution ──────────────────────────────────────────────────

    /// Spawn a process with the given arguments.
    pub async fn exec(&self, args: &[&str]) -> Result<crate::process::Process, std::io::Error> {
        crate::process::spawn(&self.cwd, args, None).await
    }

    /// Spawn a process with explicit environment variables.
    pub async fn exec_with_env(
        &self,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<crate::process::Process, std::io::Error> {
        crate::process::spawn(&self.cwd, args, Some(env)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::Environment;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    #[test]
    fn with_cwd_creates_independent_instance() {
        let a = Kaos::new(dummy_env(), "/foo");
        let b = a.with_cwd("/bar");
        assert_eq!(a.getcwd(), "/foo");
        assert_eq!(b.getcwd(), "/bar");
    }

    #[tokio::test]
    async fn chdir_only_mutates_internal_state() {
        let tmp = tempfile::tempdir().unwrap();
        let mut kaos = Kaos::new(dummy_env(), tmp.path());
        let sub = tmp.path().join("sub");
        tokio::fs::create_dir(&sub).await.unwrap();
        kaos.chdir("sub").await.unwrap();
        assert_eq!(kaos.getcwd(), path::normpath(&sub));
    }

    #[tokio::test]
    async fn chdir_to_non_directory_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let mut kaos = Kaos::new(dummy_env(), tmp.path());
        let file = tmp.path().join("file.txt");
        tokio::fs::write(&file, "x").await.unwrap();
        let err = kaos.chdir("file.txt").await.unwrap_err();
        assert!(err.to_string().contains("Not a directory"));
    }

    #[test]
    fn path_class_delegates_to_host() {
        let kaos = Kaos::new(dummy_env(), "/foo");
        let cls = kaos.path_class();
        assert!(cls == "posix" || cls == "win32");
    }

    #[test]
    fn normpath_resolves_dotdot() {
        let kaos = Kaos::new(dummy_env(), "/foo");
        assert_eq!(kaos.normpath("/foo/bar/../baz"), "/foo/baz");
    }

    #[test]
    fn gethome_returns_some_directory() {
        let kaos = Kaos::new(dummy_env(), "/foo");
        assert!(kaos.gethome().is_some());
    }
}
