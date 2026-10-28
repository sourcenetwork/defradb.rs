//! HTTPS configuration.

use std::io;
use std::path::Path;
use std::sync::Arc;

use net::tls::{ServerConfig, TlsAcceptor};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};

/// A validated TLS certificate chain and matching private key.
pub struct TlsConfig(Arc<ServerConfig>);

impl TlsConfig {
    /// Load PEM files before starting the server. Invalid or mismatched keys
    /// are rejected rather than falling back to plaintext.
    pub async fn from_pem_file(cert: impl AsRef<Path>, key: impl AsRef<Path>) -> io::Result<Self> {
        let cert = cert.as_ref().to_path_buf();
        let key = key.as_ref().to_path_buf();
        nuclei::spawn_blocking(move || {
            let certificates = CertificateDer::pem_file_iter(&cert)
                .and_then(Iterator::collect::<std::result::Result<Vec<_>, _>>)
                .map_err(|e| io::Error::other(format!("certificate {}: {e}", cert.display())))?;
            let key = PrivateKeyDer::from_pem_file(&key)
                .map_err(|e| io::Error::other(format!("private key {}: {e}", key.display())))?;
            let mut config = ServerConfig::builder_with_provider(net::tls::provider())
                .with_safe_default_protocol_versions()
                .map_err(io::Error::other)?
                .with_no_client_auth()
                .with_single_cert(certificates, key)
                .map_err(io::Error::other)?;
            config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
            Ok(Self(Arc::new(config)))
        })
        .join()
        .await
        .map_err(|error| io::Error::other(error.to_string()))?
    }

    pub(crate) fn acceptor(self) -> TlsAcceptor {
        TlsAcceptor::from(self.0)
    }
}
