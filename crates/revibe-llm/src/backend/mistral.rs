//! Mistral API backend implementation.

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{Backend, ChunkStream, CompletionOptions, ModelConfig, ProviderConfig};
use crate::error::{LlmError, LlmResult};
use crate::types::{AvailableTool, FunctionCall, LlmChunk, LlmMessage, LlmUsage, Role, ToolCall};

/// Mistral API backend.
pub struct MistralBackend {
    client: reqwest::Client,
    provider: ProviderConfig,
}

impl MistralBackend {
    /// Create a new Mistral backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the client cannot be created.
    pub fn new(provider: ProviderConfig, timeout: Duration) -> LlmResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(LlmError::RequestFailed)?;

        Ok(Self { client, provider })
    }

    /// Get the API key from the environment.
    ///
    /// # Errors
    ///
    /// Returns an error if the API key is not found.
    fn get_api_key(&self) -> LlmResult<String> {
        std::env::var(&self.provider.api_key_env_var).map_err(|_| LlmError::MissingApiKey {
            provider: self.provider.name.clone(),
            env_var: self.provider.api_key_env_var.clone(),
        })
    }

    fn build_headers(
        &self,
        extra: Option<&std::collections::HashMap<String, String>>,
    ) -> LlmResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let api_key = self.get_api_key()?;
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key)).expect("Invalid API key format"),
        );

        if let Some(extra) = extra {
            for (key, value) in extra {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::try_from(key),
                    HeaderValue::from_str(value),
                ) {
                    headers.insert(name, val);
                }
            }
        }

        Ok(headers)
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.provider.api_base)
    }
}

// API request/response types
#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<MistralMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<MistralTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<MistralToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralToolCall {
    id: Option<String>,
    #[serde(
        rename = "type",
        default = "default_function_type",
        skip_serializing_if = "Option::is_none"
    )]
    type_: Option<String>,
    function: MistralFunctionCall,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
}

fn default_function_type() -> Option<String> {
    Some("function".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct MistralFunctionCall {
    name: String,
    #[serde(default)]
    arguments: String,
}

#[derive(Debug, Serialize)]
struct MistralTool {
    #[serde(rename = "type")]
    type_: String,
    function: MistralFunction,
}

#[derive(Debug, Serialize)]
struct MistralFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Option<ResponseMessage>,
    delta: Option<ResponseMessage>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    #[serde(default, deserialize_with = "deserialize_content")]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<MistralToolCall>>,
}

/// Deserialize content that can be either a string or an array of content chunks.
fn deserialize_content<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct ContentVisitor;

    impl<'de> Visitor<'de> for ContentVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or array of content chunks")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut parts = Vec::new();
            while let Some(chunk) = seq.next_element::<serde_json::Value>()? {
                if let Some(obj) = chunk.as_object() {
                    // Handle {"type": "text", "text": "..."} format
                    if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                        parts.push(text.to_string());
                    }
                }
            }
            if parts.is_empty() {
                Ok(None)
            } else {
                Ok(Some(parts.join("\n")))
            }
        }
    }

    deserializer.deserialize_any(ContentVisitor)
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

impl MistralBackend {
    fn convert_message(msg: &LlmMessage) -> MistralMessage {
        // For assistant messages, ensure we don't send empty content when there are tool_calls
        // Mistral API requires assistant messages to have either content or tool_calls (not both empty)
        let content = match (&msg.content, &msg.tool_calls, &msg.role) {
            // If assistant has tool_calls, content can be None
            (Some(c), Some(_), _) if c.is_empty() => None,
            // If assistant has empty content and no tool_calls, this is invalid - but let it through
            // as it will be caught by the API
            (content, _, _) => content.clone(),
        };

        MistralMessage {
            role: msg.role.to_string(),
            content,
            tool_calls: msg.tool_calls.as_ref().map(|tcs| {
                tcs.iter()
                    .map(|tc| MistralToolCall {
                        id: tc.id.clone(),
                        type_: tc.r#type.clone().into(),
                        function: MistralFunctionCall {
                            name: tc.function.name.clone().unwrap_or_default(),
                            arguments: tc.function.arguments.clone().unwrap_or_default(),
                        },
                        index: tc.index,
                    })
                    .collect()
            }),
            name: msg.name.clone(),
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    fn convert_tool(tool: &AvailableTool) -> MistralTool {
        MistralTool {
            type_: "function".to_string(),
            function: MistralFunction {
                name: tool.function.name.clone(),
                description: tool.function.description.clone(),
                parameters: tool.function.parameters.clone(),
            },
        }
    }

    fn convert_response_message(msg: &ResponseMessage) -> LlmMessage {
        LlmMessage {
            role: Role::Assistant,
            content: msg.content.clone(),
            tool_calls: msg.tool_calls.as_ref().map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        index: tc.index,
                        function: FunctionCall {
                            name: Some(tc.function.name.clone()),
                            arguments: Some(tc.function.arguments.clone()),
                        },
                        r#type: tc.type_.clone().unwrap_or_else(|| "function".to_string()),
                    })
                    .collect()
            }),
            name: None,
            tool_call_id: None,
        }
    }
}

#[async_trait]
impl Backend for MistralBackend {
    async fn complete(
        &self,
        model: &ModelConfig,
        messages: &[LlmMessage],
        tools: Option<&[AvailableTool]>,
        options: &CompletionOptions,
    ) -> LlmResult<LlmChunk> {
        let request = ChatRequest {
            model: &model.name,
            messages: messages.iter().map(Self::convert_message).collect(),
            temperature: model.temperature,
            max_tokens: options.max_tokens,
            tools: tools.map(|t| t.iter().map(Self::convert_tool).collect()),
            tool_choice: options.tool_choice.as_ref().map(|tc| {
                serde_json::to_value(tc).unwrap_or(serde_json::Value::String("auto".to_string()))
            }),
            stream: false,
            stream_options: None,
        };

        let response = self
            .client
            .post(self.endpoint())
            .headers(self.build_headers(options.extra_headers.as_ref())?)
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(LlmError::ApiError {
                provider: self.provider.name.clone(),
                status: status.as_u16(),
                message: body,
            });
        }

        let chat_response: ChatResponse = serde_json::from_str(&body).map_err(|e| {
            LlmError::ParseError(format!("Failed to parse response: {e}\nBody: {body}"))
        })?;

        let choice = chat_response
            .choices
            .first()
            .ok_or_else(|| LlmError::ParseError("No choices in response".to_string()))?;

        let message = choice
            .message
            .as_ref()
            .ok_or_else(|| LlmError::ParseError("No message in choice".to_string()))?;

        Ok(LlmChunk {
            message: Self::convert_response_message(message),
            finish_reason: choice.finish_reason.clone(),
            usage: chat_response.usage.map(|u| LlmUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
            }),
        })
    }

    async fn complete_streaming(
        &self,
        model: &ModelConfig,
        messages: &[LlmMessage],
        tools: Option<&[AvailableTool]>,
        options: &CompletionOptions,
    ) -> LlmResult<ChunkStream> {
        let request = ChatRequest {
            model: &model.name,
            messages: messages.iter().map(Self::convert_message).collect(),
            temperature: model.temperature,
            max_tokens: options.max_tokens,
            tools: tools.map(|t| t.iter().map(Self::convert_tool).collect()),
            tool_choice: options.tool_choice.as_ref().map(|tc| {
                serde_json::to_value(tc).unwrap_or(serde_json::Value::String("auto".to_string()))
            }),
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        };

        let response = self
            .client
            .post(self.endpoint())
            .headers(self.build_headers(options.extra_headers.as_ref())?)
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                provider: self.provider.name.clone(),
                status: status.as_u16(),
                message: body,
            });
        }

        // Use a buffered stream to handle SSE lines that may be split across TCP chunks
        use std::sync::{Arc, Mutex};
        let line_buffer = Arc::new(Mutex::new(String::new()));

        let stream = response.bytes_stream().map(move |result| {
            result.map_err(LlmError::RequestFailed).map(|bytes| {
                let text = String::from_utf8_lossy(&bytes);

                // Append to buffer and process complete lines
                let mut buffer = line_buffer.lock().unwrap();
                buffer.push_str(&text);

                // Parse SSE format - accumulate all data lines in this chunk
                let mut accumulated_content = String::new();
                let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
                let mut final_finish_reason: Option<String> = None;
                let mut final_usage: Option<LlmUsage> = None;

                // Process complete lines (ending with \n)
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                    buffer.drain(..=newline_pos);

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            // Set finish_reason to "stop" when we receive [DONE]
                            if final_finish_reason.is_none() {
                                final_finish_reason = Some("stop".to_string());
                            }
                            continue;
                        }
                        if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data)
                            && let Some(choice) = chunk.choices.first()
                        {
                            // Accumulate content
                            if let Some(delta) = &choice.delta {
                                if let Some(content) = &delta.content {
                                    accumulated_content.push_str(content);
                                }
                                if let Some(tcs) = &delta.tool_calls {
                                    for tc in tcs {
                                        accumulated_tool_calls.push(ToolCall {
                                            id: tc.id.clone(),
                                            index: tc.index,
                                            function: FunctionCall {
                                                name: if tc.function.name.is_empty() {
                                                    None
                                                } else {
                                                    Some(tc.function.name.clone())
                                                },
                                                arguments: if tc.function.arguments.is_empty() {
                                                    None
                                                } else {
                                                    Some(tc.function.arguments.clone())
                                                },
                                            },
                                            r#type: tc
                                                .type_
                                                .clone()
                                                .unwrap_or_else(|| "function".to_string()),
                                        });
                                    }
                                }
                            } else if let Some(msg) = &choice.message {
                                if let Some(content) = &msg.content {
                                    accumulated_content.push_str(content);
                                }
                                if let Some(tcs) = &msg.tool_calls {
                                    for tc in tcs {
                                        accumulated_tool_calls.push(ToolCall {
                                            id: tc.id.clone(),
                                            index: tc.index,
                                            function: FunctionCall {
                                                name: if tc.function.name.is_empty() {
                                                    None
                                                } else {
                                                    Some(tc.function.name.clone())
                                                },
                                                arguments: if tc.function.arguments.is_empty() {
                                                    None
                                                } else {
                                                    Some(tc.function.arguments.clone())
                                                },
                                            },
                                            r#type: tc
                                                .type_
                                                .clone()
                                                .unwrap_or_else(|| "function".to_string()),
                                        });
                                    }
                                }
                            }

                            // Capture finish_reason when present
                            if choice.finish_reason.is_some() {
                                final_finish_reason = choice.finish_reason.clone();
                            }

                            // Capture usage when present
                            if let Some(usage) = chunk.usage {
                                final_usage = Some(LlmUsage {
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                });
                            }
                        }
                    }
                }

                // Build the combined message
                let message = LlmMessage {
                    role: Role::Assistant,
                    content: if accumulated_content.is_empty() {
                        None
                    } else {
                        Some(accumulated_content)
                    },
                    tool_calls: if accumulated_tool_calls.is_empty() {
                        None
                    } else {
                        Some(accumulated_tool_calls)
                    },
                    name: None,
                    tool_call_id: None,
                };

                LlmChunk {
                    message,
                    finish_reason: final_finish_reason,
                    usage: final_usage,
                }
            })
        });

        Ok(Box::pin(stream))
    }

    async fn count_tokens(
        &self,
        model: &ModelConfig,
        messages: &[LlmMessage],
        tools: Option<&[AvailableTool]>,
    ) -> LlmResult<u32> {
        // Use a completion with max_tokens=1 to get token count
        let result = self
            .complete(
                model,
                messages,
                tools,
                &CompletionOptions {
                    max_tokens: Some(1),
                    ..Default::default()
                },
            )
            .await?;

        result
            .usage
            .map(|u| u.prompt_tokens)
            .ok_or(LlmError::MissingUsage)
    }
}
