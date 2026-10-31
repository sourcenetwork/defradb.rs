use std::sync::Arc;

use async_trait::async_trait;
use storage::corekv::Store;

use crate::DB;

/// Query-layer adapter for collection truncate mutations.
pub struct DbCollectionTruncator<S: Store> {
    db: Arc<DB<S>>,
}

impl<S: Store> DbCollectionTruncator<S> {
    pub fn new_arc(db: Arc<DB<S>>) -> Arc<Self> {
        Arc::new(Self { db })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> query::CollectionTruncator for DbCollectionTruncator<S> {
    async fn truncate(
        &self,
        collection_name: &str,
        filter: Option<query::Filter>,
        identity: Option<&identity::Did>,
    ) -> query::Result<()> {
        self.truncate_as(
            &crate::access::nac::ambient_as(identity),
            collection_name,
            filter,
        )
        .await
    }

    async fn truncate_as(
        &self,
        context: &defra_core::RequestContext,
        collection_name: &str,
        filter: Option<query::Filter>,
    ) -> query::Result<()> {
        self.db
            .truncate_parsed_as(context, collection_name, filter.as_ref())
            .await
            .map_err(|error| query::QueryError::execution(error.to_string()))
    }
}
