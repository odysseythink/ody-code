use reqwest::Client;
use serde::Deserialize;

use crate::config::ProviderConfig;
use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmError, LlmProvider, ToolCallDelta};

pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    default_model: String,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let base_url = config
            .base_url
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        Self {
            api_key: config.api_key,
            base_url,
            default_model: config.default_model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
            client: Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut (dyn FnMut(ChatDelta) + Send),
    ) -> Result<FinishReason, LlmError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed { source: e })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status, body });
        }

        let mut finish_reason = FinishReason::Other;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        let mut buf = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| LlmError::StreamParse {
                message: e.to_string(),
            })?;
            let text = String::from_utf8_lossy(&chunk);
            buf.push_str(&text);

            while let Some(line_end) = buf.find('\n') {
                let line = buf[..line_end].trim().to_string();
                buf = buf[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }
                if !line.starts_with("data: ") {
                    continue;
                }
                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }

                match serde_json::from_str::<SseChunk>(data) {
                    Ok(sse) => {
                        for choice in sse.choices {
                            let delta = ChatDelta {
                                index: choice.index,
                                content: choice.delta.content,
                                tool_call: choice.delta.tool_calls.and_then(|calls| {
                                    calls.into_iter().next().map(|tc| ToolCallDelta {
                                        id: tc.id.unwrap_or_default(),
                                        name: tc.function.name.unwrap_or_default(),
                                        arguments: tc
                                            .function
                                            .arguments
                                            .and_then(|a| serde_json::from_str(&a).ok())
                                            .unwrap_or(serde_json::Value::Null),
                                    })
                                }),
                            };
                            on_delta(delta);

                            if let Some(ref reason) = choice.finish_reason {
                                finish_reason = match reason.as_str() {
                                    "stop" => FinishReason::Stop,
                                    "tool_calls" => FinishReason::ToolCalls,
                                    "length" => FinishReason::Length,
                                    "content_filter" => FinishReason::ContentFilter,
                                    _ => FinishReason::Other,
                                };
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("SSE parse warning: {e} for line: {data}");
                    }
                }
            }
        }

        Ok(finish_reason)
    }
}

#[derive(Debug, Deserialize)]
struct SseChunk {
    choices: Vec<SseChoice>,
}

#[derive(Debug, Deserialize)]
struct SseChoice {
    #[serde(default)]
    index: usize,
    delta: SseDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    content: Option<String>,
    role: Option<String>,
    tool_calls: Option<Vec<SseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct SseToolCall {
    id: Option<String>,
    #[serde(rename = "type")]
    call_type: Option<String>,
    function: SseFunction,
}

#[derive(Debug, Deserialize)]
struct SseFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use crate::llm::{ChatRequest, FinishReason, Message, Role};

    #[tokio::test]
    async fn streams_text_deltas_from_sse() {
        let server = httptest::ServerBuilder::new()
            .bind_addr("127.0.0.1:0".parse().unwrap())
            .run()
            .unwrap();
        server.expect(
            httptest::Expectation::matching(httptest::matchers::request::method_path("POST", "/v1/chat/completions"))
                .respond_with(httptest::responders::status_code(200).body(
                    "data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\n\
                     data: {\"id\":\"2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\n\
                     data: {\"id\":\"3\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                     data: [DONE]\n\n",
                )),
        );

        let base = server.url_str("/v1");
        let provider = OpenAiProvider::new(ProviderConfig {
            provider_id: "openai".to_string(),
            api_key: "sk-test".to_string(),
            base_url: Some(base),
            default_model: Some("gpt-4o-mini".to_string()),
        });

        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message { role: Role::User, content: "hi".to_string() }],
            tools: vec![],
            stream: true,
        };

        let mut deltas = Vec::new();
        let reason = provider.chat_stream(request, &mut |d| {
            if let Some(c) = d.content { deltas.push(c); }
        }).await.unwrap();

        assert_eq!(deltas, vec!["Hello", " world"]);
        assert_eq!(reason, FinishReason::Stop);
    }
}
