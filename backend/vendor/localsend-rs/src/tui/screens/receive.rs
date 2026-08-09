//! Receive screen showing received files, with selection + reveal.

use crate::tui::theme::THEME;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, StatefulWidget, Table, TableState, Widget, Wrap},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::protocol::ReceivedFile;

/// Receive screen state.
pub struct ReceiveScreen {
    pub received_files: Arc<RwLock<Vec<ReceivedFile>>>,
    pub port: u16,
    pub save_dir: PathBuf,
    /// Used only to drive the table highlight during a frame; the persistent
    /// selection lives in `selected_key`.
    pub table_state: TableState,
    /// The selected item's on-disk path (its stable identity). Selecting by
    /// path — not by row index — keeps the highlight on the same item when a
    /// new arrival prepends to the most-recent-first list.
    selected_key: Option<PathBuf>,
}

impl ReceiveScreen {
    pub fn new(
        received_files: Arc<RwLock<Vec<ReceivedFile>>>,
        port: u16,
        save_dir: PathBuf,
    ) -> Self {
        Self {
            received_files,
            port,
            save_dir,
            table_state: TableState::default(),
            selected_key: None,
        }
    }

    /// Snapshot of received files in display order (most-recent first).
    /// Empty if the writer holds the lock this instant.
    fn snapshot(&self) -> Vec<ReceivedFile> {
        match self.received_files.try_read() {
            Ok(files) => files.iter().rev().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Index of the current selection within `files`, if it still exists.
    fn current_index(&self, files: &[ReceivedFile]) -> Option<usize> {
        let key = self.selected_key.as_ref()?;
        files.iter().position(|f| &f.path == key)
    }

    pub fn next(&mut self) {
        let files = self.snapshot();
        if files.is_empty() {
            return;
        }
        let i = match self.current_index(&files) {
            Some(i) => (i + 1) % files.len(),
            None => 0,
        };
        self.selected_key = Some(files[i].path.clone());
    }

    pub fn previous(&mut self) {
        let files = self.snapshot();
        if files.is_empty() {
            return;
        }
        let i = match self.current_index(&files) {
            Some(0) => files.len() - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.selected_key = Some(files[i].path.clone());
    }

    /// The on-disk path of the currently selected received item, if any.
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_key.clone()
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" 📥 Received Files ")
            .title_style(THEME.title)
            .borders(Borders::ALL);

        let inner = block.inner(area);
        block.render(area, buf);

        let files = self.snapshot();

        // Resolve the persistent selection to a row index for this frame.
        let selected_index = self.current_index(&files);
        self.table_state.select(selected_index);

        // Reserve a preview pane only when the selected item is a text message.
        let selected_message = selected_index
            .and_then(|i| files.get(i))
            .and_then(|f| f.message_text.clone());
        let preview_height: u16 = if selected_message.is_some() { 4 } else { 0 };

        let layout = Layout::vertical([
            Constraint::Length(3),              // Status (2 lines + margin)
            Constraint::Min(0),                 // File list
            Constraint::Length(preview_height), // Message preview (optional)
            Constraint::Length(2),              // Help
        ])
        .split(inner);

        // Status line + where files are saved.
        let status = vec![
            Line::from(vec![
                Span::raw("Status: "),
                Span::styled("🟢 Listening", THEME.status_success),
                Span::raw(format!(" on port {}", self.port)),
            ]),
            Line::from(vec![
                Span::raw("Saving to: "),
                Span::styled(self.save_dir.display().to_string(), THEME.status_info),
            ]),
        ];
        Paragraph::new(status).render(layout[0], buf);

        if files.is_empty() {
            let msg = Paragraph::new("No files received yet.")
                .style(THEME.normal)
                .centered();
            msg.render(layout[1], buf);
        } else {
            let rows: Vec<Row> = files
                .iter()
                .map(|f| {
                    // A text message shows its body inline (💬); a file shows the
                    // actual saved name (which may differ after a collision
                    // rename).
                    let label = if let Some(text) = &f.message_text {
                        format!("💬 {}", one_line(text, 48))
                    } else {
                        f.path
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| f.file_name.clone())
                    };
                    Row::new(vec![
                        label,
                        format_size(f.size),
                        f.sender.clone(),
                        f.time.clone(),
                    ])
                })
                .collect();

            let widths = [
                Constraint::Percentage(40),
                Constraint::Percentage(15),
                Constraint::Percentage(25),
                Constraint::Percentage(20),
            ];

            let table = Table::new(rows, widths)
                .header(
                    Row::new(vec!["File / Message", "Size", "From", "Time"])
                        .style(THEME.title)
                        .bottom_margin(1),
                )
                .row_highlight_style(THEME.selected)
                .highlight_symbol("▶ ");

            StatefulWidget::render(table, layout[1], buf, &mut self.table_state);
        }

        // Message preview pane for the selected text message.
        if let Some(text) = selected_message {
            let preview = Paragraph::new(text)
                .block(
                    Block::default()
                        .title(" 💬 Message ")
                        .title_style(THEME.title)
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: false });
            preview.render(layout[2], buf);
        }

        // Help — navigation + reveal; Tab/q handled globally.
        let help = Line::from(vec![
            Span::styled(" ↑/↓ ", THEME.key),
            Span::styled(" Select ", THEME.key_desc),
            Span::styled(" Enter ", THEME.key),
            Span::styled(" Reveal in file manager ", THEME.key_desc),
            Span::styled(" q ", THEME.key),
            Span::styled(" Quit ", THEME.key_desc),
        ]);
        Paragraph::new(help).centered().render(layout[3], buf);
    }
}

/// Collapse newlines/whitespace and truncate for a single-line table cell.
fn one_line(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        let mut s: String = flat.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        flat
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
