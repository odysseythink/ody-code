use crate::provider::ModelCapability;

const OPENAI_RESPONSES_DEVELOPER_ROLE_MODELS: &[&str] = &[
    "gpt-4.1",
    "gpt-4.1-mini",
    "gpt-4.1-nano",
    "gpt-5-codex",
    "o1",
    "o1-mini",
    "o1-pro",
    "o3",
    "o3-mini",
    "o3-pro",
    "o4-mini",
];

const OPENAI_VISION_TOOL_PREFIXES: &[&str] = &["gpt-4o", "gpt-4-turbo", "gpt-4.1", "gpt-4.5"];
const CLAUDE_3_PREFIXES: &[&str] = &["claude-3-", "claude-3.5-", "claude-3.7-"];
const CLAUDE_4_PREFIXES: &[&str] = &["claude-opus-4", "claude-sonnet-4", "claude-haiku-4"];
const GEMINI_CATALOGUED_PREFIXES: &[&str] = &[
    "gemini-1.5-pro",
    "gemini-1.5-flash",
    "gemini-2.0-flash",
    "gemini-2.0-pro",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
];

fn openai_reasoning_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 16_384,
    }
}

fn openai_vision_tool_capability() -> ModelCapability {
    ModelCapability {
        image_in: true,
        video_in: false,
        audio_in: false,
        thinking: false,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 16_384,
    }
}

fn openai_text_tool_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: false,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 16_384,
    }
}

fn anthropic_vision_tool_capability() -> ModelCapability {
    ModelCapability {
        image_in: true,
        video_in: false,
        audio_in: false,
        thinking: false,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 8_192,
    }
}

fn anthropic_thinking_vision_tool_capability() -> ModelCapability {
    ModelCapability {
        image_in: true,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 8_192,
    }
}

fn gemini_multimodal_tool_capability() -> ModelCapability {
    ModelCapability {
        image_in: true,
        video_in: true,
        audio_in: true,
        thinking: false,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 8_192,
    }
}

fn gemini_thinking_multimodal_tool_capability() -> ModelCapability {
    ModelCapability {
        image_in: true,
        video_in: true,
        audio_in: true,
        thinking: true,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 8_192,
    }
}

fn normalize_model_name(model_name: &str) -> String {
    model_name.to_lowercase()
}

fn has_prefix(model_name: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| model_name.starts_with(p))
}

fn is_openai_reasoning_model(model_name: &str) -> bool {
    regex::Regex::new(r"^o\d").unwrap().is_match(model_name)
}

fn capability_from_catalog(
    model_name: &str,
    catalog: &[(fn(&str) -> bool, ModelCapability)],
) -> ModelCapability {
    let normalized = normalize_model_name(model_name);
    for (matches_fn, cap) in catalog {
        if matches_fn(&normalized) {
            return cap.clone();
        }
    }
    ModelCapability::unknown()
}

pub fn get_openai_legacy_model_capability(model_name: &str) -> ModelCapability {
    let catalog: &[(fn(&str) -> bool, ModelCapability)] = &[
        (is_openai_reasoning_model, openai_reasoning_capability()),
        (
            |name| has_prefix(name, OPENAI_VISION_TOOL_PREFIXES),
            openai_vision_tool_capability(),
        ),
        (
            |name| name.starts_with("gpt-3.5-turbo"),
            openai_text_tool_capability(),
        ),
    ];
    capability_from_catalog(model_name, catalog)
}

pub fn get_openai_responses_model_capability(model_name: &str) -> ModelCapability {
    let catalog: &[(fn(&str) -> bool, ModelCapability)] = &[
        (is_openai_reasoning_model, openai_reasoning_capability()),
        (
            |name| has_prefix(name, OPENAI_VISION_TOOL_PREFIXES),
            openai_vision_tool_capability(),
        ),
    ];
    capability_from_catalog(model_name, catalog)
}

pub fn get_anthropic_model_capability(model_name: &str) -> ModelCapability {
    let catalog: &[(fn(&str) -> bool, ModelCapability)] = &[
        (
            |name| has_prefix(name, CLAUDE_3_PREFIXES),
            anthropic_vision_tool_capability(),
        ),
        (
            |name| has_prefix(name, CLAUDE_4_PREFIXES),
            anthropic_thinking_vision_tool_capability(),
        ),
    ];
    capability_from_catalog(model_name, catalog)
}

pub fn get_google_genai_model_capability(model_name: &str) -> ModelCapability {
    let normalized = normalize_model_name(model_name);
    if !normalized.starts_with("gemini-") {
        return ModelCapability::unknown();
    }
    if !has_prefix(&normalized, GEMINI_CATALOGUED_PREFIXES) {
        return ModelCapability::unknown();
    }
    if normalized.starts_with("gemini-2.5-") || normalized.contains("thinking") {
        gemini_thinking_multimodal_tool_capability()
    } else {
        gemini_multimodal_tool_capability()
    }
}

fn kimi_k2_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 0,
    }
}

fn deepseek_reasoner_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: false,
        max_context_tokens: 0,
        max_output_tokens: 0,
    }
}

fn deepseek_chat_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: false,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 0,
    }
}

fn deepseek_v4_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: true,
        max_context_tokens: 1_000_000,
        max_output_tokens: 384_000,
    }
}

pub fn get_kimi_model_capability(model_name: &str) -> ModelCapability {
    let normalized = normalize_model_name(model_name);
    if normalized.starts_with("kimi-k2") {
        return kimi_k2_capability();
    }
    ModelCapability::unknown()
}

pub fn get_deepseek_model_capability(model_name: &str) -> ModelCapability {
    let normalized = normalize_model_name(model_name);
    if normalized.starts_with("deepseek-reasoner") {
        return deepseek_reasoner_capability();
    }
    if normalized.starts_with("deepseek-v4-") {
        return deepseek_v4_capability();
    }
    if normalized.starts_with("deepseek-chat") {
        return deepseek_chat_capability();
    }
    ModelCapability::unknown()
}

pub fn get_glm_model_capability(_model_name: &str) -> ModelCapability {
    ModelCapability::unknown()
}

pub fn uses_openai_responses_developer_role(model_name: &str) -> bool {
    let normalized = normalize_model_name(model_name);
    OPENAI_RESPONSES_DEVELOPER_ROLE_MODELS
        .iter()
        .any(|m| *m == normalized || normalized.starts_with(&format!("{}-", m)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_reasoning_model() {
        let cap = get_openai_legacy_model_capability("o3-mini");
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert!(!cap.image_in);
        assert_eq!(cap.max_output_tokens, 16_384);
    }

    #[test]
    fn openai_vision_tool_model() {
        let cap = get_openai_legacy_model_capability("gpt-4o-2024-05-13");
        assert!(cap.image_in);
        assert!(cap.tool_use);
        assert!(!cap.thinking);
    }

    #[test]
    fn unknown_model_returns_unknown() {
        let cap = get_openai_legacy_model_capability("unknown");
        assert!(cap.is_unknown());
    }

    #[test]
    fn anthropic_claude_4_thinks() {
        let cap = get_anthropic_model_capability("claude-opus-4-20250514");
        assert!(cap.thinking);
        assert!(cap.image_in);
        assert_eq!(cap.max_output_tokens, 8_192);
    }

    #[test]
    fn google_gemini_multimodal() {
        let cap = get_google_genai_model_capability("gemini-2.0-flash-exp");
        assert!(cap.image_in);
        assert!(cap.video_in);
        assert!(cap.audio_in);
        assert!(cap.tool_use);
    }

    #[test]
    fn google_gemini_thinking() {
        let cap = get_google_genai_model_capability("gemini-2.5-pro-preview-05-06");
        assert!(cap.thinking);
    }

    #[test]
    fn developer_role_models() {
        assert!(uses_openai_responses_developer_role("gpt-4.1"));
        assert!(uses_openai_responses_developer_role("o3-mini"));
        assert!(!uses_openai_responses_developer_role("gpt-4o"));
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn kimi_k2_thinks_and_uses_tools() {
        let cap = get_kimi_model_capability("kimi-k2-0711");
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert!(!cap.image_in);
        assert!(!cap.video_in);
        assert!(!cap.audio_in);
    }

    #[test]
    fn kimi_unknown_is_unknown() {
        let cap = get_kimi_model_capability("kimi-unknown");
        assert!(cap.is_unknown());
    }

    #[test]
    fn deepseek_reasoner_thinks_no_tools() {
        let cap = get_deepseek_model_capability("deepseek-reasoner");
        assert!(cap.thinking);
        assert!(!cap.tool_use);
    }

    #[test]
    fn deepseek_chat_uses_tools_no_thinking() {
        let cap = get_deepseek_model_capability("deepseek-chat");
        assert!(!cap.thinking);
        assert!(cap.tool_use);
    }

    #[test]
    fn deepseek_v4_thinks_and_uses_tools_with_context() {
        let cap = get_deepseek_model_capability("deepseek-v4-0320");
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert_eq!(cap.max_context_tokens, 1_000_000);
        assert_eq!(cap.max_output_tokens, 384_000);
    }

    #[test]
    fn glm_always_unknown() {
        let cap = get_glm_model_capability("glm-4-flash");
        assert!(cap.is_unknown());
    }

    #[test]
    fn glm_returns_unknown_capability() {
        let cap = get_glm_model_capability("glm-4-plus");
        assert!(cap.is_unknown());
    }
}
