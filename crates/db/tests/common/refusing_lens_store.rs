//! A lens store that refuses to register a lens under its ID while told to, as
//! a store does when a module cannot be read or compiled at that moment.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use lens::{
    LensConfig, LensDocResultStream, LensDocStream, LensModule, MemoryTransformStore, TransformId,
    TransformStore,
};
use rapidhash::RapidHashMap;

/// A transform store that refuses to register a lens under its ID while
/// `refuse` is set, as a store does when a module cannot be read or compiled
/// at that moment. Transactions register into a store that never refuses.
pub struct RefusingStore {
    pub inner: MemoryTransformStore,
    pub refuse: AtomicBool,
}

#[async_trait]
impl TransformStore for RefusingStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(MemoryTransformStore::new())
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        self.inner.add(config).await
    }

    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> lens::Result<()> {
        if self.refuse.load(Ordering::SeqCst) {
            return Err(lens::Error::Pipeline("registration refused".to_string()));
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

impl RefusingStore {
    /// A store that refuses registration until `allow` is called.
    pub fn refusing() -> Arc<Self> {
        Arc::new(Self {
            inner: MemoryTransformStore::new(),
            refuse: AtomicBool::new(true),
        })
    }

    pub fn allow(&self) {
        self.refuse.store(false, Ordering::SeqCst);
    }
}
