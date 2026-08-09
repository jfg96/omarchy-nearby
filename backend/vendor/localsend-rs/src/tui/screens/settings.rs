//! Settings screen.

use crate::protocol::DeviceInfo;
use crate::tui::theme::THEME;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Settings screen state.
pub struct SettingsScreen {
    pub device_info: DeviceInfo,
    pub auto_accept: bool,
    pub save_directory: String,
}

impl SettingsScreen {
    pub fn new(device_info: DeviceInfo, save_directory: String) -> Self {
        Self {
            device_info,
            auto_accept: false,
            save_directory,
        }
    }
}

impl Widget for &SettingsScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" ⚙️ Settings ")
            .title_style(THEME.title)
            .borders(Borders::ALL);

        let inner = block.inner(area);
        block.render(area, buf);

        let layout = Layout::vertical([
            Constraint::Min(0),    // Settings list
            Constraint::Length(2), // Help
        ])
        .split(inner);

        let lines = vec![
            Line::from(vec![
                Span::styled("Device Name:   ", THEME.normal),
                Span::styled(&self.device_info.alias, THEME.device_alias),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Port:          ", THEME.normal),
                Span::raw(self.device_info.port.to_string()),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Protocol:      ", THEME.normal),
                Span::raw(self.device_info.protocol.to_string()),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Save Directory:", THEME.normal),
                Span::raw(&self.save_directory),
            ]),
            Line::raw(""),
            // The one interactive setting: always highlighted so it reads as
            // the focused control; Space/Enter flips it.
            Line::from(vec![
                Span::styled("▶ Auto Accept: ", THEME.selected),
                Span::styled(
                    if self.auto_accept {
                        "[x] On"
                    } else {
                        "[ ] Off"
                    },
                    THEME.selected,
                ),
            ]),
        ];

        Paragraph::new(lines).render(layout[0], buf);

        // Help
        let help = Line::from(vec![
            Span::styled(" Space ", THEME.key),
            Span::styled(" Toggle auto-accept ", THEME.key_desc),
            Span::styled(" Esc ", THEME.key),
            Span::styled(" Back ", THEME.key_desc),
        ]);
        Paragraph::new(help).centered().render(layout[1], buf);
    }
}
