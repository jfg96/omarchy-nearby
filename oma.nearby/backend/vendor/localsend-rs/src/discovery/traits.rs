use crate::error::LocalSendError;
use crate::protocol::DeviceInfo;

#[async_trait::async_trait]
pub trait Discovery {
    async fn start(&mut self) -> std::result::Result<(), LocalSendError>;
    fn stop(&mut self);
    async fn announce_presence(&self) -> std::result::Result<(), LocalSendError>;
    fn on_discovered<F>(&mut self, callback: F)
    where
        F: Fn(DeviceInfo) + Send + Sync + 'static;
    fn get_known_devices(&self) -> Vec<DeviceInfo>;
}
