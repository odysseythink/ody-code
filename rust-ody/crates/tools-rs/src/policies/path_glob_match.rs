use globset::GlobBuilder;

use crate::policies::path_access::{canonicalize_path, PathClass};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionPathMatchOptions {
    pub cwd: Option<String>,
    pub path_class: Option<PathClass>,
    pub home_dir: Option<String>,
    pub case_insensitive_paths: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PathMatchSemantics {
    path_class: PathClass,
}

/// Match a glob pattern against a value. Supports `*`, `**`, `?`, character
/// classes, backslash escaping, and recursive brace expansion.
pub fn glob_match(value: &str, pattern: &str, nocase: bool) -> bool {
    for p in expand_braces(pattern) {
        let glob = match GlobBuilder::new(&p)
            .literal_separator(true)
            .backslash_escape(true)
            .case_insensitive(nocase)
            .build()
        {
            Ok(g) => g,
            Err(_) => continue,
        };
        if glob.compile_matcher().is_match(value) {
            return true;
        }
    }
    false
}

/// Match file path fields, normalizing equivalent spellings (`./a`,
/// `dir/../a`, Windows separators) before glob matching.
pub fn path_glob_match(
    value: &str,
    pattern: &str,
    options: Option<&PermissionPathMatchOptions>,
) -> bool {
    let semantics = path_match_semantics(value, pattern, options);
    let nocase = options
        .and_then(|o| o.case_insensitive_paths)
        .unwrap_or(true);

    if glob_match(value, pattern, nocase) {
        return true;
    }

    let value_variants = path_variants(value, &semantics, options);
    let pattern_variants = path_variants(pattern, &semantics, options);
    for value_variant in &value_variants {
        for pattern_variant in &pattern_variants {
            if glob_match(value_variant, pattern_variant, nocase) {
                return true;
            }
        }
    }
    false
}

fn path_variants(
    value: &str,
    semantics: &PathMatchSemantics,
    options: Option<&PermissionPathMatchOptions>,
) -> Vec<String> {
    let mut variants = std::collections::HashSet::new();
    add_path_variant(&mut variants, value, semantics.path_class);
    add_path_variant(
        &mut variants,
        &strip_leading_dot_path(value, semantics.path_class),
        semantics.path_class,
    );
    if let Some(canonical) = canonicalize_path_pattern(value, semantics, options) {
        add_path_variant(&mut variants, &canonical, semantics.path_class);
    }
    variants.into_iter().collect()
}

fn canonicalize_path_pattern(
    value: &str,
    semantics: &PathMatchSemantics,
    options: Option<&PermissionPathMatchOptions>,
) -> Option<String> {
    let expanded = expand_user_path(
        value,
        semantics.path_class,
        options.and_then(|o| o.home_dir.as_deref()),
    );
    let cwd_owned = match options.and_then(|o| o.cwd.as_deref()) {
        Some(c) => c.to_string(),
        None => default_cwd_for_path(&expanded, semantics.path_class)?,
    };
    canonicalize_path(&expanded, &cwd_owned, semantics.path_class).ok()
}

fn default_cwd_for_path(value: &str, path_class: PathClass) -> Option<String> {
    if !is_absolute_path(value, path_class) {
        return None;
    }
    match path_class {
        PathClass::Posix => Some("/".to_string()),
        PathClass::Win32 => {
            let s = value.replace('\\', "/");
            if s.starts_with("//") {
                let rest = &s[2..];
                let first = rest.find('/')?;
                let after = &rest[first + 1..];
                let second = after.find('/').unwrap_or(after.len());
                Some(format!("//{}/{}", &rest[..first], &after[..second]))
            } else if s.len() >= 2 && s.as_bytes()[1] == b':' {
                let drive = s[..2].to_uppercase();
                Some(format!("{}/", drive))
            } else {
                Some("C:/".to_string())
            }
        }
    }
}

fn is_absolute_path(path: &str, path_class: PathClass) -> bool {
    match path_class {
        PathClass::Posix => path.starts_with('/'),
        PathClass::Win32 => {
            path.starts_with("//")
                || path.starts_with("\\\\")
                || (path.len() >= 2
                    && path.as_bytes()[1] == b':'
                    && path.as_bytes()[0].is_ascii_alphabetic())
        }
    }
}

fn expand_user_path(value: &str, path_class: PathClass, home_dir: Option<&str>) -> String {
    let home = match home_dir {
        Some(h) => h,
        None => return value.to_string(),
    };
    if value == "~" {
        return home.to_string();
    }
    if value.starts_with("~/") || (path_class == PathClass::Win32 && value.starts_with("~\\")) {
        let rest = &value[2..];
        return format!("{}/{}", home.replace('\\', "/"), rest);
    }
    value.to_string()
}

fn path_match_semantics(
    value: &str,
    pattern: &str,
    options: Option<&PermissionPathMatchOptions>,
) -> PathMatchSemantics {
    let path_class = options.and_then(|o| o.path_class).unwrap_or_else(|| {
        let is_win32 = [value, pattern].iter().any(|candidate| {
            candidate.starts_with("\\\\")
                || candidate.starts_with("//")
                || candidate.contains('\\')
                || (candidate.len() >= 2
                    && candidate.as_bytes()[1] == b':'
                    && candidate.as_bytes()[0].is_ascii_alphabetic())
        });
        if is_win32 {
            PathClass::Win32
        } else {
            PathClass::Posix
        }
    });
    PathMatchSemantics { path_class }
}

fn add_path_variant(
    variants: &mut std::collections::HashSet<String>,
    value: &str,
    path_class: PathClass,
) {
    variants.insert(value.to_string());
    if path_class == PathClass::Win32 {
        variants.insert(value.replace('\\', "/"));
    }
}

fn strip_leading_dot_path(value: &str, path_class: PathClass) -> String {
    if value.starts_with("./") {
        value[2..].to_string()
    } else if path_class == PathClass::Win32 && value.starts_with(".\\") {
        value[2..].to_string()
    } else {
        value.to_string()
    }
}

/// Recursively expand `{a,b}` braces, ignoring braces inside `[...]` character
/// classes. Returns the original pattern if no braces are present.
fn expand_braces(pattern: &str) -> Vec<String> {
    let mut bracket_depth = 0i32;
    let mut brace_start: Option<usize> = None;

    for (i, &b) in pattern.as_bytes().iter().enumerate() {
        match b {
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = (bracket_depth - 1).max(0),
            b'{' if bracket_depth == 0 => {
                brace_start = Some(i);
                break;
            }
            _ => {}
        }
    }

    let start = match brace_start {
        Some(s) => s,
        None => return vec![pattern.to_string()],
    };

    let mut i = start + 1;
    let mut inner_bracket_depth = 0i32;
    let mut inner_brace_depth = 0i32;
    let mut brace_end: Option<usize> = None;

    while i < pattern.len() {
        match pattern.as_bytes()[i] {
            b'[' => inner_bracket_depth += 1,
            b']' => inner_bracket_depth = (inner_bracket_depth - 1).max(0),
            b'{' if inner_bracket_depth == 0 => inner_brace_depth += 1,
            b'}' if inner_bracket_depth == 0 => {
                if inner_brace_depth == 0 {
                    brace_end = Some(i);
                    break;
                }
                inner_brace_depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }

    let end = match brace_end {
        Some(e) => e,
        None => return vec![pattern.to_string()],
    };

    let prefix = &pattern[..start];
    let inner = &pattern[start + 1..end];
    let suffix = &pattern[end + 1..];

    let mut out = Vec::new();
    for choice in split_top_level_commas(inner) {
        let partial = format!("{}{}{}", prefix, choice, suffix);
        for expanded in expand_braces(&partial) {
            out.push(expanded);
        }
    }
    out
}

fn split_top_level_commas(inner: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;

    for (i, &b) in inner.as_bytes().iter().enumerate() {
        match b {
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = (bracket_depth - 1).max(0),
            b'{' if bracket_depth == 0 => brace_depth += 1,
            b'}' if bracket_depth == 0 => brace_depth = (brace_depth - 1).max(0),
            b',' if bracket_depth == 0 && brace_depth == 0 => {
                items.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&inner[start..]);
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_in_same_segment() {
        assert!(glob_match("main.ts", "*.ts", false));
        assert!(!glob_match("src/main.ts", "*.ts", false));
    }

    #[test]
    fn double_star_matches_across_segments() {
        assert!(glob_match("src/deep/main.ts", "src/**/*.ts", false));
        assert!(!glob_match("main.ts", "src/**/*.ts", false));
    }

    #[test]
    fn brace_expansion_matches_alternatives() {
        assert!(glob_match("a/b.ts", "a/{b,c}.ts", false));
        assert!(glob_match("a/c.ts", "a/{b,c}.ts", false));
        assert!(!glob_match("a/d.ts", "a/{b,c}.ts", false));
    }

    #[test]
    fn nested_brace_expansion_works() {
        assert!(glob_match("a/c.ts", "a/{b,{c,d}}.ts", false));
        assert!(glob_match("a/d.ts", "a/{b,{c,d}}.ts", false));
        assert!(!glob_match("a/z.ts", "a/{b,{c,d}}.ts", false));
    }

    #[test]
    fn nocase_option_is_honored() {
        assert!(glob_match("MAIN.TS", "*.ts", true));
        assert!(!glob_match("MAIN.TS", "*.ts", false));
    }

    #[test]
    fn escaped_special_is_literal() {
        assert!(glob_match("a*b", "a\\*b", false));
        assert!(!glob_match("aXb", "a\\*b", false));
    }

    #[test]
    fn question_mark_matches_single_char() {
        assert!(glob_match("aXb", "a?b", false));
        assert!(!glob_match("a/b", "a?b", false));
    }

    #[test]
    fn character_class_matches() {
        assert!(glob_match("abc", "a[bc]c", false));
        assert!(!glob_match("adc", "a[bc]c", false));
    }

    #[test]
    fn path_glob_strips_leading_dot_slash() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(path_glob_match("./main.ts", "*.ts", Some(&opts)));
    }

    #[test]
    fn path_glob_is_case_insensitive_by_default() {
        assert!(path_glob_match("MAIN.TS", "*.ts", None));
    }

    #[test]
    fn path_glob_uses_canonical_variant() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(path_glob_match("src/../main.ts", "main.ts", Some(&opts)));
    }
}
