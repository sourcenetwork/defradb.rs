//! Secondary-index keys stay inside their tagged keyspace across short IDs.

use std::collections::BTreeMap;
use std::sync::Arc;

use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use query::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::corekv::{IterOptions, Iterator as _};
use storage::keys::IndexDataStoreKey;
use storage::RegolithStore;

fn schema(number: usize) -> CollectionVersion {
    let name = format!("Tally{number}");
    CollectionVersion::new(
        name.clone(),
        format!("{name}-v1"),
        format!("{name}-collection"),
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "count", FieldKind::int(), schema::CType::LwwRegister),
        ],
    )
    .with_index(IndexDescription::new(format!("{name}_count")).with_field("count", false))
}

fn collection_name(number: usize) -> String {
    format!("Tally{number}")
}

async fn create_tally(db: &Arc<DB<RegolithStore>>, number: usize) {
    let mut doc = Document::new(schema(number));
    doc.set("count", NormalValue::Int(number as i64)).unwrap();
    AutoCommitMutator::new(db.clone())
        .create(&collection_name(number), doc)
        .await
        .unwrap();
}

async fn keys_under(db: &Arc<DB<RegolithStore>>, prefix: &[u8]) -> Vec<Vec<u8>> {
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iterator = datastore
        .iterator(IterOptions::new().with_prefix(prefix.to_vec()))
        .await
        .unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = iterator.next().await.unwrap() {
        keys.push(pair.key);
    }
    iterator.close().await.unwrap();
    keys
}

/// One collection for every writable single-byte short ID, plus the first
/// multi-byte ID: 1..=239 and 240. Collection short ID 0 is invalid.
#[test]
fn index_keyspace_scans_stay_per_collection_across_short_ids() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        for number in 1..=241 {
            db.create_collection(schema(number)).await.unwrap();
        }
        for number in 1..=241 {
            create_tally(&db, number).await;
        }

        let mut short_ids = BTreeMap::new();
        for number in 1..=241 {
            let collection = db.require_collection(&collection_name(number)).unwrap();
            let short_id = collection.resolved_root_id();
            short_ids.insert(short_id, number);
            let index_id = collection.schema().indexes[0].id;

            let index_prefix = IndexDataStoreKey::index_prefix(short_id, index_id);
            let index_keys = keys_under(&db, &index_prefix).await;
            assert_eq!(
                index_keys.len(),
                1,
                "short ID {short_id} (collection {number}) has one index entry: {index_keys:?}"
            );
            assert!(
                index_keys.iter().all(|key| key.starts_with(&index_prefix)),
                "index scan for short ID {short_id} left its keyspace"
            );

            let document_prefix = format!("/d/{}/", collection.collection_id());
            let version_prefix = format!("/v/{}/", collection.collection_id());
            assert_eq!(
                keys_under(&db, document_prefix.as_bytes()).await.len(),
                1,
                "short ID {short_id} has one document key"
            );
            assert_eq!(
                keys_under(&db, version_prefix.as_bytes()).await.len(),
                1,
                "short ID {short_id} has one version key"
            );
            assert!(
                !document_prefix.as_bytes().starts_with(&index_prefix)
                    && !version_prefix.as_bytes().starts_with(&index_prefix),
                "short ID {short_id} index prefix overlaps a document family"
            );
        }

        let expected: Vec<u32> = (1..=241).collect();
        let actual: Vec<u32> = short_ids.keys().copied().collect();
        assert_eq!(actual, expected, "short IDs must cover the byte boundary");
    })
    .unwrap();
}
