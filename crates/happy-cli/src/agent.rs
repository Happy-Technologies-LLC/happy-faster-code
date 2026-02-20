use crate::config::AgentConfig;
use crate::provider::*;
use crate::repo::RepoContext;
use crate::tools;
use futures::StreamExt;
use tokio::sync::mpsc;

/// Events sent from the agent to the TUI.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCallStart { name: String },
    ToolCallResult { name: String, preview: String, is_error: bool },
    TurnComplete {
        total_input_tokens: u32,
        total_output_tokens: u32,
    },
    Error(String),
}

pub struct Agent {
    provider: Box<dyn LlmProvider>,
    config: AgentConfig,
    messages: Vec<Message>,
    system_prompt: String,
    tool_defs: Vec<ToolDefinition>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

impl Agent {
    pub fn new(provider: Box<dyn LlmProvider>, config: AgentConfig) -> Self {
        let system_prompt = build_system_prompt();
        let tool_defs = tools::tool_definitions();
        Self {
            provider,
            config,
            messages: Vec::new(),
            system_prompt,
            tool_defs,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    /// Process a user query, sending events to the TUI channel.
    pub async fn query(
        &mut self,
        repo: &RepoContext,
        user_input: &str,
        event_tx: &mpsc::UnboundedSender<AgentEvent>,
    ) {
        self.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: user_input.to_string(),
            }],
        });

        for iteration in 0..self.config.max_iterations {
            let stream_result = self
                .provider
                .stream(
                    &self.system_prompt,
                    &self.messages,
                    &self.tool_defs,
                    self.config.max_tokens,
                    self.config.temperature,
                )
                .await;

            let mut stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = event_tx.send(AgentEvent::Error(format!("LLM error: {}", e)));
                    let _ = event_tx.send(AgentEvent::TurnComplete {
                        total_input_tokens: self.total_input_tokens,
                        total_output_tokens: self.total_output_tokens,
                    });
                    return;
                }
            };

            // Accumulate response
            let mut text_accum = String::new();
            let mut tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, input_json)
            let mut current_tool_id = String::new();
            let mut current_tool_name = String::new();
            let mut current_tool_input = String::new();
            let mut stop_reason = StopReason::EndTurn;

            while let Some(event) = stream.next().await {
                match event {
                    Ok(StreamEvent::TextDelta(text)) => {
                        text_accum.push_str(&text);
                        let _ = event_tx.send(AgentEvent::TextDelta(text));
                    }
                    Ok(StreamEvent::ToolUseStart { id, name }) => {
                        // Finish any prior tool
                        if !current_tool_name.is_empty() {
                            tool_calls.push((
                                current_tool_id.clone(),
                                current_tool_name.clone(),
                                current_tool_input.clone(),
                            ));
                        }
                        current_tool_id = id;
                        current_tool_name = name.clone();
                        current_tool_input.clear();
                        let _ = event_tx.send(AgentEvent::ToolCallStart { name });
                    }
                    Ok(StreamEvent::ToolUseInputDelta(delta)) => {
                        current_tool_input.push_str(&delta);
                    }
                    Ok(StreamEvent::ToolUseEnd) => {
                        if !current_tool_name.is_empty() {
                            tool_calls.push((
                                current_tool_id.clone(),
                                current_tool_name.clone(),
                                current_tool_input.clone(),
                            ));
                            current_tool_name.clear();
                            current_tool_input.clear();
                        }
                    }
                    Ok(StreamEvent::MessageEnd {
                        stop_reason: sr,
                        usage,
                    }) => {
                        stop_reason = sr;
                        self.total_input_tokens += usage.input_tokens;
                        self.total_output_tokens += usage.output_tokens;
                    }
                    Err(e) => {
                        let _ = event_tx.send(AgentEvent::Error(format!("Stream error: {}", e)));
                        break;
                    }
                }
            }

            // Flush any pending tool call (OpenAI doesn't emit ToolUseEnd)
            if !current_tool_name.is_empty() {
                tool_calls.push((
                    current_tool_id.clone(),
                    current_tool_name.clone(),
                    current_tool_input.clone(),
                ));
                current_tool_name.clear();
                current_tool_input.clear();
            }

            // Build assistant message — Anthropic requires non-empty content
            let mut assistant_content: Vec<ContentBlock> = Vec::new();
            if !text_accum.is_empty() {
                assistant_content.push(ContentBlock::Text { text: text_accum });
            }
            for (id, name, input_str) in &tool_calls {
                let input: serde_json::Value = serde_json::from_str(input_str)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                assistant_content.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                });
            }

            // Guard: never push an assistant message with empty content
            if assistant_content.is_empty() {
                let _ = event_tx.send(AgentEvent::Error(
                    "LLM returned an empty response".to_string(),
                ));
                let _ = event_tx.send(AgentEvent::TurnComplete {
                    total_input_tokens: self.total_input_tokens,
                    total_output_tokens: self.total_output_tokens,
                });
                return;
            }

            // If stop_reason is NOT ToolUse but we have tool_use blocks
            // (e.g. truncated by max_tokens mid-tool-call), strip them to
            // avoid an invalid message sequence (tool_use without tool_result).
            if stop_reason != StopReason::ToolUse && !tool_calls.is_empty() {
                assistant_content.retain(|b| !matches!(b, ContentBlock::ToolUse { .. }));
                tool_calls.clear();
                if assistant_content.is_empty() {
                    assistant_content.push(ContentBlock::Text {
                        text: "[Response truncated]".to_string(),
                    });
                }
            }

            self.messages.push(Message {
                role: Role::Assistant,
                content: assistant_content,
            });

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                let _ = event_tx.send(AgentEvent::TurnComplete {
                    total_input_tokens: self.total_input_tokens,
                    total_output_tokens: self.total_output_tokens,
                });
                return;
            }

            // Execute tool calls
            let mut tool_results: Vec<ContentBlock> = Vec::new();
            for (id, name, input_str) in &tool_calls {
                let input: serde_json::Value = serde_json::from_str(input_str)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let result = tools::execute_tool(repo, name, &input);
                let (content, is_error) = match result {
                    Ok(output) => {
                        let truncated = if output.len() > 8000 {
                            format!(
                                "{}...\n[truncated, {} total chars]",
                                &output[..8000],
                                output.len()
                            )
                        } else {
                            output
                        };
                        (truncated, false)
                    }
                    Err(err) => (err, true),
                };

                let preview = if content.len() > 100 {
                    format!("{}...", &content[..100])
                } else {
                    content.clone()
                };
                let _ = event_tx.send(AgentEvent::ToolCallResult {
                    name: name.clone(),
                    preview,
                    is_error,
                });

                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content,
                    is_error,
                });
            }

            self.messages.push(Message {
                role: Role::User,
                content: tool_results,
            });

            // Loop back to send results to LLM
            if iteration == self.config.max_iterations - 1 {
                let _ = event_tx.send(AgentEvent::Error(
                    "Max iterations reached".to_string(),
                ));
                let _ = event_tx.send(AgentEvent::TurnComplete {
                    total_input_tokens: self.total_input_tokens,
                    total_output_tokens: self.total_output_tokens,
                });
            }
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.total_input_tokens = 0;
        self.total_output_tokens = 0;
    }

    pub fn model_name(&self) -> &str {
        &self.config.model
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }
}

fn build_system_prompt() -> String {
    r#"You are a code analysis assistant with access to a fully indexed code repository.
You have tools to search code, read source files, and navigate the code graph (callers, callees, dependencies, inheritance, etc.).

When answering questions about the codebase:
1. Start by searching for relevant code using search_code
2. Get source code of specific elements using get_source
3. Navigate relationships using find_callers, find_callees, get_dependencies, etc.
4. Use read_file for raw file contents when needed

Be precise. Reference specific files, line numbers, and function names.
Show relevant code snippets in your answers.
If you can't find something, say so clearly."#
        .to_string()
}
