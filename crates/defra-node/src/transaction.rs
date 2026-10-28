use crate::{Node, QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
use query::{TransactionError, TransactionGuard};

/// An owning transaction that uses the node's signing and identity context.
///
/// Dropping it abandons uncommitted writes, including during task cancellation
/// or runtime shutdown. Cloned handles are non-owning. Cancellation after a
/// durable commit cannot undo it; see [`TransactionGuard`].
#[must_use = "dropping the transaction abandons its uncommitted writes"]
pub struct NodeTransaction<'a, S = storage::RegolithStore>
where
    S: storage::corekv::Store + 'static,
{
    node: &'a Node<S>,
    guard: TransactionGuard<'a, dyn QueryExecutor>,
}

impl<S> Node<S>
where
    S: storage::corekv::Store + 'static,
{
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
    ) -> Result<NodeTransaction<'_, S>, TransactionError> {
        let guard =
            TransactionGuard::begin_as(self.runner().as_ref(), self.node_context(), readonly)
                .await?;
        Ok(NodeTransaction { node: self, guard })
    }
}

impl<S> NodeTransaction<'_, S>
where
    S: storage::corekv::Store + 'static,
{
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

impl<S> Node<S>
where
    S: storage::corekv::Store + 'static,
{
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
