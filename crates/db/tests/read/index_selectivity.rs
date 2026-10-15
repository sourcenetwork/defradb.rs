//! A filter naming several indexed fields must scan the most selective index.
//!
//! Every document here shares one `market`, whose index is declared first,
//! while each `hour` value matches a handful. Scored by filter shape alone,
//! the two equality conditions tie and the declaration order picks `market`,
//! so the query assembles the whole collection to return ten documents.
//! [`CountingStore`] counts point lookups below the fetcher, which is where
//! document assembly shows up.

use crate::common::counting_store::CountingStore;
use db::AutoCommitMutator;
use db::LensedAutoCommitFetcher;
use db::DB;
use document::Document;
use document::NormalValue;
use query::DocMutator;
use query::QueryExecutor;
use query::QueryRequest;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;
use schema::IndexDescription;
use schema::IndexedFieldDescription;
use std::sync::Arc;
use storage::RegolithStore;

const COLLECTION_SIZE: usize = 2000;
const HOURS: usize = 200;
const PER_HOUR: usize = COLLECTION_SIZE / HOURS;

fn single_field_index(id: u32, field: &str) -> IndexDescription {
    IndexDescription {
        name: format!("Events_{field}_ASC"),
        id,
        fields: vec![IndexedFieldDescription {
            name: field.to_string(),
            descending: false,
        }],
        unique: false,
        kind: None,
        auto_generated: false,
    }
}

fn schema() -> CollectionVersion {
    let mut version = CollectionVersion::new(
        "Events",
        "events-v1",
        "events-collection",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "market", FieldKind::string()),
            FieldDescription::new("3", "hour", FieldKind::string()),
            FieldDescription::new("4", "seq", FieldKind::int()),
        ],
    );
    version.is_materialized = true;
    // `market` first: on a shape-score tie, declaration order decides.
    version.indexes = vec![
        single_field_index(1, "market"),
        single_field_index(2, "hour"),
    ];
    version
}

async fn seeded_db() -> Arc<DB<CountingStore<RegolithStore>>> {
    let db = Arc::new(DB::new(CountingStore::new(RegolithStore::in_memory().unwrap())).unwrap());
    db.create_collection(schema()).await.unwrap();
    let docs = (0..COLLECTION_SIZE)
        .map(|i| {
            let mut doc = Document::new();
            doc.set("market", NormalValue::String("zcat".to_string()));
            doc.set("hour", NormalValue::String(format!("h{:03}", i % HOURS)));
            // Document IDs are content-addressed; `seq` keeps each one distinct.
            doc.set("seq", NormalValue::Int(i as i64));
            doc
        })
        .collect();
    AutoCommitMutator::new(db.clone())
        .create_many("Events", docs)
        .await
        .unwrap();
    db
}

async fn point_gets_for(db: &Arc<DB<CountingStore<RegolithStore>>>, filter: &str) -> usize {
    let runner = query::QueryRunner::with_provider(
        LensedAutoCommitFetcher::new(db.clone()),
        db::DbCollectionProvider::new_arc(db.clone()),
    );
    let before = db.store().point_gets();
    let response = runner
        .execute(QueryRequest::new(format!(
            "query {{ Events(filter: {filter}) {{ hour }} }}"
        )))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let rows = response.data.as_ref().unwrap()["Events"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        rows.len(),
        PER_HOUR,
        "{filter} must match one hour's documents"
    );
    assert!(rows.iter().all(|row| row["hour"] == "h007"));
    db.store().point_gets() - before
}

#[tokio::test]
async fn equality_on_a_shared_field_does_not_outrank_a_selective_index() {
    let db = seeded_db().await;
    let gets = point_gets_for(&db, r#"{market: {_eq: "zcat"}, hour: {_eq: "h007"}}"#).await;
    assert!(
        gets <= PER_HOUR * 8,
        "a filter matching {PER_HOUR} of {COLLECTION_SIZE} documents performed {gets} point \
         lookups (observed: 63 scanning the hour index, 12003 scanning the market index); the \
         hour index must be scanned, not the market index every document shares"
    );
}

#[tokio::test]
async fn a_range_on_a_selective_field_outranks_equality_on_a_shared_field() {
    let db = seeded_db().await;
    let gets = point_gets_for(
        &db,
        r#"{market: {_eq: "zcat"}, hour: {_ge: "h007", _le: "h007"}}"#,
    )
    .await;
    assert!(
        gets <= PER_HOUR * 8,
        "a range matching {PER_HOUR} of {COLLECTION_SIZE} documents performed {gets} point \
         lookups (observed: 63 scanning the hour index, 12003 scanning the market index); the \
         hour index must be scanned, not the market index every document shares"
    );
}

#[tokio::test]
async fn exclusive_range_estimate_skips_duplicate_boundary_keys() {
    use query::planner::index_selection::{IndexScanParams, IndexScanType};
    use query::DocFetcher;
    use storage::index::Bound;
    let db = seeded_db().await;
    let fetcher = LensedAutoCommitFetcher::new(db.clone());
    let params = IndexScanParams {
        index_name: "Events_market_ASC".into(),
        scan_type: IndexScanType::RangeScan {
            prefix_values: vec![],
            lower: Bound::Exclusive(NormalValue::String("zcat".into())),
            upper: Bound::Unbounded,
            reverse: false,
        },
        limit: None,
        offset: 0,
        value_filter: None,
        cursor_seek: None,
    };
    let before = db.store().keys_read();
    let count = fetcher
        .estimate_index_scan("Events", &params, 1)
        .await
        .unwrap();
    let reads = db.store().keys_read() - before;
    assert_eq!(count, Some(0));
    assert!(reads <= 1, "cap=1 read {reads} index keys");
}

#[tokio::test]
async fn estimation_skips_vector_indexes() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let mut version = schema();
    version.fields.push(FieldDescription::new(
        "5",
        "embedding",
        FieldKind::float64_array(),
    ));
    let mut vector = single_field_index(3, "embedding");
    vector.kind = Some(crate::common::schema::vector_kind());
    version.indexes.push(vector);
    let selects = query::query_parse::parse_query(
        r#"{ Events(filter: {market: {_eq: "zcat"}, embedding: {_any: {_gt: 0.0}}}) { market } }"#,
    )
    .unwrap();
    let selected = query::planner::index_selection::select_best_index(
        selects[0].filter.as_ref().unwrap(),
        &version.indexes,
    )
    .unwrap();
    assert_eq!(selected.name, "Events_market_ASC");
    db.create_collection(version).await.unwrap();
    let runner = query::QueryRunner::with_provider(
        LensedAutoCommitFetcher::new(db.clone()),
        db::DbCollectionProvider::new_arc(db.clone()),
    );
    let response = runner.execute(QueryRequest::new(
        r#"{ Events(filter: {market: {_eq: "zcat"}, embedding: {_any: {_gt: 0.0}}}) { market } }"#,
    )).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[tokio::test]
async fn estimation_work_shrinks_after_a_selective_candidate() {
    use query::planner::index_selection::{
        estimate_filter_indexes, select_best_index_with_estimates, ESTIMATE_CAP,
    };
    let db = seeded_db().await;
    let fetcher = LensedAutoCommitFetcher::new(db.clone());
    let mut collection = schema();
    // Put the selective equality first so it establishes the competing budget.
    collection.indexes.reverse();
    for (filter, max_keys, winner) in [
        (
            r#"{market: {_eq: "zcat"}, hour: {_eq: "h007"}}"#,
            PER_HOUR * 2 + 1,
            Some("Events_hour_ASC"),
        ),
        (
            r#"{market: {_eq: "zcat"}, hour: {_eq: "missing"}}"#,
            1,
            Some("Events_hour_ASC"),
        ),
        (r#"{hour: {_eq: "h007"}}"#, 0, None),
        (
            r#"{market: {_eq: "zcat"}, hour: {_ge: "h000"}}"#,
            ESTIMATE_CAP as usize * 2,
            Some("Events_market_ASC"),
        ),
    ] {
        let selects =
            query::query_parse::parse_query(&format!("{{ Events(filter: {filter}) {{ hour }} }}"))
                .unwrap();
        let filter = selects[0].filter.as_ref().unwrap();
        let before = db.store().keys_read();
        let counts = estimate_filter_indexes(&fetcher, &collection, filter)
            .await
            .unwrap();
        let keys = db.store().keys_read() - before;
        assert!(
            keys <= max_keys,
            "estimate read {keys} keys, budget {max_keys}"
        );
        if let Some(winner) = winner {
            assert_eq!(
                select_best_index_with_estimates(filter, &collection.indexes, &counts)
                    .unwrap()
                    .name,
                winner
            );
        } else {
            assert!(counts.is_empty(), "one candidate needs no estimates");
        }
        eprintln!("estimate keys: {keys} (budget {max_keys})");
    }
}

#[tokio::test]
async fn empty_in_probes_share_the_estimation_budget() {
    use query::planner::index_selection::{IndexScanParams, IndexScanType};
    use query::DocFetcher;
    let db = Arc::new(DB::new(CountingStore::new(RegolithStore::in_memory().unwrap())).unwrap());
    let mut collection = schema();
    let mut index = single_field_index(3, "seq");
    index.unique = true;
    collection.indexes.push(index);
    db.create_collection(collection).await.unwrap();
    let fetcher = LensedAutoCommitFetcher::new(db.clone());
    let mut params = IndexScanParams {
        index_name: "Events_seq_ASC".into(),
        scan_type: IndexScanType::InScan {
            values: (0..2000).map(NormalValue::Int).collect(),
            suffix_values: vec![],
        },
        limit: None,
        offset: 0,
        value_filter: None,
        cursor_seek: None,
    };
    for cap in [0, 1, 10] {
        let before = db.store().point_gets();
        let count = fetcher
            .estimate_index_scan("Events", &params, cap)
            .await
            .unwrap();
        let gets = db.store().point_gets() - before;
        assert_eq!(count, Some(cap));
        assert!(gets <= cap as usize + 3, "cap={cap} performed {gets} reads");
    }
    // The budget must also be shared by nested branches, not reset per branch.
    params.scan_type = IndexScanType::OrScan {
        branches: vec![
            IndexScanType::InScan {
                values: vec![NormalValue::Int(0), NormalValue::Int(1)],
                suffix_values: vec![],
            };
            1000
        ],
    };
    let before = db.store().point_gets();
    assert_eq!(
        fetcher
            .estimate_index_scan("Events", &params, 10)
            .await
            .unwrap(),
        Some(10)
    );
    assert!(db.store().point_gets() - before <= 13);
}
