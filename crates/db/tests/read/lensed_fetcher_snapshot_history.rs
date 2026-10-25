//! A lensed fetcher's schema history: a read-only fetcher keeps the history of
//! its own snapshot, and a writable one sees the migrations it registers.

use std::sync::Arc;

use storage::RegolithStore;

use super::LensedDocFetcher;
use crate::database::DB;

#[test]
fn readonly_history_stays_in_its_snapshot_and_new_snapshot_observes_schema_change() {
    workers::block_on(async {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collections_atomic(
        query::parse_sdl("type SnapshotHistory { value: String }").unwrap(),
    )
    .await
    .unwrap();
    let collection = db.get_collection("SnapshotHistory").unwrap().unwrap();
    let old = LensedDocFetcher::new(
        db.clone(),
        db.new_txn(true).await.unwrap(),
        db.lens_store().clone(),
    );
    let first = old
        .load_versions_and_check_migrations(&collection)
        .await
        .unwrap();
    assert_eq!(first.0.len(), 1);
    db.patch_collection("SnapshotHistory", r#"[{"op":"add","path":"/SnapshotHistory/Fields/-","value":{"Name":"extra","Kind":"String"}}]"#, None).await.unwrap();
    let cached = old
        .load_versions_and_check_migrations(&collection)
        .await
        .unwrap();
    assert_eq!(cached.0.len(), 1);
    let fresh = LensedDocFetcher::new(
        db.clone(),
        db.new_txn(true).await.unwrap(),
        db.lens_store().clone(),
    );
    let latest = db.get_collection("SnapshotHistory").unwrap().unwrap();
    assert_eq!(
        fresh
            .load_versions_and_check_migrations(&latest)
            .await
            .unwrap()
            .0
            .len(),
        2
    );
    assert_eq!(
        old.stream_clone()
            .load_versions_and_check_migrations(&collection)
            .await
            .unwrap()
            .0
            .len(),
        1
    );
})
    .unwrap()
}

#[test]
fn writable_history_observes_its_own_migration_registration() {
    workers::block_on(async {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collections_atomic(
        query::parse_sdl("type WritableHistory { value: String }").unwrap(),
    )
    .await
    .unwrap();
    let v1 = db
        .get_collection("WritableHistory")
        .unwrap()
        .unwrap()
        .version_id()
        .to_owned();
    let v2 = db.patch_collection("WritableHistory", r#"[{"op":"add","path":"/WritableHistory/Fields/-","value":{"Name":"extra","Kind":"String"}}]"#, None).await.unwrap().version_id;
    let collection = db.get_collection("WritableHistory").unwrap().unwrap();
    let fetcher = LensedDocFetcher::new(
        db.clone(),
        db.new_txn(false).await.unwrap(),
        db.lens_store().clone(),
    );
    assert!(
        !fetcher
            .load_versions_and_check_migrations(&collection)
            .await
            .unwrap()
            .1
    );
    {
        let txn = fetcher.shared_txn().unwrap();
        db.set_migration_in_txn_with_store(
            &txn,
            fetcher.lens_store.clone(),
            lens::LensConfig::new(
                &v1,
                &v2,
                lens::LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
        )
        .await
        .unwrap();
    }
    assert!(
        fetcher
            .load_versions_and_check_migrations(&collection)
            .await
            .unwrap()
            .1
    );
    let collection_id = &collection.schema().collection_id;
    let generation = fetcher.caches.version.generation();
    assert!(fetcher
        .caches
        .version
        .get(generation, collection_id)
        .is_none());
    let _ = fetcher.take_txn().await.unwrap().discard();
})
    .unwrap()
}
