use crate::{Node, QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
use query::{TransactionError, TransactionGuard};

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
        let guard =
            TransactionGuard::begin_as(self.runner().as_ref(), self.node_context(), readonly)
                .await?;
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

    /// Commit the writes and consume the owning transaction.
    pub async fn commit(self) -> Result<(), TransactionError> {
        self.guard.commit().await
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
