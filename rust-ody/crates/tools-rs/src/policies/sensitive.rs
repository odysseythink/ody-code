use std::collections::HashSet;

const SENSITIVE_BASENAMES: &[&str] = &[".env", "id_rsa", "id_ed25519", "id_ecdsa", "credentials"];

const SENSITIVE_PATH_SUFFIXES: &[&[&str]] = &[&[".aws", "credentials"], &[".gcp", "credentials"]];

const ENV_PREFIX: &str = ".env.";

const ENV_EXEMPTIONS: &[&str] = &[".env.example", ".env.sample", ".env.template"];

const SENSITIVE_BASENAME_PREFIXES: &[&str] = &["id_rsa", "id_ed25519", "id_ecdsa", "credentials"];

const PUBLIC_KEY_BASENAMES: &[&str] = &["id_rsa.pub", "id_ed25519.pub", "id_ecdsa.pub"];

const SENSITIVE_DOT_VARIANT_SUFFIXES: &[&str] = &[
    ".bak",
    ".backup",
    ".copy",
    ".disabled",
    ".key",
    ".old",
    ".orig",
    ".pem",
    ".save",
    ".tmp",
];

fn comparable(path: &str) -> String {
    path.to_lowercase()
}

fn basename(path: &str) -> &str {
    path.rsplit_once(&['/', '\\'][..])
        .map(|(_, name)| name)
        .unwrap_or(path)
}

pub fn is_sensitive_file(path: &str) -> bool {
    let name = basename(path);
    let comparable_name = comparable(name);
    let comparable_path = comparable(path);

    if ENV_EXEMPTIONS.contains(&comparable_name.as_str()) {
        return false;
    }
    if PUBLIC_KEY_BASENAMES.contains(&comparable_name.as_str()) {
        return false;
    }

    let sensitive_basenames: HashSet<&str> = SENSITIVE_BASENAMES.iter().copied().collect();
    if sensitive_basenames.contains(comparable_name.as_str()) {
        return true;
    }

    if comparable_name.starts_with(ENV_PREFIX) {
        return true;
    }

    let dot_variant_suffixes: HashSet<&str> =
        SENSITIVE_DOT_VARIANT_SUFFIXES.iter().copied().collect();
    for prefix in SENSITIVE_BASENAME_PREFIXES {
        if comparable_name == *prefix {
            return true;
        }
        if comparable_name.len() > prefix.len() && comparable_name.starts_with(prefix) {
            let suffix = &comparable_name[prefix.len()..];
            let next = suffix.chars().next().unwrap();
            if next == '-' || next == '_' {
                return true;
            }
            if next == '.' && dot_variant_suffixes.contains(suffix) {
                return true;
            }
        }
    }

    for suffix_parts in SENSITIVE_PATH_SUFFIXES {
        let suffix = suffix_parts.join("/");
        let comparable_suffix = comparable(&suffix);
        if comparable_path.ends_with(&format!("/{}", comparable_suffix))
            || comparable_path.contains(&format!("/{}/", comparable_suffix))
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_sensitive_basenames() {
        for path in [".env", "/app/.env", "project/.env"] {
            assert!(is_sensitive_file(path), "{path}");
        }
    }

    #[test]
    fn flags_env_variants() {
        for path in [".env.local", ".env.production", "/app/.env.staging"] {
            assert!(is_sensitive_file(path), "{path}");
        }
    }

    #[test]
    fn flags_cloud_credentials() {
        for path in [
            "/home/user/.aws/credentials",
            "/home/user/.gcp/credentials",
            ".aws/credentials",
            ".gcp/credentials",
            "credentials",
        ] {
            assert!(is_sensitive_file(path), "{path}");
        }
    }

    #[test]
    fn matches_ssh_key_variants() {
        assert!(is_sensitive_file("id_rsa"));
        assert!(is_sensitive_file("/home/user/.ssh/id_rsa"));
        assert!(is_sensitive_file("id_ed25519_old"));
        assert!(is_sensitive_file("id_rsa.bak"));
    }

    #[test]
    fn must_survive_inputs_are_not_sensitive() {
        for path in [
            "app.py",
            "config.yml",
            "README.md",
            "package.json",
            "server.key.example",
            "id_rsa.pub",
            "credentials.json",
            ".envrc",
            "environment.py",
            ".env_example",
            ".env.example",
            ".ENV.EXAMPLE",
            ".env.sample",
            ".ENV.SAMPLE",
            ".env.template",
            ".ENV.TEMPLATE",
            "/app/.env.example",
            "/app/.ENV.EXAMPLE",
            "id_rsafoo",
        ] {
            assert!(!is_sensitive_file(path), "{path} must not be flagged");
        }
    }
}
