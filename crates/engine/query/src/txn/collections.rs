//! A transaction's collections, seen from outside the transaction.
//!
//! A REST operation scoped to a transaction resolves collections the
//! transaction sees, uncommitted schema changes included. The transaction
//! lives where its registry keeps it, so each lookup runs there, as a request
//! of its own, after the requests queued before it.

use std::sync::Arc;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeBoxFuture;
use defra_core::RequestContext;
use schema::CollectionVersion;
use storage::corekv::MaybeSend;

use crate::collection_provider::CollectionProvider;
use crate::error::{QueryError, Result};

use super::{txn_job, TransactionHandle, TransactionRegistry};

/// The collections transaction `handle` sees, for the request `context`
/// serves, falling back to `fallback` where the transaction has no provider
/// of its own.
pub struct TxnCollections<R> {
    registry: Arc<R>,
    context: RequestContext,
    handle: TransactionHandle,
    fallback: Arc<dyn CollectionProvider>,
}

impl<R> TxnCollections<R>
where
    R: TransactionRegistry,
{
    pub fn new(
        registry: Arc<R>,
        context: RequestContext,
        handle: TransactionHandle,
        fallback: Arc<dyn CollectionProvider>,
    ) -> Self {
        Self {
            registry,
            context,
            handle,
            fallback,
        }
    }

    /// Runs `lookup` on the transaction's provider, where the transaction
    /// lives.
    async fn lookup<T, L>(&self, lookup: L) -> Result<T>
    where
        T: MaybeSend + 'static,
        L: FnOnce(Arc<dyn CollectionProvider>) -> MaybeBoxFuture<'static, Result<T>>
            + Send
            + 'static,
    {
        let fallback = Arc::clone(&self.fallback);
        let job = txn_job(move |turn| {
            Box::pin(async move {
                let provider = turn
                    .context()
                    .map_err(|error| QueryError::execution(error.to_string()))?
                    .collection_provider()
                    .unwrap_or(fallback);
                lookup(provider).await
            })
        });
        self.registry
            .run_as(&self.context, &self.handle, None, job)
            .await
            .map_err(|error| QueryError::execution(error.to_string()))?
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<R> CollectionProvider for TxnCollections<R>
where
    R: TransactionRegistry,
{
    async fn get_collection(&self, name: &str) -> Result<Option<Arc<CollectionVersion>>> {
        let name = name.to_owned();
        self.lookup(move |provider| Box::pin(async move { provider.get_collection(&name).await }))
            .await
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
        self.lookup(|provider| Box::pin(async move { provider.list_collections().await }))
            .await
    }

    async fn get_collection_by_version_id(
        &self,
        version_id: &str,
    ) -> Result<Option<Arc<CollectionVersion>>> {
        let version_id = version_id.to_owned();
        self.lookup(move |provider| {
            Box::pin(async move { provider.get_collection_by_version_id(&version_id).await })
        })
        .await
    }
}
