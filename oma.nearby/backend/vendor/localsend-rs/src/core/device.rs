use crate::error::{LocalSendError, Result};

pub fn get_local_ip() -> Result<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    let local_addr = socket.local_addr()?.ip();
    match local_addr {
        std::net::IpAddr::V4(addr) => Ok(addr),
        _ => Err(LocalSendError::network("Local IP is not IPv4")),
    }
}

pub fn get_device_model() -> String {
    std::env::consts::OS.to_string()
}

pub fn get_device_type() -> crate::protocol::DeviceType {
    if std::env::var("HEADLESS").is_ok() {
        crate::protocol::DeviceType::Headless
    } else if std::env::var("SERVER").is_ok() {
        crate::protocol::DeviceType::Server
    } else if cfg!(target_os = "android") || cfg!(target_os = "ios") {
        crate::protocol::DeviceType::Mobile
    } else if cfg!(target_arch = "wasm32") || cfg!(target_arch = "wasm64") {
        crate::protocol::DeviceType::Web
    } else {
        crate::protocol::DeviceType::Desktop
    }
}
