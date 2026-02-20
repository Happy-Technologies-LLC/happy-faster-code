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
    r#"You are an expert AI software engineer with full access to a codebase that has been structurally indexed into a code graph.

You have 18 tools organized into three categories:

**Code Graph Navigation** (unique to this tool — not available in other AI CLIs):
- search_code: BM25 keyword search across all indexed code elements
- get_source: Get source code of any function/class/module by ID
- find_callers / find_callees: Who calls this? What does it call? (graph traversal, not grep)
- get_dependencies / get_dependents: Import graph traversal
- get_subclasses / get_superclasses: Class hierarchy navigation
- find_path: Shortest path between any two symbols in the code graph
- get_related: Multi-hop neighbor traversal
- repo_stats: Graph statistics

**Read & Search**:
- read_file: Read file contents with line numbers (supports offset/limit for large files)
- list_files: All indexed source files
- list_directory: Browse directory contents
- grep_files: Regex search across file contents

**Write & Execute**:
- write_file: Create or overwrite files (creates parent directories automatically)
- edit_file: Precise string replacement in files (old_string must be unique)
- bash: Execute any shell command (builds, tests, git, linters, package managers, etc.)

## How to work effectively

1. **Understand before modifying**: Use the code graph tools to understand structure and relationships before making changes. search_code and find_callers/find_callees reveal connections that grep misses.

2. **Read before editing**: Always read_file before using edit_file. The old_string must match exactly.

3. **Make targeted changes**: Use edit_file for surgical modifications. Use write_file only for new files or complete rewrites.

4. **Verify your work**: After making changes, run builds and tests using bash to confirm everything works.

5. **Be precise**: Reference specific files, line numbers, and function names. Show relevant code in your responses.

6. **Iterate**: If a build or test fails, read the error, fix the issue, and try again.

Be concise and direct. Focus on solving the task, not explaining what you're about to do."#
        .to_string()
}
