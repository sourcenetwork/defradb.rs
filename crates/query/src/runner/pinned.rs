//! A collection provider that answers one selected collection version for
//! its name, active or not, so a request working through that version
//! validates and plans against its fields (`proofs/tla/VersionPin.tla`).

use std::sync::Arc;

use async_trait::async_trait;
use schema::CollectionVersion;

use crate::collection_provider::CollectionProvider;
use crate::error::Result;

/// `inner`, except that the version `version_id` names answers lookups of
/// its own collection's name.
pub(crate) struct PinnedProvider {
    inner: Arc<dyn CollectionProvider>,
    version_id: String,
}

impl PinnedProvider {
    pub(crate) fn new(inner: Arc<dyn CollectionProvider>, version_id: String) -> Self {
        Self { inner, version_id }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl CollectionProvider for PinnedProvider {
    async fn get_collection(&self, name: &str) -> Result<Option<Arc<CollectionVersion>>> {
        if let Some(pinned) = self
            .inner
            .get_collection_by_version_id(&self.version_id)
            .await?
            .filter(|version| version.name == name)
        {
            return Ok(Some(pinned));
        }
        self.inner.get_collection(name).await
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
        self.inner.list_collections().await
    }

    /// No epoch: what this provider answers depends on the request, so a
    /// cache keyed by the schema epoch must not keep it.
    fn schema_epoch(&self) -> Option<u64> {
        None
    }

    async fn get_collection_by_version_id(
        &self,
        version_id: &str,
    ) -> Result<Option<Arc<CollectionVersion>>> {
        self.inner.get_collection_by_version_id(version_id).await
    }
}
