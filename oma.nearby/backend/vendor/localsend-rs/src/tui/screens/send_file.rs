//! Send file screen with device selection.

use crate::protocol::DeviceInfo;
use crate::tui::theme::THEME;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    prelude::Widget,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Row, Table, TableState},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tui_input::Input;

/// Stage in send file flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFileStage {
    SelectDevice,
    EnterFilePath,
}

/// Send file screen state.
pub struct SendFileScreen {
    pub stage: SendFileStage,
    pub devices: Arc<RwLock<Vec<DeviceInfo>>>,
    pub table_state: TableState,
    pub selected_device: Option<DeviceInfo>,
    pub input: Input,
    pub is_sending: bool,
    pub progress: f64,
    pub current_file: Option<String>,
    pub needs_refresh: bool,
    /// Set by the app while discovery is still warming up, to show "Scanning…"
    /// instead of "No devices found".
    pub scanning: bool,
}

impl SendFileScreen {
    pub fn new(devices: Arc<RwLock<Vec<DeviceInfo>>>) -> Self {
        Self {
            stage: SendFileStage::SelectDevice,
            devices,
            table_state: TableState::default(),
            selected_device: None,
            input: Input::default(),
            is_sending: false,
            progress: 0.0,
            current_file: None,
            needs_refresh: false,
            scanning: false,
        }
    }

    pub fn clear(&mut self) {
        self.stage = SendFileStage::SelectDevice;
        self.selected_device = None;
        self.input.reset();
        self.is_sending = false;
        self.progress = 0.0;
        self.current_file = None;
        self.table_state = TableState::default();
    }

    pub fn file_path(&self) -> &str {
        self.input.value()
    }

    #[allow(dead_code)]
    pub fn set_progress(&mut self, file: &str, progress: f64) {
        self.current_file = Some(file.to_string());
        self.progress = progress;
    }

    pub fn next_device(&mut self) {
        let Ok(devices) = self.devices.try_read() else {
            return;
        };
        if devices.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => (i + 1) % devices.len(),
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous_device(&mut self) {
        let Ok(devices) = self.devices.try_read() else {
            return;
        };
        if devices.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    devices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn select_current_device(&mut self) {
        let Ok(devices) = self.devices.try_read() else {
            return;
        };
        if let Some(i) = self.table_state.selected()
            && let Some(device) = devices.get(i)
        {
            self.selected_device = Some(device.clone());
            self.stage = SendFileStage::EnterFilePath;
        }
    }

    pub fn request_refresh(&mut self) {
        self.needs_refresh = true;
    }

    pub fn consume_refresh(&mut self) -> bool {
        let result = self.needs_refresh;
        self.needs_refresh = false;
        result
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        match self.stage {
            SendFileStage::SelectDevice => self.render_device_selection(area, buf),
            SendFileStage::EnterFilePath => self.render_file_input(area, buf),
        }
    }

    fn render_device_selection(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" 📁 Send File - Select Device ")
            .title_style(THEME.title)
            .borders(Borders::ALL);

        let inner = block.inner(area);
        block.render(area, buf);

        let layout = Layout::vertical([
            Constraint::Min(0),    // Device table
            Constraint::Length(2), // Help text
        ])
        .split(inner);

        // If the discovery writer holds the lock this frame, show a brief
        // placeholder rather than crash; the next frame will render the table.
        let Ok(devices) = self.devices.try_read() else {
            Paragraph::new("…")
                .style(THEME.status_info)
                .centered()
                .render(layout[0], buf);
            return;
        };

        if devices.is_empty() {
            let text = if self.scanning {
                "🔍 Scanning for devices…"
            } else {
                "No devices found. Press R to refresh."
            };
            let msg = Paragraph::new(text).style(THEME.status_info).centered();
            msg.render(layout[0], buf);
        } else {
            // Ensure selection
            if self.table_state.selected().is_none() && !devices.is_empty() {
                self.table_state.select(Some(0));
            }

            let rows: Vec<Row> = devices
                .iter()
                .map(|d| {
                    Row::new(vec![
                        d.alias.clone(),
                        d.ip.clone().unwrap_or_else(|| "Unknown".into()),
                        d.port.to_string(),
                        d.device_model.clone().unwrap_or_default(),
                    ])
                })
                .collect();

            let widths = [
                Constraint::Percentage(30),
                Constraint::Percentage(25),
                Constraint::Percentage(15),
                Constraint::Percentage(30),
            ];

            let table = Table::new(rows, widths)
                .header(
                    Row::new(vec!["Name", "IP", "Port", "Model"])
                        .style(THEME.title)
                        .bottom_margin(1),
                )
                .row_highlight_style(THEME.selected)
                .highlight_symbol("▶ ");

            ratatui::widgets::StatefulWidget::render(table, layout[0], buf, &mut self.table_state);
        }

        // Help text
        let help = Line::from(vec![
            Span::styled(" ↑/k ", THEME.key),
            Span::styled(" Up ", THEME.key_desc),
            Span::styled(" ↓/j ", THEME.key),
            Span::styled(" Down ", THEME.key_desc),
            Span::styled(" Enter ", THEME.key),
            Span::styled(" Select ", THEME.key_desc),
            Span::styled(" R ", THEME.key),
            Span::styled(" Refresh ", THEME.key_desc),
        ]);
        Paragraph::new(help).centered().render(layout[1], buf);
    }

    fn render_file_input(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" 📁 Send File - Enter File Path ")
            .title_style(THEME.title)
            .borders(Borders::ALL);

        let inner = block.inner(area);
        block.render(area, buf);

        let layout = Layout::vertical([
            Constraint::Length(3), // Target info
            Constraint::Length(3), // Input/Progress
            Constraint::Min(0),    // Spacer
            Constraint::Length(2), // Help
        ])
        .split(inner);

        // Target info
        let target_text = if let Some(ref device) = self.selected_device {
            Line::from(vec![
                Span::raw("Target: "),
                Span::styled(&device.alias, THEME.device_alias),
                Span::raw(" ("),
                Span::styled(device.ip.as_deref().unwrap_or("Unknown"), THEME.device_ip),
                Span::raw(")"),
            ])
        } else {
            Line::styled("No device selected", THEME.status_error)
        };
        Paragraph::new(target_text).render(layout[0], buf);

        // Input or progress
        if self.is_sending {
            let label = format!(
                "{}: {:.0}%",
                self.current_file.as_deref().unwrap_or("Uploading"),
                self.progress * 100.0
            );
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL))
                .gauge_style(THEME.status_success)
                .ratio(self.progress)
                .label(label);
            gauge.render(layout[1], buf);
        } else {
            let input_block = Block::default().title(" File Path ").borders(Borders::ALL);
            let input_inner = input_block.inner(layout[1]);
            input_block.render(layout[1], buf);
            Paragraph::new(self.input.value()).render(input_inner, buf);
        }

        // Help text
        let help = if self.is_sending {
            Line::from(vec![
                Span::styled(" Esc ", THEME.key),
                Span::styled(" Cancel ", THEME.key_desc),
            ])
        } else if self.selected_device.is_some() {
            Line::from(vec![
                Span::styled(" Enter ", THEME.key),
                Span::styled(" Send ", THEME.key_desc),
                Span::styled(" Esc ", THEME.key),
                Span::styled(" Back ", THEME.key_desc),
            ])
        } else {
            Line::from(vec![
                Span::styled(" Esc ", THEME.key),
                Span::styled(" Back ", THEME.key_desc),
            ])
        };
        Paragraph::new(help).centered().render(layout[3], buf);
    }
}
