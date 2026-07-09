use regex::Regex;

use crate::policies::path_glob_match::{glob_match, path_glob_match, PermissionPathMatchOptions};

pub fn literal_rule_pattern(tool_name: &str, subject: &str) -> String {
    format!("{}({})", tool_name, escape_rule_subject_literal(subject))
}

pub fn escape_rule_subject_literal(subject: &str) -> String {
    Regex::new(r"[\\*?\[\]{},()!+@|]")
        .unwrap()
        .replace_all(subject, "\\$0")
        .to_string()
}

pub fn matches_glob_rule_subject(rule_args: &str, subject: &str) -> bool {
    match_rule_subjects(rule_args, &[subject], |pattern, value| {
        glob_match(value, pattern, true)
    })
}

pub fn matches_path_rule_subject(
    rule_args: &str,
    subject: &str,
    options: Option<&PermissionPathMatchOptions>,
) -> bool {
    match_rule_subjects(rule_args, &[subject], |pattern, value| {
        path_glob_match(value, pattern, options)
    })
}

fn match_rule_subjects(
    rule_args: &str,
    subjects: &[&str],
    matcher: impl Fn(&str, &str) -> bool,
) -> bool {
    if rule_args.is_empty() {
        return true;
    }
    let negated = rule_args.starts_with('!');
    let positive_pattern = if negated { &rule_args[1..] } else { rule_args };
    let hit = subjects
        .iter()
        .any(|subject| matcher(positive_pattern, subject));
    if negated {
        !hit
    } else {
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policies::path_glob_match::PermissionPathMatchOptions;

    #[test]
    fn literal_pattern_wraps_subject() {
        assert_eq!(
            literal_rule_pattern("read", "/repo/src/main.ts"),
            "read(/repo/src/main.ts)"
        );
    }

    #[test]
    fn escapes_glob_metacharacters_in_literal() {
        assert_eq!(
            escape_rule_subject_literal("a*b[c]d{e,f}"),
            "a\\*b\\[c\\]d\\{e\\,f\\}"
        );
    }

    #[test]
    fn glob_subject_match() {
        assert!(matches_glob_rule_subject("*.ts", "main.ts"));
        assert!(!matches_glob_rule_subject("*.py", "main.ts"));
    }

    #[test]
    fn negated_glob_rule_inverts_match() {
        assert!(!matches_glob_rule_subject("!*.ts", "main.ts"));
        assert!(matches_glob_rule_subject("!*.py", "main.ts"));
    }

    #[test]
    fn empty_rule_args_matches_everything() {
        assert!(matches_glob_rule_subject("", "anything"));
    }

    #[test]
    fn path_subject_match() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(matches_path_rule_subject(
            "src/**/*.ts",
            "src/main.ts",
            Some(&opts)
        ));
        assert!(!matches_path_rule_subject(
            "src/**/*.ts",
            "src/main.py",
            Some(&opts)
        ));
    }

    #[test]
    fn negated_path_rule_inverts_match() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(!matches_path_rule_subject(
            "!src/**/*.ts",
            "src/main.ts",
            Some(&opts)
        ));
        assert!(matches_path_rule_subject(
            "!src/**/*.py",
            "src/main.ts",
            Some(&opts)
        ));
    }
}
