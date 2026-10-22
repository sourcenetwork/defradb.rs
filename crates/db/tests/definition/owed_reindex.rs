//! A schema change owes its collection a reindex. The change stands when the
//! rebuild after it fails: the reindex stays owed, durably, until a later
//! settle rebuilds the indexes and clears it.

use std::sync::Arc;

use db::database::DB;
use db::definition::migration::ReindexReport;
use db::DbCollectionProvider;
use document::Document;
use lens::{LensConfig, LensModule};
use query::fetcher::CollectionProvider;
use query::mutator::DocMutator;
use storage::RegolithStore;

use crate::common::refusing_lens_store::RefusingStore;

const ADD_AGE_PATCH: &str =
    r#"[{"op": "add", "path": "/Users/Fields/-", "value": {"Name": "age", "Kind": "Int"}}]"#;

/// A database whose lens store refuses registration, with a `Users` collection
/// declared by `sdl` holding one document.
async fn node(sdl: &str) -> (Arc<DB<RegolithStore>>, Arc<RefusingStore>) {
    let store = RefusingStore::refusing();
    let mut db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    db.set_lens_store(store.clone());
    db.create_collections_atomic(query::parse_sdl(sdl).unwrap())
        .await
        .unwrap();
    let db = Arc::new(db);
    let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
    db::AutoCommitMutator::new(db.clone())
        .create(
            "Users",
            Document::from_json_str(r#"{"name": "Ada"}"#, users).unwrap(),
        )
        .await
        .unwrap();
    (db, store)
}

fn migration() -> LensConfig {
    LensConfig::new("", "", LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()))
}

#[test]
fn a_reindex_that_fails_after_a_patch_stays_owed_until_it_succeeds() {
    workers::block_on(async {
        let (db, store) = node("type Users { name: String @index }").await;
        let patched = db
            .patch_collection_with_migration("Users", ADD_AGE_PATCH, Some(migration()), None)
            .await
            .expect("the patch stands when the reindex after it fails");

        assert_eq!(
            db.owed_reindexes().await.unwrap(),
            vec![patched.collection_id.clone()],
            "the rebuild could not run the migration's lens, so the reindex stays owed"
        );
        assert_eq!(
            db.settle_owed_reindexes().await.unwrap(),
            ReindexReport {
                settled: 0,
                failed: 1
            }
        );

        store.allow();
        assert_eq!(
            db.settle_owed_reindexes().await.unwrap(),
            ReindexReport {
                settled: 1,
                failed: 0
            }
        );
        assert!(db.owed_reindexes().await.unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn a_schema_change_with_nothing_to_rebuild_owes_nothing_once_it_commits() {
    workers::block_on(async {
        let (db, _store) = node("type Users { name: String }").await;
        db.patch_collection_with_migration("Users", ADD_AGE_PATCH, Some(migration()), None)
            .await
            .unwrap();

        assert!(
            db.owed_reindexes().await.unwrap().is_empty(),
            "a collection without indexes has nothing to rebuild"
        );
    })
    .unwrap()
}

#[test]
fn while_a_reindex_is_owed_no_index_answers_queries_and_writers_keep_them() {
    workers::block_on(async {
        let (db, store) = node("type Users { name: String @index }").await;
        db.patch_collection_with_migration("Users", ADD_AGE_PATCH, Some(migration()), None)
            .await
            .unwrap();
        assert_eq!(db.owed_reindexes().await.unwrap().len(), 1);

        let provider = DbCollectionProvider::new(db.clone());
        let planned = provider.get_collection("Users").await.unwrap().unwrap();
        assert!(
            planned.indexes.is_empty(),
            "the planner sees no index while the reindex is owed, so it plans a scan"
        );
        assert_eq!(
            db.get_collection("Users")
                .unwrap()
                .unwrap()
                .get_indexes()
                .len(),
            1
        );

        let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
        db::AutoCommitMutator::new(db.clone())
            .create(
                "Users",
                Document::from_json_str(r#"{"name": "Grace"}"#, users).unwrap(),
            )
            .await
            .expect("an owed reindex changes nothing for writers");

        store.allow();
        db.settle_owed_reindexes().await.unwrap();
        assert!(db.owed_reindexes().await.unwrap().is_empty());
        assert_eq!(
            provider
                .get_collection("Users")
                .await
                .unwrap()
                .unwrap()
                .indexes
                .len(),
            1,
            "the settled reindex gives the planner its index back"
        );
    })
    .unwrap()
}
