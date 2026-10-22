//! The transactional lensed fetcher's caches: shared with its stream clones,
//! and dropped whole by a schema change in the transaction.

use std::sync::Arc;

use lens::TargetedHistoryLink;
use rapidhash::RapidHashMap;
use storage::RegolithStore;

use super::super::LensedDocFetcher;
use crate::collection::Collection;
use crate::DB;

const KEY: &str = "users-collection:v2";

fn users_v2() -> Collection {
    Collection::new(
        schema::CollectionVersion::new(
            "Users",
            "v2",
            "users-collection",
            vec![schema::FieldDescription::new(
                "1",
                "name",
                schema::FieldKind::string(),
                schema::CType::LwwRegister,
            )],
        ),
        crdt::Crdts::default(),
    )
}

/// A history no stored version backs: served only from the cache, so a load
/// that misses the cache fails with "no versions found".
fn seeded_history() -> RapidHashMap<String, TargetedHistoryLink> {
    RapidHashMap::from_iter([(
        "v1".to_string(),
        TargetedHistoryLink::new("v1", "users-collection").with_next("v2"),
    )])
}

async fn fetcher() -> LensedDocFetcher<RegolithStore> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let txn = db.new_txn(false).await.unwrap();
    LensedDocFetcher::new(db, txn, Arc::new(lens::MemoryTransformStore::new()))
}

#[test]
fn a_stream_clone_serves_the_history_its_fetcher_cached() {
    workers::block_on(async {
        let fetcher = fetcher().await;
        fetcher
            .insert_history(KEY.to_string(), seeded_history())
            .await;

        let clone = fetcher.stream_clone();
        let served = clone.load_collection_history(&users_v2()).await.unwrap();

        assert!(served.contains_key("v1"));
        drop(clone);
        fetcher.take_txn().await.unwrap().discard().unwrap();
    })
    .unwrap()
}

#[test]
fn a_schema_change_drops_the_history_for_the_fetcher_and_its_clones() {
    workers::block_on(async {
        let fetcher = fetcher().await;
        fetcher
            .insert_history(KEY.to_string(), seeded_history())
            .await;
        let clone = fetcher.stream_clone();
        assert!(clone.load_collection_history(&users_v2()).await.is_ok());

        fetcher.invalidate_migration_cache();

        for reader in [&fetcher, &clone] {
            let error = reader
                .load_collection_history(&users_v2())
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("no versions found"),
                "a dropped history must be reloaded from storage: {error}"
            );
        }
        drop(clone);
        fetcher.take_txn().await.unwrap().discard().unwrap();
    })
    .unwrap()
}
