//! Screen modules for TUI.

pub mod receive;
pub mod send_file;
pub mod send_text;
pub mod settings;

use strum::{Display, EnumIter, FromRepr};

/// Available screens in the TUI.
#[derive(Debug, Clone, Copy, Default, Display, EnumIter, FromRepr, PartialEq, Eq)]
pub enum Screen {
    #[default]
    SendText,

    SendFile,
    Receive,
    Settings,
}
