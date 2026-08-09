pub mod discover;
pub mod receive;
pub mod send;
#[cfg(feature = "tui")]
pub mod tui;

pub use discover::DiscoverCommand;
pub use receive::ReceiveCommand;
pub use send::SendCommand;
#[cfg(feature = "tui")]
pub use tui::TuiCommand;

pub use discover::execute as run_discover;
pub use receive::execute as run_receive;
pub use send::execute as run_send;
#[cfg(feature = "tui")]
pub use tui::execute as run_tui;
