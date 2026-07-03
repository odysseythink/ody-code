use serde::{Deserialize, Serialize};

mod run_e2e_tests;
pub use run_e2e_tests::RunE2ETestsTool;

use crate::builtin::AbortSignal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailurePolicy {
    Block,
    Warn,
    Ignore,
}

impl Default for FailurePolicy {
    fn default() -> Self {
        FailurePolicy::Warn
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct E2EConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub failure_policy: FailurePolicy,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_generated_test_dir")]
    pub generated_test_dir: String,
}

impl Default for E2EConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_policy: FailurePolicy::default(),
            max_concurrency: default_max_concurrency(),
            generated_test_dir: default_generated_test_dir(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_max_concurrency() -> usize {
    4
}

fn default_generated_test_dir() -> String {
    ".ody-code/test-generated".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedTool {
    pub tool_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct E2EImpact {
    pub affected_tools: Vec<AffectedTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct E2EResult {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub summary: String,
    pub test_files: Vec<String>,
}

impl E2EResult {
    pub fn is_error(&self, policy: &FailurePolicy) -> bool {
        self.failed > 0 && matches!(policy, FailurePolicy::Block)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum E2ETestRunnerError {
    #[error("no E2E generator found for project")]
    NoGenerator,
    #[error("E2E generation produced no test files")]
    NoTestsGenerated,
    #[error("E2E execution failed: {0}")]
    ExecutionFailed(String),
    #[error("E2E tests cancelled")]
    Cancelled,
}

#[async_trait::async_trait]
pub trait E2ETestRunner: Send + Sync {
    async fn detect_generator(&self, project_root: &str) -> Result<(), E2ETestRunnerError>;

    async fn analyze_impact(
        &self,
        changed_files: &[String],
        config: &E2EConfig,
        project_root: &str,
    ) -> Result<E2EImpact, E2ETestRunnerError>;

    async fn generate_tests(
        &self,
        tool: &AffectedTool,
        changed_files: &[String],
        project_root: &str,
        generated_test_dir: &str,
    ) -> Result<Vec<String>, E2ETestRunnerError>;

    async fn run_e2e_tests(
        &self,
        test_files: &[String],
        project_root: &str,
        signal: &AbortSignal,
    ) -> Result<E2EResult, E2ETestRunnerError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn default_e2e_config_is_enabled_warn_policy() {
        let cfg = E2EConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.failure_policy, FailurePolicy::Warn);
        assert_eq!(cfg.max_concurrency, 4);
        assert_eq!(cfg.generated_test_dir, ".ody-code/test-generated");
    }

    #[test]
    fn failure_policy_deserializes_from_snake_case() {
        let cfg: E2EConfig =
            serde_json::from_str(r#"{"failurePolicy":"block","maxConcurrency":8}"#).unwrap();
        assert_eq!(cfg.failure_policy, FailurePolicy::Block);
        assert_eq!(cfg.max_concurrency, 8);
    }

    #[test]
    fn e2e_result_summary_includes_counts() {
        let result = E2EResult {
            passed: 5,
            failed: 1,
            skipped: 0,
            summary: "custom".to_string(),
            test_files: vec!["a.test.ts".to_string()],
        };
        assert!(result.is_error(&FailurePolicy::Block));
        assert!(!result.is_error(&FailurePolicy::Ignore));
    }

    #[derive(Clone)]
    struct MockRunner {
        result: E2EResult,
    }

    #[async_trait::async_trait]
    impl E2ETestRunner for MockRunner {
        async fn detect_generator(&self, _root: &str) -> Result<(), E2ETestRunnerError> {
            Ok(())
        }

        async fn analyze_impact(
            &self,
            _changed_files: &[String],
            _config: &E2EConfig,
            _root: &str,
        ) -> Result<E2EImpact, E2ETestRunnerError> {
            Ok(E2EImpact {
                affected_tools: vec![],
            })
        }

        async fn generate_tests(
            &self,
            _tool: &AffectedTool,
            _changed_files: &[String],
            _root: &str,
            _dir: &str,
        ) -> Result<Vec<String>, E2ETestRunnerError> {
            Ok(vec![])
        }

        async fn run_e2e_tests(
            &self,
            _test_files: &[String],
            _root: &str,
            _signal: &AbortSignal,
        ) -> Result<E2EResult, E2ETestRunnerError> {
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn mock_runner_returns_expected_result() {
        let expected = E2EResult {
            passed: 1,
            failed: 0,
            skipped: 0,
            summary: "ok".to_string(),
            test_files: vec!["foo.test.ts".to_string()],
        };
        let runner = Arc::new(MockRunner {
            result: expected.clone(),
        });
        let got = runner
            .run_e2e_tests(&[], "/tmp", &AbortSignal::new())
            .await
            .unwrap();
        assert_eq!(got.passed, expected.passed);
        assert_eq!(got.test_files, expected.test_files);
    }
}
