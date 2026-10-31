//! KV snapshots in the core (`go.shinzo.kv_snapshot`): what an export
//! reads as its caller, how an import's replay is judged, and the frames.
//! Interop with Go Shinzo is `tools/integration-test/tests/shinzo/kv_snapshot.rs`.

use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use db::database::{DbOptions, DB};
use db::kv_snapshot::frame::{read_entries, write_end, write_entry, Entry};
use db::kv_snapshot::{ExportOptions, KvSnapshotError};
use db::retention::{RetentionFloor, StaticRetentionRule};
use db::AutoCommitMutator;
use defra_core::RequestContext;
use document::{DocID, Document, NormalValue};
use identity::Did;
use query::mutator::DocMutator;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap};
use storage::RegolithStore;

use super::retention::logs;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const OTHER: &str = "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG";

fn db_with(options: DbOptions) -> Arc<DB<RegolithStore>> {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    Arc::new(DB::from_arc_with_options(store, options).unwrap())
}

async fn create(
    db: &Arc<DB<RegolithStore>>,
    schema: &schema::CollectionVersion,
    height: i64,
) -> String {
    let mut doc = Document::new(schema.clone());
    doc.set("address", NormalValue::String(format!("a{height}")))
        .unwrap();
    doc.set("blockNumber", NormalValue::Int(height)).unwrap();
    AutoCommitMutator::new(db.clone())
        .create("Log", doc)
        .await
        .unwrap()
        .doc_id
        .to_string()
}

async fn read(db: &Arc<DB<RegolithStore>>, doc_id: &str) -> Option<Document> {
    let collection = db.get_collection("Log").unwrap().unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let doc = {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        collection
            .get_by_doc_id(
                &datastore,
                &systemstore,
                &DocID::from_string(doc_id).unwrap(),
            )
            .await
            .unwrap()
    };
    txn.force_discard().unwrap();
    doc
}

async fn stored(db: &Arc<DB<RegolithStore>>, doc_id: &str) -> bool {
    read(db, doc_id).await.is_some()
}

struct RefuseAll;

impl db::replication_filter::ReplicationFilter for RefuseAll {
    fn allow_replication(
        &self,
        _collection_id: &str,
        _doc_id: &str,
        _fields: &db::replication_filter::FieldValues,
    ) -> bool {
        false
    }
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.restore")]
fn an_import_restores_what_the_floor_and_the_filter_refuse_from_peers() {
    workers::block_on(async {
        let schema = logs("col-log");
        let source = db_with(DbOptions::new());
        source.create_collection(schema.clone()).await.unwrap();
        let low = create(&source, &schema, 5).await;
        let mut snapshot = Vec::new();
        let exported = source
            .export_kv_snapshot_as(
                &RequestContext::default(),
                "Log",
                &[],
                ExportOptions::default(),
                &mut snapshot,
            )
            .await
            .unwrap();

        let mut floors = RapidHashMap::new();
        floors.insert(
            "col-log".to_string(),
            RetentionFloor {
                field: "blockNumber".to_string(),
                floor: 100,
            },
        );
        let target = db_with(
            DbOptions::new()
                .with_retention_rule(Arc::new(StaticRetentionRule::new(floors)))
                .with_replication_filter(Arc::new(RefuseAll)),
        );
        target.create_collection(schema).await.unwrap();
        let imported = target
            .import_kv_snapshot_as(
                &RequestContext::default(),
                &snapshot[..],
                Some(&exported.mapping),
            )
            .await
            .unwrap();
        assert_eq!(imported.documents, 1);
        assert!(stored(&target, &low).await);
    })
    .unwrap()
}

/// A provider under which every document is registered and only `OWNER`
/// reads one.
struct OwnerReads;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for OwnerReads {
    async fn register_doc_object(&self, _: &Did, _: &str, _: &str, _: &str) -> acp::Result<()> {
        Ok(())
    }
    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> acp::Result<bool> {
        Ok(true)
    }
    async fn check_doc_access(
        &self,
        actor: &Identity,
        _: DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> acp::Result<bool> {
        Ok(actor.did().is_some_and(|did| did.as_str() == OWNER))
    }
    async fn add_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }
    async fn delete_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }
    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> acp::Result<()> {
        Ok(())
    }
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.export")]
fn an_export_leaves_out_documents_its_caller_cannot_read() {
    workers::block_on(async {
        let db = db_with(DbOptions::new());
        let mut schema = logs("col-log");
        schema.policy = Some(schema::PolicyDescription::new("policy", "Log"));
        db.create_collection(schema.clone()).await.unwrap();
        let doc = create(&db, &schema, 1).await;
        db.set_document_acp(Arc::new(OwnerReads));
        let exported = |identity: Option<&str>| {
            let db = db.clone();
            let doc = doc.clone();
            let identity = identity.map(str::to_owned);
            async move {
                let mut snapshot = Vec::new();
                let exported = db
                    .export_kv_snapshot_as(
                        &RequestContext::default().with_identity(identity),
                        "Log",
                        &[doc],
                        ExportOptions::default(),
                        &mut snapshot,
                    )
                    .await
                    .unwrap();
                exported
                    .mapping
                    .documents
                    .into_iter()
                    .map(|entry| entry.doc_id)
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(exported(Some(OWNER)).await, vec![doc.clone()]);
        assert!(exported(Some(OTHER)).await.is_empty());
        assert!(exported(None).await.is_empty());
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.batches")]
fn live_writes_during_an_import_are_kept() {
    workers::block_on(async {
        let schema = logs("col-log");
        let source = db_with(DbOptions::new());
        source.create_collection(schema.clone()).await.unwrap();
        let doc = create(&source, &schema, 1).await;
        let mut snapshot = Vec::new();
        let exported = source
            .export_kv_snapshot_as(
                &RequestContext::default(),
                "Log",
                &[],
                ExportOptions::default(),
                &mut snapshot,
            )
            .await
            .unwrap();

        // The target already holds the document and has updated it since.
        let target = db_with(DbOptions::new());
        target.create_collection(schema.clone()).await.unwrap();
        target
            .import_kv_snapshot_as(
                &RequestContext::default(),
                &snapshot[..],
                Some(&exported.mapping),
            )
            .await
            .unwrap();
        let mut update = read(&target, &doc).await.unwrap();
        update.set("blockNumber", NormalValue::Int(2)).unwrap();
        let mut changed = rapidhash::RapidHashSet::new();
        changed.insert("blockNumber".to_string());
        AutoCommitMutator::new(target.clone())
            .update("Log", update, changed)
            .await
            .unwrap();

        // Importing the older snapshot again keeps the newer write.
        target
            .import_kv_snapshot_as(
                &RequestContext::default(),
                &snapshot[..],
                Some(&exported.mapping),
            )
            .await
            .unwrap();
        let read = read(&target, &doc).await.unwrap();
        assert_eq!(read.get("blockNumber"), Some(&NormalValue::Int(2)));
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.format")]
fn frames_read_back_and_a_cut_frame_is_an_error() {
    let mut bytes = Vec::new();
    write_entry(&mut bytes, b'd', b"/k", b"value").unwrap();
    write_entry(&mut bytes, b'b', b"cid", b"").unwrap();
    write_end(&mut bytes).unwrap();
    bytes.extend_from_slice(b"after the end mark");
    let mut entries = read_entries(&bytes[..]);
    assert_eq!(
        entries.next_entry().unwrap(),
        Some(Entry {
            store: b'd',
            key: b"/k".to_vec(),
            value: b"value".to_vec()
        })
    );
    assert_eq!(entries.next_entry().unwrap().unwrap().store, b'b');
    assert_eq!(entries.next_entry().unwrap(), None);

    // A clean end between frames is a complete stream; a cut frame is not.
    let whole = bytes.len() - b"after the end mark".len() - 4;
    assert!(read_entries(&bytes[..whole])
        .next_entry()
        .unwrap()
        .is_some());
    for cut in [2, 6, 8, 14] {
        let mut entries = read_entries(&bytes[..cut]);
        assert!(
            matches!(entries.next_entry(), Err(KvSnapshotError::Truncated)),
            "cut at {cut}"
        );
    }
}
