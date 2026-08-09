#![allow(clippy::module_inception)]

pub mod events;
pub mod server;
pub mod web_share;

pub(crate) mod handlers;
pub(crate) mod pin;
pub(crate) mod routes;
pub(crate) mod state;

pub use events::{PendingRequest, PendingWebShareRequest, ServerEvent, TransferDecision};
pub use server::{LocalSendServer, LocalSendServerBuilder};
pub use web_share::{WebShareFile, WebShareSource};
