//! Widget rendering for the TUI.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};
use revibe_core::modes::AgentMode;

use super::messages::*;
use super::spinner::Spinner;

/// Colors matching Mistral Vibe's theme (extracted from app.tcss).
#[allow(dead_code)]
pub mod colors {
    use ratatui::style::{Color, Style};

    // Exact colors from Mistral Vibe's CSS variables
    pub const PRIMARY: Color = Color::Rgb(255, 175, 0); // #FFAF00 - Mistral orange
    pub const SUCCESS: Color = Color::Rgb(0, 175, 0); // Green for success
    pub const WARNING: Color = Color::Rgb(255, 175, 0); // Orange for warnings
    pub const ERROR: Color = Color::Rgb(255, 85, 85); // Red for errors
    pub const MUTED: Color = Color::Rgb(128, 128, 128); // Gray for muted text (foreground-muted)
    pub const BORDER: Color = Color::Rgb(176, 88, 0); // #b05800 - Darker orange
    pub const BACKGROUND: Color = Color::Rgb(20, 20, 20); // Dark background
    pub const SURFACE: Color = Color::Rgb(38, 38, 38); // $surface - slightly lighter than background
    pub const TEXT: Color = Color::Rgb(220, 220, 220); // Light text
    pub const ACCENT: Color = Color::Rgb(100, 180, 255); // Blue accent

    pub const MODE_SAFE: Color = Color::Rgb(0, 175, 0); // Green for safe mode
    pub const MODE_NEUTRAL: Color = Color::Rgb(128, 128, 128); // Gray for neutral mode
    pub const MODE_DESTRUCTIVE: Color = Color::Rgb(255, 175, 0); // Orange for destructive
    pub const MODE_YOLO: Color = Color::Rgb(255, 85, 85); // Red for yolo mode

    /// Gradient colors for animations (Mistral orange gradient).
    pub const GRADIENT: &[Color] = &[
        Color::Rgb(255, 216, 0), // #FFD800 - Light orange
        Color::Rgb(255, 175, 0), // #FFAF00 - Medium orange
        Color::Rgb(255, 130, 5), // #FF8205 - Darker orange
        Color::Rgb(250, 80, 15), // #FA500F - Red-orange
        Color::Rgb(225, 5, 0),   // #E10500 - Dark red
    ];

    /// Foreground colors to match Mistral Vibe's theme
    pub const FOREGROUND: Color = Color::Rgb(220, 220, 220); // $foreground
    pub const FOREGROUND_MUTED: Color = Color::Rgb(128, 128, 128); // $foreground-muted
    pub const TEXT_MUTED: Color = Color::Rgb(100, 100, 100); // $text-muted

    /// User message colors
    pub const USER_NAME: Color = Color::Rgb(0, 180, 255); // Blue for user name
    pub const USER_TEXT: Color = Color::Rgb(220, 220, 220); // White for user text

    /// Assistant message colors
    pub const ASSISTANT_NAME: Color = Color::Rgb(255, 175, 0); // Orange for assistant
    pub const ASSISTANT_TEXT: Color = Color::Rgb(220, 220, 220); // White for assistant text

    /// Tool colors
    pub const TOOL_NAME: Color = Color::Rgb(180, 120, 255); // Purple for tool names
    pub const TOOL_ARGS: Color = Color::Rgb(120, 200, 120); // Green for tool args
    pub const TOOL_SUCCESS: Color = Color::Rgb(120, 200, 120); // Green for success
    pub const TOOL_ERROR: Color = Color::Rgb(255, 120, 120); // Red for errors

    /// Trust dialog colors
    pub const DIALOG_BG: Color = Color::Rgb(30, 30, 30); // Dark background
    pub const DIALOG_TEXT: Color = Color::Rgb(220, 220, 220); // Light text
    pub const PATH_TEXT: Color = Color::Rgb(255, 175, 0); // Orange for path
    pub const OPTION_TEXT: Color = Color::Rgb(200, 200, 200); // Light gray for options
    pub const SELECTED_OPTION: Color = Color::Rgb(255, 175, 0); // Orange for selected
    pub const HELP_TEXT: Color = Color::Rgb(100, 100, 100); // Gray for help text (matching original)
    pub const SAVE_INFO_TEXT: Color = Color::Rgb(100, 180, 255); // Blue for save info

    /// Trust dialog color functions
    pub fn dialog_bg() -> Style {
        Style::default().bg(DIALOG_BG)
    }

    pub fn dialog_text() -> Style {
        Style::default().fg(DIALOG_TEXT)
    }

    pub fn path_text() -> Style {
        Style::default().fg(PATH_TEXT)
    }

    pub fn option_text() -> Style {
        Style::default().fg(OPTION_TEXT)
    }

    pub fn selected_option() -> Style {
        use ratatui::prelude::Stylize;
        Style::default().fg(SELECTED_OPTION).bold()
    }

    pub fn help_text() -> Style {
        Style::default().fg(HELP_TEXT)
    }

    pub fn save_info_text() -> Style {
        Style::default().fg(SAVE_INFO_TEXT)
    }

    /// Get the appropriate border color for the current mode
    pub fn mode_border_color(mode: &revibe_core::modes::AgentMode) -> Color {
        use revibe_core::modes::AgentMode;
        match mode {
            AgentMode::Default => Color::Rgb(128, 128, 128), // Neutral
            AgentMode::Plan => Color::Rgb(0, 175, 0),        // Safe (green)
            AgentMode::AcceptEdits => Color::Rgb(255, 175, 0), // Destructive (orange)
            AgentMode::AutoApprove => Color::Rgb(255, 85, 85), // Yolo (red)
        }
    }
}

/// Render a user message (matching Mistral Vibe style with surface background).
/// In Python, user messages have:
///   - margin-top: 1
///   - padding: 1 0 (vertical padding)
///   - background: $surface
///     We simulate the surface background with box-drawing characters.
pub fn render_user_message(msg: &UserMessage, width: u16) -> Vec<Line<'static>> {
    let prompt_style = Style::default().fg(colors::PRIMARY).bold();

    let content_style = if msg.pending {
        Style::default().fg(colors::FOREGROUND_MUTED).italic()
    } else {
        Style::default().fg(colors::FOREGROUND).bold()
    };

    let mut lines = Vec::new();

    // margin-top: 1
    lines.push(Line::from(""));

    // Create a surface-styled line (simulating padding and background)
    // Python uses padding: 1 0 which adds 1 line of padding top/bottom
    let surface_style = Style::default().bg(colors::SURFACE);

    // Top padding line with surface background
    let padding_line: String = " ".repeat(width as usize);
    lines.push(Line::styled(padding_line.clone(), surface_style));

    // Content line with surface background
    lines.push(Line::from(vec![
        Span::styled("> ", prompt_style.bg(colors::SURFACE)),
        Span::styled(msg.content.clone(), content_style.bg(colors::SURFACE)),
        // Fill remaining width with surface color
        Span::styled(
            " ".repeat((width as usize).saturating_sub(msg.content.len() + 2)),
            surface_style,
        ),
    ]));

    // Bottom padding line with surface background
    lines.push(Line::styled(padding_line, surface_style));

    lines
}

/// Render an assistant message with markdown formatting.
/// margin-top: 1 (empty line before)
pub fn render_assistant_message(msg: &AssistantMessage) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // margin-top: 1
    lines.push(Line::from(""));

    if msg.content.is_empty() {
        // If no content yet, just show the bullet
        lines.push(Line::from(vec![Span::styled(
            "● ",
            Style::default().fg(colors::PRIMARY),
        )]));
        return lines;
    }

    // Parse markdown and render with styling
    let rendered_lines = render_markdown(&msg.content);

    // Add bullet point to first line, indent subsequent lines
    for (i, line) in rendered_lines.into_iter().enumerate() {
        if i == 0 {
            let mut spans = vec![Span::styled("● ", Style::default().fg(colors::PRIMARY))];
            spans.extend(line.spans);
            lines.push(Line::from(spans));
        } else {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(line.spans);
            lines.push(Line::from(spans));
        }
    }

    lines
}

/// Render markdown content to styled lines.
fn render_markdown(content: &str) -> Vec<Line<'static>> {
    let parser = Parser::new(content);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();

    // Style stack for nested formatting
    let mut bold = false;
    let mut italic = false;
    let mut in_code_block = false;
    let mut heading_level: Option<u8> = None;
    let mut list_depth: usize = 0;

    for event in parser {
        match event {
            Event::Start(tag) => {
                match tag {
                    Tag::Heading { level, .. } => {
                        heading_level = Some(level as u8);
                    }
                    Tag::Paragraph => {}
                    Tag::CodeBlock(_) => {
                        in_code_block = true;
                        // Flush current line before code block
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    Tag::Strong => {
                        bold = true;
                    }
                    Tag::Emphasis => {
                        italic = true;
                    }
                    Tag::List(_) => {
                        list_depth += 1;
                    }
                    Tag::Item => {
                        // Flush current line
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                        // Add list bullet with indentation
                        let indent = "  ".repeat(list_depth.saturating_sub(1));
                        current_spans.push(Span::styled(
                            format!("{}• ", indent),
                            Style::default().fg(colors::PRIMARY),
                        ));
                    }
                    Tag::Link { dest_url, .. } => {
                        // We'll render the link text, then show the URL
                        current_spans.push(Span::styled("[", Style::default().fg(colors::ACCENT)));
                        // Store URL for later (simplified: just mark we're in a link)
                        let _ = dest_url; // Will be shown in End event
                    }
                    Tag::BlockQuote(_) => {
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                        current_spans.push(Span::styled("│ ", Style::default().fg(colors::MUTED)));
                    }
                    _ => {}
                }
            }
            Event::End(tag_end) => {
                match tag_end {
                    TagEnd::Heading(_) => {
                        heading_level = None;
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                        // Add blank line after heading
                        lines.push(Line::from(""));
                    }
                    TagEnd::Paragraph => {
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    TagEnd::CodeBlock => {
                        in_code_block = false;
                    }
                    TagEnd::Strong => {
                        bold = false;
                    }
                    TagEnd::Emphasis => {
                        italic = false;
                    }
                    TagEnd::List(_) => {
                        list_depth = list_depth.saturating_sub(1);
                    }
                    TagEnd::Item => {
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    TagEnd::Link => {
                        current_spans.push(Span::styled("]", Style::default().fg(colors::ACCENT)));
                    }
                    TagEnd::BlockQuote(_) => {
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(text) => {
                let text_str = text.to_string();

                if in_code_block {
                    // Render code block lines with background
                    for line in text_str.lines() {
                        lines.push(Line::from(vec![Span::styled(
                            line.to_string(),
                            Style::default().bg(colors::SURFACE).fg(colors::FOREGROUND),
                        )]));
                    }
                } else {
                    let style = build_text_style(bold, italic, heading_level);
                    current_spans.push(Span::styled(text_str, style));
                }
            }
            Event::Code(code) => {
                // Inline code
                current_spans.push(Span::styled(
                    format!("`{}`", code),
                    Style::default().bg(colors::SURFACE).fg(colors::FOREGROUND),
                ));
            }
            Event::SoftBreak => {
                current_spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
            }
            Event::Rule => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
                lines.push(Line::from(vec![Span::styled(
                    "────────────────────────────────",
                    Style::default().fg(colors::MUTED),
                )]));
            }
            _ => {}
        }
    }

    // Flush any remaining content
    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    lines
}

/// Build a text style based on current formatting state.
fn build_text_style(bold: bool, italic: bool, heading_level: Option<u8>) -> Style {
    let mut style = Style::default();

    if let Some(level) = heading_level {
        match level {
            1 => {
                style = style.fg(colors::PRIMARY).add_modifier(Modifier::BOLD);
            }
            2 => {
                style = style.fg(colors::ACCENT).add_modifier(Modifier::BOLD);
            }
            3 => {
                style = style.fg(colors::SUCCESS).add_modifier(Modifier::BOLD);
            }
            4..=6 => {
                style = style.add_modifier(Modifier::BOLD);
            }
            _ => {}
        }
    } else {
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
    }

    style
}

/// Line spinner frames for tool calls (matching Mistral Vibe's StatusMessage).
const LINE_FRAMES: &[char] = &['|', '/', '-', '\\'];

/// Render a tool call message (matching Mistral Vibe StatusMessage style with line spinner).
/// margin-top: 1 (empty line before)
pub fn render_tool_call(
    msg: &ToolCallMessage,
    _spinner: &Spinner,
    color_index: usize,
) -> Vec<Line<'static>> {
    let icon = if msg.spinning {
        // Use line spinner for tool calls
        let frame = LINE_FRAMES[color_index % LINE_FRAMES.len()];
        Span::styled(format!("{} ", frame), Style::default().fg(colors::WARNING))
    } else {
        Span::styled("✓ ", Style::default().fg(colors::SUCCESS))
    };

    vec![
        // margin-top: 1
        Line::from(""),
        Line::from(vec![icon, Span::raw(msg.summary.clone())]),
    ]
}

/// Render a tool result message (matching Mistral Vibe style with expanding border).
/// margin-top: 0 (no extra spacing before)
pub fn render_tool_result(msg: &ToolResultMessage) -> Vec<Line<'static>> {
    let border_style = Style::default().fg(colors::MUTED);

    // Determine text style based on result type
    let text_style = if msg.error.is_some() {
        Style::default().fg(colors::ERROR)
    } else if msg.skipped {
        Style::default().fg(colors::WARNING)
    } else {
        Style::default()
    };

    // Get display text and split into lines
    let display_text = msg.summary();
    let content_lines: Vec<&str> = display_text.lines().collect();
    let line_count = content_lines.len().max(1);

    let mut lines = Vec::new();

    // Render with expanding border (⎢ for middle lines, ⎣ for last line)
    for (i, line) in content_lines.iter().enumerate() {
        let border = if i == line_count - 1 { "⎣" } else { "⎢" };

        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", border), border_style),
            Span::styled(line.to_string(), text_style),
        ]));
    }

    // If no lines, show single bordered line
    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  ⎣ ", border_style),
            Span::styled(display_text, text_style),
        ]));
    }

    lines
}

/// Render a system message with markdown support.
/// margin-top: 1 for visibility (system messages are important notifications)
pub fn render_system_message(msg: &SystemMessage) -> Vec<Line<'static>> {
    let text_style = match msg.kind {
        SystemMessageKind::Info => Style::default(),
        SystemMessageKind::Warning => Style::default().fg(colors::WARNING),
        SystemMessageKind::Error => Style::default().fg(colors::ERROR),
    };

    let border_style = Style::default().fg(colors::MUTED);
    let content_lines: Vec<&str> = msg.content.lines().collect();
    let line_count = content_lines.len().max(1);

    let mut lines = Vec::new();

    // margin-top: 1
    lines.push(Line::from(""));

    // Render with expanding border
    for (i, line) in content_lines.iter().enumerate() {
        let border = if i == line_count - 1 { "⎣" } else { "⎢" };

        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", border), border_style),
            Span::styled(line.to_string(), text_style),
        ]));
    }

    lines
}

/// Render a user command message.
/// margin-top: 0
pub fn render_user_command_message(msg: &UserCommandMessage) -> Vec<Line<'static>> {
    let border_style = Style::default().fg(colors::MUTED);
    let content_lines: Vec<&str> = msg.content.lines().collect();
    let line_count = content_lines.len().max(1);

    let mut lines = Vec::new();

    for (i, line) in content_lines.iter().enumerate() {
        let border = if i == line_count - 1 { "⎣" } else { "⎢" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", border), border_style),
            Span::raw(line.to_string()),
        ]));
    }

    lines
}

/// Render an error message.
/// margin-top: 0
pub fn render_error_message(msg: &ErrorMessage) -> Vec<Line<'static>> {
    let border_style = Style::default().fg(colors::MUTED);
    let error_style = Style::default().fg(colors::ERROR).bold();
    let content_lines: Vec<&str> = msg.content.lines().collect();
    let line_count = content_lines.len().max(1);

    let mut lines = Vec::new();

    for (i, line) in content_lines.iter().enumerate() {
        let border = if i == line_count - 1 { "⎣" } else { "⎢" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {border} "), border_style),
            Span::styled(line.to_string(), error_style),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  ⎣ ", border_style),
            Span::styled(msg.content.clone(), error_style),
        ]));
    }

    lines
}

/// Render a warning message.
/// margin-top: 0
pub fn render_warning_message(msg: &WarningMessage) -> Vec<Line<'static>> {
    let border_style = Style::default().fg(colors::MUTED);
    let warning_style = Style::default().fg(colors::WARNING);
    let content_lines: Vec<&str> = msg.content.lines().collect();
    let line_count = content_lines.len().max(1);

    let mut lines = Vec::new();

    for (i, line) in content_lines.iter().enumerate() {
        let border = if i == line_count - 1 { "⎣" } else { "⎢" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", border), border_style),
            Span::styled(line.to_string(), warning_style),
        ]));
    }

    lines
}

/// Render an interrupt message.
pub fn render_interrupt() -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::styled("  ⎣ ", Style::default().fg(colors::MUTED)),
        Span::styled(
            "Interrupted · What would you like Revibe to do instead?",
            Style::default().fg(colors::WARNING),
        ),
    ])]
}

/// Render a bash output message with surface background.
/// In Python: margin-top: 1, background: $surface, padding: 1 2
pub fn render_bash_output(
    msg: &super::messages::BashOutputMessage,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let surface_style = Style::default().bg(colors::SURFACE);
    let padding = "  "; // padding: 1 2 means 2 chars horizontal padding

    // margin-top: 1
    lines.push(Line::from(""));

    // Helper to pad line to full width
    let pad_line = |spans: Vec<Span<'static>>, content_len: usize| -> Line<'static> {
        let mut all_spans = vec![Span::styled(padding, surface_style)];
        all_spans.extend(spans);
        let remaining = (width as usize).saturating_sub(content_len + padding.len() * 2);
        all_spans.push(Span::styled(
            " ".repeat(remaining + padding.len()),
            surface_style,
        ));
        Line::from(all_spans)
    };

    // Top padding line
    lines.push(Line::styled(" ".repeat(width as usize), surface_style));

    // CWD line with exit status
    let exit_part = if msg.exit_code == 0 {
        Span::styled(
            "✓",
            Style::default().fg(colors::SUCCESS).bg(colors::SURFACE),
        )
    } else {
        Span::styled(
            format!("✗ ({})", msg.exit_code),
            Style::default().fg(colors::ERROR).bg(colors::SURFACE),
        )
    };

    let cwd_len = msg.cwd.len() + 1 + if msg.exit_code == 0 { 1 } else { 6 };
    lines.push(pad_line(
        vec![
            Span::styled(
                msg.cwd.clone(),
                Style::default().fg(colors::MUTED).bg(colors::SURFACE),
            ),
            Span::styled(" ", surface_style),
            exit_part,
        ],
        cwd_len,
    ));

    // Command line
    let cmd_len = 2 + msg.command.len();
    lines.push(pad_line(
        vec![
            Span::styled(
                "> ",
                Style::default()
                    .fg(colors::PRIMARY)
                    .bold()
                    .bg(colors::SURFACE),
            ),
            Span::styled(
                msg.command.clone(),
                Style::default().fg(colors::FOREGROUND).bg(colors::SURFACE),
            ),
        ],
        cmd_len,
    ));

    // Blank line before output (matching Python's margin-bottom: 1 on command line)
    lines.push(Line::styled(" ".repeat(width as usize), surface_style));

    // Output lines with surface background
    for line in msg.output.lines() {
        let line_len = line.len();
        lines.push(pad_line(
            vec![Span::styled(
                line.to_string(),
                Style::default().fg(colors::FOREGROUND).bg(colors::SURFACE),
            )],
            line_len,
        ));
    }

    // Bottom padding line
    lines.push(Line::styled(" ".repeat(width as usize), surface_style));

    lines
}

/// Render a compact message.
/// margin-top: 1
pub fn render_compact(
    msg: &CompactMessage,
    spinner: &Spinner,
    color_index: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("")]; // margin-top: 1

    if msg.in_progress {
        let color = colors::GRADIENT[color_index % colors::GRADIENT.len()];
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", spinner.current_frame()),
                Style::default().fg(color),
            ),
            Span::raw("Compacting conversation history..."),
        ]));
    } else if let Some(error) = &msg.error {
        lines.push(Line::from(vec![
            Span::styled("✗ ", Style::default().fg(colors::ERROR)),
            Span::styled(
                format!("Error: {error}"),
                Style::default().fg(colors::ERROR),
            ),
        ]));
    } else {
        let old = msg.old_tokens.unwrap_or(0);
        let new = msg.new_tokens.unwrap_or(0);
        lines.push(Line::from(vec![
            Span::styled("✓ ", Style::default().fg(colors::SUCCESS)),
            Span::styled(
                format!("Compacted: {}k → {}k tokens", old / 1000, new / 1000),
                Style::default().fg(colors::SUCCESS),
            ),
        ]));
    }

    lines
}

/// Render the loading indicator (matching Mistral Vibe's gradient wave animation).
///
/// The animation works by having a "wave" that propagates through the text,
/// changing each character from the current color to the next color in sequence.
pub fn render_loading(
    elapsed_secs: u64,
    status: &str,
    spinner: &Spinner,
    transition_progress: usize,
) -> Line<'static> {
    let mut spans = Vec::new();

    // Total elements: spinner + each char of status + ellipsis + space
    let total_elements = 1 + status.chars().count() + 2;

    // Current and next color indices in the gradient
    let current_color_index = (transition_progress / total_elements) % colors::GRADIENT.len();
    let next_color_index = (current_color_index + 1) % colors::GRADIENT.len();

    // Position of the wave front (which character is currently transitioning)
    let wave_position = transition_progress % total_elements;

    // Get color for a position - characters behind the wave are the next color,
    // characters at or ahead of the wave are the current color
    let get_color = |position: usize| -> Color {
        if position < wave_position {
            colors::GRADIENT[next_color_index]
        } else {
            colors::GRADIENT[current_color_index]
        }
    };

    // Spinner character with wave color
    let spinner_color = get_color(0);
    spans.push(Span::styled(
        format!("{} ", spinner.current_frame()),
        Style::default().fg(spinner_color),
    ));

    // Each character of status with its own wave color
    for (i, c) in status.chars().enumerate() {
        let color = get_color(1 + i);
        spans.push(Span::styled(c.to_string(), Style::default().fg(color)));
    }

    // Ellipsis with wave color
    let ellipsis_pos = 1 + status.chars().count();
    let ellipsis_color = get_color(ellipsis_pos);
    spans.push(Span::styled("…", Style::default().fg(ellipsis_color)));

    // Space after ellipsis
    let space_color = get_color(ellipsis_pos + 1);
    spans.push(Span::styled(" ", Style::default().fg(space_color)));

    // Hint text (neutral color, not part of wave)
    spans.push(Span::styled(
        format!("({}s esc to interrupt)", elapsed_secs),
        Style::default().fg(colors::MUTED),
    ));

    Line::from(spans)
}



/// Render the input box (matching Mistral Vibe style).
pub fn render_input_box<'a>(
    content: &'a str,
    _cursor: usize,
    mode: AgentMode,
    multiline: bool,
) -> Paragraph<'a> {
    let border_color = colors::mode_border_color(&mode);

    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(border_color))
        .border_type(ratatui::widgets::BorderType::Plain) // Use plain borders like original
        .padding(Padding::new(1, 1, 0, 0));

    // Build input content with proper multiline handling
    let prompt = Span::styled("> ", Style::default().fg(colors::PRIMARY).bold());

    // Show multiline indicator
    let multiline_indicator = if multiline {
        Span::styled("📝 ", Style::default().fg(colors::ACCENT))
    } else {
        Span::raw("")
    };

    // Show placeholder when empty (matching Mistral Vibe's "Ask anything..." placeholder)
    let content_span = if content.is_empty() {
        Span::styled(
            "Ask anything...",
            Style::default().fg(colors::FOREGROUND_MUTED).italic(),
        )
    } else {
        Span::raw(content)
    };

    // For multiline mode, we need to handle each line separately
    if multiline && !content.is_empty() {
        let lines: Vec<Line> = content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                if i == 0 {
                    // First line gets the full prompt
                    Line::from(vec![
                        multiline_indicator.clone(),
                        prompt.clone(),
                        Span::raw(line),
                    ])
                } else {
                    // Subsequent lines get indentation to match prompt width
                    Line::from(vec![Span::raw("  "), Span::raw(line)])
                }
            })
            .collect();

        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(colors::FOREGROUND))
    } else {
        // Single line mode
        let text = Line::from(vec![multiline_indicator, prompt, content_span]);
        Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(colors::FOREGROUND))
    }
}

/// Render the approval dialog (matching Mistral Vibe style with enhanced formatting).
pub fn render_approval_dialog(
    frame: &mut Frame,
    area: Rect,
    tool_name: &str,
    args: &serde_json::Value,
    selected: usize,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::WARNING))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        // Title - matching Mistral's "⚠ {tool_name} command" style
        Line::from(vec![Span::styled(
            format!("⚠ {} command", tool_name),
            Style::default().fg(colors::WARNING).bold(),
        )]),
        Line::from(""),
    ];

    // Show args with special handling for different tool types
    if let Some(obj) = args.as_object() {
        for (key, value) in obj {
            let value_str = match value {
                serde_json::Value::String(s) => {
                    // Truncate long strings
                    if s.len() > 60 {
                        format!("{}...", &s[..57])
                    } else {
                        s.clone()
                    }
                }
                _ => value.to_string(),
            };

            // Special styling for command/path fields
            let value_style = if key == "command" || key == "path" || key == "file_path" {
                Style::default().fg(colors::PRIMARY)
            } else {
                Style::default()
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{}: ", key), Style::default().fg(colors::MUTED)),
                Span::styled(value_str, value_style),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Options with keyboard shortcuts (matching Mistral Vibe's ApprovalApp)
    let options = [
        ("Yes", "yes", "y"),
        (
            &format!("Yes and always allow {tool_name} for this session"),
            "yes",
            "",
        ),
        ("No and tell the agent what to do instead", "no", "n"),
    ];

    for (i, (text, color_type, shortcut)) in options.iter().enumerate() {
        let is_selected = i == selected;
        let cursor = if is_selected { "› " } else { "  " };

        // Style matching Mistral Vibe: yes options are green, no options are red
        let style = if is_selected {
            if *color_type == "no" {
                Style::default().fg(colors::ERROR).bold()
            } else {
                Style::default().fg(colors::SUCCESS).bold()
            }
        } else if *color_type == "no" {
            Style::default().fg(colors::ERROR)
        } else {
            Style::default().fg(colors::SUCCESS)
        };

        let mut spans = vec![
            Span::styled(cursor, style),
            Span::styled(format!("{}. {}", i + 1, text), style),
        ];

        // Show keyboard shortcut hint for first and last option
        if !shortcut.is_empty() {
            spans.push(Span::styled(
                format!(" ({})", shortcut),
                Style::default().fg(colors::MUTED),
            ));
        }

        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));

    // Enhanced help line with keyboard shortcuts
    lines.push(Line::from(vec![
        Span::styled("↑↓ navigate  ", Style::default().fg(colors::MUTED)),
        Span::styled("Enter", Style::default().fg(colors::TEXT)),
        Span::styled(" select  ", Style::default().fg(colors::MUTED)),
        Span::styled("y/n", Style::default().fg(colors::TEXT)),
        Span::styled(" quick  ", Style::default().fg(colors::MUTED)),
        Span::styled("ESC", Style::default().fg(colors::TEXT)),
        Span::styled(" reject", Style::default().fg(colors::MUTED)),
    ]));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

/// Render the completion popup (matching Mistral Vibe style).
pub fn render_completion_popup(
    frame: &mut Frame,
    area: Rect,
    suggestions: &[super::completion::CompletionSuggestion],
    selected_index: usize,
) {
    if suggestions.is_empty() {
        return;
    }

    // Calculate popup height (1 line per suggestion + 2 for padding)
    let height = (suggestions.len() as u16 + 2).min(12);

    // Position popup above the input area, ensuring it doesn't go off-screen
    let popup_y = if area.y >= height {
        area.y.saturating_sub(height) // Position above input
    } else {
        area.y + area.height // Position below input if not enough space above
    };

    // Calculate width based on longest suggestion
    let max_suggestion_width = suggestions
        .iter()
        .map(|s| {
            s.completion.len()
                + if s.description.is_empty() {
                    0
                } else {
                    s.description.len() + 4
                }
        })
        .max()
        .unwrap_or(0)
        .min(60);

    let popup_area = Rect {
        x: area.x + 2, // Align with input content (after "> ")
        y: popup_y,
        width: max_suggestion_width as u16,
        height,
    };

    // Clear the area with background (matching Mistral Vibe's #completion-popup styling)
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::MUTED))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(colors::SURFACE))
        .padding(Padding::new(1, 1, 1, 1));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Render suggestions
    let mut lines = Vec::new();
    for (i, suggestion) in suggestions.iter().enumerate() {
        let is_selected = i == selected_index;

        let label_style = if is_selected {
            Style::default()
                .fg(colors::PRIMARY)
                .bold()
                .bg(colors::BACKGROUND)
        } else {
            Style::default().fg(colors::TEXT).bold()
        };

        let desc_style = if is_selected {
            Style::default()
                .fg(colors::MUTED)
                .italic()
                .bg(colors::BACKGROUND)
        } else {
            Style::default().fg(colors::MUTED)
        };

        let mut spans = vec![Span::styled(suggestion.completion.clone(), label_style)];
        if !suggestion.description.is_empty() {
            spans.push(Span::styled("  ", desc_style));
            spans.push(Span::styled(suggestion.description.clone(), desc_style));
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
