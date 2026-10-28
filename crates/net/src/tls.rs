//! TLS over nuclei streams, with rustls on the ring provider.
//!
//! The provider is chosen explicitly everywhere: another part of the binary
//! may enable a different one in the same process.

use std::sync::Arc;

use regolith::sync::Lazy;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, RootCertStore, SupportedProtocolVersion};

pub use futures_rustls::{TlsAcceptor, TlsConnector};
pub use rustls::ServerConfig;

/// Why a TLS configuration could not be built.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum TlsError {
    #[error("invalid certificate: {0}")]
    Certificate(String),
    #[error("no TLS version satisfies the minimum and maximum asked for")]
    NoVersion,
    #[error("invalid TLS configuration: {0}")]
    Config(String),
}

/// A TLS protocol version a client can be held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Version {
    Tls12,
    Tls13,
}

/// A root certificate a client trusts besides the system's.
#[derive(Debug, Clone)]
pub struct Certificate(CertificateDer<'static>);

impl Certificate {
    pub fn from_der(der: &[u8]) -> Result<Self, TlsError> {
        Ok(Self(CertificateDer::from(der.to_vec())))
    }

    pub fn from_pem(pem: &[u8]) -> Result<Self, TlsError> {
        CertificateDer::from_pem_slice(pem)
            .map(Self)
            .map_err(|error| TlsError::Certificate(error.to_string()))
    }
}

/// The provider every configuration here uses.
pub fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// What a client's TLS configuration is built from.
#[derive(Debug, Clone)]
pub(crate) struct ClientTls {
    pub(crate) system_roots: bool,
    pub(crate) roots: Vec<Certificate>,
    pub(crate) min: Option<Version>,
    pub(crate) max: Option<Version>,
    pub(crate) alpn: Vec<Vec<u8>>,
}

impl Default for ClientTls {
    fn default() -> Self {
        Self {
            system_roots: true,
            roots: Vec::new(),
            min: None,
            max: None,
            alpn: vec![b"h2".to_vec(), b"http/1.1".to_vec()],
        }
    }
}

/// The system's root certificates, read once per process.
static SYSTEM_ROOTS: Lazy<RootCertStore> = Lazy::new(system_roots);

fn system_roots() -> RootCertStore {
    let found = rustls_native_certs::load_native_certs();
    if let Some(error) = found.errors.first() {
        tracing::warn!(
            unreadable = found.errors.len(),
            %error,
            "some system root certificates could not be read"
        );
    }
    let mut roots = RootCertStore::empty();
    let (added, ignored) = roots.add_parsable_certificates(found.certs);
    tracing::debug!(added, ignored, "system root certificates loaded");
    roots
}

impl ClientTls {
    pub(crate) fn config(&self) -> Result<Arc<ClientConfig>, TlsError> {
        let mut roots = if self.system_roots {
            (*SYSTEM_ROOTS).clone()
        } else {
            RootCertStore::empty()
        };
        for root in &self.roots {
            roots
                .add(root.0.clone())
                .map_err(|error| TlsError::Certificate(error.to_string()))?;
        }
        let versions = self.versions();
        if versions.is_empty() {
            return Err(TlsError::NoVersion);
        }
        let mut config = ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&versions)
            .map_err(|error| TlsError::Config(error.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = self.alpn.clone();
        Ok(Arc::new(config))
    }

    fn versions(&self) -> Vec<&'static SupportedProtocolVersion> {
        [
            (Version::Tls12, &rustls::version::TLS12),
            (Version::Tls13, &rustls::version::TLS13),
        ]
        .into_iter()
        .filter(|(version, _)| self.min.is_none_or(|min| *version >= min))
        .filter(|(version, _)| self.max.is_none_or(|max| *version <= max))
        .map(|(_, supported)| supported)
        .collect()
    }
}
