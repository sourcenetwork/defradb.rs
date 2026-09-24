//! `net.iroh_relay_server`: an iroh relay hosted by this node.

use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Relay server hosted next to the iroh endpoint. Needs a binary built with
/// the `iroh-relay-server` feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrohRelayServerConfig {
    /// Plain HTTP listener: the relay itself without `tls`, only the
    /// captive-portal probe with it.
    pub http_bind_addr: SocketAddr,
    /// URL peers use to reach this relay, e.g. `https://relay.example.com`.
    /// When set, this node joins its own relay ahead of any `iroh_relay_urls`,
    /// so peers dialing through it can reach the node too. An explicit
    /// `iroh_relay_mode` of `default` or `disabled` still takes precedence.
    #[serde(default)]
    pub public_url: Option<String>,
    #[serde(default)]
    pub tls: Option<IrohRelayServerTlsConfig>,
    /// QUIC address discovery listener. Requires `tls`.
    #[serde(default)]
    pub quic_bind_addr: Option<SocketAddr>,
    /// Endpoint ids admitted to the relay. Empty admits everyone.
    #[serde(default)]
    pub allowed_endpoints: Vec<String>,
    #[serde(default)]
    pub client_rx_bytes_per_second: Option<NonZeroU32>,
    #[serde(default)]
    pub client_rx_max_burst_bytes: Option<NonZeroU32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrohRelayServerTlsConfig {
    pub https_bind_addr: SocketAddr,
    /// PEM certificate chain, re-read daily. Relative to rootdir.
    pub cert_path: PathBuf,
    /// PEM private key, re-read daily. Relative to rootdir.
    pub key_path: PathBuf,
}

impl IrohRelayServerConfig {
    pub fn validate(&self) -> Result<()> {
        if self.quic_bind_addr.is_some() && self.tls.is_none() {
            return Err(Error::InvalidConfig(
                "net.iroh_relay_server.quic_bind_addr requires net.iroh_relay_server.tls".into(),
            ));
        }
        if self.client_rx_max_burst_bytes.is_some() && self.client_rx_bytes_per_second.is_none() {
            return Err(Error::InvalidConfig(
                "net.iroh_relay_server.client_rx_max_burst_bytes requires client_rx_bytes_per_second"
                    .into(),
            ));
        }
        if let Some(public_url) = &self.public_url {
            let parsed = url::Url::parse(public_url).map_err(|error| {
                Error::InvalidConfig(format!(
                    "net.iroh_relay_server.public_url {public_url:?} is not a URL: {error}"
                ))
            })?;
            // With tls, the plain HTTP listener only answers captive-portal
            // probes, so an http URL would hand peers something that is not a relay.
            if self.tls.is_some() && parsed.scheme() != "https" {
                return Err(Error::InvalidConfig(
                    "net.iroh_relay_server.public_url must be https when tls is set".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn resolve_paths(&mut self, rootdir: &Path) {
        if let Some(tls) = &mut self.tls {
            if tls.cert_path.is_relative() {
                tls.cert_path = rootdir.join(&tls.cert_path);
            }
            if tls.key_path.is_relative() {
                tls.key_path = rootdir.join(&tls.key_path);
            }
        }
    }

    #[cfg(feature = "iroh-relay-server")]
    pub fn to_p2p(&self) -> p2p::iroh::IrohRelayServerConfig {
        p2p::iroh::IrohRelayServerConfig {
            http_bind_addr: self.http_bind_addr,
            tls: self.tls.as_ref().map(|tls| p2p::iroh::IrohRelayTlsConfig {
                https_bind_addr: tls.https_bind_addr,
                cert_path: tls.cert_path.clone(),
                key_path: tls.key_path.clone(),
            }),
            quic_bind_addr: self.quic_bind_addr,
            allowed_endpoints: self.allowed_endpoints.clone(),
            client_rx_bytes_per_second: self.client_rx_bytes_per_second,
            client_rx_max_burst_bytes: self.client_rx_max_burst_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "http_bind_addr: 0.0.0.0:80\n";

    #[test]
    fn minimal_section_parses_with_everything_else_off() {
        let config: IrohRelayServerConfig = serde_yaml::from_str(MINIMAL).unwrap();

        assert_eq!(config.http_bind_addr, "0.0.0.0:80".parse().unwrap());
        assert!(config.tls.is_none());
        assert!(config.allowed_endpoints.is_empty());
        config.validate().unwrap();
    }

    #[test]
    fn quic_without_tls_is_rejected() {
        let config: IrohRelayServerConfig =
            serde_yaml::from_str(&format!("{MINIMAL}quic_bind_addr: 0.0.0.0:7842\n")).unwrap();

        assert!(config.validate().is_err());
    }

    #[test]
    fn burst_without_rate_is_rejected() {
        let config: IrohRelayServerConfig =
            serde_yaml::from_str(&format!("{MINIMAL}client_rx_max_burst_bytes: 4096\n")).unwrap();

        assert!(config.validate().is_err());
    }

    const TLS: &str =
        "tls:\n  https_bind_addr: 0.0.0.0:443\n  cert_path: relay.pem\n  key_path: key.pem\n";

    #[test]
    fn http_public_url_with_tls_is_rejected() {
        let config: IrohRelayServerConfig = serde_yaml::from_str(&format!(
            "{MINIMAL}public_url: http://relay.example.com\n{TLS}"
        ))
        .unwrap();

        assert!(config.validate().is_err());
    }

    #[test]
    fn https_public_url_with_tls_is_accepted() {
        let config: IrohRelayServerConfig = serde_yaml::from_str(&format!(
            "{MINIMAL}public_url: https://relay.example.com\n{TLS}"
        ))
        .unwrap();

        config.validate().unwrap();
    }

    #[test]
    fn http_public_url_without_tls_is_accepted() {
        let config: IrohRelayServerConfig =
            serde_yaml::from_str(&format!("{MINIMAL}public_url: http://relay.example.com\n"))
                .unwrap();

        config.validate().unwrap();
    }

    #[test]
    fn unparsable_public_url_is_rejected() {
        let config: IrohRelayServerConfig =
            serde_yaml::from_str(&format!("{MINIMAL}public_url: relay.example.com\n")).unwrap();

        assert!(config.validate().is_err());
    }

    #[test]
    fn relative_cert_paths_resolve_against_rootdir() {
        let mut config: IrohRelayServerConfig = serde_yaml::from_str(&format!(
            "{MINIMAL}tls:\n  https_bind_addr: 0.0.0.0:443\n  cert_path: certs/relay.pem\n  key_path: /etc/relay/key.pem\n"
        ))
        .unwrap();
        config.resolve_paths(Path::new("/var/defra"));

        let tls = config.tls.unwrap();
        assert_eq!(tls.cert_path, PathBuf::from("/var/defra/certs/relay.pem"));
        assert_eq!(tls.key_path, PathBuf::from("/etc/relay/key.pem"));
    }
}
