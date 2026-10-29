//! REST operations trait definition.

use crate::txn::TransactionHandle;
use async_trait::async_trait;
use defra_core::RequestContext;
use identity::Did;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use storage::corekv::MaybeSendSync;

use super::error::{RestError, RestResult};

/// Pagination window for listing collection document IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectionDocIdsPagination {
    pub limit: usize,
    pub offset: usize,
}

/// Paginated document IDs for a collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionDocIdsPage {
    pub doc_ids: Vec<String>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

impl CollectionDocIdsPage {
    pub fn has_more(&self) -> bool {
        self.offset.saturating_add(self.limit) < self.total
    }
}

/// REST reads: collection listing and documents by ID. Document writes are
/// [`crate::documents`].
///
/// This trait provides REST-specific operations separate from GraphQL execution.
/// Operations use auto-commit unless bound to an explicit transaction.
///
/// # Identity and ACP
///
/// All document operations accept an optional `identity` parameter for access
/// control: read operations check read permission on protected documents.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RestOperations: MaybeSendSync {
    /// These operations serving the request `context` carries, so a
    /// transaction it began and the credentials it presents reach them; `None`
    /// when they read nothing of a request.
    fn bind_request(&self, _context: &RequestContext) -> Option<Arc<dyn RestOperations>> {
        None
    }

    /// Bind subsequent operations to an existing transaction.
    fn with_transaction(&self, _handle: TransactionHandle) -> RestResult<Arc<dyn RestOperations>> {
        Err(RestError::invalid_input(
            "transactional REST operations are unavailable",
        ))
    }

    /// List all collection names.
    async fn list_collections(&self) -> RestResult<Vec<String>>;

    /// Get all document IDs in a collection.
    async fn get_collection_doc_ids(
        &self,
        collection: &str,
        identity: Option<&Did>,
    ) -> RestResult<Vec<String>>;

    /// Get a paginated page of document IDs in a collection.
    async fn get_collection_doc_ids_page(
        &self,
        collection: &str,
        pagination: CollectionDocIdsPagination,
        identity: Option<&Did>,
    ) -> RestResult<CollectionDocIdsPage> {
        let doc_ids = self.get_collection_doc_ids(collection, identity).await?;
        let total = doc_ids.len();
        let start = pagination.offset.min(total);
        let end = start.saturating_add(pagination.limit).min(total);

        Ok(CollectionDocIdsPage {
            doc_ids: doc_ids[start..end].to_vec(),
            total,
            limit: pagination.limit,
            offset: pagination.offset,
        })
    }

    /// Get a single document by ID.
    async fn get_document(
        &self,
        collection: &str,
        doc_id: &str,
        identity: Option<&Did>,
    ) -> RestResult<Option<JsonValue>>;
}
