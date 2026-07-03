use std::collections::HashMap;

/// Cross-platform probe of OS / shell.
#[derive(Debug, Clone)]
pub struct Environment {
    pub os_kind: String,
    pub os_arch: String,
    pub os_version: String,
    pub shell_name: String,
    pub shell_path: String,
}

/// Injected dependencies for `detect_environment`. Tests supply mock
/// implementations; production callers use `detect_environment_from_node`.
pub type IsFileFn = Box<dyn Fn(&str) -> bool>;
pub type FindExecutableFn = Box<dyn Fn(&str) -> Option<String>>;

pub struct EnvironmentDeps {
    pub platform: String,
    pub arch: String,
    pub release: String,
    pub env: HashMap<String, String>,
    pub is_file: IsFileFn,
    pub find_executable: FindExecutableFn,
}

#[derive(Debug, thiserror::Error)]
#[error("Git Bash was not found on this Windows host. Install Git for Windows from https://gitforwindows.org/ or set ODY_SHELL_PATH to a bash.exe. Checked: {checked}.")]
pub struct KaosShellNotFoundError {
    checked: String,
}

fn resolve_os_kind(platform: &str) -> String {
    match platform {
        "darwin" => "macOS".to_string(),
        "linux" => "Linux".to_string(),
        "win32" => "Windows".to_string(),
        _ => platform.to_string(),
    }
}

/// Detect environment from injected probes.
pub fn detect_environment(deps: &EnvironmentDeps) -> Environment {
    let os_kind = resolve_os_kind(&deps.platform);
    let os_arch = deps.arch.clone();
    let os_version = deps.release.clone();

    if deps.platform == "win32" {
        let shell_path = locate_windows_git_bash(deps);
        return Environment {
            os_kind,
            os_arch,
            os_version,
            shell_name: "bash".to_string(),
            shell_path,
        };
    }

    let candidates = ["/bin/bash", "/usr/bin/bash", "/usr/local/bin/bash"];
    for p in candidates {
        if (deps.is_file)(p) {
            return Environment {
                os_kind,
                os_arch,
                os_version,
                shell_name: "bash".to_string(),
                shell_path: p.to_string(),
            };
        }
    }

    Environment {
        os_kind,
        os_arch,
        os_version,
        shell_name: "sh".to_string(),
        shell_path: "/bin/sh".to_string(),
    }
}

fn locate_windows_git_bash(deps: &EnvironmentDeps) -> String {
    let mut checked: Vec<String> = Vec::new();

    if let Some(override_path) = deps
        .env
        .get("ODY_SHELL_PATH")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        checked.push(override_path.to_string());
        if (deps.is_file)(override_path) {
            return override_path.to_string();
        }
    }

    if let Some(git_exe) = (deps.find_executable)("git.exe") {
        if let Some(inferred) = infer_git_bash_from_git_exe(&git_exe) {
            for path in inferred {
                checked.push(path.clone());
                if (deps.is_file)(&path) {
                    return path;
                }
            }
        }
    }

    let mut candidates = vec![
        "C:\\Program Files\\Git\\bin\\bash.exe".to_string(),
        "C:\\Program Files\\Git\\usr\\bin\\bash.exe".to_string(),
        "C:\\Program Files (x86)\\Git\\bin\\bash.exe".to_string(),
        "C:\\Program Files (x86)\\Git\\usr\\bin\\bash.exe".to_string(),
    ];
    if let Some(local_app_data) = deps
        .env
        .get("LOCALAPPDATA")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        candidates.push(format!("{}\\Programs\\Git\\bin\\bash.exe", local_app_data));
        candidates.push(format!(
            "{}\\Programs\\Git\\usr\\bin\\bash.exe",
            local_app_data
        ));
    }
    for candidate in candidates {
        checked.push(candidate.clone());
        if (deps.is_file)(&candidate) {
            return candidate;
        }
    }

    panic!(
        "{}",
        KaosShellNotFoundError {
            checked: checked.join(", "),
        }
    );
}

fn infer_git_bash_from_git_exe(git_exe: &str) -> Option<Vec<String>> {
    let sep = if git_exe.contains('\\') { '\\' } else { '/' };
    let parts: Vec<&str> = git_exe.split(sep).collect();
    for i in (0..parts.len().saturating_sub(1)).rev() {
        if parts[i] == "cmd" || parts[i] == "bin" {
            let root = parts[..i].join(&sep.to_string());
            let prefix = if root.is_empty() {
                String::new()
            } else {
                format!("{}{}", root, sep)
            };
            return Some(vec![
                format!("{}bin{}bash.exe", prefix, sep),
                format!("{}usr{}bin{}bash.exe", prefix, sep, sep),
            ]);
        }
    }
    None
}

#[cfg(unix)]
fn host_release() -> String {
    unsafe {
        let mut buf: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut buf) < 0 {
            return std::env::consts::OS.to_string();
        }
        let ptr = &buf.release[0] as *const libc::c_char as *const u8;
        let mut len = 0usize;
        while len < buf.release.len() && *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        String::from_utf8_lossy(slice).to_string()
    }
}

#[cfg(not(unix))]
fn host_release() -> String {
    std::env::consts::OS.to_string()
}

/// Production convenience — derive the deps bag from the host OS.
pub fn detect_environment_from_node() -> Environment {
    use std::fs;
    let mut env = HashMap::new();
    for (k, v) in std::env::vars() {
        env.insert(k, v);
    }
    detect_environment(&EnvironmentDeps {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        release: host_release(),
        env,
        is_file: Box::new(|p| fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)),
        find_executable: Box::new(|name| {
            which::which(name)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn deps_with_files(files: &[&'static str]) -> EnvironmentDeps {
        let set: std::collections::HashSet<&'static str> = files.iter().copied().collect();
        EnvironmentDeps {
            platform: "win32".to_string(),
            arch: "x86_64".to_string(),
            release: "10.0.0".to_string(),
            env: HashMap::new(),
            is_file: Box::new(move |p| set.contains(p)),
            find_executable: Box::new(|_| None),
        }
    }

    #[test]
    fn detect_on_windows_uses_override_shell_path() {
        let mut env = HashMap::new();
        env.insert(
            "ODY_SHELL_PATH".to_string(),
            "D:\\custom\\bash.exe".to_string(),
        );
        let deps = EnvironmentDeps {
            platform: "win32".to_string(),
            arch: "x86_64".to_string(),
            release: "10.0.0".to_string(),
            env,
            is_file: Box::new(|p| p == "D:\\custom\\bash.exe"),
            find_executable: Box::new(|_| None),
        };
        let e = detect_environment(&deps);
        assert_eq!(e.os_kind, "Windows");
        assert_eq!(e.shell_path, "D:\\custom\\bash.exe");
    }

    #[test]
    fn detect_on_windows_falls_back_to_git_bash_candidates() {
        let deps = deps_with_files(&["C:\\Program Files\\Git\\bin\\bash.exe"]);
        let e = detect_environment(&deps);
        assert_eq!(e.os_kind, "Windows");
        assert_eq!(e.shell_path, "C:\\Program Files\\Git\\bin\\bash.exe");
    }

    #[test]
    fn detect_on_posix_prefers_bash() {
        let deps = EnvironmentDeps {
            platform: "darwin".to_string(),
            arch: "arm64".to_string(),
            release: "23.0.0".to_string(),
            env: HashMap::new(),
            is_file: Box::new(|p| p == "/bin/bash"),
            find_executable: Box::new(|_| None),
        };
        let e = detect_environment(&deps);
        assert_eq!(e.os_kind, "macOS");
        assert_eq!(e.shell_name, "bash");
        assert_eq!(e.shell_path, "/bin/bash");
    }

    #[test]
    fn detect_on_posix_falls_back_to_sh() {
        let deps = EnvironmentDeps {
            platform: "linux".to_string(),
            arch: "x86_64".to_string(),
            release: "6.0.0".to_string(),
            env: HashMap::new(),
            is_file: Box::new(|_| false),
            find_executable: Box::new(|_| None),
        };
        let e = detect_environment(&deps);
        assert_eq!(e.os_kind, "Linux");
        assert_eq!(e.shell_name, "sh");
        assert_eq!(e.shell_path, "/bin/sh");
    }

    #[test]
    #[should_panic(expected = "Git Bash was not found")]
    fn detect_on_windows_panics_without_git_bash() {
        let deps = deps_with_files(&[]);
        let _ = detect_environment(&deps);
    }
}
