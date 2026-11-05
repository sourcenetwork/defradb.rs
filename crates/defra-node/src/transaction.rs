use std::sync::Arc;

use defra_core::RequestContext;
use query::{TransactionError, TransactionGuard};

use crate::{Node, QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};

/// Owns a transaction while its begin result or commit task is unobserved.
pub(crate) struct OwnedTransaction {
    executor: Arc<dyn QueryExecutor>,
    handle: Option<TransactionHandle>,
}

impl OwnedTransaction {
    pub(crate) fn new(executor: Arc<dyn QueryExecutor>, handle: TransactionHandle) -> Self {
        Self {
            executor,
            handle: Some(handle),
        }
    }

    pub(crate) fn handle(&self) -> Result<&TransactionHandle, TransactionError> {
        self.handle
            .as_ref()
            .ok_or_else(|| TransactionError::already_finalized("transaction already finalized"))
    }

    pub(crate) fn into_handle(mut self) -> Result<TransactionHandle, TransactionError> {
        self.handle
            .take()
            .ok_or_else(|| TransactionError::already_finalized("transaction already finalized"))
    }

    async fn commit(mut self, context: RequestContext) -> Result<(), TransactionError> {
        self.executor
            .commit_txn_as(&context, self.handle()?)
            .await?;
        self.handle = None;
        Ok(())
    }
}

impl Drop for OwnedTransaction {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            self.executor.abandon_txn(handle);
        }
    }
}

/// An owning transaction that uses the node's signing and identity context.
///
/// Dropping it abandons uncommitted writes, including during task cancellation
/// or runtime shutdown. Cloned handles are non-owning. Cancellation after a
/// durable commit cannot undo it; see [`TransactionGuard`].
#[must_use = "dropping the transaction abandons its uncommitted writes"]
pub struct NodeTransaction<'a> {
    node: &'a Node,
    guard: TransactionGuard<'a, dyn QueryExecutor>,
}

impl Node {
    /// Add a relationship in a node-owned transaction for an actor that the
    /// embedding process has authenticated. Transaction ownership is checked
    /// as the node; native NAC and DAC check the explicit actor. This trusted
    /// embedder boundary must not be exposed as HTTP actor impersonation.
    pub async fn add_dac_actor_relationship_for_in_txn(
        &self,
        handle: &TransactionHandle,
        actor_context: &defra_core::RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> crate::Result<bool> {
        self.change_dac_actor_relationship_for_in_txn(
            handle,
            actor_context,
            crate::acp_ops::transactional::Relationship {
                collection,
                doc_id,
                relation,
                target,
            },
            false,
        )
        .await
    }

    /// Delete a relationship in a node-owned transaction for an actor that
    /// the embedding process has authenticated. Authorization checks and
    /// transaction ownership follow the add method's trusted boundary.
    pub async fn delete_dac_actor_relationship_for_in_txn(
        &self,
        handle: &TransactionHandle,
        actor_context: &defra_core::RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> crate::Result<bool> {
        self.change_dac_actor_relationship_for_in_txn(
            handle,
            actor_context,
            crate::acp_ops::transactional::Relationship {
                collection,
                doc_id,
                relation,
                target,
            },
            true,
        )
        .await
    }

    async fn change_dac_actor_relationship_for_in_txn(
        &self,
        handle: &TransactionHandle,
        actor_context: &defra_core::RequestContext,
        relationship: crate::acp_ops::transactional::Relationship<'_>,
        delete: bool,
    ) -> crate::Result<bool> {
        let id = handle
            .as_str()
            .parse()
            .map_err(|_| query::QueryError::execution("transaction ID must be numeric"))?;
        let transaction_context = self.node_context().with_txn(Some(id));
        let actor_context = actor_context.clone();
        let ops = std::sync::Arc::clone(&self.acp_ops);
        let (collection, doc_id, relation, target) = (
            relationship.collection.to_owned(),
            relationship.doc_id.to_owned(),
            relationship.relation.to_owned(),
            relationship.target.to_owned(),
        );
        self.on_acp(move || async move {
            ops.change_dac_relationship_for_in_txn(
                &transaction_context,
                &actor_context,
                crate::acp_ops::transactional::Relationship {
                    collection: &collection,
                    doc_id: &doc_id,
                    relation: &relation,
                    target: &target,
                },
                delete,
            )
            .await
        })
        .await
    }

    /// Add a document relationship as the authenticated caller, in its
    /// transaction when the request context names one.
    pub async fn add_dac_actor_relationship_as(
        &self,
        context: &defra_core::RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> crate::Result<bool> {
        let identity = context
            .identity()
            .ok_or_else(|| query::QueryError::execution("identity required for document ACP"))?
            .to_owned();
        let context = context.clone();
        let ops = std::sync::Arc::clone(&self.acp_ops);
        let (collection, doc_id, relation, target) = (
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
            target.to_owned(),
        );
        self.on_acp(move || async move {
            ops.add_dac_actor_relationship(
                &context,
                &identity,
                &collection,
                &doc_id,
                &relation,
                &target,
            )
            .await
        })
        .await
    }

    /// Delete a document relationship as the authenticated caller, in its
    /// transaction when the request context names one.
    pub async fn delete_dac_actor_relationship_as(
        &self,
        context: &defra_core::RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> crate::Result<bool> {
        let identity = context
            .identity()
            .ok_or_else(|| query::QueryError::execution("identity required for document ACP"))?
            .to_owned();
        let context = context.clone();
        let ops = std::sync::Arc::clone(&self.acp_ops);
        let (collection, doc_id, relation, target) = (
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
            target.to_owned(),
        );
        self.on_acp(move || async move {
            ops.delete_dac_actor_relationship(
                &context,
                &identity,
                &collection,
                &doc_id,
                &relation,
                &target,
            )
            .await
        })
        .await
    }

    /// Begin a transaction that is abandoned automatically unless finalized.
    ///
    /// ```no_run
    /// # async fn example(node: &defra_node::Node) -> Result<(), query::TransactionError> {
    /// let txn = node.begin_transaction_guard(false).await?;
    /// let response = txn.execute("mutation { add_Users(input: {name: \"Alice\"}) { _docID } }").await;
    /// if response.has_errors() {
    ///     txn.discard().await
    /// } else {
    ///     txn.commit().await
    /// }
    /// # }
    /// ```
    pub async fn begin_transaction_guard(
        &self,
        readonly: bool,
    ) -> Result<NodeTransaction<'_>, TransactionError> {
        // Begun as a request, like begin_transaction, then owned by the guard.
        let handle = self.begin_transaction(readonly).await?;
        let guard = TransactionGuard::adopt(self.runner().as_ref(), self.node_context(), handle);
        Ok(NodeTransaction { node: self, guard })
    }
}

impl NodeTransaction<'_> {
    /// Borrow the non-owning ID for APIs that accept a transaction handle.
    pub fn handle(&self) -> &TransactionHandle {
        self.guard.handle().expect("active transaction")
    }

    /// Execute GraphQL with the node's signing and default identity context.
    pub async fn execute(&self, query: &str) -> QueryResponse {
        self.execute_request(QueryRequest::new(query)).await
    }

    /// Execute a prepared request, preserving any explicitly supplied identity.
    pub async fn execute_request(&self, request: QueryRequest) -> QueryResponse {
        self.node
            .execute_request_in_txn(request, self.handle())
            .await
    }

    /// Commit the writes and consume the owning transaction, admitted as
    /// [`Node::commit_transaction`] admits it. The commit task owns the guard
    /// until completion when the node has a request edge, so caller
    /// cancellation does not abandon it. A refused or failed commit abandons
    /// the transaction. Without an edge, cancellation follows the executor.
    pub async fn commit(self) -> Result<(), TransactionError> {
        let handle = self
            .guard
            .into_handle()
            .ok_or_else(|| TransactionError::already_finalized("transaction already finalized"))?;
        let owned = OwnedTransaction::new(Arc::clone(self.node.runner()), handle);
        let context = self.node.node_context();
        self.node
            .as_request(owned.commit(context))
            .await
            .unwrap_or_else(|error| Err(crate::refused_transaction(error, "committed")))
    }

    /// Roll back the writes and consume the owning transaction.
    pub async fn discard(self) -> Result<(), TransactionError> {
        self.guard.discard().await
    }
}

impl Node {
    /// Observe access for an authenticated actor on known document IDs. This
    /// privileged in-process API returns no document metadata; its decisions
    /// must not be exposed to untrusted requesters. The caller authenticates
    /// the actor and chooses the authorization mode and node permission.
    /// Local ACP and row reads share the transaction. Delegated application
    /// hooks may read external state and are not fenced by this transaction.
    pub async fn observe_document_access_in_txn(
        &self,
        handle: &TransactionHandle,
        collection: &str,
        doc_ids: &[String],
        actor: identity::Did,
        mode: query::owners::DocumentAccessMode,
    ) -> query::error::Result<Vec<query::owners::DocumentAccessDecision>> {
        let context = match &mode {
            query::owners::DocumentAccessMode::DelegatedRead => {
                let node = self.node_query_identity.as_ref().ok_or_else(|| {
                    query::error::QueryError::execution(
                        "node identity is required for delegated visibility",
                    )
                })?;
                defra_core::RequestContext::default().with_identity(Some(node.to_string()))
            }
            query::owners::DocumentAccessMode::Named { .. } => {
                self.node_context().with_dac_bypass(false)
            }
        };
        self.runner()
            .observe_document_access_in_txn(context, handle, collection, doc_ids, actor, mode)
            .await
    }

    /// Read authenticated owners in the same explicit transaction as its rows.
    /// The node remains the transaction actor. A supplied document-read identity
    /// follows the ordinary GraphQL permission path; it does not borrow another
    /// actor's transaction. This grants no relationship or detached ACP read.
    pub async fn observe_document_owners_in_txn(
        &self,
        handle: &TransactionHandle,
        collection: &str,
        doc_ids: &[String],
        identity: Option<identity::Did>,
    ) -> query::error::Result<Vec<query::owners::DocumentOwnerObservation>> {
        let context = self.node_context();
        let identity = identity.or_else(|| self.node_query_identity.clone());
        self.runner()
            .observe_document_owners_in_txn(context, handle, collection, doc_ids, identity)
            .await
    }
}

#[cfg(test)]
mod cancellation_tests;
