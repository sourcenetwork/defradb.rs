use rapidhash::{RapidHashMap, RapidHashSet};
use regolith::sync::{Atom, OnceCell};
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
    visible: Atom<RapidHashSet<TransformId>>,
}

impl TxnLensStore {
    pub(crate) fn new(base: Arc<dyn TransformStore>) -> Self {
        Self {
            base,
            local: OnceCell::new(),
            visible: Atom::new(RapidHashSet::default()),
        }
    }

    fn local(&self) -> &Arc<dyn TransformStore> {
        self.local
            .get_or_init_racy(|| self.base.new_isolated_store())
    }

    /// The store holding this transaction's own registrations.
    pub(crate) fn registrations(&self) -> Arc<dyn TransformStore> {
        Arc::clone(self.local())
    }

    fn initialized_local(&self) -> Option<&Arc<dyn TransformStore>> {
        self.local.get()
    }

    /// The store that holds `id`: this transaction's registrations first, then
    /// the committed store.
    fn holder(&self, id: &TransformId) -> &Arc<dyn TransformStore> {
        self.initialized_local()
            .filter(|local| self.visible.load_clone().contains(id) && local.has_transform(id))
            .unwrap_or(&self.base)
    }

    pub(crate) fn snapshot(&self) -> RapidHashSet<TransformId> {
        self.visible.load_clone()
    }

    pub(crate) fn restore(&self, snapshot: RapidHashSet<TransformId>) {
        self.visible.store(snapshot);
    }

    fn publish(&self, id: TransformId) {
        self.visible.rcu(|ids| {
            let mut next = ids.clone();
            next.insert(id.clone());
            next
        });
    }

    fn withdraw(&self, id: &TransformId) {
        self.visible.rcu(|ids| {
            let mut next = ids.clone();
            next.remove(id);
            next
        });
    }
}

#[async_trait]
impl TransformStore for TxnLensStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        self.base.new_isolated_store()
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        let id = self.local().add(config).await?;
        self.publish(id.clone());
        Ok(id)
    }

    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> lens::Result<()> {
        self.local().add_with_id(id.clone(), config).await?;
        self.publish(id);
        Ok(())
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<lens::LensModule>>> {
        let mut result = self.base.list().await?;
        if let Some(local) = self.initialized_local() {
            let visible = self.visible.load_clone();
            result.extend(
                local
                    .list()
                    .await?
                    .into_iter()
                    .filter(|(id, _)| visible.contains(&TransformId::new(id))),
            );
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
            .is_some_and(|local| self.visible.load_clone().contains(id) && local.has_transform(id))
            || self.base.has_transform(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        let local_holds = self
            .initialized_local()
            .is_some_and(|local| self.visible.load_clone().contains(id) && local.has_transform(id));
        self.holder(id).remove(id).await?;
        if local_holds {
            self.withdraw(id);
        }
        Ok(())
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

    #[test]
    fn a_snapshot_hides_lenses_registered_after_it() {
        workers::block_on(async {
            let base = Arc::new(lens::MemoryTransformStore::new());
            let store = TxnLensStore::new(base);
            let before = store.snapshot();

            let kept = TransformId::new("kept");
            store.add_with_id(kept.clone(), config()).await.unwrap();
            let snapshot = store.snapshot();
            let discarded = TransformId::new("discarded");
            store
                .add_with_id(discarded.clone(), config())
                .await
                .unwrap();

            store.restore(snapshot);
            assert!(store.has_transform(&kept));
            assert!(!store.has_transform(&discarded));
            assert_eq!(
                store
                    .list()
                    .await
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                vec![kept.to_string()]
            );

            store.restore(before);
            assert!(!store.has_transform(&kept));
        })
        .unwrap()
    }
}
