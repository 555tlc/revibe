//! Message formatting utilities for API communication.

use crate::types::{LlmMessage, ToolCall};

/// Handles formatting and parsing of API messages.
#[derive(Debug, Default)]
pub struct ApiToolFormatHandler;

impl ApiToolFormatHandler {
    /// Create a new format handler.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Process an API response message into our internal format.
    #[must_use]
    pub fn process_api_response_message(&self, message: LlmMessage) -> LlmMessage {
        // The message is already in our internal format, but we can do
        // any necessary normalization here
        message
    }

    /// Get the default tool choice directive.
    #[must_use]
    pub fn get_tool_choice(&self) -> Option<&'static str> {
        Some("auto")
    }

    /// Merge streaming tool call chunks into a complete tool call.
    pub fn merge_tool_call_chunks(&self, chunks: &[ToolCall]) -> Vec<ToolCall> {
        use std::collections::BTreeMap;

        let mut merged: BTreeMap<usize, ToolCall> = BTreeMap::new();

        for chunk in chunks {
            let index = chunk.index.unwrap_or(0);
            if let Some(existing) = merged.get_mut(&index) {
                // Merge arguments
                if let Some(ref new_args) = chunk.function.arguments {
                    let current_args = existing.function.arguments.get_or_insert_with(String::new);
                    current_args.push_str(new_args);
                }
                // Update ID if present
                if chunk.id.is_some() {
                    existing.id = chunk.id.clone();
                }
                // Update name if present
                if chunk.function.name.is_some() {
                    existing.function.name = chunk.function.name.clone();
                }
            } else {
                merged.insert(index, chunk.clone());
            }
        }

        merged.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FunctionCall;

    #[test]
    fn test_merge_tool_call_chunks() {
        let handler = ApiToolFormatHandler::new();

        let chunks = vec![
            ToolCall {
                id: Some("call_1".to_string()),
                index: Some(0),
                function: FunctionCall {
                    name: Some("bash".to_string()),
                    arguments: Some("{\"com".to_string()),
                },
                r#type: "function".to_string(),
            },
            ToolCall {
                id: None,
                index: Some(0),
                function: FunctionCall {
                    name: None,
                    arguments: Some("mand\":".to_string()),
                },
                r#type: "function".to_string(),
            },
            ToolCall {
                id: None,
                index: Some(0),
                function: FunctionCall {
                    name: None,
                    arguments: Some("\"ls\"}".to_string()),
                },
                r#type: "function".to_string(),
            },
        ];

        let merged = handler.merge_tool_call_chunks(&chunks);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, Some("call_1".to_string()));
        assert_eq!(merged[0].function.name, Some("bash".to_string()));
        assert_eq!(
            merged[0].function.arguments,
            Some("{\"command\":\"ls\"}".to_string())
        );
    }
}
