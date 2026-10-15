use crate::common::counting_store::CountingStore;
use async_trait::async_trait;
use db::read::lensed::fetcher::LensedDocFetcher;
use db::DB;
use document::Document;
use document::NormalValue;
use futures::StreamExt;
use kovan_map::HopscotchMap;
use lens::LensConfig;
use lens::LensDocResultStream;
use lens::LensDocStream;
use lens::LensModule;
use lens::TransformId;
use lens::TransformStore;
use query::txn::TransactionRegistry;
use query::DocFetcher;
use query::DocMutator;
use rapidhash::fast::RandomState;
use rapidhash::{HashMapExt, RapidHashMap};
use std::sync::Arc;
use storage::corekv::Store;
use storage::RegolithStore;

const HISTORY_DEPTH: usize = 16;

/// Marks every migrated document `verified`.
struct MarkVerified(HopscotchMap<TransformId, (), RandomState>);

#[async_trait]
impl TransformStore for MarkVerified {
    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        let id = TransformId::new(format!("mark-verified-{}", config.lenses.len()));
        self.0.insert(id.clone(), ());
        Ok(id)
    }

    async fn add_with_id(&self, id: TransformId, _config: LensConfig) -> lens::Result<()> {
        self.0.insert(id, ());
        Ok(())
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, LensModule>> {
        Ok(RapidHashMap::new())
    }

    fn transform(
        &self,
        _id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        Ok(Box::pin(docs.map(|mut doc| {
            doc.insert("verified".to_string(), serde_json::Value::Bool(true));
            Ok(doc)
        })))
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.transform(id, docs)
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.0.contains_key(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        self.0.remove(id);
        Ok(())
    }
}

fn migration(v1: &str, v2: &str) -> LensConfig {
    LensConfig::new(v1, v2, LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()))
}

/// A collection holding one document stored at `v1`, patched to `v2` with a
/// `verified` field. Returns the database and both version ids.
async fn verified_collection<S: Store + 'static>(store: S) -> (Arc<DB<S>>, String, String) {
    let mut raw = DB::new(store).unwrap();
    raw.set_lens_store(Arc::new(MarkVerified(HopscotchMap::with_hasher(
        RandomState::default(),
    ))));
    let db = Arc::new(raw);
    db.create_collections_atomic(query::parse_sdl("type Probe { name: String }").unwrap())
        .await
        .unwrap();
    let v1 = db
        .get_collection("Probe")
        .unwrap()
        .unwrap()
        .version_id()
        .to_owned();
    let mut doc = Document::new();
    doc.set("name", NormalValue::String("Ada".to_string()));
    db::AutoCommitMutator::new(db.clone())
        .create_many("Probe", vec![doc])
        .await
        .unwrap();
    let v2 = db
        .patch_collection(
            "Probe",
            r#"[{"op":"add","path":"/Probe/Fields/-","value":{"Name":"verified","Kind":"Boolean"}}]"#,
            None,
        )
        .await
        .unwrap()
        .version_id;
    (db, v1, v2)
}

async fn deepen_history<S: Store + 'static>(db: &DB<S>) {
    for index in 0..HISTORY_DEPTH {
        db.patch_collection("Probe", &format!(r#"[{{"op":"add","path":"/Probe/Fields/-","value":{{"Name":"extra{index}","Kind":"String"}}}}]"#), None).await.unwrap();
    }
}

fn readonly_fetcher<S: Store + 'static>(db: &Arc<DB<S>>, txn: db::DbTxn<S>) -> LensedDocFetcher<S> {
    LensedDocFetcher::new(db.clone(), txn, db.lens_store().clone(), true)
}

fn verified(docs: &[Document]) -> Option<&NormalValue> {
    assert_eq!(docs.len(), 1);
    docs[0].get("verified")
}

#[tokio::test]
async fn readonly_snapshot_keeps_its_history_while_a_new_snapshot_observes_a_migration() {
    let (db, v1, v2) = verified_collection(RegolithStore::in_memory().unwrap()).await;
    let old = readonly_fetcher(&db, db.new_txn(true).await.unwrap());
    assert_eq!(verified(&old.get_all("Probe").await.unwrap()), None);

    db.set_migration(migration(&v1, &v2), None).await.unwrap();

    assert_eq!(verified(&old.get_all("Probe").await.unwrap()), None);
    let mut stream = old.stream_all_with_deleted("Probe", false).await.unwrap();
    let (streamed, _) = stream.next().await.unwrap().unwrap();
    assert_eq!(streamed.get("verified"), None);
    stream.close().await.unwrap();

    let fresh = readonly_fetcher(&db, db.new_txn(true).await.unwrap());
    assert_eq!(
        verified(&fresh.get_all("Probe").await.unwrap()),
        Some(&NormalValue::Bool(true))
    );
    let _ = old.take_txn().await.unwrap().discard();
    let _ = fresh.take_txn().await.unwrap().discard();
}

#[tokio::test]
async fn writable_transaction_observes_its_own_migration_registration() {
    let (db, v1, v2) = verified_collection(RegolithStore::in_memory().unwrap()).await;
    let registry = db::DbTransactionRegistry::new(db.clone());
    let txn_id = registry.begin(false).await.unwrap();
    let fetcher = registry
        .get(&txn_id)
        .into_result()
        .unwrap()
        .unwrap()
        .doc_fetcher();
    assert_eq!(verified(&fetcher.get_all("Probe").await.unwrap()), None);

    registry
        .set_migration_in_txn(&txn_id, migration(&v1, &v2), None)
        .await
        .unwrap();

    // Transaction-local transforms run in a real WASM store, where the
    // placeholder module fails; the failure shows the read applied it.
    let error = fetcher.get_all("Probe").await.unwrap_err().to_string();
    assert!(
        error.contains("lens migration failed"),
        "the read ignored its transaction's migration: {error}"
    );
    registry.rollback(&txn_id).await.unwrap();
}

#[tokio::test]
async fn writable_reads_reload_schema_history() {
    let (db, _, _) =
        verified_collection(CountingStore::new(RegolithStore::in_memory().unwrap())).await;
    deepen_history(&db).await;
    let fetcher = LensedDocFetcher::new(
        db.clone(),
        db.new_txn(false).await.unwrap(),
        db.lens_store().clone(),
        false,
    );
    fetcher.get_all("Probe").await.unwrap();
    let before = db.store().keys_read();
    fetcher.get_all("Probe").await.unwrap();
    let repeated = db.store().keys_read() - before;
    assert!(
        repeated >= HISTORY_DEPTH,
        "a writable read reused cached schema history: {repeated} iterator keys"
    );
    let _ = fetcher.take_txn().await.unwrap().discard();
}

#[tokio::test]
async fn stream_migrates_documents_without_rescanning_schema_history() {
    let (db, v1, v2) =
        verified_collection(CountingStore::new(RegolithStore::in_memory().unwrap())).await;
    db.set_migration(migration(&v1, &v2), None).await.unwrap();
    deepen_history(&db).await;
    let fetcher = readonly_fetcher(&db, db.new_txn(true).await.unwrap());
    assert_eq!(
        verified(&fetcher.get_all("Probe").await.unwrap()),
        Some(&NormalValue::Bool(true))
    );

    let before = db.store().keys_read();
    let mut stream = fetcher
        .stream_all_with_deleted("Probe", false)
        .await
        .unwrap();
    let (streamed, _) = stream.next().await.unwrap().unwrap();
    assert_eq!(streamed.get("verified"), Some(&NormalValue::Bool(true)));
    assert!(stream.next().await.unwrap().is_none());
    stream.close().await.unwrap();
    let streamed_keys = db.store().keys_read() - before;
    assert!(
        streamed_keys < HISTORY_DEPTH,
        "a stream rescanned schema history while migrating: {streamed_keys} iterator keys"
    );
    let _ = fetcher.take_txn().await.unwrap().discard();
}
