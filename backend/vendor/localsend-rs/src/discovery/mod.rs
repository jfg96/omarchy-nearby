pub mod http;
pub mod multicast;
pub mod traits;

pub use http::HttpDiscovery;
pub use multicast::{MulticastConfig, MulticastDiscovery};
pub use traits::Discovery;
