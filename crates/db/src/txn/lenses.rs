use rapidhash::RapidHashMap;
use regolith::sync::OnceCell;
use std::sync::Arc;

use async_trait::async_trait;
use lens::{LensConfig, LensDoc, LensDocResultStream, LensDocStream, TransformId, TransformStore};

/// Transaction-local transform store overlay.
///
/// Reads fall back to the committed global store, while writes remain isolated to
/// the transaction-local store until commit promotion runs. The local store comes
/// from the global one, so it shares the database's lens runtime and epoch clock.
pub(crate) struct TxnLensStore {
    base: Arc<dyn TransformStore>,
    local: OnceCell<Arc<dyn TransformStore>>,
}

impl TxnLensStore {
    pub(crate) fn new(base: Arc<dyn TransformStore>) -> Self {
        Self {
            base,
            local: OnceCell::new(),
        }
    }

    fn local(&self) -> &Arc<dyn TransformStore> {
        self.local
            .get_or_init_racy(|| self.base.new_isolated_store())
    }

    fn initialized_local(&self) -> Option<&Arc<dyn TransformStore>> {
        self.local.get()
    }

    /// The store that holds `id`: this transaction's registrations first, then
    /// the committed store.
    fn holder(&self, id: &TransformId) -> &Arc<dyn TransformStore> {
        self.initialized_local()
            .filter(|local| local.has_transform(id))
            .unwrap_or(&self.base)
    }
}

#[async_trait]
impl TransformStore for TxnLensStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        self.base.new_isolated_store()
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        self.local().add(config).await
    }

    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> lens::Result<()> {
        self.local().add_with_id(id, config).await
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, lens::LensModule>> {
        let mut result = self.base.list().await?;
        if let Some(local) = self.initialized_local() {
            result.extend(local.list().await?);
        }
        Ok(result)
    }

    fn transform(
        &self,
        id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        self.holder(id).transform(id, docs)
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.holder(id).inverse(id, docs)
    }

    async fn transform_doc(
        &self,
        id: &TransformId,
        doc: LensDoc,
        inverse: bool,
    ) -> lens::Result<LensDoc> {
        self.holder(id).transform_doc(id, doc, inverse).await
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.initialized_local()
            .is_some_and(|local| local.has_transform(id))
            || self.base.has_transform(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        self.holder(id).remove(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_local_store_is_created_only_for_registration() {
        workers::block_on(async {
            let base: Arc<dyn TransformStore> = Arc::new(lens::MemoryTransformStore::new());
            let store = TxnLensStore::new(base.clone());

            assert!(store.local.get().is_none());
            assert!(store.list().await.unwrap().is_empty());
            assert!(!store.has_transform(&TransformId::new("missing")));
            assert!(store.local.get().is_none());

            // The registration lands in an empty store of the base's own kind. A
            // memory store accepts a module no WASM runtime would compile, so no
            // private engine was built for the transaction.
            let id = store
                .add(LensConfig::new("v1", "v2", lens::LensModule::default()))
                .await
                .unwrap();
            assert!(store.local.get().is_some());
            assert!(store.has_transform(&id));
            assert!(!base.has_transform(&id));
            assert_eq!(store.list().await.unwrap().len(), 1);
        })
        .unwrap()
    }

    fn no_docs() -> LensDocStream {
        Box::pin(futures::stream::empty())
    }

    fn config() -> LensConfig {
        LensConfig::new("v1", "v2", lens::LensModule::default())
    }

    #[test]
    fn calls_reach_the_store_that_holds_the_transform() {
        workers::block_on(async {
            let base = Arc::new(lens::MemoryTransformStore::new());
            let committed = TransformId::new("committed");
            base.add_with_id(committed.clone(), config()).await.unwrap();
            let store = TxnLensStore::new(base.clone());
            let missing = TransformId::new("missing");

            // Before any registration every call reaches the committed store.
            assert!(store.transform(&committed, no_docs()).is_ok());
            assert!(matches!(
                store.transform(&missing, no_docs()),
                Err(lens::Error::TransformNotFound(_))
            ));

            let local = TransformId::new("local");
            store.add_with_id(local.clone(), config()).await.unwrap();
            assert!(store.transform(&local, no_docs()).is_ok());
            assert!(store.inverse(&local, no_docs()).is_ok());
            assert!(store.inverse(&committed, no_docs()).is_ok());
            assert!(!base.has_transform(&local));

            let mut doc = LensDoc::new();
            doc.insert("name".to_string(), serde_json::json!("Ada"));
            assert_eq!(
                store
                    .transform_doc(&local, doc.clone(), false)
                    .await
                    .unwrap(),
                doc
            );
            assert_eq!(
                store
                    .transform_doc(&committed, doc.clone(), true)
                    .await
                    .unwrap(),
                doc
            );
            assert!(matches!(
                store.transform_doc(&missing, doc, false).await,
                Err(lens::Error::TransformNotFound(_))
            ));

            store.remove(&local).await.unwrap();
            assert!(!store.has_transform(&local));
            assert!(base.has_transform(&committed));
            store.remove(&committed).await.unwrap();
            assert!(!base.has_transform(&committed));
            assert!(matches!(
                store.remove(&missing).await,
                Err(lens::Error::TransformNotFound(_))
            ));
        })
        .unwrap()
    }
}
