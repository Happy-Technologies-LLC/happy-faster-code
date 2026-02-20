use ratatui::style::{Color, Modifier, Style};

/// Brand blue from the HappyFasterCode logo: rgb(80, 179, 238)
const BRAND_BLUE: Color = Color::Rgb(80, 179, 238);

pub const BORDER: Style = Style::new().fg(Color::DarkGray);
pub const BORDER_FOCUSED: Style = Style::new().fg(BRAND_BLUE);
pub const TITLE: Style = Style::new().fg(BRAND_BLUE).add_modifier(Modifier::BOLD);
pub const USER_MSG: Style = Style::new().fg(Color::Green);
pub const ASSISTANT_MSG: Style = Style::new().fg(Color::White);
pub const TOOL_CALL: Style = Style::new().fg(Color::Yellow);
pub const TOOL_RESULT: Style = Style::new().fg(Color::DarkGray);
pub const ERROR_MSG: Style = Style::new().fg(Color::Red);
pub const STATUS_BAR: Style = Style::new().fg(BRAND_BLUE);
pub const FILE_ITEM: Style = Style::new().fg(Color::White);
pub const FILE_SELECTED: Style = Style::new().fg(Color::Black).bg(BRAND_BLUE);
pub const CODE_TEXT: Style = Style::new().fg(Color::White);
pub const INPUT_CURSOR: Style = Style::new().fg(BRAND_BLUE);
