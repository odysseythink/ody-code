use regex::Regex;

/// Convert a glob pattern segment (e.g. `"*.txt"`, `"file?.log"`) into a RegExp.
/// Mirrors Python pathlib behavior: includes dotfiles, case-sensitive by default.
pub fn glob_pattern_to_regex(pattern: &str, case_sensitive: bool) -> Regex {
    let mut regex = String::from('^');
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '*' => regex.push_str("[^/]*"),
            '?' => regex.push_str("[^/]"),
            '[' => {
                let mut j = i + 1;
                let mut found = false;
                while j < chars.len() {
                    if chars[j] == ']' {
                        found = true;
                        break;
                    }
                    j += 1;
                }
                if !found {
                    regex.push_str("\\[");
                } else {
                    let mut char_class: String = chars[i + 1..j].iter().collect();
                    // Escape backslashes inside the class so a trailing backslash
                    // does not accidentally escape the closing `]`.
                    char_class = char_class.replace('\\', "\\\\");
                    if char_class.starts_with('!') {
                        char_class = format!("^{}", &char_class[1..]);
                    } else if char_class.starts_with('^') {
                        char_class = format!("\\{}", char_class);
                    }
                    regex.push('[');
                    regex.push_str(&char_class);
                    regex.push(']');
                    i = j;
                }
            }
            '\\' => {
                if i + 1 < chars.len() {
                    let next = chars[i + 1];
                    regex.push_str(&regex_escape_char(next));
                    i += 1;
                } else {
                    regex.push_str("\\\\");
                }
            }
            _ => regex.push_str(&regex_escape_char(ch)),
        }
        i += 1;
    }
    regex.push('$');
    let flags = if case_sensitive { "" } else { "(?i)" };
    Regex::new(&format!("{}{}", flags, regex)).expect("generated regex is valid")
}

/// Escape a single character for use in a regex pattern.
/// Mirrors the TS escape table, extended with `*` and `?` which are
/// regex metacharacters that must be literal after a glob backslash.
fn regex_escape_char(ch: char) -> String {
    match ch {
        '{' | '}' | '(' | ')' | '+' | '.' | '\\' | '[' | ']' | '^' | '$' | '|' | '*' | '?' => {
            format!("\\{}", ch)
        }
        _ => ch.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, case_sensitive: bool, input: &str) -> bool {
        glob_pattern_to_regex(pattern, case_sensitive).is_match(input)
    }

    #[test]
    fn star_matches_any_chars_except_slash() {
        assert!(matches("*.txt", true, "a.txt"));
        assert!(!matches("*.txt", true, "a/b.txt"));
    }

    #[test]
    fn question_matches_single_char() {
        assert!(matches("file?.log", true, "file1.log"));
        assert!(!matches("file?.log", true, "file12.log"));
    }

    #[test]
    fn char_class_negation_with_bang() {
        assert!(matches("[!a].txt", true, "b.txt"));
        assert!(!matches("[!a].txt", true, "a.txt"));
    }

    #[test]
    fn char_class_literal_caret_is_escaped() {
        assert!(matches("[a^].txt", true, "^.txt"));
        assert!(matches("[a^].txt", true, "a.txt"));
    }

    #[test]
    fn backslash_escapes_metachar() {
        assert!(matches("file\\*.txt", true, "file*.txt"));
        assert!(!matches("file\\*.txt", true, "fileA.txt"));
    }

    #[test]
    fn case_insensitive_flag_works() {
        assert!(matches("*.TXT", false, "a.txt"));
        assert!(!matches("*.TXT", true, "a.txt"));
    }
}
