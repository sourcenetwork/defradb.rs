//! A store stamped with format 5 rebuilds its JSON indexes on open, so they
//! gain the entries under empty arrays that format 6 adds, and leaves every
//! other index as it is. An older store still rebuilds every index.

use std::sync::Arc;

use db::definition::migration::format::{CURRENT_INDEX_FORMAT, INDEX_FORMAT_KEY};
use db::AutoCommitMutator;
use document::{Document, NormalValue};
use query::DocMutator;
use storage::corekv::{IterOptions, Iterator as _};
use storage::encoding::json::{JSON_EMPTY_ARRAY, JSON_EMPTY_ARRAY_DESC, JSON_PATH_END};
use storage::keys::IndexDataStoreKey;
use storage::RegolithStore;

const SDL: &str = r#"type Profiles {
    count: Int @index(name: "counts")
    custom: JSON @index(name: "customs")
    descending: JSON @index(name: "descending", direction: DESC)
}"#;

fn is_marker_entry(key: &[u8], marker: u8) -> bool {
    key.windows(3)
        .any(|window| window == [0x01, JSON_PATH_END, marker])
}

fn marker_count(keys: &[Vec<u8>], marker: u8) -> usize {
    keys.iter()
        .filter(|key| is_marker_entry(key, marker))
        .count()
}

async fn index_keys(db: &db::DB<RegolithStore>, prefix: &[u8]) -> Vec<Vec<u8>> {
    let txn = db.new_txn(true).await.unwrap();
    let data = txn.datastore().unwrap();
    let mut iter = data
        .iterator(IterOptions::new().with_prefix(prefix.to_vec()))
        .await
        .unwrap();
    let mut keys = Vec::new();
    while let Some(entry) = iter.next().await.unwrap() {
        keys.push(entry.key.to_vec());
    }
    iter.close().await.unwrap();
    keys
}

#[test]
fn a_format_5_store_rebuilds_only_its_json_indexes() {
    rebuilds_json_indexes_stamped(5, false);
}

#[test]
fn a_format_4_store_rebuilds_every_index() {
    rebuilds_json_indexes_stamped(4, true);
}

/// Opens a store stamped `stored` whose JSON index lacks its empty-array
/// entry and whose count index lost its entries; `every` says whether the
/// count index comes back too.
fn rebuilds_json_indexes_stamped(stored: u32, every: bool) {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = Arc::new(db::DB::open(store.clone()).await.unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), SDL)
            .await
            .unwrap();
        let collection = db.require_collection("Profiles").unwrap();
        let prefix_of = |name: &str| {
            let index = collection
                .schema()
                .indexes
                .iter()
                .find(|index| index.name == name)
                .unwrap();
            IndexDataStoreKey::index_prefix(collection.resolved_root_id(), index.id)
        };
        let (counts, customs, descending) = (
            prefix_of("counts"),
            prefix_of("customs"),
            prefix_of("descending"),
        );
        let mut doc = Document::new(collection.schema().clone());
        doc.set("count", NormalValue::Int(42)).unwrap();
        doc.set(
            "custom",
            NormalValue::Json(serde_json::json!({ "a": [], "b": 1 })),
        )
        .unwrap();
        doc.set(
            "descending",
            NormalValue::Json(serde_json::json!({ "a": [] })),
        )
        .unwrap();
        AutoCommitMutator::new(db.clone())
            .create("Profiles", doc)
            .await
            .unwrap();
        let written = index_keys(&db, &customs).await;
        let written_descending = index_keys(&db, &descending).await;
        assert_eq!(marker_count(&written, JSON_EMPTY_ARRAY), 1);
        assert_eq!(marker_count(&written_descending, JSON_EMPTY_ARRAY_DESC), 1);

        // Leave the store as format `stored` wrote it, without the marker, and drop
        // the count entry to see that its index is not rebuilt.
        let txn = db.new_txn(false).await.unwrap();
        let data = txn.datastore().unwrap();
        for key in written
            .iter()
            .filter(|key| is_marker_entry(key, JSON_EMPTY_ARRAY))
            .chain(
                written_descending
                    .iter()
                    .filter(|key| is_marker_entry(key, JSON_EMPTY_ARRAY_DESC)),
            )
        {
            data.delete(key).await.unwrap();
        }
        for key in index_keys(&db, &counts).await {
            data.delete(&key).await.unwrap();
        }
        txn.systemstore()
            .unwrap()
            .set(INDEX_FORMAT_KEY, &stored.to_be_bytes())
            .await
            .unwrap();
        drop(data);
        txn.commit().await.unwrap();
        drop(collection);
        drop(db);

        let db = db::DB::open(store.clone()).await.unwrap();
        assert_eq!(
            db.stored_index_format().await.unwrap(),
            Some(CURRENT_INDEX_FORMAT)
        );
        let rebuilt = index_keys(&db, &customs).await;
        let rebuilt_descending = index_keys(&db, &descending).await;
        assert_eq!(rebuilt.len(), written.len());
        assert_eq!(marker_count(&rebuilt, JSON_EMPTY_ARRAY), 1);
        assert_eq!(marker_count(&rebuilt_descending, JSON_EMPTY_ARRAY_DESC), 1);
        assert_eq!(index_keys(&db, &counts).await.is_empty(), !every);

        // A current-format store must not send JSON indexes through the rebuild
        // again: remove their markers and check the second open leaves them out.
        let collection = db.require_collection("Profiles").unwrap();
        let mut markers_to_delete = Vec::new();
        for (name, marker) in [
            ("customs", JSON_EMPTY_ARRAY),
            ("descending", JSON_EMPTY_ARRAY_DESC),
        ] {
            let index = collection
                .schema()
                .indexes
                .iter()
                .find(|index| index.name == name)
                .unwrap();
            let prefix = IndexDataStoreKey::index_prefix(collection.resolved_root_id(), index.id);
            markers_to_delete.extend(
                index_keys(&db, &prefix)
                    .await
                    .into_iter()
                    .filter(|key| is_marker_entry(key, marker)),
            );
        }
        let txn = db.new_txn(false).await.unwrap();
        let data = txn.datastore().unwrap();
        for key in markers_to_delete {
            data.delete(&key).await.unwrap();
        }
        drop(data);
        txn.commit().await.unwrap();
        for (name, marker) in [
            ("customs", JSON_EMPTY_ARRAY),
            ("descending", JSON_EMPTY_ARRAY_DESC),
        ] {
            let index = collection
                .schema()
                .indexes
                .iter()
                .find(|index| index.name == name)
                .unwrap();
            let prefix = IndexDataStoreKey::index_prefix(collection.resolved_root_id(), index.id);
            assert_eq!(marker_count(&index_keys(&db, &prefix).await, marker), 0);
        }
        drop(collection);
        drop(db);

        let db = db::DB::open(store).await.unwrap();
        assert_eq!(
            db.stored_index_format().await.unwrap(),
            Some(CURRENT_INDEX_FORMAT)
        );
        let collection = db.require_collection("Profiles").unwrap();
        for (name, marker) in [
            ("customs", JSON_EMPTY_ARRAY),
            ("descending", JSON_EMPTY_ARRAY_DESC),
        ] {
            let index = collection
                .schema()
                .indexes
                .iter()
                .find(|index| index.name == name)
                .unwrap();
            let prefix = IndexDataStoreKey::index_prefix(collection.resolved_root_id(), index.id);
            assert_eq!(marker_count(&index_keys(&db, &prefix).await, marker), 0);
        }
    })
    .unwrap()
}
