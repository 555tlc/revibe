//! LLM backend implementations.

mod factory;
mod generic;
mod mistral;

pub use factory::{BackendFactory, BackendType};
pub use generic::GenericBackend;
pub use mistral::MistralBackend;

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::error::LlmResult;
use crate::types::{AvailableTool, LlmChunk, LlmMessage, ToolChoice};

/// Configuration for a model.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Model name/identifier.
    pub name: String,
    /// Temperature for generation.
    pub temperature: f32,
}

/// Configuration for a provider.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Provider name.
    pub name: String,
    /// Base URL for the API.
    pub api_base: String,
    /// Environment variable containing the API key.
    pub api_key_env_var: String,
    /// Backend type to use.
    pub backend: BackendType,
}

/// Options for a completion request.
#[derive(Debug, Clone, Default)]
pub struct CompletionOptions {
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Tool choice directive.
    pub tool_choice: Option<ToolChoice>,
    /// Extra headers to include.
    pub extra_headers: Option<std::collections::HashMap<String, String>>,
}

/// Type alias for boxed stream of chunks.
pub type ChunkStream = Pin<Box<dyn Stream<Item = LlmResult<LlmChunk>> + Send>>;

/// Trait for LLM backends.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Complete a chat conversation.
    async fn complete(
        &self,
        model: &ModelConfig,
        messages: &[LlmMessage],
        tools: Option<&[AvailableTool]>,
        options: &CompletionOptions,
    ) -> LlmResult<LlmChunk>;

    /// Complete a chat conversation with streaming.
    async fn complete_streaming(
        &self,
        model: &ModelConfig,
        messages: &[LlmMessage],
        tools: Option<&[AvailableTool]>,
        options: &CompletionOptions,
    ) -> LlmResult<ChunkStream>;

    /// Count tokens in a conversation.
    async fn count_tokens(
        &self,
        model: &ModelConfig,
        messages: &[LlmMessage],
        tools: Option<&[AvailableTool]>,
    ) -> LlmResult<u32>;
}
