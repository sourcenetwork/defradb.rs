use crate::common::schema::parsed;
use std::sync::Arc;

use db::{DbDocMutator, DB};
use document::Document;
use query::mutator::DocMutator;
use schema::{CollectionVersion, IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::RegolithStore;

fn schema_sdl(name: &str) -> String {
    format!("type {name} {{ value: String }}")
}

fn schema(name: &str) -> CollectionVersion {
    parsed(&schema_sdl(name))
}

fn is_txn_conflict(error: db::Error) -> bool {
    matches!(
        error,
        db::Error::Datastore(datastore::Error::Storage(error)) if error.is_txn_conflict()
    )
}

async fn stage_create(db: &Arc<DB<RegolithStore>>, collection: &str) -> db::DbTxn<RegolithStore> {
    let mutator = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
    let mut doc = Document::new(schema(collection));
    doc.set("value", "staged").unwrap();
    mutator.create(collection, doc).await.unwrap();
    mutator.take_txn().await.unwrap()
}

async fn open() -> Arc<DB<RegolithStore>> {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    Arc::new(DB::from_arc(store).unwrap())
}

#[test]
fn document_write_conflicts_with_collection_drop() {
    workers::block_on(async {
        let db = open().await;
        db.add_collection(&defra_core::RequestContext::ambient(), &schema_sdl("race"))
            .await
            .unwrap();
        let writer = stage_create(&db, "race").await;

        db.delete_collection("race").await.unwrap();

        assert!(is_txn_conflict(writer.commit().await.unwrap_err()));
    })
    .unwrap()
}

#[test]
fn document_write_conflicts_with_index_definition() {
    workers::block_on(async {
        let db = open().await;
        db.add_collection(&defra_core::RequestContext::ambient(), &schema_sdl("race"))
            .await
            .unwrap();
        let writer = stage_create(&db, "race").await;

        db.create_index(
            "race",
            Some("by_value"),
            vec![IndexedFieldDescription {
                name: "value".into(),
                descending: false,
            }],
            IndexKind::Ordered(OrderedIndexDescription { unique: false }),
        )
        .await
        .unwrap();

        assert!(is_txn_conflict(writer.commit().await.unwrap_err()));
    })
    .unwrap()
}

#[test]
fn disjoint_collection_writes_both_commit() {
    workers::block_on(async {
        let db = open().await;
        db.add_collection(&defra_core::RequestContext::ambient(), &schema_sdl("left"))
            .await
            .unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), &schema_sdl("right"))
            .await
            .unwrap();
        let left = stage_create(&db, "left").await;
        let right = stage_create(&db, "right").await;

        left.commit().await.unwrap();
        right.commit().await.unwrap();
    })
    .unwrap()
}
