//! Adapter to bridge DbTransactionRegistry to HTTP's TransactionOperations trait.
//!
//! Each call goes straight to the registry, which runs it on the worker that
//! owns the transaction, in the context of the request the adapter is bound
//! to.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::TransactionOperations;
use identity::Did;
use storage::corekv::Store;

/// Adapter that implements TransactionOperations using a shared DbTransactionRegistry.
pub struct TxnRegistryAdapter<S>
where
    S: Store + 'static,
{
    registry: Arc<db::DbTransactionRegistry<S>>,
    document_acp: Option<Arc<dyn acp::DocumentACP>>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S> TxnRegistryAdapter<S>
where
    S: Store + 'static,
{
    /// Create an Arc-wrapped adapter backed by the shared transaction registry.
    pub fn new_arc(registry: Arc<db::DbTransactionRegistry<S>>) -> Arc<dyn TransactionOperations> {
        Arc::new(Self {
            registry,
            document_acp: None,
            context: None,
        })
    }

    /// Create an Arc-wrapped adapter with document ACP registration support.
    pub fn new_arc_with_acp(
        registry: Arc<db::DbTransactionRegistry<S>>,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Arc<dyn TransactionOperations> {
        Arc::new(Self {
            registry,
            document_acp: Some(document_acp),
            context: None,
        })
    }

    /// The context of the request this adapter serves.
    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }
}

#[async_trait]
impl<S> TransactionOperations for TxnRegistryAdapter<S>
where
    S: Store + 'static,
{
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn TransactionOperations>> {
        Some(Arc::new(Self {
            registry: Arc::clone(&self.registry),
            document_acp: self.document_acp.clone(),
            context: Some(context.clone()),
        }))
    }

    async fn set_migration_in_txn(&self, txn_id: &str, config: &str) -> Result<String, String> {
        let lens_config: lens::LensConfig = serde_json::from_str(config)
            .map_err(|e| format!("failed to parse lens config: {}", e))?;
        self.registry
            .set_migration_in_txn_as(&self.context(), txn_id, lens_config)
            .await
            .map(|id| id.to_string())
            .map_err(|e| e.to_string())
    }

    async fn get_collections_in_txn(
        &self,
        txn_id: &str,
    ) -> Result<Vec<schema::CollectionVersion>, String> {
        self.registry
            .get_collections_in_txn_as(&self.context(), txn_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn add_collection_in_txn(
        &self,
        txn_id: &str,
        sdl: &str,
    ) -> Result<Vec<schema::CollectionVersion>, String> {
        let context = self.context();
        // Fail closed: a present-but-malformed acting identity must error, not
        // silently degrade a permissioned collection to public (unregistered).
        let creator = context
            .identity()
            .map(Did::new)
            .transpose()
            .map_err(|e| format!("malformed acting identity: {}", e))?;
        self.registry
            .add_collection_in_txn_with_acp_as(
                &context,
                txn_id,
                sdl,
                self.document_acp.clone(),
                creator,
            )
            .await
            .map_err(|e| e.to_string())
    }
}
