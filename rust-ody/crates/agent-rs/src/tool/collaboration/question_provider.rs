use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tools_rs::builtin::collaboration::{
    QuestionError, QuestionProvider, QuestionRequest, QuestionResult,
};
use tools_rs::builtin::AbortSignal;

pub type QuestionCallback = Arc<
    dyn Fn(
            QuestionRequest,
            AbortSignal,
        ) -> Pin<Box<dyn Future<Output = Result<QuestionResult, QuestionError>> + Send>>
        + Send
        + Sync,
>;

pub struct AgentQuestionProvider {
    callback: QuestionCallback,
}

impl AgentQuestionProvider {
    pub fn new(callback: QuestionCallback) -> Self {
        Self { callback }
    }
}

#[async_trait::async_trait]
impl QuestionProvider for AgentQuestionProvider {
    async fn request_question(
        &self,
        req: QuestionRequest,
        signal: &AbortSignal,
    ) -> Result<QuestionResult, QuestionError> {
        (self.callback)(req, signal.clone()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tools_rs::builtin::collaboration::{QuestionAnswers, QuestionItem, QuestionOption};

    #[tokio::test]
    async fn callback_receives_request_and_returns_answers() {
        let captured = Arc::new(Mutex::new(None::<QuestionRequest>));
        let captured_clone = Arc::clone(&captured);
        let callback: QuestionCallback = Arc::new(move |req, _signal| {
            *captured_clone.lock().unwrap() = Some(req);
            Box::pin(async move {
                let mut answers = HashMap::new();
                answers.insert("Pick a color?".into(), serde_json::json!("Red"));
                Ok(QuestionResult::Answers(QuestionAnswers {
                    answers,
                    method: Some("enter".into()),
                }))
            })
        });

        let provider = AgentQuestionProvider::new(callback);
        let result = provider
            .request_question(
                QuestionRequest {
                    turn_id: Some(7),
                    tool_call_id: "call_q".into(),
                    questions: vec![QuestionItem {
                        question: "Pick a color?".into(),
                        header: "Style".into(),
                        options: vec![
                            QuestionOption {
                                label: "Red".into(),
                                description: "warm".into(),
                            },
                            QuestionOption {
                                label: "Blue".into(),
                                description: "cool".into(),
                            },
                        ],
                        multi_select: false,
                    }],
                },
                &AbortSignal::new(),
            )
            .await
            .unwrap();

        match result {
            QuestionResult::Answers(a) => {
                assert_eq!(
                    a.answers.get("Pick a color?"),
                    Some(&serde_json::json!("Red"))
                );
            }
            _ => panic!("expected answers"),
        }

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.turn_id, Some(7));
        assert_eq!(req.tool_call_id, "call_q");
        assert_eq!(req.questions.len(), 1);
    }
}
