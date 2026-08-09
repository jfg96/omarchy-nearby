//! TUI module for LocalSend Rust client.
//!
//! Provides an interactive terminal user interface using ratatui.

mod app;
mod popup;
mod screens;
mod theme;

pub use app::run_tui;
