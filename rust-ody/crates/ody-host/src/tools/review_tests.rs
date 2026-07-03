use std::sync::Arc;

use async_trait::async_trait;
use tools_rs::builtin::test_review::{
    AdvancedSessionReviewResult, AuditLevel, TestReviewError, TestReviewer,
};
use tools_rs::builtin::AbortSignal;

use crate::llm::{ChatRequest, ContentPart, LlmProvider, Message, Role};

pub struct LlmTestReviewer {
    provider: Arc<dyn LlmProvider>,
    model: String,
}

impl LlmTestReviewer {
    pub fn new(provider: Arc<dyn LlmProvider>, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
        }
    }
}

pub fn parse_audit_level_from_content(content: &str) -> AuditLevel {
    tools_rs::builtin::test_review::parse_audit_level(content)
}

pub fn parse_review_response(raw: &str) -> Option<AdvancedSessionReviewResult> {
    let findings = tools_rs::builtin::test_review::parse_findings(raw)?;
    let probes = tools_rs::builtin::test_review::parse_mutation_probes(raw);
    Some(AdvancedSessionReviewResult {
        audit_level: tools_rs::builtin::test_review::parse_audit_level(raw),
        findings,
        mutation_probes: if probes.is_empty() {
            None
        } else {
            Some(probes)
        },
        ok: true,
        note: None,
    })
}

#[async_trait]
impl TestReviewer for LlmTestReviewer {
    async fn review_tests(
        &self,
        content: &str,
        _reviewer_alias: &str,
        signal: &AbortSignal,
    ) -> Result<AdvancedSessionReviewResult, TestReviewError> {
        if signal.aborted() {
            return Err(TestReviewError::Cancelled);
        }
        let prompt = tools_rs::builtin::test_review::build_critic_prompt();
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text {
                        text: prompt.into(),
                    }],
                },
                Message {
                    role: Role::User,
                    content: vec![ContentPart::Text {
                        text: content.into(),
                    }],
                },
            ],
            tools: vec![],
            stream: false,
        };
        let mut output = String::new();
        let result = self
            .provider
            .chat_stream(request, &mut |delta| {
                if let Some(text) = delta.content {
                    output.push_str(&text);
                }
            })
            .await;
        if signal.aborted() {
            return Err(TestReviewError::Cancelled);
        }
        if let Err(e) = result {
            return Err(TestReviewError::GenerationFailed(e.to_string()));
        }
        match parse_review_response(&output) {
            Some(result) => Ok(result),
            None => Err(TestReviewError::Unparseable),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools_rs::builtin::test_review::{Confidence, Severity};
    use tools_rs::builtin::AbortSignal;

    #[tokio::test]
    async fn parses_mock_llm_response() {
        let raw = r#"{"findings":[{"severity":"high","confidence":"certain","title":"Tautology","detail":"asserts x==x"}]}"#;
        let result = parse_review_response(raw).unwrap();
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, Severity::High);
        assert_eq!(result.findings[0].confidence, Some(Confidence::Certain));
    }

    #[test]
    fn parses_audit_level_from_content() {
        assert_eq!(
            parse_audit_level_from_content("## Audit Level\n**Deep**"),
            AuditLevel::Deep
        );
    }
}
