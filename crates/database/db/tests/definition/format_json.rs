//! A store stamped with format 4 rebuilds its JSON indexes on open, so they
//! gain the entries under empty arrays that format 5 adds, and leaves every
//! other index as it is.

use std::sync::Arc;

use db::definition::migration::format::{CURRENT_INDEX_FORMAT, INDEX_FORMAT_KEY};
use db::AutoCommitMutator;
use document::{Document, NormalValue};
use query::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::corekv::{IterOptions, Iterator as _};
use storage::encoding::json::{JSON_EMPTY_ARRAY, JSON_PATH_END};
use storage::keys::IndexDataStoreKey;
use storage::RegolithStore;

fn schema() -> CollectionVersion {
    CollectionVersion::new(
        "Profiles",
        "profiles-v1",
        "profiles-collection",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "count", FieldKind::int(), schema::CType::LwwRegister),
            FieldDescription::new("3", "custom", FieldKind::json(), schema::CType::LwwRegister),
        ],
    )
    .with_index(IndexDescription::new("counts").with_field("count", false))
    .with_index(IndexDescription::new("customs").with_field("custom", false))
}

fn is_marker_entry(key: &[u8]) -> bool {
    key.windows(3)
        .any(|window| window == [0x01, JSON_PATH_END, JSON_EMPTY_ARRAY])
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
fn a_format_4_store_rebuilds_only_its_json_indexes() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = Arc::new(db::DB::open(store.clone()).await.unwrap());
        db.create_collection(schema()).await.unwrap();
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
        let (counts, customs) = (prefix_of("counts"), prefix_of("customs"));
        let mut doc = Document::new(collection.schema().clone());
        doc.set("count", NormalValue::Int(42)).unwrap();
        doc.set(
            "custom",
            NormalValue::Json(serde_json::json!({ "a": [], "b": 1 })),
        )
        .unwrap();
        AutoCommitMutator::new(db.clone())
            .create("Profiles", doc)
            .await
            .unwrap();
        let written = index_keys(&db, &customs).await;
        assert_eq!(written.iter().filter(|key| is_marker_entry(key)).count(), 1);

        // Leave the store as format 4 wrote it, without the marker, and drop
        // the count entry to see that its index is not rebuilt.
        let txn = db.new_txn(false).await.unwrap();
        let data = txn.datastore().unwrap();
        for key in written.iter().filter(|key| is_marker_entry(key)) {
            data.delete(key).await.unwrap();
        }
        for key in index_keys(&db, &counts).await {
            data.delete(&key).await.unwrap();
        }
        txn.systemstore()
            .unwrap()
            .set(INDEX_FORMAT_KEY, &4u32.to_be_bytes())
            .await
            .unwrap();
        drop(data);
        txn.commit().await.unwrap();
        drop(collection);
        drop(db);

        let db = db::DB::open(store).await.unwrap();
        assert_eq!(
            db.stored_index_format().await.unwrap(),
            Some(CURRENT_INDEX_FORMAT)
        );
        let rebuilt = index_keys(&db, &customs).await;
        assert_eq!(rebuilt.len(), written.len());
        assert_eq!(rebuilt.iter().filter(|key| is_marker_entry(key)).count(), 1);
        assert!(index_keys(&db, &counts).await.is_empty());
    })
    .unwrap()
}
