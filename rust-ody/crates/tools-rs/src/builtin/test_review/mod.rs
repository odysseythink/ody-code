use serde::{Deserialize, Serialize};

mod review_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Med,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Certain,
    Likely,
    Speculative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AuditLevel {
    Basic,
    Standard,
    Deep,
}

impl Default for AuditLevel {
    fn default() -> Self {
        AuditLevel::Standard
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    pub title: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationProbe {
    pub location: String,
    pub mutation: String,
    pub expected_catch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedSessionReviewResult {
    pub audit_level: AuditLevel,
    pub findings: Vec<ReviewFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation_probes: Option<Vec<MutationProbe>>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub fn escalated_severities(level: AuditLevel) -> &'static [Severity] {
    match level {
        AuditLevel::Basic => &[Severity::High],
        AuditLevel::Standard => &[Severity::High, Severity::Med],
        AuditLevel::Deep => &[Severity::High, Severity::Med, Severity::Low],
    }
}

pub fn should_escalate(
    severity: Severity,
    confidence: Option<Confidence>,
    level: AuditLevel,
) -> bool {
    escalated_severities(level).contains(&severity) && confidence != Some(Confidence::Speculative)
}

pub fn parse_audit_level(content: &str) -> AuditLevel {
    let re =
        regex::Regex::new(r"##\s*Audit Level[\s\S]{0,300}?\*\*\s*(Basic|Standard|Deep)\s*\*\*")
            .unwrap();
    re.captures(content)
        .and_then(|caps| caps.get(1))
        .map(|m| match m.as_str().to_lowercase().as_str() {
            "basic" => AuditLevel::Basic,
            "deep" => AuditLevel::Deep,
            _ => AuditLevel::Standard,
        })
        .unwrap_or_default()
}

pub fn parse_findings(raw: &str) -> Option<Vec<ReviewFinding>> {
    let binding = strip_code_fences(raw);
    let stripped = binding.trim();
    if stripped.is_empty() {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(stripped).ok().or_else(|| {
        let start = stripped.find('{')?;
        let end = stripped.rfind('}')?;
        if end <= start {
            return None;
        }
        serde_json::from_str(&stripped[start..=end]).ok()
    })?;
    let raw_findings = parsed.get("findings")?;
    if !raw_findings.is_array() {
        return None;
    }
    let mut findings = Vec::new();
    for entry in raw_findings.as_array().unwrap() {
        if let Some(f) = coerce_finding(entry) {
            findings.push(f);
        }
    }
    Some(findings)
}

pub fn parse_mutation_probes(raw: &str) -> Vec<MutationProbe> {
    let binding = strip_code_fences(raw);
    let stripped = binding.trim();
    if stripped.is_empty() {
        return Vec::new();
    }
    let parsed: serde_json::Value = serde_json::from_str(stripped)
        .ok()
        .or_else(|| {
            let start = stripped.find('{')?;
            let end = stripped.rfind('}')?;
            if end <= start {
                return None;
            }
            serde_json::from_str(&stripped[start..=end]).ok()
        })
        .unwrap_or(serde_json::Value::Null);
    let raw_probes = match parsed.get("mutationProbes") {
        Some(v) if v.is_array() => v.as_array().unwrap(),
        _ => return Vec::new(),
    };
    raw_probes.iter().filter_map(coerce_probe).collect()
}

fn coerce_finding(entry: &serde_json::Value) -> Option<ReviewFinding> {
    let obj = entry.as_object()?;
    let severity = match obj.get("severity")?.as_str()? {
        "high" => Severity::High,
        "med" => Severity::Med,
        "low" => Severity::Low,
        _ => return None,
    };
    let title = obj
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let detail = obj
        .get("detail")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if title.is_empty() && detail.is_empty() {
        return None;
    }
    let location = obj
        .get("location")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let suggested_fix = obj
        .get("suggestedFix")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let confidence = obj.get("confidence").and_then(|v| match v.as_str()? {
        "certain" => Some(Confidence::Certain),
        "likely" => Some(Confidence::Likely),
        "speculative" => Some(Confidence::Speculative),
        _ => None,
    });
    Some(ReviewFinding {
        severity,
        confidence,
        title: if title.is_empty() {
            detail.chars().take(60).collect()
        } else {
            title
        },
        detail,
        location,
        suggested_fix,
    })
}

fn coerce_probe(entry: &serde_json::Value) -> Option<MutationProbe> {
    let obj = entry.as_object()?;
    let location = obj.get("location")?.as_str()?.trim().to_string();
    let mutation = obj.get("mutation")?.as_str()?.trim().to_string();
    let expected_catch = obj
        .get("expectedCatch")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if mutation.is_empty() {
        return None;
    }
    Some(MutationProbe {
        location,
        mutation,
        expected_catch,
    })
}

fn strip_code_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return raw.to_string();
    }
    let end = trimmed.rfind("```");
    if end.map(|e| e <= 3).unwrap_or(true) {
        return raw.to_string();
    }
    let first_newline = trimmed.find('\n');
    let start = first_newline.map(|n| n + 1).unwrap_or(3);
    trimmed[start..end.unwrap()].trim_end().to_string()
}

pub fn format_report(
    result: &AdvancedSessionReviewResult,
    reviewer_alias: &str,
    test_files: &[String],
) -> String {
    let mut out = Vec::new();
    out.push(format!(
        "# Independent test review (reviewer: {})",
        reviewer_alias
    ));
    out.push(format!(
        "Reviewed {} changed test file(s): {}",
        test_files.len(),
        test_files.join(", ")
    ));
    if result.findings.is_empty() {
        out.push("\n**Findings:** none — the reviewer could not break the tests.".to_string());
    } else {
        out.push(format!("\n## Findings ({})", result.findings.len()));
        for f in &result.findings {
            out.push(format_finding(f, result.audit_level));
        }
    }
    let probes = result.mutation_probes.as_deref().unwrap_or_default();
    if !probes.is_empty() {
        out.push(format!(
            "\n## Mutation probes ({}) — RUN THESE",
            probes.len()
        ));
        out.push(
            "The reviewer cannot run code. For EACH probe: apply the one-line break, run the named test, \
             and observe the result. If the test stays GREEN under the break, that test is vacuous (a confirmed \
             defect) — fix the test. Then REVERT the break. Report caught/missed for each."
                .to_string(),
        );
        for (i, p) in probes.iter().enumerate() {
            out.push(format_probe(p, i));
        }
    } else {
        out.push("\n_No mutation probes were emitted._._.".to_string());
    }
    out.join("\n")
}

fn format_finding(f: &ReviewFinding, audit_level: AuditLevel) -> String {
    let escalate = should_escalate(f.severity, f.confidence, audit_level);
    let mut tags = vec![format!("{:?}", f.severity).to_uppercase()];
    tags.push(
        f.confidence
            .map(|c| format!("{:?}", c).to_lowercase())
            .unwrap_or_else(|| "unrated".to_string()),
    );
    if escalate {
        tags.push("ESCALATE".to_string());
    }
    let mut lines = vec![format!("- **[{}] {}**", tags.join(" / "), f.title)];
    lines.push(format!("  {}", f.detail));
    if let Some(loc) = &f.location {
        lines.push(format!("  _at {}_", loc));
    }
    if let Some(fix) = &f.suggested_fix {
        lines.push(format!("  _fix: {}_", fix));
    }
    lines.join("\n")
}

fn format_probe(p: &MutationProbe, i: usize) -> String {
    let where_ = if p.location.is_empty() {
        String::new()
    } else {
        format!(" at `{}`", p.location)
    };
    let expect = if p.expected_catch.is_empty() {
        String::new()
    } else {
        format!(" — should be caught by: {}", p.expected_catch)
    };
    format!("{}. {}{}{}", i + 1, p.mutation, where_, expect)
}

pub use review_tests::ReviewTestsTool;

use crate::builtin::AbortSignal;

#[derive(Debug, thiserror::Error)]
pub enum TestReviewError {
    #[error("reviewer model unavailable: {0}")]
    ModelUnavailable(String),
    #[error("review generation failed: {0}")]
    GenerationFailed(String),
    #[error("reviewer output could not be parsed")]
    Unparseable,
    #[error("review cancelled")]
    Cancelled,
}

#[async_trait::async_trait]
pub trait TestReviewer: Send + Sync {
    async fn review_tests(
        &self,
        content: &str,
        reviewer_alias: &str,
        signal: &AbortSignal,
    ) -> Result<AdvancedSessionReviewResult, TestReviewError>;
}

const TEST_CODE_ATTACK_SURFACE: &str = "- Tautology: an assertion that re-states a value the implementation itself computed, or that mirrors the test setup rather than exercising independent behaviour.
- Mock theatre: tests that only verify mocks were called in the order the test itself arranged, without asserting real outcomes.
- Happy-path only: no must-reject or edge cases for invalid / empty / malformed inputs.
- Weak assertions: assertions that would pass even if the implementation were broken (e.g., toBeDefined, truthy checks, snapshot updates that hide real changes).
- Unguarded behaviour: tests that rely on shared mutable state, non-deterministic time, network, or file system without isolation.
- Assertion-vs-constant contradiction: an assertion that checks a literal copied from the test setup instead of an independently computed expectation.";

pub fn build_critic_prompt() -> String {
    format!(
        "You are an ADVERSARY reviewing changed tests. Your job is to find ways to BREAK the tests or show they are vacuous. Do not be polite; be ruthless but fair.

Attack surface:
{}

Rules of engagement:
1. Only report concrete problems with the changed tests.
2. For each finding, emit severity (high/med/low), confidence (certain/likely/speculative), title, detail, location, and suggestedFix.
3. Emit mutationProbes: one-line changes that should make a test fail. If a test stays green under a probe, it is vacuous.
4. Respond with STRICT JSON matching {{ auditLevel, findings[], mutationProbes[], ok, note? }}.
5. If no problems are found, set ok:true and findings:[].",
        TEST_CODE_ATTACK_SURFACE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_confidence_escalation_standard() {
        assert!(should_escalate(
            Severity::High,
            Some(Confidence::Certain),
            AuditLevel::Standard
        ));
        assert!(should_escalate(
            Severity::Med,
            Some(Confidence::Likely),
            AuditLevel::Standard
        ));
        assert!(!should_escalate(
            Severity::Low,
            Some(Confidence::Certain),
            AuditLevel::Standard
        ));
        assert!(!should_escalate(
            Severity::High,
            Some(Confidence::Speculative),
            AuditLevel::Standard
        ));
    }

    #[test]
    fn audit_level_parsed_from_content() {
        assert_eq!(
            parse_audit_level("## Audit Level\n**Basic**"),
            AuditLevel::Basic
        );
        assert_eq!(parse_audit_level("no level"), AuditLevel::Standard);
    }

    #[test]
    fn parse_findings_extracts_valid_entries() {
        let raw = r#"{"findings":[{"severity":"high","confidence":"certain","title":"Tautology","detail":"asserts result==result","location":"foo.test.ts:12","suggestedFix":"assert actual value"}]}"#;
        let findings = parse_findings(raw).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.severity, Severity::High);
        assert_eq!(f.confidence, Some(Confidence::Certain));
        assert_eq!(f.title, "Tautology");
    }

    #[test]
    fn parse_findings_returns_none_for_invalid_json() {
        assert!(parse_findings("not json").is_none());
    }

    #[test]
    fn parse_mutation_probes_skips_probe_without_mutation() {
        let raw = r#"{"findings":[],"mutationProbes":[{"location":"a.ts:1","mutation":"","expectedCatch":"t"}]}"#;
        let probes = parse_mutation_probes(raw);
        assert!(probes.is_empty());
    }

    #[test]
    fn format_report_renders_findings_and_probes() {
        let result = AdvancedSessionReviewResult {
            audit_level: AuditLevel::Standard,
            findings: vec![ReviewFinding {
                severity: Severity::Med,
                confidence: Some(Confidence::Likely),
                title: "Weak assertion".to_string(),
                detail: "uses toBeDefined where value matters".to_string(),
                location: Some("bar.test.ts:8".to_string()),
                suggested_fix: Some("assert exact value".to_string()),
            }],
            mutation_probes: Some(vec![MutationProbe {
                location: "bar.ts:4".to_string(),
                mutation: "return 0;".to_string(),
                expected_catch: "bar should compute sum".to_string(),
            }]),
            ok: true,
            note: None,
        };
        let out = format_report(&result, "kimi-for-coding", &["bar.test.ts".to_string()]);
        assert!(out.contains("Independent test review"));
        assert!(out.contains("Weak assertion"));
        assert!(out.contains("bar.test.ts"));
        assert!(out.contains("return 0;"));
    }

    #[derive(Clone)]
    struct MockReviewer {
        result: AdvancedSessionReviewResult,
    }

    #[async_trait::async_trait]
    impl TestReviewer for MockReviewer {
        async fn review_tests(
            &self,
            _content: &str,
            _reviewer_alias: &str,
            _signal: &AbortSignal,
        ) -> Result<AdvancedSessionReviewResult, TestReviewError> {
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn mock_reviewer_returns_expected_result() {
        let expected = AdvancedSessionReviewResult {
            audit_level: AuditLevel::Deep,
            findings: vec![],
            mutation_probes: None,
            ok: true,
            note: None,
        };
        let reviewer = MockReviewer {
            result: expected.clone(),
        };
        let got = reviewer
            .review_tests("content", "alias", &AbortSignal::new())
            .await
            .unwrap();
        assert_eq!(got.audit_level, expected.audit_level);
        assert!(got.findings.is_empty());
    }

    #[test]
    fn critic_prompt_for_tests_mentions_tautology_and_mutation_probes() {
        let prompt = build_critic_prompt().to_lowercase();
        assert!(prompt.contains("tautology"));
        assert!(prompt.contains("mutationprobes"));
        assert!(prompt.contains("strict json"));
    }
}
