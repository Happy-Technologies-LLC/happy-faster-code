use super::*;
use anyhow::Context;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    api_base: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String, api_base: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            api_base: api_base.unwrap_or_else(|| "https://api.anthropic.com".into()),
            model,
        }
    }

    fn build_messages(&self, messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|msg| {
                let content: Vec<Value> = msg
                    .content
                    .iter()
                    .map(|block| match block {
                        ContentBlock::Text { text } => {
                            json!({"type": "text", "text": text})
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            json!({"type": "tool_use", "id": id, "name": name, "input": input})
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } => {
                            json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content, "is_error": is_error})
                        }
                    })
                    .collect();
                json!({"role": format!("{}", match msg.role { Role::User => "user", Role::Assistant => "assistant" }), "content": content})
            })
            .collect()
    }

    fn build_tools(&self, tools: &[ToolDefinition]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        max_tokens: u32,
        temperature: f32,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
        let url = format!("{}/v1/messages", self.api_base);

        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true,
            "system": system,
            "messages": self.build_messages(messages),
        });

        if !tools.is_empty() {
            body["tools"] = json!(self.build_tools(tools));
        }

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Anthropic")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error ({}): {}", status, body);
        }

        // Parse SSE stream
        use eventsource_stream::Eventsource as _;
        let byte_stream = response.bytes_stream();
        let event_stream = byte_stream.eventsource();

        let mapped = event_stream.filter_map(|event| async move {
            let event = match event {
                Ok(e) => e,
                Err(e) => return Some(Err(anyhow::anyhow!("SSE error: {}", e))),
            };

            let data = &event.data;
            if data == "[DONE]" {
                return None;
            }

            let parsed: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(e) => return Some(Err(anyhow::anyhow!("JSON parse error: {}", e))),
            };

            let event_type = parsed["type"].as_str().unwrap_or("");

            match event_type {
                "content_block_start" => {
                    let block = &parsed["content_block"];
                    let block_type = block["type"].as_str().unwrap_or("");
                    if block_type == "tool_use" {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        Some(Ok(StreamEvent::ToolUseStart { id, name }))
                    } else {
                        None
                    }
                }
                "content_block_delta" => {
                    let delta = &parsed["delta"];
                    let delta_type = delta["type"].as_str().unwrap_or("");
                    match delta_type {
                        "text_delta" => {
                            let text = delta["text"].as_str().unwrap_or("").to_string();
                            Some(Ok(StreamEvent::TextDelta(text)))
                        }
                        "input_json_delta" => {
                            let json_str =
                                delta["partial_json"].as_str().unwrap_or("").to_string();
                            Some(Ok(StreamEvent::ToolUseInputDelta(json_str)))
                        }
                        _ => None,
                    }
                }
                "content_block_stop" => Some(Ok(StreamEvent::ToolUseEnd)),
                "message_delta" => {
                    let delta = &parsed["delta"];
                    let stop_reason = match delta["stop_reason"].as_str().unwrap_or("") {
                        "tool_use" => StopReason::ToolUse,
                        "max_tokens" => StopReason::MaxTokens,
                        _ => StopReason::EndTurn,
                    };
                    let usage_data = &parsed["usage"];
                    let usage = Usage {
                        input_tokens: usage_data["input_tokens"].as_u64().unwrap_or(0) as u32,
                        output_tokens: usage_data["output_tokens"].as_u64().unwrap_or(0) as u32,
                    };
                    Some(Ok(StreamEvent::MessageEnd { stop_reason, usage }))
                }
                _ => None,
            }
        });

        Ok(Box::pin(mapped))
    }
}
