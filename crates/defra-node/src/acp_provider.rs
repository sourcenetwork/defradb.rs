//! A document ACP provider the host builds and hands to the node.

use std::sync::Arc;
use std::time::Duration;

/// A host's [`vera::VeraProvider`] for document ACP, compared by identity.
///
/// The node wires it as it wires its configured vera.rs backend: access
/// decisions are verified per request and not cached, remote policies are
/// mirrored locally, and owed registrations are gated and redriven.
#[derive(Clone)]
pub struct DocumentAcpProvider {
    pub(crate) provider: Arc<dyn vera::VeraProvider>,
    pub(crate) request_timeout: Duration,
}

impl DocumentAcpProvider {
    /// A provider whose remote decisions use the default ACP request timeout.
    pub fn new(provider: Arc<dyn vera::VeraProvider>) -> Self {
        Self {
            provider,
            request_timeout: vera::AcpTuning::default().request_timeout,
        }
    }

    /// Bound each remote decision by `request_timeout` instead.
    #[must_use]
    pub fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }
}

impl std::fmt::Debug for DocumentAcpProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DocumentAcpProvider")
            .field("request_timeout", &self.request_timeout)
            .finish_non_exhaustive()
    }
}

impl PartialEq for DocumentAcpProvider {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.provider, &other.provider)
            && self.request_timeout == other.request_timeout
    }
}

impl Eq for DocumentAcpProvider {}
