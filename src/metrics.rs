use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsConfig {
    pub bind: SocketAddr,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9_464),
        }
    }
}

pub fn metrics_bind_is_localhost(config: &MetricsConfig) -> bool {
    config.bind.ip().is_loopback()
}
