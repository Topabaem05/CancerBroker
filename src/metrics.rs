use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const DEFAULT_METRICS_PORT: u16 = 9_464;

fn default_metrics_bind() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_METRICS_PORT)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsConfig {
    pub bind: SocketAddr,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            bind: default_metrics_bind(),
        }
    }
}

pub fn metrics_bind_is_localhost(config: &MetricsConfig) -> bool {
    config.bind.ip().is_loopback()
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::{
        DEFAULT_METRICS_PORT, MetricsConfig, default_metrics_bind, metrics_bind_is_localhost,
    };

    #[test]
    fn default_metrics_bind_uses_localhost_and_default_port() {
        let bind = default_metrics_bind();

        assert_eq!(
            bind,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_METRICS_PORT)
        );
    }

    #[test]
    fn metrics_config_default_uses_loopback_bind() {
        let config = MetricsConfig::default();

        assert_eq!(config.bind, default_metrics_bind());
        assert!(metrics_bind_is_localhost(&config));
    }

    #[test]
    fn metrics_bind_is_localhost_rejects_non_loopback_addresses() {
        let config = MetricsConfig {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), DEFAULT_METRICS_PORT),
        };

        assert!(!metrics_bind_is_localhost(&config));
    }
}
