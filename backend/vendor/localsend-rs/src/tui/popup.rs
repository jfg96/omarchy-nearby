//! Popup overlays for transfer confirmations and status messages.

use crate::protocol::{DeviceInfo, FileId};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::theme::THEME;

/// One offered file in the confirm dialog, with its per-file accept toggle.
pub struct ConfirmFile {
    pub id: FileId,
    pub name: String,
    pub size: u64,
    pub selected: bool,
}

/// Interactive state for the incoming-transfer confirmation dialog: an ordered,
/// individually selectable list of the offered files plus a cursor.
pub struct ConfirmState {
    pub request: crate::server::PendingRequest,
    pub files: Vec<ConfirmFile>,
    pub cursor: usize,
}

impl ConfirmState {
    pub fn move_up(&mut self) {
        if self.files.is_empty() {
            return;
        }
        self.cursor = if self.cursor == 0 {
            self.files.len() - 1
        } else {
            self.cursor - 1
        };
    }

    pub fn move_down(&mut self) {
        if self.files.is_empty() {
            return;
        }
        self.cursor = (self.cursor + 1) % self.files.len();
    }

    /// Flip the file under the cursor.
    pub fn toggle(&mut self) {
        if let Some(f) = self.files.get_mut(self.cursor) {
            f.selected = !f.selected;
        }
    }

    /// If everything is selected, clear all; otherwise select all.
    pub fn toggle_all(&mut self) {
        let all = self.files.iter().all(|f| f.selected);
        for f in &mut self.files {
            f.selected = !all;
        }
    }

    pub fn selected_ids(&self) -> Vec<FileId> {
        self.files
            .iter()
            .filter(|f| f.selected)
            .map(|f| f.id.clone())
            .collect()
    }
}

/// Types of popup overlays.
#[allow(dead_code)]
pub enum Popup {
    /// Confirmation dialog for incoming transfer request.
    TransferConfirm(ConfirmState),
    /// Progress indicator for active transfer.
    TransferProgress {
        file_name: String,
        received: u64,
        total: u64,
    },
    /// Simple message popup (success/error/info).
    Message { text: String, level: MessageLevel },
    /// Prompt the sender for a PIN after the receiver answered 401.
    PinEntry { input: tui_input::Input },
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MessageLevel {
    Success,
    Error,
    Info,
}

impl Popup {
    /// Build an interactive confirm dialog from an incoming request. Files are
    /// listed in a stable order (by name, then id) and all start selected, so
    /// hitting Enter without touching anything accepts the whole transfer.
    pub fn confirm(request: crate::server::PendingRequest) -> Self {
        let mut files: Vec<ConfirmFile> = request
            .files()
            .iter()
            .map(|(id, meta)| ConfirmFile {
                id: id.clone(),
                name: meta.file_name.clone(),
                size: meta.size,
                selected: true,
            })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.as_str().cmp(b.id.as_str())));
        Popup::TransferConfirm(ConfirmState {
            request,
            files,
            cursor: 0,
        })
    }

    /// Render the popup as an overlay.
    pub fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Calculate centered popup area (60% width, varying height)
        let popup_area = centered_rect(60, 40, area);

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        match self {
            Popup::TransferConfirm(state) => {
                self.render_transfer_confirm(frame, popup_area, state);
            }
            Popup::TransferProgress {
                file_name,
                received,
                total,
            } => {
                self.render_progress(frame, popup_area, file_name, *received, *total);
            }
            Popup::Message { text, level } => {
                self.render_message(frame, popup_area, text, *level);
            }
            Popup::PinEntry { input } => {
                self.render_pin_entry(frame, popup_area, input.value());
            }
        }
    }

    fn render_pin_entry(&self, frame: &mut ratatui::Frame, area: Rect, value: &str) {
        let block = Block::default()
            .title(" 🔒 PIN Required ")
            .title_style(THEME.popup_title)
            .borders(Borders::ALL)
            .border_style(THEME.popup_border);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines = vec![
            Line::raw("The receiver requires a PIN to accept this transfer."),
            Line::raw(""),
            Line::from(vec![
                Span::raw("PIN: "),
                Span::styled(value, THEME.device_alias),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled(" Enter ", THEME.key),
                Span::styled(" Submit ", THEME.key_desc),
                Span::styled(" Esc ", THEME.key),
                Span::styled(" Cancel ", THEME.key_desc),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
    }

    fn render_transfer_confirm(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        state: &ConfirmState,
    ) {
        let sender: &DeviceInfo = state.request.sender();

        let block = Block::default()
            .title(" 📥 Incoming Transfer ")
            .title_style(THEME.popup_title)
            .borders(Borders::ALL)
            .border_style(THEME.popup_border);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let selected_count = state.files.iter().filter(|f| f.selected).count();
        let selected_size: u64 = state
            .files
            .iter()
            .filter(|f| f.selected)
            .map(|f| f.size)
            .sum();

        let mut lines = vec![
            Line::from(vec![
                Span::raw("From: "),
                Span::styled(&sender.alias, THEME.device_alias),
            ]),
            Line::from(format!(
                "{} of {} file(s), {}",
                selected_count,
                state.files.len(),
                format_size(selected_size)
            )),
            Line::raw(""),
        ];

        // Show up to 8 files with their checkbox + cursor highlight. If more,
        // window around the cursor so the highlighted row is always visible.
        const MAX_ROWS: usize = 8;
        let start = state
            .cursor
            .saturating_sub(MAX_ROWS - 1)
            .min(state.files.len().saturating_sub(MAX_ROWS));
        for (i, f) in state.files.iter().enumerate().skip(start).take(MAX_ROWS) {
            let check = if f.selected { "[x]" } else { "[ ]" };
            let cursor = if i == state.cursor { "▶ " } else { "  " };
            let style = if i == state.cursor {
                THEME.selected
            } else {
                THEME.normal
            };
            lines.push(Line::styled(
                format!("{cursor}{check} {} ({})", f.name, format_size(f.size)),
                style,
            ));
        }
        if state.files.len() > MAX_ROWS {
            lines.push(Line::raw(format!(
                "  … {} file(s), use ↑/↓ to scroll",
                state.files.len()
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(" ↑/↓ ", THEME.key),
            Span::styled(" Move ", THEME.key_desc),
            Span::styled(" Space ", THEME.key),
            Span::styled(" Toggle ", THEME.key_desc),
            Span::styled(" a ", THEME.key),
            Span::styled(" All ", THEME.key_desc),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Enter ", THEME.key),
            Span::styled(" Accept selected ", THEME.key_desc),
            Span::styled(" N ", THEME.key),
            Span::styled(" Decline ", THEME.key_desc),
        ]));

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
    }

    fn render_progress(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        file_name: &str,
        received: u64,
        total: u64,
    ) {
        let block = Block::default()
            .title(" 📦 Receiving... ")
            .title_style(THEME.popup_title)
            .borders(Borders::ALL)
            .border_style(THEME.popup_border);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let percent = if total > 0 {
            (received as f64 / total as f64 * 100.0) as u16
        } else {
            0
        };

        let lines = vec![
            Line::raw(file_name),
            Line::raw(""),
            Line::raw(format!(
                "{} / {} ({percent}%)",
                format_size(received),
                format_size(total)
            )),
        ];

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn render_message(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        text: &str,
        level: MessageLevel,
    ) {
        let (title, style) = match level {
            MessageLevel::Success => (" ✓ Success ", THEME.status_success),
            MessageLevel::Error => (" ✗ Error ", THEME.status_error),
            MessageLevel::Info => (" ℹ Info ", THEME.status_info),
        };

        let block = Block::default()
            .title(title)
            .title_style(style)
            .borders(Borders::ALL)
            .border_style(style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines = vec![
            Line::raw(text),
            Line::raw(""),
            Line::from(vec![
                Span::styled(" Enter ", THEME.key),
                Span::styled(" OK ", THEME.key_desc),
            ]),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
    }
}

/// Calculate a centered rectangle with percentage-based sizing.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Format bytes to human-readable size.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
