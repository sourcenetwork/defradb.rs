//! Requests that carry their own context: the caller acting, and the
//! explicit transaction they run in.

use std::sync::Arc;

use defra_core::{RequestContext, TxnId};
use identity::Did;
use query::{QueryRequest, QueryResponse, TransactionError, TransactionHandle};
use schema::CollectionVersion;

use crate::{refused_transaction, Error, Node, Result};

impl Node {
    /// Parse `sdl`, check it and create every collection it defines, or none,
    /// in the transaction `context` carries if it carries one.
    pub async fn add_collection_as(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>> {
        if context.txn().is_some() {
            return self
                .runner()
                .add_collection_as(&self.caller(context), sdl)
                .await
                .map_err(transaction_error);
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
        let identity = acting_identity(context)?;
        Ok(self.run_as_identity(context, identity, request).await)
    }

    /// Run `request` in all of `context` as `identity`, in the transaction
    /// `context` carries if it carries one.
    pub(crate) async fn run_as_identity(
        &self,
        context: &RequestContext,
        identity: Option<Did>,
        request: QueryRequest,
    ) -> QueryResponse {
        let request = request_in(context, identity, request);
        match context.txn() {
            Some(id) => self.execute_request_in_txn(request, &handle(id)).await,
            None => self.execute_request(request).await,
        }
    }

    /// Begin an explicit transaction for the caller `context` names. Requests
    /// whose context carries the returned ID run in it. Begun as a request,
    /// as [`Node::begin_transaction`] is.
    pub async fn txn_begin(&self, context: &RequestContext, read_only: bool) -> Result<TxnId> {
        let owned = self
            .begin_owned_transaction(self.caller(context), read_only)
            .await
            .map_err(transaction_error)?;
        let handle = owned.handle().map_err(transaction_error)?;
        let id = handle.parse().map_err(|_| {
            Error::Transaction(TransactionError::Operation(format!(
                "transaction ID '{handle}' is not a number"
            )))
        })?;
        owned.into_handle().map_err(transaction_error)?;
        Ok(id)
    }

    /// Make the writes of transaction `id` durable and end it, admitted as
    /// [`Node::commit_transaction`] admits it.
    pub async fn txn_commit(&self, context: &RequestContext, id: TxnId) -> Result<()> {
        let (runner, caller) = (Arc::clone(self.runner()), self.caller(context));
        self.as_request(async move { runner.commit_txn_as(&caller, &handle(id)).await })
            .await
            .unwrap_or_else(|error| Err(refused_transaction(error, "committed")))
            .map_err(transaction_error)
    }

    /// Drop the writes of transaction `id` and end it.
    pub async fn txn_discard(&self, context: &RequestContext, id: TxnId) -> Result<()> {
        self.runner()
            .discard_txn_as(&self.caller(context), &handle(id))
            .await
            .map_err(transaction_error)
    }

    /// Node access control for `permission`, for the operations that check
    /// one before the work they hand on, as Go's do.
    pub(crate) async fn check_access(
        &self,
        context: &RequestContext,
        permission: acp::nac::NodePermission,
    ) -> Result<()> {
        let (database, context) = (Arc::clone(self.database()), self.caller(context));
        self.on_pool(move || async move {
            Ok(database.check_node_access_as(&context, permission).await?)
        })
        .await
    }

    /// `context` acting as the node when it names no identity of its own, as
    /// [`Node::begin_transaction`] does.
    pub(crate) fn caller(&self, context: &RequestContext) -> RequestContext {
        match context.identity() {
            Some(_) => context.clone(),
            None => self.node_context().with_txn(context.txn()),
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

/// The identity acting in `context`; a malformed one fails.
pub(crate) fn acting_identity(context: &RequestContext) -> Result<Option<Did>> {
    context
        .identity()
        .map(Did::new)
        .transpose()
        .map_err(Error::InvalidActingIdentity)
}

fn handle(id: TxnId) -> TransactionHandle {
    TransactionHandle::new(id.to_string())
}

/// A transaction failure with Go's words: only a missing transaction has its
/// own text.
pub(crate) fn transaction_error(error: TransactionError) -> Error {
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
