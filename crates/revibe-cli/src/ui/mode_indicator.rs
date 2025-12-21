//! Mode indicator widget (matching Mistral Vibe style).

use ratatui::prelude::*;
use revibe_core::modes::{AgentMode, ModeSafety};

use super::widgets::colors;

/// Get the icon for a mode (matching Mistral Vibe).
fn mode_icon(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Default => "⏵",
        AgentMode::Plan => "⏸︎",
        AgentMode::AcceptEdits => "⏵⏵",
        AgentMode::AutoApprove => "⏵⏵⏵",
    }
}

/// Mode indicator widget.
pub struct ModeIndicator {
    /// Current mode.
    pub mode: AgentMode,
}

impl ModeIndicator {
    /// Create a new mode indicator.
    pub fn new(mode: AgentMode) -> Self {
        Self { mode }
    }

    /// Render the mode indicator (matching Mistral Vibe: "{icon} {name} mode (shift+tab to cycle)").
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let mode_color = match self.mode.safety() {
            ModeSafety::Safe => colors::MODE_SAFE,
            ModeSafety::Neutral => colors::MODE_NEUTRAL,
            ModeSafety::Destructive => colors::MODE_DESTRUCTIVE,
            ModeSafety::Yolo => colors::MODE_YOLO,
        };

        let icon = mode_icon(self.mode);
        let name = self.mode.display_name().to_lowercase();
        let text = format!("{} {} mode (shift+tab to cycle)", icon, name);

        let line = Line::from(vec![Span::styled(text, Style::default().fg(mode_color))]);

        buf.set_line(area.x, area.y, &line, area.width);
    }

    /// Update the mode.
    pub fn set_mode(&mut self, mode: AgentMode) {
        self.mode = mode;
    }
}
