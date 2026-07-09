use regex::Regex;

/// Sensitive words that disqualify a generated topic.
const SENSITIVE_TOPIC_WORDS: &[&str] = &["key", "token", "password", "secret", "credential"];

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Lowercase + hyphen-separate, max 50 chars, strip non-alphanumeric (except
/// hyphens).
/// Mirrors TS `slugifyTitle`.
pub fn slugify_title(title: &str) -> String {
    let mut slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Collapse multiple hyphens/spaces into single hyphens
    slug = Regex::new(r"[\s-]+")
        .unwrap()
        .replace_all(&slug, "-")
        .to_string();

    // Trim leading/trailing hyphens
    slug = slug.trim_matches('-').to_string();

    // Truncate to 50 chars
    if slug.len() > 50 {
        slug.truncate(50);
        slug = slug.trim_end_matches('-').to_string();
    }

    slug
}

/// Extract the first markdown H1 heading from content.
/// Looks for a line starting with `"# "`.
/// Mirrors TS `extractFirstHeading`.
pub fn extract_first_heading(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix("# ") {
            if !stripped.is_empty() {
                return Some(stripped.to_string());
            }
        }
    }
    None
}

/// Strip a `YYYY-MM-DD-` date prefix from a slug.
/// Mirrors TS `stripDatePrefix`.
pub fn strip_date_prefix(slug: &str) -> String {
    let re = Regex::new(r"^\d{4}-\d{2}-\d{2}-").unwrap();
    re.replace(slug, "").to_string()
}

/// Check whether a topic (lowercased) contains any sensitive word.
/// Mirrors TS sensitive-word check.
pub fn topic_contains_sensitive_word(topic: &str) -> bool {
    let lower = topic.to_lowercase();
    SENSITIVE_TOPIC_WORDS.iter().any(|w| lower.contains(w))
}

/// Strip locators (unix/windows paths, URLs) from user text before topic
/// extraction.  Mirrors TS `stripLocators`.
pub fn strip_locators(text: &str) -> String {
    // Absolute unix paths like `/home/user/file.txt` (repeated segments separated
    // by `/`).
    let re_path = Regex::new(r"(?:/[^\s,]+)+").unwrap();
    // URLs.
    let re_url = Regex::new(r"https?://[^\s]+").unwrap();

    let result = re_path.replace_all(text, "");
    let result = re_url.replace_all(&result, "");
    result.trim().to_string()
}

/// Return today's date in UTC as `YYYY-MM-DD`.
/// Mirrors TS `formatDatePrefix`.
pub fn format_date_prefix() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();

    // seconds since epoch → days since 1970-01-01
    let total_days = (now.as_secs() / 86400) as i64;
    let (year, month, day) = civil_from_days(total_days);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

// ---------------------------------------------------------------------------
// Internal: civil date from days since 1970-01-01
// Algorithm from Howard Hinnant: http://howardhinnant.github.io/date_algorithms.html
// ---------------------------------------------------------------------------
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_title_basic() {
        assert_eq!(slugify_title("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_title_special_chars() {
        assert_eq!(slugify_title("Foo: Bar & Baz!"), "foo-bar-baz");
    }

    #[test]
    fn slugify_title_truncates_to_50() {
        let long = "a".repeat(100);
        let slug = slugify_title(&long);
        assert!(slug.len() <= 50);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn extract_first_heading_finds_h1() {
        let content = "# My Title\nSome text\n## Subtitle";
        assert_eq!(extract_first_heading(content), Some("My Title".into()));
    }

    #[test]
    fn extract_first_heading_no_h1() {
        let content = "Some text\n## Subtitle";
        assert_eq!(extract_first_heading(content), None);
    }

    #[test]
    fn topic_contains_sensitive_word_detects() {
        assert!(topic_contains_sensitive_word("my-api-key"));
        assert!(!topic_contains_sensitive_word("my-safe-topic"));
    }

    #[test]
    fn strip_date_prefix_removes_iso_date() {
        assert_eq!(strip_date_prefix("2026-06-28-my-plan"), "my-plan");
    }

    #[test]
    fn strip_date_prefix_no_date() {
        assert_eq!(strip_date_prefix("my-plan"), "my-plan");
    }

    #[test]
    fn format_date_prefix_produces_iso_date() {
        let date = format_date_prefix();
        // Must be 10 chars: YYYY-MM-DD
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
        // Parseable as numbers
        let year: i64 = date[0..4].parse().unwrap();
        let month: u32 = date[5..7].parse().unwrap();
        let day: u32 = date[8..10].parse().unwrap();
        assert!(year >= 2026);
        assert!((1..=12).contains(&month));
        assert!((1..=31).contains(&day));
    }
}
