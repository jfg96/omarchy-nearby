#![allow(clippy::module_inception)]

pub mod client;
pub mod trust_policy;

pub use client::LocalSendClient;
pub use trust_policy::TlsTrustPolicy;
