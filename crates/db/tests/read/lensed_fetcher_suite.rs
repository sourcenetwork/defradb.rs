use db::read::lensed::fetcher::*;
use document::Document;
use lens::TargetedHistoryLink;
use rapidhash::RapidHashMap;
use serde_json::Value;
use std::sync::Arc;

#[test]
fn test_doc_to_lens_doc_conversion() {
    let mut doc = Document::new(crate::common::schema::open_collection(&["name", "age"]));
    doc.set("name", Value::String("Alice".to_string())).unwrap();
    doc.set("age", Value::Number(30.into())).unwrap();

    let lens_doc = LensedDocFetcher::<storage::RegolithStore>::doc_to_lens_doc(&doc).unwrap();

    assert_eq!(
        lens_doc.get("name").unwrap(),
        &Value::String("Alice".to_string())
    );
    assert_eq!(lens_doc.get("age").unwrap(), &Value::Number(30.into()));
}

#[test]
fn unknown_document_version_passes_through() {
    workers::block_on(async {
        let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        let txn = db.new_txn(false).await.unwrap();
        let datastore = txn.datastore().unwrap();
        let fetcher = LensedDocFetcher::new(db, txn, Arc::new(lens::MemoryTransformStore::new()));

        let collection = db::Collection::new(
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
        );
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

        let mut doc = Document::new(collection.schema_arc());
        doc.set("name", "Alice").unwrap();
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
    })
    .unwrap()
}

#[test]
fn readonly_alias_reads_reuse_schema_history_without_rescanning_storage() {
    workers::block_on(async {
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
    let fetcher = LensedDocFetcher::new(db.clone(), txn, db.lens_store().clone());
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
})
    .unwrap()
}

#[test]
#[ignore]
fn schema_history_alias_cost_measurement() {
    workers::block_on(async {
    use query::mutator::DocMutator;
    use query::QueryExecutor;
    use std::io::Write;
    let output = std::env::var("HISTORY_COST_OUTPUT").unwrap();
    let mut metrics = std::fs::File::create(output).unwrap();
    for versions in [1, 32, 128] {
        for payload_bytes in [64, 1048576] {
            let store = storage::RegolithStore::open_with_options(
                "memory",
                storage::RegolithStoreOptions::memory().with_encryption_key([42; 32]),
            )
            .unwrap();
            let db = Arc::new(db::DB::new(store).unwrap());
            db.create_collections_atomic(
                query::parse_sdl("type AliasCost { ordinal: Int payload: String }").unwrap(),
            )
            .await
            .unwrap();
            for index in 1..versions {
                db.patch_collection("AliasCost", &format!(r#"[{{"op":"add","path":"/AliasCost/Fields/-","value":{{"Name":"extra{index}","Kind":"String"}}}}]"#), None).await.unwrap();
            }
            let mut doc = Document::new(db.get_collection("AliasCost").unwrap().unwrap().schema_arc());
            doc.set("ordinal", document::NormalValue::Int(1)).unwrap();
            doc.set(
                "payload",
                document::NormalValue::String("x".repeat(payload_bytes)),
            ).unwrap();
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
})
    .unwrap()
}

/// A document's identity keys (`/d/p/`, `/d/s/`) and its version key (`/v/`).
fn identity_or_version_key(key: &[u8]) -> bool {
    use storage::namespace::Namespace;
    let Some((&namespace, rest)) = key.split_first() else {
        return false;
    };
    (namespace == Namespace::Systemstore.prefix()
        && (rest.starts_with(b"/d/p/") || rest.starts_with(b"/d/s/")))
        || (namespace == Namespace::Datastore.prefix() && rest.starts_with(b"/v/"))
}

/// A projected read by ID reads each document's identity and version once:
/// the check that the projection is exact and the read share them.
#[test]
fn a_projected_read_by_id_reads_each_identity_and_version_once() {
    workers::block_on(async {
        use crate::common::counting_store::CountingStore;
        use query::fetcher::DocFetcher;
        use query::mutator::DocMutator;
        let db = Arc::new(
            db::DB::new(CountingStore::new(
                storage::RegolithStore::in_memory().unwrap(),
            ))
            .unwrap(),
        );
        db.create_collections_atomic(
            query::parse_sdl("type Probe { left: Int right: Int }").unwrap(),
        )
        .await
        .unwrap();
        let mutator = db::AutoCommitMutator::new(db.clone());
        let mut ids = Vec::new();
        for value in 0..3 {
            let created = mutator
                .create(
                    "Probe",
                    Document::from_json_str(
                        &format!(r#"{{"left": {value}, "right": {value}}}"#),
                        db.get_collection("Probe").unwrap().unwrap().schema_arc(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
            ids.push(created.doc_id.to_string());
        }
        let txn = db.new_txn(false).await.unwrap();
        let fetcher = LensedDocFetcher::new(db.clone(), txn, db.lens_store().clone());
        let fields: rapidhash::RapidHashSet<String> = ["left".to_string()].into_iter().collect();

        db.store().take_point_keys();
        let result = fetcher
            .get_by_ids_projected("Probe", &ids, &fields)
            .await
            .unwrap()
            .expect("documents at the current version read projected");
        let keys = db.store().take_point_keys();

        assert_eq!(result.found_count(), 3);
        let mut reads: RapidHashMap<Vec<u8>, usize> = RapidHashMap::default();
        for key in keys.into_iter().filter(|key| identity_or_version_key(key)) {
            *reads.entry(key).or_default() += 1;
        }
        assert_eq!(
            reads.len(),
            9,
            "two identity keys and a version key per document"
        );
        assert!(
            reads.values().all(|&count| count == 1),
            "a key was read more than once: {reads:?}"
        );
        let _ = fetcher.take_txn().await.unwrap().discard();
    })
    .unwrap()
}
