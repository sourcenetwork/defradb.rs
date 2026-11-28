//! Type-erased document-ACP relationship operations bridging DB to Node's
//! public DAC API.

use crate::{Error, Result};
use std::sync::Arc;

use defra_core::RequestContext;

pub(crate) mod transactional;
pub(crate) mod transfer;

#[async_trait::async_trait]
pub(crate) trait AcpOps: Send + Sync {
    async fn change_nac_source_for_in_txn(
        &self,
        _transaction_context: &RequestContext,
        _actor_context: &RequestContext,
        _edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        Err(query::QueryError::execution(
            "transaction-bound node source permissions are unavailable",
        )
        .into())
    }
    /// A trusted embedder selects a document actor while the node owns the
    /// transaction; the HTTP adapter never exposes this operation.
    async fn change_dac_relationship_for_in_txn(
        &self,
        _transaction_context: &RequestContext,
        _actor_context: &RequestContext,
        _relationship: transactional::Relationship<'_>,
        _delete: bool,
    ) -> Result<transactional::RelationshipChange> {
        Err(query::QueryError::execution(
            "transaction-bound relationship delegation is unavailable",
        )
        .into())
    }
    /// A trusted embedder gives a document to another owner as the actor it
    /// authenticated, while the node owns the transaction.
    async fn transfer_dac_owner_for_in_txn(
        &self,
        _transaction_context: &RequestContext,
        _actor_context: &RequestContext,
        _transfer: transfer::Transfer<'_>,
    ) -> Result<()> {
        Err(
            query::QueryError::execution("transaction-bound ownership transfer is unavailable")
                .into(),
        )
    }
    /// A relationship change as the authenticated caller, in the transaction
    /// `context` names when it names one, saying whether it was applied or
    /// staged.
    async fn change_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: transactional::Relationship<'_>,
        delete: bool,
    ) -> Result<transactional::RelationshipChange> {
        applied(self, context, identity, relationship, delete).await
    }

    #[cfg(feature = "http")]
    async fn check_doc_access(
        &self,
        context: &RequestContext,
        actor: &identity::Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool>;
    async fn add_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool>;
    async fn delete_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool>;
    /// Every relationship on a document, for its owner or a requester holding
    /// a relation that manages one on that resource.
    async fn list_dac_relationships(
        &self,
        _context: &RequestContext,
        _identity: &str,
        _collection: &str,
        _doc_id: &str,
    ) -> Result<Vec<acp::DocumentRelationship>> {
        Err(Error::Acp(acp::Error::UnsupportedRelationshipListing))
    }
    async fn observe_dac_actor_relationship(
        &self,
        _context: &RequestContext,
        _identity: &str,
        _relationship: transactional::Relationship<'_>,
    ) -> Result<bool> {
        Err(Error::Acp(acp::Error::UnsupportedRelationshipListing))
    }
}

/// A relationship change through the calls that answer a boolean, which
/// apply it before they return.
async fn applied<A>(
    ops: &A,
    context: &RequestContext,
    identity: &str,
    relationship: transactional::Relationship<'_>,
    delete: bool,
) -> Result<transactional::RelationshipChange>
where
    A: AcpOps + ?Sized,
{
    let transactional::Relationship {
        collection,
        doc_id,
        relation,
        target,
    } = relationship;
    let existed = if delete {
        ops.delete_dac_actor_relationship(context, identity, collection, doc_id, relation, target)
            .await?
    } else {
        ops.add_dac_actor_relationship(context, identity, collection, doc_id, relation, target)
            .await?
    };
    Ok(transactional::RelationshipChange::Applied { existed })
}

/// [`AcpOps`] over the core's document relationship operations. Only the
/// node source permission and ownership transfer remain node operations.
pub(crate) struct DbAcpOps<S>
where
    S: storage::corekv::Store + 'static,
{
    database: Arc<db::DB<S>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    registry: Arc<db::DbTransactionRegistry<S>>,
    relationships: db::DocumentRelationships<S>,
}

impl<S> DbAcpOps<S>
where
    S: storage::corekv::Store + 'static,
{
    pub(crate) fn new(
        database: Arc<db::DB<S>>,
        registry: Arc<db::DbTransactionRegistry<S>>,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Self {
        Self {
            relationships: db::DocumentRelationships::new(
                Arc::clone(&registry),
                Arc::clone(&document_acp),
            ),
            database,
            document_acp,
            registry,
        }
    }
}

#[async_trait::async_trait]
impl<S> AcpOps for DbAcpOps<S>
where
    S: storage::corekv::Store + 'static,
{
    async fn observe_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: transactional::Relationship<'_>,
    ) -> Result<bool> {
        Ok(self
            .relationships
            .observe_as(context, identity, relationship)
            .await?)
    }

    async fn list_dac_relationships(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
    ) -> Result<Vec<acp::DocumentRelationship>> {
        Ok(self
            .relationships
            .list_as(context, identity, collection, doc_id)
            .await?)
    }

    async fn change_nac_source_for_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        use query::txn::TransactionRegistry as _;
        let id = transaction_context
            .txn()
            .ok_or_else(|| query::QueryError::execution("an explicit transaction is required"))?;
        if !transaction_context.authorization_valid_now()
            || actor_context.txn().is_some_and(|actor_id| actor_id != id)
        {
            return Err(query::QueryError::permission_denied(
                "invalid transaction or actor authorization",
            )
            .into());
        }
        let handle = query::txn::TransactionHandle::new(id.to_string());
        // A projection the edge sets off may need a remote input: its
        // statement is undone, the input answered here, and the edge run again.
        let inputs = query::prepared::PreparedInputMemo::default();
        loop {
            let job = query::txn::embedded_source_permission_job(
                actor_context.clone(),
                edge.clone(),
                inputs.clone(),
            )?;
            match self
                .registry
                .run_as(transaction_context, &handle, None, job)
                .await?
            {
                Err(query::TransactionError::NeedsInput(request)) => inputs
                    .answer_wave(&request, |wave| {
                        self.registry
                            .db()
                            .resolve_projection_inputs(actor_context, &inputs, wave)
                    })
                    .await
                    .map_err(Error::from)?,
                done => return Ok(done?),
            }
        }
    }
    async fn transfer_dac_owner_for_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        transfer: transfer::Transfer<'_>,
    ) -> Result<()> {
        self.transfer_in_txn(transaction_context, actor_context, transfer)
            .await
    }

    async fn change_dac_relationship_for_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        relationship: transactional::Relationship<'_>,
        delete: bool,
    ) -> Result<transactional::RelationshipChange> {
        Ok(self
            .relationships
            .change_for_in_txn(transaction_context, actor_context, relationship, delete)
            .await?)
    }

    async fn change_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: transactional::Relationship<'_>,
        delete: bool,
    ) -> Result<transactional::RelationshipChange> {
        Ok(self
            .relationships
            .change_as(context, identity, relationship, delete)
            .await?)
    }

    #[cfg(feature = "http")]
    async fn check_doc_access(
        &self,
        context: &RequestContext,
        actor: &identity::Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        Ok(self
            .relationships
            .check_doc_access_as(context, actor, permission, policy_id, resource_name, doc_id)
            .await?)
    }

    async fn add_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let relationship = transactional::Relationship {
            collection,
            doc_id,
            relation,
            target,
        };
        Ok(self
            .relationships
            .add_as(context, identity, relationship)
            .await?)
    }

    async fn delete_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let relationship = transactional::Relationship {
            collection,
            doc_id,
            relation,
            target,
        };
        Ok(self
            .relationships
            .delete_as(context, identity, relationship)
            .await?)
    }
}
