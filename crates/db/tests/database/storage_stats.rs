use std::{collections::BTreeMap, path::Path, sync::Arc};

use db::{database::storage_stats, DbDocMutator, DB};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use storage::{
    backends::RegolithTxn,
    corekv::{IterOptions, Key, Store, Txn, Writer},
    RegolithStore,
};

#[tokio::test]
async fn live_scan_accounts_for_every_byte_without_exposing_values() {
    let db = crate::common::fixture::fixture_with_docs(3).await;
    let stats = db.storage_stats(true).await.unwrap();
    let txn = db.store().new_txn(true).await.unwrap();
    let mut iter = txn.iterator(IterOptions::new()).await.unwrap();
    let mut counts = (0, 0, 0);
    while let Some(pair) = iter.next().await.unwrap() {
        counts.0 += 1;
        counts.1 += pair.key.len() as u64;
        counts.2 += pair.value.len() as u64;
    }
    iter.close().await.unwrap();
    txn.discard();
    assert_eq!(
        (
            stats.total.keys,
            stats.total.key_bytes,
            stats.total.value_bytes
        ),
        counts
    );
    assert_eq!(
        stats.stores.values().map(|store| store.keys).sum::<u64>(),
        stats.total.keys
    );
    let collection = stats
        .collections
        .values()
        .find(|col| col.names == ["Users"])
        .unwrap();
    assert!(collection.datastore.keys > 0);
    assert!(collection.blocks.keys > 0);
    let field = &collection.fields["name"];
    assert_eq!(field.documents, Some(3));
    assert_eq!(field.max_versions_per_document, Some(1));
    let json = serde_json::to_string(&stats).unwrap();
    for value in ["user-0", "user-1", "user-2"] {
        assert!(!json.contains(value));
    }
}

async fn stats_schema(txn: &mut dyn Txn, short_id: u32, indexes: Vec<schema::IndexDescription>) {
    let mut schema = crate::common::schema::test_schema();
    schema.collection_id = format!("col-{short_id}");
    schema.version_id = format!("version-{short_id}");
    schema.name = format!("Collection{short_id}");
    schema.root_id = short_id;
    schema.indexes = indexes;
    schema.fields.push(schema::FieldDescription::new(
        "3",
        "number",
        schema::FieldKind::int(),
    ));
    schema.fields.push(schema::FieldDescription::new(
        "4",
        "numbers",
        schema::FieldKind::int_array(),
    ));
    txn.set(
        format!("s/collection/id/{}", schema.version_id).as_bytes(),
        &serde_json::to_vec(&schema).unwrap(),
    )
    .await
    .unwrap();
    txn.set(
        format!("s/collection/shortID/{}", schema.collection_id).as_bytes(),
        short_id.to_string().as_bytes(),
    )
    .await
    .unwrap();
}

async fn stats_entries(txn: &mut dyn Txn, keys: Vec<Vec<u8>>) -> storage_stats::ByteCounts {
    let mut counts = storage_stats::ByteCounts::default();
    for key in keys {
        let key = [b"d".as_slice(), &key].concat();
        let value = b"private payload";
        txn.set(&key, value).await.unwrap();
        counts.keys += 1;
        counts.key_bytes += key.len() as u64;
        counts.value_bytes += value.len() as u64;
    }
    counts
}

#[tokio::test]
async fn vector_and_ordinary_keys_with_overlapping_encodings_are_attributed() {
    use storage::keys::datastore::{VectorAuxKey, VectorIndexKey};
    use storage::keys::{DataStoreKey, IndexDataStoreKey, IndexedField, InstanceType};

    let store = RegolithStore::in_memory().unwrap();
    let mut txn = store.new_txn(false).await.unwrap();
    let mut vector = schema::IndexDescription::new("vector");
    vector.id = 1;
    vector.kind = Some(crate::common::schema::vector_kind());
    let mut ordered = schema::IndexDescription::new("ordered").with_field("number", false);
    ordered.id = 137;
    let mut vector_two = vector.clone();
    vector_two.id = 2;
    vector_two.name = "vector_two".into();
    let mut strings = schema::IndexDescription::new("strings").with_field("name", true);
    strings.id = 140;
    let mut no_vector = ordered.clone();
    no_vector.id = 139;
    no_vector.name = "no_vector".into();
    stats_schema(txn.as_mut(), 1, vec![vector.clone(), vector_two]).await;
    stats_schema(txn.as_mut(), 137, vec![ordered, strings, no_vector, vector]).await;

    // The collection prefixes collide, but these index prefixes do not.
    let first = stats_entries(
        txn.as_mut(),
        vec![
            VectorIndexKey::node(1, 2, 1, 300).bytes(),
            VectorIndexKey::meta(1, 2, 300).bytes(),
            VectorAuxKey::new(1, 2, 1, b's', b"").bytes(),
            VectorAuxKey::new(1, 2, 300, b'c', b"\0/centroid").bytes(),
            VectorAuxKey::new(1, 2, 1, 0xff, b"future-kind").bytes(),
        ],
    )
    .await;
    let second = stats_entries(
        txn.as_mut(),
        vec![
            DataStoreKey::new(137, InstanceType::Value, 1, "2").bytes(),
            DataStoreKey::new(137, InstanceType::Priority, 1, "2").bytes(),
            DataStoreKey::new(137, InstanceType::Deleted, 1, "2").bytes(),
            IndexDataStoreKey::with_doc_short_id(
                137,
                140,
                vec![IndexedField::descending(NormalValue::String(
                    "/m/private".into(),
                ))],
                300,
            )
            .bytes(),
            IndexDataStoreKey::with_doc_short_id(
                137,
                139,
                vec![IndexedField::ascending(NormalValue::Int(1))],
                u64::from(b'm'),
            )
            .bytes(),
            VectorIndexKey::node(137, 1, 1, 1).bytes(),
        ],
    )
    .await;
    let ambiguous = stats_entries(
        txn.as_mut(),
        vec![
            VectorIndexKey::node(1, 1, 1, 300).bytes(),
            VectorAuxKey::new(1, 1, 300, 0xff, b"future-kind").bytes(),
            IndexDataStoreKey::with_doc_short_id(
                137,
                137,
                vec![IndexedField::ascending(NormalValue::Int(1))],
                42,
            )
            .bytes(),
        ],
    )
    .await;
    txn.commit().await.unwrap();
    let txn = store.new_txn(true).await.unwrap();
    let stats = storage_stats::collect(txn.as_ref(), false).await.unwrap();
    assert_eq!(stats.collections["col-1"].datastore, first);
    assert_eq!(stats.collections["col-137"].datastore, second);
    assert_eq!(stats.unattributed_datastore, ambiguous);
    assert!(!serde_json::to_string(&stats).unwrap().contains("private"));
}

#[tokio::test]
async fn ambiguous_scalar_array_and_unique_override_indexes_are_not_guessed() {
    use storage::keys::datastore::VectorIndexKey;
    use storage::keys::{IndexDataStoreKey, IndexedField};

    for (field, legacy_unique) in [("number", false), ("numbers", false), ("number", true)] {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        let mut vector = schema::IndexDescription::new("vector");
        vector.id = 1;
        vector.kind = Some(crate::common::schema::vector_kind());
        let mut ordered = schema::IndexDescription::new("ordered").with_field(field, false);
        ordered.id = 137;
        ordered.unique = legacy_unique;
        ordered.kind = Some(schema::IndexKind::Ordered(
            schema::OrderedIndexDescription { unique: false },
        ));
        assert!(!ordered.resolved_unique());
        stats_schema(txn.as_mut(), 1, vec![vector]).await;
        stats_schema(txn.as_mut(), 137, vec![ordered]).await;
        let key = VectorIndexKey::meta(1, 1, 1).bytes();
        assert_eq!(
            key,
            IndexDataStoreKey::with_doc_short_id(
                137,
                137,
                vec![IndexedField::ascending(NormalValue::Int(1))],
                u64::from(b'm'),
            )
            .bytes()
        );
        let expected = stats_entries(txn.as_mut(), vec![key]).await;
        txn.commit().await.unwrap();
        let txn = store.new_txn(true).await.unwrap();
        let stats = storage_stats::collect(txn.as_ref(), false).await.unwrap();
        assert_eq!(stats.unattributed_datastore, expected);
        assert_eq!(stats.stores["datastore"], expected);
        assert_eq!(stats.collections["col-1"].datastore.keys, 0);
        assert_eq!(stats.collections["col-137"].datastore.keys, 0);
    }
}

#[tokio::test]
async fn searchable_encryption_and_cached_views_resolve_collections() {
    use storage::keys::datastore::ViewCacheKey;
    use storage::keys::DatastoreSE;

    let store = RegolithStore::in_memory().unwrap();
    let mut txn = store.new_txn(false).await.unwrap();
    stats_schema(txn.as_mut(), 1, vec![]).await;
    stats_schema(txn.as_mut(), 137, vec![]).await;
    let first = stats_entries(
        txn.as_mut(),
        vec![
            DatastoreSE::new("col-1", "name", vec![1, 2], "private-doc").bytes(),
            ViewCacheKey::new(1, 0).bytes(),
        ],
    )
    .await;
    let second = stats_entries(
        txn.as_mut(),
        vec![
            DatastoreSE::new("col-137", "name", vec![3, 4], "private-doc").bytes(),
            ViewCacheKey::new(137, 300).bytes(),
        ],
    )
    .await;
    let unknown = stats_entries(
        txn.as_mut(),
        vec![
            DatastoreSE::new("missing", "name", vec![], "doc").bytes(),
            ViewCacheKey::new(999, 1).bytes(),
        ],
    )
    .await;
    txn.commit().await.unwrap();
    let txn = store.new_txn(true).await.unwrap();
    let stats = storage_stats::collect(txn.as_ref(), false).await.unwrap();
    assert_eq!(stats.collections["col-1"].datastore, first);
    assert_eq!(stats.collections["col-137"].datastore, second);
    assert_eq!(stats.unattributed_datastore, unknown);
    assert!(!serde_json::to_string(&stats).unwrap().contains("private"));
}

fn files(path: &Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            result.extend(files(&entry.path()));
        } else {
            result.insert(entry.path(), std::fs::read(entry.path()).unwrap());
        }
    }
    result
}

#[tokio::test]
async fn offline_scan_is_read_only_and_matches_the_live_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data");
    let db = Arc::new(DB::new(RegolithStore::open(&path).unwrap()).unwrap());
    db.create_collection(crate::common::schema::test_schema())
        .await
        .unwrap();
    let mutator = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
    let mut doc = Document::new();
    doc.set("name", NormalValue::String("private value".into()));
    mutator.create("Users", doc).await.unwrap();
    mutator.take_txn().await.unwrap().commit().await.unwrap();
    drop(mutator);
    let live = db.storage_stats(true).await.unwrap();
    db.close().await.unwrap();
    drop(db);
    let before = files(&path);
    let mut reader = RegolithTxn::open_read_only(&path).unwrap();
    assert!(reader.set(b"forbidden", b"write").await.is_err());
    let offline = storage_stats::collect(&reader, true).await.unwrap();
    assert_eq!(
        serde_json::to_value(offline).unwrap(),
        serde_json::to_value(live).unwrap()
    );
    drop(reader);
    assert_eq!(files(&path), before);
}

#[test]
fn opening_missing_store_does_not_create_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing");
    assert!(RegolithTxn::open_read_only(&path).is_err());
    assert!(!path.exists());
}

#[tokio::test]
async fn unknown_keys_remain_in_the_totals() {
    let store = RegolithStore::in_memory().unwrap();
    let mut txn = store.new_txn(false).await.unwrap();
    for key in [b"b/merge-marker".as_slice(), b"d/unknown", b"x/unknown"] {
        txn.set(key, b"private data").await.unwrap();
    }
    txn.commit().await.unwrap();
    let txn = store.new_txn(true).await.unwrap();
    let stats = storage_stats::collect(txn.as_ref(), false).await.unwrap();
    assert_eq!(stats.total.keys, 3);
    assert_eq!(stats.unattributed_blocks.keys, 1);
    assert_eq!(stats.unattributed_datastore.keys, 1);
    assert_eq!(stats.stores["other"].keys, 1);
    assert!(!serde_json::to_string(&stats)
        .unwrap()
        .contains("private data"));
}

#[tokio::test]
async fn malformed_schema_is_reported_without_its_contents() {
    let store = RegolithStore::in_memory().unwrap();
    let mut txn = store.new_txn(false).await.unwrap();
    txn.set(b"s/collection/id/broken", b"private invalid schema")
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let txn = store.new_txn(true).await.unwrap();
    let error = storage_stats::collect(txn.as_ref(), false)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("invalid stored collection schema"));
    assert!(!error.to_string().contains("private invalid schema"));
}

#[tokio::test]
async fn shared_blocks_count_each_document_history_once() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let mut schema = crate::common::schema::test_schema();
    schema.fields.push(schema::FieldDescription::new(
        "3",
        "other",
        schema::FieldKind::string(),
    ));
    db.create_collection(schema).await.unwrap();
    let mut ids = Vec::new();
    for other in ["first", "second"] {
        let mutator = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        let mut doc = Document::new();
        doc.set("name", NormalValue::String("shared".into()));
        doc.set("other", NormalValue::String(other.into()));
        ids.push(mutator.create("Users", doc).await.unwrap().doc_id);
        mutator.take_txn().await.unwrap().commit().await.unwrap();
    }
    let stats = db.storage_stats(true).await.unwrap();
    let field = &stats.collections["col-users"].fields["name"];
    assert_eq!(field.blocks.keys, 1);
    assert_eq!(field.documents, Some(2));
    assert_eq!(field.versions, Some(2));

    let mutator = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
    let mut doc = Document::new();
    doc.set_id(ids[0].clone());
    doc.set("name", NormalValue::String("changed".into()));
    mutator
        .update("Users", doc, ["name".to_string()].into_iter().collect())
        .await
        .unwrap();
    mutator.take_txn().await.unwrap().commit().await.unwrap();
    let stats = db.storage_stats(true).await.unwrap();
    let field = &stats.collections["col-users"].fields["name"];
    assert_eq!(field.blocks.keys, 2);
    assert_eq!(field.documents, Some(2));
    assert_eq!(field.versions, Some(3));
    assert_eq!(field.max_versions_per_document, Some(2));
    let stats = db.storage_stats(false).await.unwrap();
    assert_eq!(stats.collections["col-users"].fields["name"].versions, None);
}
