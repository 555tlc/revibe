//! LLM backend implementations for Revibe CLI.
//!
//! This crate provides abstractions and implementations for communicating with
//! various LLM providers, including Mistral and OpenAI-compatible APIs.

pub mod backend;
pub mod error;
pub mod format;
pub mod types;

pub use backend::{Backend, BackendFactory, GenericBackend, MistralBackend};
pub use error::{LlmError, LlmResult};
pub use types::{
    AvailableFunction, AvailableTool, FunctionCall, LlmChunk, LlmMessage, LlmUsage, Role,
    StrToolChoice, ToolCall, ToolChoice,
};
