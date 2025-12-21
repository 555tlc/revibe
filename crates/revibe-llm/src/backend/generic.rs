//! Generic OpenAI-compatible API backend implementation.

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{Backend, ChunkStream, CompletionOptions, ModelConfig, ProviderConfig};
use crate::error::{LlmError, LlmResult};
use crate::types::{AvailableTool, FunctionCall, LlmChunk, LlmMessage, LlmUsage, Role, ToolCall};

/// Generic OpenAI-compatible API backend.
pub struct GenericBackend {
    client: reqwest::Client,
    provider: ProviderConfig,
    api_key: Option<String>,
}

impl GenericBackend {
    /// Create a new generic backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new(provider: ProviderConfig, timeout: Duration) -> LlmResult<Self> {
        let api_key = if provider.api_key_env_var.is_empty() {
            None
        } else {
            std::env::var(&provider.api_key_env_var).ok()
        };

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(LlmError::RequestFailed)?;

        Ok(Self {
            client,
            provider,
            api_key,
        })
    }

    fn build_headers(
        &self,
        extra: Option<&std::collections::HashMap<String, String>>,
    ) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(ref key) = self.api_key
            && let Ok(val) = HeaderValue::from_str(&format!("Bearer {key}"))
        {
            headers.insert(AUTHORIZATION, val);
        }

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

        headers
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.provider.api_base)
    }
}

// API request/response types (OpenAI-compatible format)
#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
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
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIToolCall {
    id: Option<String>,
    #[serde(rename = "type", default = "default_function_type")]
    type_: String,
    function: OpenAIFunctionCall,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
}

fn default_function_type() -> String {
    "function".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    #[serde(default)]
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    type_: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
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
    tool_calls: Option<Vec<OpenAIToolCall>>,
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

impl GenericBackend {
    fn convert_message(msg: &LlmMessage) -> OpenAIMessage {
        // For assistant messages, ensure we don't send empty content when there are tool_calls
        // OpenAI-compatible APIs may require assistant messages to have either content or tool_calls
        let content = match (&msg.content, &msg.tool_calls) {
            // If assistant has tool_calls, empty content should be None
            (Some(c), Some(_)) if c.is_empty() => None,
            (content, _) => content.clone(),
        };

        OpenAIMessage {
            role: msg.role.to_string(),
            content,
            tool_calls: msg.tool_calls.as_ref().map(|tcs| {
                tcs.iter()
                    .map(|tc| OpenAIToolCall {
                        id: tc.id.clone(),
                        type_: tc.r#type.clone(),
                        function: OpenAIFunctionCall {
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

    fn convert_tool(tool: &AvailableTool) -> OpenAITool {
        OpenAITool {
            type_: "function".to_string(),
            function: OpenAIFunction {
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
                        r#type: tc.type_.clone(),
                    })
                    .collect()
            }),
            name: None,
            tool_call_id: None,
        }
    }
}

#[async_trait]
impl Backend for GenericBackend {
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
            .headers(self.build_headers(options.extra_headers.as_ref()))
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
            .headers(self.build_headers(options.extra_headers.as_ref()))
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

        let stream = response.bytes_stream().map(move |result| {
            result.map_err(LlmError::RequestFailed).map(|bytes| {
                let text = String::from_utf8_lossy(&bytes);
                // Parse SSE format
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ")
                        && data != "[DONE]"
                        && let Ok(chunk) = serde_json::from_str::<StreamChunk>(data)
                        && let Some(choice) = chunk.choices.first()
                    {
                        let message = if let Some(delta) = &choice.delta {
                            Self::convert_response_message(delta)
                        } else if let Some(msg) = &choice.message {
                            Self::convert_response_message(msg)
                        } else {
                            LlmMessage::assistant("")
                        };

                        return LlmChunk {
                            message,
                            finish_reason: choice.finish_reason.clone(),
                            usage: chunk.usage.map(|u| LlmUsage {
                                prompt_tokens: u.prompt_tokens,
                                completion_tokens: u.completion_tokens,
                            }),
                        };
                    }
                }
                // Return empty chunk if nothing parsed
                LlmChunk {
                    message: LlmMessage::assistant(""),
                    finish_reason: None,
                    usage: None,
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
