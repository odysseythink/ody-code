use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

mod save_idea_report;

pub const IDEA_SKILL_NAMES: &[&str] = &["idea-generator", "idea-evaluator"];
pub const SENSITIVE_TITLE_WORDS: &[&str] = &["key", "token", "password", "secret", "credential"];
pub const MAX_SUFFIX: u32 = 1000;
pub const GITIGNORE_ENTRY: &str = ".ody-code/";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeaReportInput {
    pub title: String,
    pub content: String,
    #[serde(rename = "type")]
    pub report_type: String,
    pub score: Option<f64>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeaReportType {
    Generator,
    Evaluator,
}

#[derive(Debug, Clone)]
pub struct ValidatedIdeaReportInput {
    pub title: String,
    pub content: String,
    pub report_type: IdeaReportType,
    pub score: Option<f64>,
    pub tags: Option<Vec<String>>,
}

/// Runtime dependencies for `SaveIdeaReportTool`.
/// The host implementation reads the conversation history to decide whether an
/// idea skill is active; the mock implementation is used for golden tests.
pub trait IdeaReportContext: Send + Sync {
    fn is_idea_skill_active(&self) -> bool;
    fn now(&self) -> DateTime<Utc>;
}

pub use save_idea_report::SaveIdeaReportTool;

pub fn validate_idea_report_input(
    input: &IdeaReportInput,
) -> Result<ValidatedIdeaReportInput, String> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err("title is required and must be non-empty".into());
    }
    let lower = title.to_lowercase();
    for word in SENSITIVE_TITLE_WORDS {
        let re = Regex::new(&format!(r"\b{}\b", regex::escape(word))).unwrap();
        if re.is_match(&lower) {
            return Err("title contains sensitive words; provide a different title".into());
        }
    }

    let report_type = match input.report_type.as_str() {
        "generator" => IdeaReportType::Generator,
        "evaluator" => IdeaReportType::Evaluator,
        _ => return Err("type must be \"generator\" or \"evaluator\"".into()),
    };

    if let Some(score) = input.score {
        if !score.is_finite() || score < 0.0 || score > 10.0 {
            return Err("score must be a number between 0 and 10".into());
        }
    }

    let tags = input.tags.as_ref().map(|tags| {
        let mut seen = std::collections::HashSet::new();
        tags.iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty() && seen.insert(t.clone()))
            .collect::<Vec<_>>()
    });

    Ok(ValidatedIdeaReportInput {
        title,
        content: input.content.clone(),
        report_type,
        score: input.score,
        tags,
    })
}

pub fn slugify_title(title: &str) -> String {
    let re = Regex::new(r"[^\p{L}\p{N}]+").unwrap();
    let mut s = re.replace_all(&title.to_lowercase(), "-").to_string();
    s = s.trim_matches('-').to_string();
    s = Regex::new(r"-+").unwrap().replace_all(&s, "-").to_string();
    if s.len() > 50 {
        s = s[..50].trim_end_matches('-').to_string();
    }
    s
}

pub fn strip_date_prefix(slug: &str) -> String {
    Regex::new(r"^(?:\d{4}-\d{2}-\d{2}(?:-|$))+")
        .unwrap()
        .replace(slug, "")
        .to_string()
}

pub fn format_date_prefix(date: &DateTime<Utc>) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn generate_idea_file_path<F>(
    ideas_dir: &str,
    title: &str,
    now: &DateTime<Utc>,
    mut exists: F,
) -> String
where
    F: FnMut(&str) -> bool,
{
    let slug = strip_date_prefix(&slugify_title(title));
    let base_stem = format!(
        "{}-{}",
        format_date_prefix(now),
        if slug.is_empty() { "untitled" } else { &slug }
    );

    for suffix in 1..=MAX_SUFFIX {
        let stem = if suffix == 1 {
            base_stem.clone()
        } else {
            format!("{}-{}", base_stem, suffix - 1)
        };
        let candidate = format!("{}/{}.md", ideas_dir, stem);
        if !exists(&candidate) {
            return candidate;
        }
    }
    format!("{}/{}-{}.md", ideas_dir, base_stem, now.timestamp_millis())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Frontmatter<'a> {
    title: &'a str,
    #[serde(rename = "type")]
    report_type: IdeaReportType,
    date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<&'a Vec<String>>,
}

pub fn build_idea_report_body(input: &ValidatedIdeaReportInput, now: &DateTime<Utc>) -> String {
    let fm = Frontmatter {
        title: &input.title,
        report_type: input.report_type,
        date: now.to_rfc3339(),
        score: input.score,
        tags: input.tags.as_ref(),
    };
    let yaml = serde_yaml::to_string(&fm).unwrap().trim_end().to_string();
    format!("---\n{}\n---\n\n{}\n", yaml, input.content.trim())
}

/// Create the parent directory for the report and keep `.ody-code/` ignored.
pub async fn ensure_ideas_directory(
    kaos: &kaos_rs::kaos::Kaos,
    file_path: &str,
) -> Result<(), String> {
    let parent = std::path::Path::new(file_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Invalid file path: no parent directory".to_string())?;
    kaos.mkdir(&parent, true, true)
        .await
        .map_err(|e| format!("mkdir failed: {}", e))?;
    ensure_gitignore(kaos, &kaos.getcwd()).await
}

async fn ensure_gitignore(kaos: &kaos_rs::kaos::Kaos, cwd: &str) -> Result<(), String> {
    let gitignore_path = format!("{}/.gitignore", cwd);
    let content = match kaos.read_text(&gitignore_path, None, None).await {
        Ok(c) => c,
        Err(_) => {
            kaos.write_text(
                &gitignore_path,
                &format!("{}\n", GITIGNORE_ENTRY),
                Some("w"),
                None,
            )
            .await
            .map_err(|e| format!("write .gitignore: {}", e))?;
            return Ok(());
        }
    };

    if content.trim().is_empty() {
        kaos.write_text(
            &gitignore_path,
            &format!("{}\n", GITIGNORE_ENTRY),
            Some("w"),
            None,
        )
        .await
        .map_err(|e| format!("write .gitignore: {}", e))?;
        return Ok(());
    }

    if content.lines().any(|line| line.trim() == GITIGNORE_ENTRY) {
        return Ok(());
    }

    let separator = if content.ends_with('\n') { "" } else { "\n" };
    kaos.write_text(
        &gitignore_path,
        &format!("{}{}{}\n", content, separator, GITIGNORE_ENTRY),
        Some("w"),
        None,
    )
    .await
    .map_err(|e| format!("write .gitignore: {}", e))?;
    Ok(())
}

/// Deterministic mock context for golden tests.
#[derive(Clone)]
pub struct MockIdeaReportContext {
    active: bool,
    now: DateTime<Utc>,
}

impl MockIdeaReportContext {
    pub fn new(active: bool, now: DateTime<Utc>) -> Self {
        Self { active, now }
    }
}

impl IdeaReportContext for MockIdeaReportContext {
    fn is_idea_skill_active(&self) -> bool {
        self.active
    }
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now() -> DateTime<Utc> {
        "2026-01-02T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
    }

    #[test]
    fn validate_accepts_valid_generator_input() {
        let input = IdeaReportInput {
            title: "B2B AI Assistant".into(),
            content: "# Report\n\nBody.".into(),
            report_type: "generator".into(),
            score: None,
            tags: Some(vec!["B2B".into(), "AI".into()]),
        };
        let v = validate_idea_report_input(&input).unwrap();
        assert_eq!(v.title, "B2B AI Assistant");
        assert_eq!(v.report_type, IdeaReportType::Generator);
        assert_eq!(v.tags.as_ref().unwrap(), &["B2B", "AI"]);
    }

    #[test]
    fn validate_rejects_empty_title() {
        let input = IdeaReportInput {
            title: "   ".into(),
            content: "x".into(),
            report_type: "generator".into(),
            score: None,
            tags: None,
        };
        assert!(validate_idea_report_input(&input).is_err());
    }

    #[test]
    fn validate_rejects_sensitive_title() {
        let input = IdeaReportInput {
            title: "My API Key Idea".into(),
            content: "x".into(),
            report_type: "generator".into(),
            score: None,
            tags: None,
        };
        let err = validate_idea_report_input(&input).unwrap_err();
        assert!(err.contains("sensitive"));
    }

    #[test]
    fn validate_accepts_title_with_substring_that_is_not_a_word_boundary() {
        // "keychain" contains "key" but must survive because it is not a standalone word.
        let input = IdeaReportInput {
            title: "Keychain Product Ideas".into(),
            content: "x".into(),
            report_type: "generator".into(),
            score: None,
            tags: None,
        };
        assert!(validate_idea_report_input(&input).is_ok());
    }

    #[test]
    fn validate_rejects_invalid_type() {
        let input = IdeaReportInput {
            title: "Valid".into(),
            content: "x".into(),
            report_type: "summary".into(),
            score: None,
            tags: None,
        };
        assert!(validate_idea_report_input(&input).is_err());
    }

    #[test]
    fn validate_rejects_score_out_of_bounds() {
        let input = IdeaReportInput {
            title: "Valid".into(),
            content: "x".into(),
            report_type: "evaluator".into(),
            score: Some(11.0),
            tags: None,
        };
        assert!(validate_idea_report_input(&input).is_err());
    }

    #[test]
    fn slugify_and_strip_date_prefix_work() {
        assert_eq!(slugify_title("Hello World!"), "hello-world");
        assert_eq!(strip_date_prefix("2026-01-01-hello"), "hello");
        assert_eq!(
            generate_idea_file_path("/ideas", "Hello", &fixed_now(), |_| false),
            "/ideas/2026-01-02-hello.md"
        );
    }

    #[test]
    fn generate_path_increments_suffix_on_collision() {
        let now = fixed_now();
        let mut calls = 0;
        let path = generate_idea_file_path("/ideas", "Hello", &now, |p| {
            calls += 1;
            p == "/ideas/2026-01-02-hello.md"
        });
        assert_eq!(path, "/ideas/2026-01-02-hello-1.md");
        assert_eq!(calls, 2);
    }

    #[test]
    fn build_body_contains_yaml_frontmatter_and_content() {
        let input = ValidatedIdeaReportInput {
            title: "B2B AI".into(),
            content: "# Report\n\nBody.".into(),
            report_type: IdeaReportType::Evaluator,
            score: Some(8.5),
            tags: Some(vec!["B2B".into(), "AI".into()]),
        };
        let body = build_idea_report_body(&input, &fixed_now());
        assert!(body.starts_with("---\n"));
        assert!(body.contains("title: B2B AI"));
        assert!(body.contains("type: evaluator"));
        assert!(body.contains("score: 8.5"));
        assert!(body.contains("tags:\n- B2B\n- AI"));
        assert!(body.ends_with("# Report\n\nBody.\n"));
    }
}
