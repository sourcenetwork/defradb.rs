//! A lens store whose first registration under an ID waits until released, as
//! a slow module compile keeps a committed lens out of the store for a while.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use lens::{
    LensConfig, LensDocResultStream, LensDocStream, LensModule, MemoryTransformStore, TransformId,
    TransformStore,
};
use rapidhash::RapidHashMap;
use regolith::sync::Event;

/// A transform store whose first `add_with_id` signals `entered` and waits for
/// `release`; every later registration goes straight through. Transactions
/// register into a store that never waits.
#[derive(Default)]
pub struct GatedStore {
    pub inner: MemoryTransformStore,
    held: AtomicBool,
    pub entered: Event,
    released: Event,
}

impl GatedStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn release(&self) {
        self.released.set();
    }
}

#[async_trait]
impl TransformStore for GatedStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(MemoryTransformStore::new())
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        self.inner.add(config).await
    }

    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> lens::Result<()> {
        if !self.held.swap(true, Ordering::SeqCst) {
            self.entered.set();
            self.released.wait().await;
        }
        self.inner.add_with_id(id, config).await
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<LensModule>>> {
        self.inner.list().await
    }

    fn transform(
        &self,
        id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        self.inner.transform(id, docs)
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.inner.inverse(id, docs)
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.inner.has_transform(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        self.inner.remove(id).await
    }
}
