#![allow(clippy::module_inception)]

pub mod cli;
pub mod commands;
pub mod ui;

pub use cli::{Cli, Commands};
pub use commands::discover::DiscoverCommand;
pub use commands::discover::execute as run_discover;
pub use commands::receive::ReceiveCommand;
pub use commands::receive::execute as run_receive;
pub use commands::send::SendCommand;
pub use commands::send::execute as run_send;
#[cfg(feature = "tui")]
pub use commands::tui::TuiCommand;
#[cfg(feature = "tui")]
pub use commands::tui::execute as run_tui;
