pub mod capability_registry;
pub mod catalog;
pub mod chat_completions_stream;
pub mod errors;
pub mod generate;
pub mod http_client;
pub mod kimi_files;
pub mod kimi_schema;
pub mod message;
pub mod mock;
pub mod openai_common;
pub mod openai_legacy;
pub mod openai_responses;
pub mod provider;
pub mod provider_factory;
pub mod providers;
pub mod request_auth;
pub mod tool_call_id;
pub mod usage;

// Re-exports for convenience (used by golden binary)
pub use chat_completions_stream::{
    parse_non_stream_response, parse_stream_response, parse_stream_response_with_usage_extractor,
    BufferedChatCompletionToolCall,
};
pub use errors::ChatProviderError;
pub use generate::generate;
pub use http_client::{HttpClient, MockHttpClient, ReqwestClient};
pub use message::{ContentPart, Message, Role, StreamedMessagePart};
pub use mock::MockProvider;
pub use openai_common::{
    convert_content_part, convert_openai_error, extract_usage, normalize_openai_finish_reason,
    thinking_effort_to_reasoning_effort, tool_to_openai, ToolMessageConversion,
};
pub use openai_legacy::{OpenAILegacyChatProvider, OpenAILegacyOptions};
pub use openai_responses::{
    OpenAIResponsesChatProvider, OpenAIResponsesOptions, OpenAIResponsesStreamedMessage,
};
pub use provider::{GenerateOptions, ModelCapability, ProviderRequestAuth, ProviderType, Tool};
pub use provider_factory::{
    create_chat_provider, resolve_model_capability, ProviderFactoryConfig, ProviderFactoryError,
};
pub use providers::anthropic::{AnthropicChatProvider, AnthropicOptions};
pub use providers::deepseek::{DeepSeekChatProvider, DeepSeekOptions};
pub use providers::google_genai::GoogleGenAIChatProvider;
pub use usage::TokenUsage;

pub use capability_registry::{
    get_anthropic_model_capability, get_deepseek_model_capability, get_glm_model_capability,
    get_google_genai_model_capability, get_kimi_model_capability,
    get_openai_legacy_model_capability, get_openai_responses_model_capability,
    uses_openai_responses_developer_role,
};
pub use catalog::{
    catalog_base_url, catalog_model_to_capability, catalog_provider_models, infer_wire_type,
    Catalog, CatalogModel, CatalogModelEntry, CatalogProviderEntry,
};
pub use kimi_files::{
    KimiFiles, KimiFilesOptions, KimiUploadOptions, KimiVideoUpload, VideoUploadInput,
};
pub use kimi_schema::normalize_kimi_tool_schema;
pub use providers::kimi::{KimiChatProvider, KimiOptions};
pub use request_auth::{
    merge_request_headers, require_provider_api_key, resolve_auth_backed_client,
    AuthBackedClientState,
};
pub use tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_openai_responses_call_id, sanitize_tool_call_id,
    ToolCallIdPolicy,
};
