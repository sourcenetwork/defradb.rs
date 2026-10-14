use super::*;
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use schema::{DistanceMetric, HnswParams, VectorAlgorithm, VectorIndexDescription};
use std::collections::BTreeSet;
use storage::corekv::{IterOptions, Iterator, Reader};

/// Every datastore key but the id sequences, which only ever grow.
async fn datastore_keys(db: &db::DB<storage::RegolithStore>) -> BTreeSet<Vec<u8>> {
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(IterOptions::new().with_keys_only(true))
        .await
        .unwrap();
    let mut keys = BTreeSet::new();
    while let Some(pair) = iter.next().await.unwrap() {
        if !pair.key.starts_with(b"/seq/") {
            keys.insert(pair.key.to_vec());
        }
    }
    iter.close().await.unwrap();
    let _ = txn.discard();
    keys
}

/// Dropping an index removes its entries from the datastore, where they are
/// written, for an ordered and a vector index alike.
#[tokio::test]
async fn delete_index_removes_its_entries() {
    let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collections_atomic(
        query::parse_sdl("type Note { title: String embedding: [Float32!] }").unwrap(),
    )
    .await
    .unwrap();
    let mutator = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
    for i in 0..5 {
        let mut doc = Document::new();
        doc.set("title", NormalValue::String(format!("n{i}")));
        doc.set(
            "embedding",
            NormalValue::Float32Array(vec![i as f32, 1.0, 0.0]),
        );
        mutator.create("Note", doc).await.unwrap();
    }
    mutator.take_txn().await.unwrap().commit().await.unwrap();
    drop(mutator);

    let adapter = IndexAdapter::new(db.clone());
    let without_indexes = datastore_keys(&db).await;
    let vector = VectorIndexDescription {
        algorithm: VectorAlgorithm::Hnsw,
        metric: DistanceMetric::Cosine,
        dimensions: 3,
        hnsw: Some(HnswParams::default()),
        ivfpq: None,
        ivfflat: None,
        ssg: None,
        diskann: None,
    };
    for (field, name, vector) in [
        ("title", "by_title", None),
        ("embedding", "by_embedding", Some(vector)),
    ] {
        adapter
            .create_index("Note", vec![field.into()], Some(name), false, vector)
            .await
            .unwrap();
        let created = datastore_keys(&db).await;
        assert!(
            created.len() > without_indexes.len(),
            "{name} wrote no entries"
        );
        adapter.delete_index("Note", name).await.unwrap();
        let left = datastore_keys(&db)
            .await
            .difference(&without_indexes)
            .count();
        assert_eq!(left, 0, "{name} left {left} entries");
    }
}
