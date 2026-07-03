use std::path::{Path, PathBuf};

use path_clean::PathClean;

/// Return the path style used by this environment.
pub fn path_class() -> &'static str {
    if cfg!(windows) {
        "win32"
    } else {
        "posix"
    }
}

/// Normalize the given path string (resolve `.` / `..` segments).
/// Mirrors `pathe.normalize`: always returns `/`-separated paths.
pub fn normpath(path: impl AsRef<Path>) -> String {
    let cleaned = path.as_ref().clean();
    cleaned.to_string_lossy().replace('\\', "/")
}

/// Return the home directory of the current user.
pub fn gethome() -> Option<PathBuf> {
    dirs::home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_class_is_posix_or_win32() {
        let cls = path_class();
        assert!(cls == "posix" || cls == "win32");
    }

    #[test]
    fn normpath_resolves_dot_and_dotdot() {
        assert_eq!(normpath("/foo/bar/../baz"), "/foo/baz");
        assert_eq!(normpath("/foo/./bar"), "/foo/bar");
        assert_eq!(normpath("foo//bar/../baz"), "foo/baz");
    }

    #[test]
    fn normpath_preserves_relative_above_root() {
        // Node/pathe behavior: leading .. segments that go above cwd are kept.
        assert_eq!(normpath("../foo"), "../foo");
        assert_eq!(normpath("../../foo"), "../../foo");
    }
}
