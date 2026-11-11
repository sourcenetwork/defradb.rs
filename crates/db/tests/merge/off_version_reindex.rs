//! A merge stored off the active version owes its collection a reindex only
//! when the active version is indexed and its history crosses a lens, whether
//! the lens leads into the active version or into an older one.

use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use datastore::{Namespace, NamespaceView, SharedTxn};
use db::merge::DbMergeHandler;
use db::DB;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{Document, NormalValue};
use lens::{LensConfig, LensModule, MemoryTransformStore};
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::{RegolithStore, Store};

const V1: &str = "v1";
const COLLECTION_ID: &str = "col-users";
const CREATOR: &str = "did:key:z6MkrOffVersionReindex";

fn users() -> CollectionVersion {
    let mut version = CollectionVersion::new(
        "Users",
        V1,
        COLLECTION_ID,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    );
    version.indexes = vec![IndexDescription::new("idx_name").with_field("name", false)];
    version
}

async fn add_field(db: &DB<RegolithStore>, field: &str) -> String {
    db.patch_collection(
        &defra_core::RequestContext::default(),
        &format!(
            r#"[{{"op":"add","path":"/Users/Fields/-","value":{{"Name":"{field}","Kind":"Int"}}}}]"#
        ),
        None,
    )
    .await
    .unwrap();
    db.get_collection("Users")
        .unwrap()
        .unwrap()
        .version_id()
        .to_string()
}

async fn migrate(db: &DB<RegolithStore>, source: &str, destination: &str) {
    db.set_migration(
        LensConfig::new(
            source,
            destination,
            LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
        ),
        None,
    )
    .await
    .unwrap();
}

/// Merges one document's block written at `v1` into `db`, and returns
/// whether that merge left the collection owed a reindex.
async fn merge_at_v1_owes(db: Arc<DB<RegolithStore>>, store: Arc<RegolithStore>) -> bool {
    db.settle_owed_reindexes().await.unwrap();
    assert!(db.owed_reindexes().await.unwrap().is_empty());

    let source = RegolithStore::in_memory()
        .unwrap()
        .new_txn(false)
        .await
        .unwrap();
    let shared = SharedTxn::new(source, storage::Crdts::default());
    let source_blocks = NamespaceView::new(shared.clone(), Namespace::Blockstore);
    let source_heads = NamespaceView::new(shared, Namespace::Headstore);
    let mut doc = Document::new(users());
    doc.set("name", NormalValue::String("Ada".into())).unwrap();
    let written = db::block::builder::write_document_blocks(
        &source_blocks,
        &source_heads,
        &doc,
        &users(),
        db::block::builder::DocRef::new(1, 1),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    for cid in std::iter::once(written.cid).chain(written.field_cids.iter().copied()) {
        let bytes = source_blocks.get(&cid.to_bytes()).await.unwrap().unwrap();
        blockstore.put(&cid, &bytes).await.unwrap();
    }
    let handler = DbMergeHandler::new(db.clone(), blockstore);
    let metadata = BlockMetadata::normal(&written.doc_id, COLLECTION_ID, CREATOR, None, false);
    let outcome = handler
        .handle_block(&written.cid, &written.block, metadata)
        .await
        .unwrap();
    assert!(matches!(outcome, MergeOutcome::Merged), "{outcome:?}");
    !db.owed_reindexes().await.unwrap().is_empty()
}

async fn database() -> (Arc<DB<RegolithStore>>, Arc<RegolithStore>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let mut raw = DB::from_arc(store.clone()).unwrap();
    raw.set_lens_store(Arc::new(MemoryTransformStore::new()));
    let db = Arc::new(raw);
    db.create_collection(users()).await.unwrap();
    (db, store)
}

#[test]
fn a_lens_into_the_active_version_owes_a_reindex() {
    workers::block_on(async {
        let (db, store) = database().await;
        let v2 = add_field(&db, "level").await;
        migrate(&db, V1, &v2).await;
        assert!(merge_at_v1_owes(db, store).await);
    })
    .unwrap()
}

#[test]
fn a_lens_behind_an_unlensed_active_version_owes_a_reindex() {
    workers::block_on(async {
        let (db, store) = database().await;
        let v2 = add_field(&db, "level").await;
        migrate(&db, V1, &v2).await;
        let v3 = add_field(&db, "rank").await;
        assert_eq!(
            db.get_collection("Users").unwrap().unwrap().version_id(),
            v3,
            "the unlensed patch is the active version"
        );
        assert!(merge_at_v1_owes(db, store).await);
    })
    .unwrap()
}

#[test]
fn a_history_without_a_lens_owes_nothing() {
    workers::block_on(async {
        let (db, store) = database().await;
        add_field(&db, "level").await;
        assert!(!merge_at_v1_owes(db, store).await);
    })
    .unwrap()
}
