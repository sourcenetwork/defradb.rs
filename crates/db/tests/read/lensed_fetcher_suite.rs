use db::read::lensed::fetcher::*;
use document::Document;
use lens::TargetedHistoryLink;
use rapidhash::RapidHashMap;
use serde_json::Value;
use std::sync::Arc;

#[test]
fn test_doc_to_lens_doc_conversion() {
    let mut doc = Document::new();
    doc.set("name", Value::String("Alice".to_string()));
    doc.set("age", Value::Number(30.into()));

    let lens_doc = LensedDocFetcher::<storage::RegolithStore>::doc_to_lens_doc(&doc).unwrap();

    assert_eq!(
        lens_doc.get("name").unwrap(),
        &Value::String("Alice".to_string())
    );
    assert_eq!(lens_doc.get("age").unwrap(), &Value::Number(30.into()));
}

#[tokio::test]
async fn unknown_document_version_passes_through() {
    let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
    let txn = db.new_txn(false).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let fetcher =
        LensedDocFetcher::new(db, txn, Arc::new(lens::MemoryTransformStore::new()), false);

    let collection = db::Collection::new(schema::CollectionVersion::new(
        "Users",
        "v2",
        "users-collection",
        vec![schema::FieldDescription::new(
            "1",
            "name",
            schema::FieldKind::string(),
        )],
    ));
    let history = RapidHashMap::from_iter([
        (
            "v1".to_string(),
            TargetedHistoryLink::new("v1", "users-collection").with_next("v2"),
        ),
        (
            "v2".to_string(),
            TargetedHistoryLink::new("v2", "users-collection")
                .with_transform(Some("transform-v1-v2".to_string()))
                .with_previous("v1"),
        ),
    ]);
    fetcher
        .insert_history("users-collection:v2".to_string(), history)
        .await;

    let mut doc = Document::new();
    doc.set("name", "Alice");
    doc.set_schema_version_id("foreign-v3");

    let returned = fetcher
        .process_document(doc, &collection, &datastore, true)
        .await
        .unwrap();
    assert_eq!(
        returned.get("name").and_then(|value| value.as_str()),
        Some("Alice")
    );
    assert_eq!(returned.schema_version_id(), Some("foreign-v3"));

    drop(datastore);
    fetcher.take_txn().await.unwrap().discard().unwrap();
}

#[tokio::test]
async fn readonly_alias_reads_reuse_schema_history_without_rescanning_storage() {
    use crate::common::counting_store::CountingStore;
    use query::fetcher::DocFetcher;
    let db = Arc::new(
        db::DB::new(CountingStore::new(
            storage::RegolithStore::in_memory().unwrap(),
        ))
        .unwrap(),
    );
    db.create_collections_atomic(query::parse_sdl("type HistoryProbe { value: String }").unwrap())
        .await
        .unwrap();
    for index in 0..16 {
        db.patch_collection("HistoryProbe", &format!(r#"[{{"op":"add","path":"/HistoryProbe/Fields/-","value":{{"Name":"extra{index}","Kind":"String"}}}}]"#), None).await.unwrap();
    }
    let txn = db.new_txn(true).await.unwrap();
    let fetcher = LensedDocFetcher::new(db.clone(), txn, db.lens_store().clone(), true);
    assert!(fetcher.get_all("HistoryProbe").await.unwrap().is_empty());
    let before = db.store().keys_read();
    assert!(fetcher.get_all("HistoryProbe").await.unwrap().is_empty());
    let repeated = db.store().keys_read() - before;
    assert!(
        repeated < 16,
        "a repeated alias rescanned schema history: {repeated} iterator keys"
    );
    let before = db.store().keys_read();
    let mut stream = fetcher
        .stream_all_with_deleted("HistoryProbe", false)
        .await
        .unwrap();
    assert!(stream.next().await.unwrap().is_none());
    stream.close().await.unwrap();
    assert!(
        db.store().keys_read() - before < 16,
        "a stream rescanned schema history"
    );
    let _ = fetcher.take_txn().await.unwrap().discard();
    assert!(
        fetcher.get_all("HistoryProbe").await.is_err(),
        "cached facts must not resurrect a consumed transaction"
    );
}

#[tokio::test]
#[ignore]
async fn schema_history_alias_cost_measurement() {
    use query::mutator::DocMutator;
    use query::QueryExecutor;
    use std::io::Write;
    let output = std::env::var("HISTORY_COST_OUTPUT").unwrap();
    let mut metrics = std::fs::File::create(output).unwrap();
    for versions in [1, 32, 128] {
        for payload_bytes in [64, 1048576] {
            let store = storage::encrypted_store::EncryptedStore::new(
                storage::RegolithStore::in_memory().unwrap(),
                [42; 32],
            );
            let db = Arc::new(db::DB::new(store).unwrap());
            db.create_collections_atomic(
                query::parse_sdl("type AliasCost { ordinal: Int payload: String }").unwrap(),
            )
            .await
            .unwrap();
            for index in 1..versions {
                db.patch_collection("AliasCost", &format!(r#"[{{"op":"add","path":"/AliasCost/Fields/-","value":{{"Name":"extra{index}","Kind":"String"}}}}]"#), None).await.unwrap();
            }
            let mut doc = Document::new();
            doc.set("ordinal", document::NormalValue::Int(1));
            doc.set(
                "payload",
                document::NormalValue::String("x".repeat(payload_bytes)),
            );
            db::AutoCommitMutator::new(db.clone())
                .create_many("AliasCost", vec![doc])
                .await
                .unwrap();
            let registry = Arc::new(db::DbTransactionRegistry::new(db.clone()));
            let runner = query::QueryRunner::with_arc_registry_and_provider(
                db::LensedAutoCommitFetcher::new(db.clone()),
                db::DbCollectionProvider::new_arc(db.clone()),
                registry,
            );
            for aliases in [1, 8, 32] {
                let selections = (0..aliases)
                    .map(|index| format!("a{index}: AliasCost(limit: 1) {{ _docID ordinal }}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let query = format!("{{ {selections} }}");
                let mut elapsed = Vec::new();
                for _ in 0..5 {
                    let start = std::time::Instant::now();
                    let response = runner
                        .execute(query::QueryRequest::new(query.clone()))
                        .await;
                    assert!(response.errors.is_empty(), "{:?}", response.errors);
                    for index in 0..aliases {
                        let rows = response.data.as_ref().unwrap()[format!("a{index}")]
                            .as_array()
                            .unwrap();
                        assert_eq!(rows.len(), 1);
                        assert_eq!(rows[0]["ordinal"], 1);
                    }
                    elapsed.push(start.elapsed().as_micros());
                }
                elapsed.sort();
                writeln!(metrics,"versions={versions} payload_bytes={payload_bytes} rows=1 aliases={aliases} repeats=5 median_us={} min_us={} max_us={}",elapsed[2],elapsed[0],elapsed[4]).unwrap();
                metrics.flush().unwrap();
            }
        }
    }
}
