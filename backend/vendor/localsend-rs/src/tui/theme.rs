//! TUI theme and styling constants.

use ratatui::style::{Color, Modifier, Style};

/// Application color scheme.
pub struct Theme {
    pub root: Style,
    pub title: Style,
    pub selected: Style,
    pub normal: Style,
    pub status_bar: Style,
    pub status_success: Style,
    pub status_error: Style,
    pub status_info: Style,
    pub key: Style,
    pub key_desc: Style,
    pub popup_border: Style,
    pub popup_title: Style,
    pub device_alias: Style,
    pub device_ip: Style,
}

/// Default dark theme.
pub static THEME: Theme = Theme {
    root: Style::new().bg(Color::Indexed(234)),
    title: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    selected: Style::new()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD),
    normal: Style::new().fg(Color::White),
    status_bar: Style::new().fg(Color::White).bg(Color::Indexed(236)),
    status_success: Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
    status_error: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
    status_info: Style::new().fg(Color::Yellow),
    key: Style::new()
        .fg(Color::Black)
        .bg(Color::Indexed(240))
        .add_modifier(Modifier::BOLD),
    key_desc: Style::new().fg(Color::White).bg(Color::Indexed(236)),
    popup_border: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    popup_title: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    device_alias: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    device_ip: Style::new().fg(Color::Gray),
};
