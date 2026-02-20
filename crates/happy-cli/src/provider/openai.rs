use super::*;
use anyhow::Context;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};

pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    api_base: String,
    model: String,
}

impl OpenAIProvider {
    pub fn new(api_key: String, model: String, api_base: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            api_base: api_base.unwrap_or_else(|| "https://api.openai.com".into()),
            model,
        }
    }

    /// Convert our internal messages to OpenAI format.
    fn build_messages(&self, system: &str, messages: &[Message]) -> Vec<Value> {
        let mut result = vec![json!({"role": "system", "content": system})];

        for msg in messages {
            match msg.role {
                Role::User => {
                    // Check if this is a tool_result message
                    let has_tool_results = msg
                        .content
                        .iter()
                        .any(|b| matches!(b, ContentBlock::ToolResult { .. }));

                    if has_tool_results {
                        // Each tool result becomes a separate "tool" role message
                        for block in &msg.content {
                            if let ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } = block
                            {
                                result.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tool_use_id,
                                    "content": content,
                                }));
                            }
                        }
                    } else {
                        let text: String = msg
                            .content
                            .iter()
                            .filter_map(|b| {
                                if let ContentBlock::Text { text } = b {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        result.push(json!({"role": "user", "content": text}));
                    }
                }
                Role::Assistant => {
                    let text: String = msg
                        .content
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text { text } = b {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    let tool_calls: Vec<Value> = msg
                        .content
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::ToolUse { id, name, input } = b {
                                Some(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string(),
                                    }
                                }))
                            } else {
                                None
                            }
                        })
                        .collect();

                    let mut msg_json = json!({"role": "assistant"});
                    if !text.is_empty() {
                        msg_json["content"] = json!(text);
                    } else {
                        // OpenAI requires "content": null when tool_calls are present
                        msg_json["content"] = serde_json::Value::Null;
                    }
                    if !tool_calls.is_empty() {
                        msg_json["tool_calls"] = json!(tool_calls);
                    }
                    result.push(msg_json);
                }
            }
        }

        result
    }

    fn build_tools(&self, tools: &[ToolDefinition]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        max_tokens: u32,
        temperature: f32,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
        let url = format!("{}/v1/chat/completions", self.api_base);

        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true,
            "messages": self.build_messages(system, messages),
        });

        if !tools.is_empty() {
            body["tools"] = json!(self.build_tools(tools));
        }

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to OpenAI")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, body);
        }

        use eventsource_stream::Eventsource as _;
        let byte_stream = response.bytes_stream();
        let event_stream = byte_stream.eventsource();

        // Track tool call state across events
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

            let choice = &parsed["choices"][0];
            let delta = &choice["delta"];
            let finish_reason = choice["finish_reason"].as_str();

            // Check for finish
            if let Some(reason) = finish_reason {
                let stop_reason = match reason {
                    "tool_calls" => StopReason::ToolUse,
                    "length" => StopReason::MaxTokens,
                    _ => StopReason::EndTurn,
                };
                let usage_data = &parsed["usage"];
                let usage = Usage {
                    input_tokens: usage_data["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                    output_tokens: usage_data["completion_tokens"].as_u64().unwrap_or(0) as u32,
                };
                return Some(Ok(StreamEvent::MessageEnd { stop_reason, usage }));
            }

            // Text content delta
            if let Some(content) = delta["content"].as_str() {
                if !content.is_empty() {
                    return Some(Ok(StreamEvent::TextDelta(content.to_string())));
                }
            }

            // Tool call deltas
            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                for tc in tool_calls {
                    let function = &tc["function"];

                    // Tool call start (has name)
                    if let Some(name) = function["name"].as_str() {
                        let id = tc["id"].as_str().unwrap_or("").to_string();
                        return Some(Ok(StreamEvent::ToolUseStart {
                            id,
                            name: name.to_string(),
                        }));
                    }

                    // Tool call argument delta
                    if let Some(args) = function["arguments"].as_str() {
                        if !args.is_empty() {
                            return Some(Ok(StreamEvent::ToolUseInputDelta(args.to_string())));
                        }
                    }
                }
            }

            None
        });

        Ok(Box::pin(mapped))
    }
}
