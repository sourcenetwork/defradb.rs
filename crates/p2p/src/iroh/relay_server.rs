//! An iroh relay server hosted in the same process as the endpoint.
//!
//! Peers that cannot reach each other directly exchange encrypted QUIC
//! datagrams through a relay they are both connected to. Running one next to
//! a publicly reachable node removes the dependency on n0's public relays.

use rapidhash::RapidHashSet;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;

use iroh::EndpointId;
use iroh_relay::server::{
    reloading_resolver, Access, AccessControl, CertConfig, ClientRateLimit, ClientRequest,
    QuicConfig, RelayConfig, Server, ServerConfig, TlsConfig, DEFAULT_CERT_RELOAD_INTERVAL,
};

use crate::error::{Error, Result};

/// Configuration for [`IrohRelayServer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrohRelayServerConfig {
    /// Plain HTTP listener. Without `tls` it serves the relay itself;
    /// with `tls` it serves only the captive-portal probe.
    pub http_bind_addr: SocketAddr,
    /// HTTPS listener and certificate. `None` serves the relay over plain
    /// HTTP, for deployments that terminate TLS at a reverse proxy.
    pub tls: Option<IrohRelayTlsConfig>,
    /// QUIC address discovery listener. Requires `tls`.
    pub quic_bind_addr: Option<SocketAddr>,
    /// Endpoint ids admitted to the relay. Empty admits everyone.
    pub allowed_endpoints: Vec<String>,
    /// Per-client inbound byte rate. `None` is unlimited.
    pub client_rx_bytes_per_second: Option<NonZeroU32>,
    /// Per-client inbound burst. Only meaningful with a byte rate.
    pub client_rx_max_burst_bytes: Option<NonZeroU32>,
}

/// TLS settings for [`IrohRelayServerConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrohRelayTlsConfig {
    pub https_bind_addr: SocketAddr,
    /// PEM certificate chain, re-read daily so external rotation takes effect.
    pub cert_path: PathBuf,
    /// PEM private key, re-read alongside the certificate.
    pub key_path: PathBuf,
}

/// A running relay server. Dropping it stops the server.
#[derive(Debug)]
pub struct IrohRelayServer {
    server: Server,
}

impl IrohRelayServer {
    /// Bind every configured listener and start serving.
    pub async fn spawn(config: IrohRelayServerConfig) -> Result<Self> {
        let server_config = build_server_config(config).await?;
        let server = Server::spawn(server_config)
            .await
            .map_err(|e| Error::Transport(format!("failed to start iroh relay server: {e}")))?;
        tracing::info!(
            http = ?server.http_addr(),
            https = ?server.https_addr(),
            quic = ?server.quic_addr(),
            "iroh relay server listening"
        );
        Ok(Self { server })
    }

    /// Address serving the relay over plain HTTP, when TLS is off, or the
    /// captive-portal probe, when it is on.
    pub fn http_addr(&self) -> Option<SocketAddr> {
        self.server.http_addr()
    }

    /// Address serving the relay over HTTPS, when TLS is on.
    pub fn https_addr(&self) -> Option<SocketAddr> {
        self.server.https_addr()
    }

    /// Address serving QUIC address discovery, when enabled.
    pub fn quic_addr(&self) -> Option<SocketAddr> {
        self.server.quic_addr()
    }

    /// Stop accepting connections and wait for the server tasks to finish.
    pub async fn shutdown(self) -> Result<()> {
        self.server
            .shutdown()
            .await
            .map_err(|e| Error::Transport(format!("iroh relay server shutdown failed: {e}")))
    }
}

async fn build_server_config(config: IrohRelayServerConfig) -> Result<ServerConfig> {
    if config.quic_bind_addr.is_some() && config.tls.is_none() {
        return Err(Error::Transport(
            "iroh relay QUIC address discovery requires TLS".to_string(),
        ));
    }
    if config.client_rx_max_burst_bytes.is_some() && config.client_rx_bytes_per_second.is_none() {
        return Err(Error::Transport(
            "iroh relay client_rx_max_burst_bytes requires client_rx_bytes_per_second".to_string(),
        ));
    }

    let mut relay = RelayConfig::new(config.http_bind_addr);
    if let Some(tls) = &config.tls {
        relay.tls = Some(TlsConfig::new(tls.https_bind_addr, load_cert(tls).await?));
    }
    if let Some(bytes_per_second) = config.client_rx_bytes_per_second {
        let mut limit = ClientRateLimit::new(bytes_per_second);
        limit.max_burst_bytes = config.client_rx_max_burst_bytes;
        relay.limits.client_rx = Some(limit);
    }
    if !config.allowed_endpoints.is_empty() {
        relay.access = Arc::new(EndpointAllowlist::parse(&config.allowed_endpoints)?);
    }

    let mut server = ServerConfig::default();
    server.relay = Some(relay);
    server.quic = config.quic_bind_addr.map(QuicConfig::new);
    Ok(server)
}

async fn load_cert(tls: &IrohRelayTlsConfig) -> Result<CertConfig> {
    let builder = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Error::Transport(format!("iroh relay TLS setup failed: {e}")))?
    .with_no_client_auth();
    let resolver = reloading_resolver(
        builder.crypto_provider(),
        tls.cert_path.clone(),
        tls.key_path.clone(),
        DEFAULT_CERT_RELOAD_INTERVAL,
    )
    .await
    .map_err(|e| {
        Error::Transport(format!(
            "failed to load iroh relay certificate {} / key {}: {e}",
            tls.cert_path.display(),
            tls.key_path.display()
        ))
    })?;
    Ok(CertConfig::Manual {
        server_config: builder.with_cert_resolver(resolver),
    })
}

#[derive(Debug)]
struct EndpointAllowlist {
    ids: RapidHashSet<EndpointId>,
}

impl EndpointAllowlist {
    fn parse(ids: &[String]) -> Result<Self> {
        let ids = ids
            .iter()
            .map(|id| {
                id.parse::<EndpointId>().map_err(|e| {
                    Error::Transport(format!("invalid iroh relay allowed endpoint '{id}': {e}"))
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { ids })
    }

    fn admits(&self, id: &EndpointId) -> bool {
        self.ids.contains(id)
    }
}

impl AccessControl for EndpointAllowlist {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        if self.admits(&request.endpoint_id()) {
            Access::Allow
        } else {
            Access::Deny {
                reason: Some("endpoint not allowed on this relay".to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::SecretKey;

    fn plain_config() -> IrohRelayServerConfig {
        IrohRelayServerConfig {
            http_bind_addr: "127.0.0.1:0".parse().unwrap(),
            tls: None,
            quic_bind_addr: None,
            allowed_endpoints: Vec::new(),
            client_rx_bytes_per_second: None,
            client_rx_max_burst_bytes: None,
        }
    }

    #[test]
    fn allowlist_admits_only_listed_endpoints() {
        let listed = SecretKey::generate().public();
        let other = SecretKey::generate().public();
        let allowlist = EndpointAllowlist::parse(&[listed.to_string()]).unwrap();

        assert!(allowlist.admits(&listed));
        assert!(!allowlist.admits(&other));
    }

    #[test]
    fn allowlist_rejects_malformed_ids() {
        assert!(EndpointAllowlist::parse(&["not-an-endpoint".to_string()]).is_err());
    }

    #[tokio::test]
    async fn quic_address_discovery_requires_tls() {
        let mut config = plain_config();
        config.quic_bind_addr = Some("127.0.0.1:0".parse().unwrap());

        assert!(build_server_config(config).await.is_err());
    }

    #[tokio::test]
    async fn burst_without_rate_is_rejected() {
        let mut config = plain_config();
        config.client_rx_max_burst_bytes = NonZeroU32::new(1024);

        assert!(build_server_config(config).await.is_err());
    }

    #[tokio::test]
    async fn missing_certificate_fails_at_spawn() {
        let mut config = plain_config();
        config.tls = Some(IrohRelayTlsConfig {
            https_bind_addr: "127.0.0.1:0".parse().unwrap(),
            cert_path: "/nonexistent/cert.pem".into(),
            key_path: "/nonexistent/key.pem".into(),
        });

        assert!(IrohRelayServer::spawn(config).await.is_err());
    }
}
