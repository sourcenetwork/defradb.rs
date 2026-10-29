//! Requests that carry their own context: the caller acting, and the
//! explicit transaction they run in.

use std::sync::Arc;

use defra_core::{RequestContext, TxnId};
use identity::Did;
use query::{QueryRequest, QueryResponse, TransactionError, TransactionHandle};
use schema::CollectionVersion;

use crate::{Error, Node, Result};

impl Node {
    /// Parse `sdl`, check it and create every collection it defines, or none,
    /// in the transaction `context` carries if it carries one.
    pub async fn add_collection_as(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>> {
        if let Some(id) = context.txn() {
            return self
                .txn_registry()
                .add_collection_in_txn_as(&self.caller(context), &id.to_string(), sdl)
                .await
                .map_err(Error::from);
        }
        let (ops, context, sdl) = (
            Arc::clone(&self.schema_ops),
            context.clone(),
            sdl.to_owned(),
        );
        self.on_pool(move || async move { ops.add_collection(&context, &sdl).await })
            .await
    }

    /// Run `request` as the identity `context` names, in the transaction it
    /// carries if it carries one. Errors in the request are in the response.
    pub async fn execute_request_as(
        &self,
        context: &RequestContext,
        request: QueryRequest,
    ) -> Result<QueryResponse> {
        let identity = context
            .identity()
            .map(Did::new)
            .transpose()
            .map_err(Error::InvalidActingIdentity)?;
        let request = request_in(context, identity, request);
        Ok(match context.txn() {
            Some(id) => self.execute_request_in_txn(request, &handle(id)).await,
            None => self.execute_request(request).await,
        })
    }

    /// Begin an explicit transaction for the caller `context` names. Requests
    /// whose context carries the returned ID run in it.
    pub async fn txn_begin(&self, context: &RequestContext, read_only: bool) -> Result<TxnId> {
        let handle = self
            .runner()
            .begin_txn_as(&self.caller(context), read_only)
            .await
            .map_err(transaction_error)?;
        handle.parse().map_err(|_| {
            Error::Transaction(TransactionError::Operation(format!(
                "transaction ID '{handle}' is not a number"
            )))
        })
    }

    /// Make the writes of transaction `id` durable and end it.
    pub async fn txn_commit(&self, context: &RequestContext, id: TxnId) -> Result<()> {
        self.runner()
            .commit_txn_as(&self.caller(context), &handle(id))
            .await
            .map_err(transaction_error)
    }

    /// Drop the writes of transaction `id` and end it.
    pub async fn txn_discard(&self, context: &RequestContext, id: TxnId) -> Result<()> {
        self.runner()
            .discard_txn_as(&self.caller(context), &handle(id))
            .await
            .map_err(transaction_error)
    }

    /// `context` acting as the node when it names no identity of its own, as
    /// [`Node::begin_transaction`] does.
    fn caller(&self, context: &RequestContext) -> RequestContext {
        match context.identity() {
            Some(_) => context.clone(),
            None => self.node_context(),
        }
    }
}

/// `request` running in all of `context` (its DAC bypass, encryption, signing,
/// batch session, transaction) as `identity`.
fn request_in(
    context: &RequestContext,
    identity: Option<Did>,
    request: QueryRequest,
) -> QueryRequest {
    request
        .with_context(context.clone())
        .with_identity(identity)
}

fn handle(id: TxnId) -> TransactionHandle {
    TransactionHandle::new(id.to_string())
}

/// A transaction failure with Go's words: only a missing transaction has its
/// own text.
fn transaction_error(error: TransactionError) -> Error {
    match error {
        TransactionError::NotFound(_) => Error::TransactionNotFound,
        TransactionError::Conflict(_) => Error::TransactionConflict,
        other => Error::Transaction(other),
    }
}

#[cfg(test)]
mod tests {
    use defra_core::{RequestContext, TxnId};
    use query::QueryRequest;

    use super::request_in;

    #[test]
    fn a_request_runs_in_the_whole_context() {
        let context = RequestContext::ambient()
            .with_dac_bypass(true)
            .with_batch_session(Some("session".to_owned()))
            .with_txn(Some(TxnId(3)));

        let request = request_in(&context, None, QueryRequest::new("{ User { name } }"));

        assert!(request.context.dac_bypass());
        assert_eq!(request.context.batch_session(), Some("session"));
        assert_eq!(request.context.txn(), Some(TxnId(3)));
    }
}
