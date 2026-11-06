use std::sync::Arc;

use async_trait::async_trait;
use datastore::NamespaceView;
use identity::Did;
use storage::corekv::{MaybeSendSync, Result};

/// Trusted context for projecting an authorized document mutation. The
/// provider is the node's configured document ACP, and the view belongs to
/// the same transaction as the mutation. Native ACP remains authoritative.
pub struct DocumentMutationContext<'a> {
    pub view: &'a NamespaceView,
    pub document_acp: &'a dyn acp::DocumentACP,
    pub node_did: &'a Did,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait DocumentMutationProjector: MaybeSendSync {
    async fn reconcile(&self, context: &DocumentMutationContext<'_>) -> Result<()>;
    async fn project(&self, context: &DocumentMutationContext<'_>, collection: &str, doc_id: &str) -> Result<()>;
}

pub(crate) struct BoundProjection {
    pub(crate) projector: Arc<dyn DocumentMutationProjector>,
    pub(crate) document_acp: Arc<dyn acp::DocumentACP>,
    pub(crate) node_did: Did,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl datastore::DocumentMutationProjector for BoundProjection {
    async fn reconcile(&self, view: &NamespaceView) -> Result<()> {
        self.projector.reconcile(&DocumentMutationContext {
            view,
            document_acp: self.document_acp.as_ref(),
            node_did: &self.node_did,
        }).await
    }

    async fn project(&self, view: &NamespaceView, collection: &str, doc_id: &str) -> Result<()> {
        self.projector.project(&DocumentMutationContext {
            view,
            document_acp: self.document_acp.as_ref(),
            node_did: &self.node_did,
        }, collection, doc_id).await
    }
}
