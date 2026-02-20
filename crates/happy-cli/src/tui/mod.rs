pub mod theme;

use crate::agent::{Agent, AgentEvent};
use crate::repo::RepoContext;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use std::time::Duration;
use tokio::sync::mpsc;

/// Commands sent from the TUI to the agent task.
enum AgentCommand {
    Query(String),
    Clear,
}

/// Which panel has focus.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Focus {
    Chat,
    Files,
    Preview,
}

/// A chat entry in the conversation.
#[derive(Debug, Clone)]
enum ChatEntry {
    User(String),
    AssistantText(String),
    ToolCall(String),
    ToolResult {
        name: String,
        preview: String,
        is_error: bool,
    },
    Error(String),
}

/// Application state for the TUI.
struct AppState {
    focus: Focus,
    // Chat
    chat_entries: Vec<ChatEntry>,
    chat_scroll: u16,
    current_streaming: String,
    is_streaming: bool,
    // Input
    input: String,
    input_cursor: usize,
    // Files
    files: Vec<String>,
    file_state: ListState,
    // Preview
    preview_content: String,
    preview_title: String,
    // Status
    model_name: String,
    provider_name: String,
    total_tokens: u32,
    repo_stats: String,
    repo_path: String,
}

impl AppState {
    fn new(
        files: Vec<String>,
        repo_stats: String,
        repo_path: String,
        model_name: String,
        provider_name: String,
    ) -> Self {
        let mut file_state = ListState::default();
        if !files.is_empty() {
            file_state.select(Some(0));
        }

        let welcome = format!(
            "Welcome to HappyFasterCode!\n\
             Indexed {} — ask me anything about this codebase.\n\
             Type /clear to reset, /quit to exit, Tab to switch panels.",
            repo_stats
        );

        Self {
            focus: Focus::Chat,
            chat_entries: vec![ChatEntry::AssistantText(welcome)],
            chat_scroll: 0,
            current_streaming: String::new(),
            is_streaming: false,
            input: String::new(),
            input_cursor: 0,
            files,
            file_state,
            preview_content: String::new(),
            preview_title: String::new(),
            model_name,
            provider_name,
            total_tokens: 0,
            repo_stats,
            repo_path,
        }
    }

    fn flush_streaming(&mut self) {
        if !self.current_streaming.is_empty() {
            let text = std::mem::take(&mut self.current_streaming);
            self.chat_entries.push(ChatEntry::AssistantText(text));
        }
    }

    fn selected_file(&self) -> Option<&str> {
        self.file_state
            .selected()
            .and_then(|i| self.files.get(i))
            .map(|s| s.as_str())
    }
}

/// Start the TUI event loop.
pub async fn run(agent: Agent, repo: RepoContext) -> anyhow::Result<()> {
    // Extract what the TUI needs before moving agent+repo to the background task
    let files = repo.list_files();
    let stats = repo.graph.stats();
    let repo_stats = format!(
        "{} nodes, {} edges, {} files",
        stats.node_count, stats.edge_count, stats.file_count
    );
    let repo_path = repo.repo_path.clone();
    let model_name = agent.model_name().to_string();
    let provider_name = agent.provider_name().to_string();

    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(files, repo_stats, repo_path, model_name, provider_name);

    // Channels: TUI -> Agent (commands) and Agent -> TUI (events)
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<AgentCommand>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

    // Spawn agent in a background task — it owns agent + repo
    tokio::spawn(agent_task(agent, repo, cmd_rx, event_tx));

    let result = run_loop(&mut terminal, &mut state, &cmd_tx, &mut event_rx).await;

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Background task: receives commands, runs agent queries, sends events back.
async fn agent_task(
    mut agent: Agent,
    repo: RepoContext,
    mut cmd_rx: mpsc::UnboundedReceiver<AgentCommand>,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            AgentCommand::Query(input) => {
                agent.query(&repo, &input, &event_tx).await;
            }
            AgentCommand::Clear => {
                agent.clear();
            }
        }
    }
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut AppState,
    cmd_tx: &mpsc::UnboundedSender<AgentCommand>,
    event_rx: &mut mpsc::UnboundedReceiver<AgentEvent>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| render(f, state))?;

        tokio::select! {
            // Poll for terminal key events at ~60fps
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                while event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = event::read()? {
                        match handle_key(key, state, cmd_tx) {
                            KeyAction::Quit => return Ok(()),
                            KeyAction::Continue => {}
                        }
                    }
                }
            }
            // Process agent events — drain all queued events before redrawing
            Some(agent_event) = event_rx.recv() => {
                handle_agent_event(agent_event, state);
                while let Ok(extra) = event_rx.try_recv() {
                    handle_agent_event(extra, state);
                }
            }
        }
    }
}

enum KeyAction {
    Continue,
    Quit,
}

fn handle_key(
    key: event::KeyEvent,
    state: &mut AppState,
    cmd_tx: &mpsc::UnboundedSender<AgentCommand>,
) -> KeyAction {
    // Global keys
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('d') => return KeyAction::Quit,
            _ => {}
        }
    }

    if key.code == KeyCode::Tab {
        state.focus = match state.focus {
            Focus::Chat => Focus::Files,
            Focus::Files => Focus::Preview,
            Focus::Preview => Focus::Chat,
        };
        return KeyAction::Continue;
    }

    match state.focus {
        Focus::Chat => handle_chat_key(key, state, cmd_tx),
        Focus::Files => {
            handle_files_key(key, state);
            KeyAction::Continue
        }
        Focus::Preview => {
            handle_preview_key(key, state);
            KeyAction::Continue
        }
    }
}

fn handle_chat_key(
    key: event::KeyEvent,
    state: &mut AppState,
    cmd_tx: &mpsc::UnboundedSender<AgentCommand>,
) -> KeyAction {
    if state.is_streaming {
        return KeyAction::Continue;
    }

    match key.code {
        KeyCode::Enter => {
            let input = state.input.trim().to_string();
            if input.is_empty() {
                return KeyAction::Continue;
            }
            state.input.clear();
            state.input_cursor = 0;

            // Handle slash commands
            match input.as_str() {
                "/clear" => {
                    let _ = cmd_tx.send(AgentCommand::Clear);
                    state.chat_entries.clear();
                    state.current_streaming.clear();
                    return KeyAction::Continue;
                }
                "/quit" | "/exit" => return KeyAction::Quit,
                _ => {}
            }

            state.chat_entries.push(ChatEntry::User(input.clone()));
            state.is_streaming = true;
            state.current_streaming.clear();

            // Send query to agent task via channel
            let _ = cmd_tx.send(AgentCommand::Query(input));
        }
        KeyCode::Char(c) => {
            state.input.insert(state.input_cursor, c);
            state.input_cursor += 1;
        }
        KeyCode::Backspace => {
            if state.input_cursor > 0 {
                state.input_cursor -= 1;
                state.input.remove(state.input_cursor);
            }
        }
        KeyCode::Left => {
            if state.input_cursor > 0 {
                state.input_cursor -= 1;
            }
        }
        KeyCode::Right => {
            if state.input_cursor < state.input.len() {
                state.input_cursor += 1;
            }
        }
        KeyCode::Up => {
            if state.chat_scroll > 0 {
                state.chat_scroll -= 1;
            }
        }
        KeyCode::Down => {
            state.chat_scroll += 1;
        }
        KeyCode::Home => state.input_cursor = 0,
        KeyCode::End => state.input_cursor = state.input.len(),
        _ => {}
    }

    KeyAction::Continue
}

fn handle_files_key(key: event::KeyEvent, state: &mut AppState) {
    match key.code {
        KeyCode::Up => {
            if let Some(i) = state.file_state.selected() {
                if i > 0 {
                    state.file_state.select(Some(i - 1));
                }
            }
        }
        KeyCode::Down => {
            if let Some(i) = state.file_state.selected() {
                if i < state.files.len().saturating_sub(1) {
                    state.file_state.select(Some(i + 1));
                }
            }
        }
        KeyCode::Enter => {
            // Clone the selected file name to release the borrow on state
            let selected = state.selected_file().map(|f| f.to_string());
            if let Some(file) = selected {
                let full_path = if std::path::Path::new(&file).is_absolute() {
                    file.clone()
                } else {
                    format!("{}/{}", state.repo_path, file)
                };
                state.preview_title = file;
                state.preview_content = std::fs::read_to_string(&full_path)
                    .unwrap_or_else(|e| format!("Error: {}", e));
            }
        }
        _ => {}
    }
}

fn handle_preview_key(key: event::KeyEvent, _state: &mut AppState) {
    match key.code {
        KeyCode::Up => {}
        KeyCode::Down => {}
        _ => {}
    }
}

fn handle_agent_event(event: AgentEvent, state: &mut AppState) {
    match event {
        AgentEvent::TextDelta(text) => {
            state.current_streaming.push_str(&text);
        }
        AgentEvent::ToolCallStart { name } => {
            state.flush_streaming();
            state.chat_entries.push(ChatEntry::ToolCall(name));
        }
        AgentEvent::ToolCallResult {
            name,
            preview,
            is_error,
        } => {
            state.chat_entries.push(ChatEntry::ToolResult {
                name,
                preview,
                is_error,
            });
        }
        AgentEvent::TurnComplete {
            total_input_tokens,
            total_output_tokens,
        } => {
            state.flush_streaming();
            state.is_streaming = false;
            state.total_tokens = total_input_tokens + total_output_tokens;
        }
        AgentEvent::Error(msg) => {
            state.flush_streaming();
            state.chat_entries.push(ChatEntry::Error(msg));
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render(f: &mut Frame, state: &AppState) {
    let size = f.area();

    // Main layout: [Files | Chat+Input | Preview]
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(50),
            Constraint::Percentage(30),
        ])
        .split(size);

    // Left panel: Files + Stats
    let files_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(main_chunks[0]);

    render_files(f, state, files_chunks[0]);
    render_stats(f, state, files_chunks[1]);

    // Center panel: Chat + Input + Status
    let chat_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .split(main_chunks[1]);

    render_chat(f, state, chat_chunks[0]);
    render_input(f, state, chat_chunks[1]);
    render_status_bar(f, state, chat_chunks[2]);

    // Right panel: Preview
    render_preview(f, state, main_chunks[2]);
}

fn render_files(f: &mut Frame, state: &AppState, area: Rect) {
    let border_style = if state.focus == Focus::Files {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };

    let items: Vec<ListItem> = state
        .files
        .iter()
        .map(|f_path| ListItem::new(f_path.as_str()).style(theme::FILE_ITEM))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Files ")
                .title_style(theme::TITLE)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(theme::FILE_SELECTED);

    f.render_stateful_widget(list, area, &mut state.file_state.clone());
}

fn render_stats(f: &mut Frame, state: &AppState, area: Rect) {
    let stats = Paragraph::new(state.repo_stats.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::BORDER),
    ).style(theme::STATUS_BAR);
    f.render_widget(stats, area);
}

fn render_chat(f: &mut Frame, state: &AppState, area: Rect) {
    let border_style = if state.focus == Focus::Chat {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };

    let mut lines: Vec<Line> = Vec::new();

    for entry in &state.chat_entries {
        match entry {
            ChatEntry::User(text) => {
                lines.push(Line::styled(format!("> {}", text), theme::USER_MSG));
                lines.push(Line::raw(""));
            }
            ChatEntry::AssistantText(text) => {
                for line in text.lines() {
                    lines.push(Line::styled(line.to_string(), theme::ASSISTANT_MSG));
                }
                lines.push(Line::raw(""));
            }
            ChatEntry::ToolCall(name) => {
                lines.push(Line::styled(
                    format!("[tool: {}]", name),
                    theme::TOOL_CALL,
                ));
            }
            ChatEntry::ToolResult {
                name,
                preview,
                is_error,
            } => {
                let style = if *is_error {
                    theme::ERROR_MSG
                } else {
                    theme::TOOL_RESULT
                };
                let prefix = if *is_error { "error" } else { "result" };
                lines.push(Line::styled(
                    format!(
                        "[{}: {} -> {}]",
                        prefix,
                        name,
                        preview.chars().take(60).collect::<String>()
                    ),
                    style,
                ));
            }
            ChatEntry::Error(msg) => {
                lines.push(Line::styled(format!("Error: {}", msg), theme::ERROR_MSG));
            }
        }
    }

    // Show streaming text
    if !state.current_streaming.is_empty() {
        for line in state.current_streaming.lines() {
            lines.push(Line::styled(line.to_string(), theme::ASSISTANT_MSG));
        }
    }

    if state.is_streaming {
        lines.push(Line::styled("▌", theme::INPUT_CURSOR));
    }

    let chat = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Chat ")
                .title_style(theme::TITLE)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((state.chat_scroll, 0));

    f.render_widget(chat, area);
}

fn render_input(f: &mut Frame, state: &AppState, area: Rect) {
    let display = if state.is_streaming {
        " (streaming...)".to_string()
    } else {
        state.input.clone()
    };

    let input = Paragraph::new(display.as_str())
        .block(
            Block::default()
                .title(" > ")
                .title_style(theme::INPUT_CURSOR)
                .borders(Borders::ALL)
                .border_style(if state.focus == Focus::Chat {
                    theme::BORDER_FOCUSED
                } else {
                    theme::BORDER
                }),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(input, area);

    // Show cursor position accounting for text wrapping
    if state.focus == Focus::Chat && !state.is_streaming {
        let inner_width = area.width.saturating_sub(2) as usize; // minus borders
        if inner_width > 0 {
            let cursor_row = state.input_cursor / inner_width;
            let cursor_col = state.input_cursor % inner_width;
            f.set_cursor_position(Position::new(
                area.x + cursor_col as u16 + 1,
                area.y + cursor_row as u16 + 1,
            ));
        }
    }
}

fn render_status_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let status = format!(
        " {} | {} | tokens: {} | Tab to switch panels | Ctrl-C to quit",
        state.provider_name, state.model_name, state.total_tokens
    );
    let bar = Paragraph::new(status).style(theme::STATUS_BAR);
    f.render_widget(bar, area);
}

fn render_preview(f: &mut Frame, state: &AppState, area: Rect) {
    let border_style = if state.focus == Focus::Preview {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };

    let title = if state.preview_title.is_empty() {
        " Preview ".to_string()
    } else {
        format!(" {} ", state.preview_title)
    };

    let content = if state.preview_content.is_empty() {
        "Select a file to preview".to_string()
    } else {
        state.preview_content.clone()
    };

    let preview = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .title_style(theme::TITLE)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .style(theme::CODE_TEXT)
        .wrap(Wrap { trim: false });

    f.render_widget(preview, area);
}
