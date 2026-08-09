#[cfg(feature = "tui")]
use crate::cli::commands::TuiCommand;
use crate::cli::commands::{DiscoverCommand, ReceiveCommand, SendCommand};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(about = "LocalSend client and CLI for Rust", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Discover(DiscoverCommand),
    Receive(ReceiveCommand),
    Send(SendCommand),
    #[cfg(feature = "tui")]
    Tui(TuiCommand),
}
