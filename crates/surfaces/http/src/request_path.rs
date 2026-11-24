//! This server's request path for a write that is not a GraphQL request: a
//! document route's write, or each write of a backup import.

use axum::http::HeaderMap;
use db::backup::{DbJob, ImportHost};
use defra_core::thread_bounds::MaybeBoxFuture;
use query::executor::{QueryRequest, QueryResponse};
use query::txn::TransactionHandle;
use workers::Target;

use crate::error::{pool_failure, HttpError};
use crate::handlers::txn_header::txn_id_from_headers;
use crate::identity_extractor::ExtractIdentity;
use crate::query_context::{execute_in_txn_with_context, execute_with_context};
use crate::router::AppState;

/// A request runs as `identity`, in the transaction `headers` names or on
/// its own worker, as the GraphQL route runs it.
pub(crate) struct Server {
    state: AppState,
    identity: ExtractIdentity,
    txn: Option<TransactionHandle>,
}

impl Server {
    /// The path of a request that may name a transaction in `headers`.
    pub(crate) fn new(
        state: AppState,
        identity: ExtractIdentity,
        headers: &HeaderMap,
    ) -> Result<Self, HttpError> {
        let txn = txn_id_from_headers(headers)?
            .map(|id| {
                id.parse()
                    .map_err(|_| HttpError::BadRequest("invalid transaction id".into()))
            })
            .transpose()?;
        Ok(Self {
            state,
            identity,
            txn,
        })
    }

    /// The path of a request outside any transaction.
    pub(crate) fn autocommit(state: AppState, identity: ExtractIdentity) -> Self {
        Self {
            state,
            identity,
            txn: None,
        }
    }

    pub(crate) async fn run(&self, mutation: String) -> QueryResponse {
        self.run_request(QueryRequest::new(mutation)).await
    }

    pub(crate) async fn run_request(&self, request: QueryRequest) -> QueryResponse {
        let (state, identity) = (self.state.clone(), self.identity.clone());
        let request = request.with_identity(identity.did().cloned());
        if let Some(handle) = self.txn.clone() {
            return execute_in_txn_with_context(&state, &identity, request, handle).await;
        }
        let target = query::pooled::request_target(&request);
        let pool = state.pool.clone();
        workers::run_on(pool.as_ref(), target, move || async move {
            execute_with_context(&state, &identity, request).await
        })
        .await
        .unwrap_or_else(|error| QueryResponse::error(pool_failure("documents", error).to_string()))
    }
}

impl ImportHost for Server {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        Box::pin(self.run_request(request))
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            workers::run_on(self.state.pool.as_ref(), Target::LeastLoaded, job)
                .await
                .unwrap_or_else(|error| Err(pool_failure("backup", error).to_string()))
        })
    }
}
