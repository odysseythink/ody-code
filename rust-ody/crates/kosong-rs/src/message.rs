use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlPayload {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Think {
        think: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted: Option<String>,
    },
    #[serde(rename = "image_url")]
    ImageUrl {
        #[serde(rename = "imageUrl")]
        image_url: UrlPayload,
    },
    #[serde(rename = "audio_url")]
    AudioUrl {
        #[serde(rename = "audioUrl")]
        audio_url: UrlPayload,
    },
    #[serde(rename = "video_url")]
    VideoUrl {
        #[serde(rename = "videoUrl")]
        video_url: UrlPayload,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(rename = "type")]
    pub call_type: String,
    pub id: String,
    pub name: String,
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
    #[serde(rename = "_streamIndex", skip_serializing_if = "Option::is_none")]
    pub stream_index: Option<StreamIndex>,
}

pub type StreamIndex = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallPart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(rename = "argumentsPart")]
    pub arguments_part: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<StreamIndex>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamedMessagePart {
    Content(ContentPart),
    ToolCall(ToolCall),
    ToolCallPart(ToolCallPart),
}

impl StreamedMessagePart {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Content(ContentPart::Text { text: s.into() })
    }

    pub fn think(s: impl Into<String>) -> Self {
        Self::Content(ContentPart::Think {
            think: s.into(),
            encrypted: None,
        })
    }

    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: Option<&str>,
    ) -> Self {
        Self::ToolCall(ToolCall {
            call_type: "function".to_string(),
            id: id.into(),
            name: name.into(),
            arguments: arguments.map(Into::into),
            extras: None,
            stream_index: None,
        })
    }

    pub fn tool_call_part(arguments_part: Option<&str>) -> Self {
        Self::ToolCallPart(ToolCallPart {
            part_type: "tool_call_part".to_string(),
            arguments_part: arguments_part.map(Into::into),
            index: None,
        })
    }

    pub fn index(&self) -> Option<&serde_json::Value> {
        match self {
            StreamedMessagePart::ToolCallPart(p) => p.index.as_ref(),
            StreamedMessagePart::ToolCall(tc) => tc.stream_index.as_ref(),
            _ => None,
        }
    }
}

pub fn is_content_part(part: &StreamedMessagePart) -> bool {
    matches!(part, StreamedMessagePart::Content(_))
}

pub fn is_tool_call(part: &StreamedMessagePart) -> bool {
    matches!(part, StreamedMessagePart::ToolCall(_))
}

pub fn is_tool_call_part(part: &StreamedMessagePart) -> bool {
    matches!(part, StreamedMessagePart::ToolCallPart(_))
}

pub fn merge_in_place(target: &mut StreamedMessagePart, source: &StreamedMessagePart) -> bool {
    use StreamedMessagePart::*;
    match (target, source) {
        (Content(ContentPart::Text { text: a }), Content(ContentPart::Text { text: b })) => {
            a.push_str(b);
            true
        }
        (
            Content(ContentPart::Think {
                think: a,
                encrypted: ea,
            }),
            Content(ContentPart::Think {
                think: b,
                encrypted: eb,
            }),
        ) => {
            if ea.is_some() {
                return false;
            }
            a.push_str(b);
            if let Some(sig) = eb {
                *ea = Some(sig.clone());
            }
            true
        }
        (ToolCall(tc), ToolCallPart(delta)) => {
            if let Some(delta_args) = &delta.arguments_part {
                tc.arguments = Some(match &tc.arguments {
                    Some(existing) => format!("{}{}", existing, delta_args),
                    None => delta_args.clone(),
                });
            }
            true
        }
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: Vec<ContentPart>,
    #[serde(rename = "toolCalls")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
}

impl Message {
    pub fn assistant(content: Vec<ContentPart>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            name: None,
            content,
            tool_calls,
            tool_call_id: None,
            partial: None,
        }
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }
    }
}

pub fn extract_text(message: &Message, sep: &str) -> String {
    message
        .content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_consecutive_text_parts() {
        let mut a = StreamedMessagePart::text("hello");
        let b = StreamedMessagePart::text(" world");
        assert!(merge_in_place(&mut a, &b));
        assert_eq!(a, StreamedMessagePart::text("hello world"));
    }

    #[test]
    fn merges_consecutive_think_parts() {
        let mut a = StreamedMessagePart::think("step1");
        let b = StreamedMessagePart::think(" step2");
        assert!(merge_in_place(&mut a, &b));
        assert_eq!(a, StreamedMessagePart::think("step1 step2"));
    }

    #[test]
    fn appends_tool_call_part_to_tool_call() {
        let mut a = StreamedMessagePart::tool_call("tc_1", "read", None);
        let b = StreamedMessagePart::tool_call_part(Some("{\"a\":1}"));
        assert!(merge_in_place(&mut a, &b));
        match a {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.arguments.as_deref(), Some("{\"a\":1}"));
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn refuses_incompatible_merge() {
        let mut a = StreamedMessagePart::text("hello");
        let b = StreamedMessagePart::think("reason");
        assert!(!merge_in_place(&mut a, &b));
        assert_eq!(a, StreamedMessagePart::text("hello"));
    }

    #[test]
    fn message_serializes_to_ts_shape() {
        let msg = Message::user_text("hi");
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"][0]["type"], "text");
        assert_eq!(v["content"][0]["text"], "hi");
        assert!(v["toolCalls"].is_array());
    }

    #[test]
    fn streamed_message_part_round_trips_through_untagged_json() {
        let cases: Vec<(StreamedMessagePart, &str)> = vec![
            (
                StreamedMessagePart::text("hello"),
                r#"{"type":"text","text":"hello"}"#,
            ),
            (
                StreamedMessagePart::think("step"),
                r#"{"type":"think","think":"step"}"#,
            ),
            (
                StreamedMessagePart::tool_call("call_1", "read", Some("{}")),
                r#"{"type":"function","id":"call_1","name":"read","arguments":"{}"}"#,
            ),
            (
                StreamedMessagePart::tool_call_part(Some("{\"a\":1}")),
                r#"{"type":"tool_call_part","argumentsPart":"{\"a\":1}"}"#,
            ),
        ];
        for (original, json) in cases {
            let parsed: StreamedMessagePart = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, original);
            let serialized = serde_json::to_string(&original).unwrap();
            assert_eq!(serialized, json);
        }
    }
}
