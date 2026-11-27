//! Privileged in-process reads of document access and owners, in an
//! explicit transaction.

use defra_core::TxnId;
use query::TransactionHandle;

use super::NodeCore;

impl NodeCore<'_> {
    /// Observe access for an authenticated actor on known document IDs. This
    /// privileged in-process API returns no document metadata; its decisions
    /// must not be exposed to untrusted requesters. The caller authenticates
    /// the actor and chooses the authorization mode and node permission.
    /// Local ACP and row reads share the transaction. Remote ACP evaluates
    /// its current authority and cannot share the row transaction; subsequent
    /// writes must still pass the provider’s own authorization. Delegated
    /// application hooks may also read external state.
    pub(crate) async fn observe_document_access(
        &self,
        id: TxnId,
        collection: &str,
        doc_ids: &[String],
        actor: identity::Did,
        mode: query::owners::DocumentAccessMode,
    ) -> query::error::Result<Vec<query::owners::DocumentAccessDecision>> {
        let context = match &mode {
            query::owners::DocumentAccessMode::DelegatedRead => {
                let node = self.node.node_query_identity.as_ref().ok_or_else(|| {
                    query::error::QueryError::execution(
                        "node identity is required for delegated visibility",
                    )
                })?;
                defra_core::RequestContext::default().with_identity(Some(node.to_string()))
            }
            query::owners::DocumentAccessMode::Named { .. }
            | query::owners::DocumentAccessMode::Document { .. } => {
                self.node.node_context().with_dac_bypass(false)
            }
        };
        self.node
            .runner()
            .observe_document_access_in_txn(context, &handle(id), collection, doc_ids, actor, mode)
            .await
    }

    /// Read authenticated owners in the same explicit transaction as its rows.
    /// The node remains the transaction actor. A supplied document-read identity
    /// follows the ordinary GraphQL permission path; it does not borrow another
    /// actor's transaction. This grants no relationship or detached ACP read.
    pub(crate) async fn observe_document_owners(
        &self,
        id: TxnId,
        collection: &str,
        doc_ids: &[String],
        identity: Option<identity::Did>,
    ) -> query::error::Result<Vec<query::owners::DocumentOwnerObservation>> {
        let context = self.node.node_context();
        let identity = identity.or_else(|| self.node.node_query_identity.clone());
        self.node
            .runner()
            .observe_document_owners_in_txn(context, &handle(id), collection, doc_ids, identity)
            .await
    }
}

fn handle(id: TxnId) -> TransactionHandle {
    TransactionHandle::new(id.to_string())
}
