//! An indexed field holding RFC3339 timestamps is found through its index,
//! whether it stores them as `String` or `DateTime`.
//!
//! The filter literal `"2026-09-29T04:20:21Z"` reads as either. The field's
//! kind, not the literal's shape, decides the seek key: a `String` index
//! stores the string's bytes, so a `Time` key matches nothing.

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

const STAMPS: [&str; 3] = [
    "2026-09-27T00:00:00Z",
    "2026-09-29T04:20:21Z",
    "2026-09-30T12:00:00Z",
];

fn index(id: u32, field: &str) -> IndexDescription {
    IndexDescription {
        name: format!("Stamps_{field}_ASC"),
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

async fn seeded_db() -> Arc<DB<RegolithStore>> {
    let mut version = CollectionVersion::new(
        "Stamps",
        "stamps-v1",
        "stamps-collection",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "owner", FieldKind::string()),
            FieldDescription::new("3", "created_at", FieldKind::string()),
            FieldDescription::new("4", "observed_at", FieldKind::datetime()),
            FieldDescription::new("5", "seen_at", FieldKind::datetime_array()),
        ],
    );
    version.is_materialized = true;
    version.indexes = vec![
        index(1, "owner"),
        index(2, "created_at"),
        index(3, "observed_at"),
        index(4, "seen_at"),
    ];
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(version).await.unwrap();
    let docs = STAMPS
        .iter()
        .map(|stamp| {
            let mut doc = Document::new();
            doc.set("owner", NormalValue::String("alice".to_string()));
            doc.set("created_at", NormalValue::String((*stamp).to_string()));
            let time = chrono::DateTime::parse_from_rfc3339(stamp).unwrap();
            doc.set("observed_at", NormalValue::Time(time));
            doc.set("seen_at", NormalValue::TimeArray(vec![time]));
            doc
        })
        .collect();
    AutoCommitMutator::new(db.clone())
        .create_many("Stamps", docs)
        .await
        .unwrap();
    db
}

async fn stamps_matching(db: &Arc<DB<RegolithStore>>, filter: &str) -> Vec<String> {
    let runner = query::QueryRunner::with_provider(
        LensedAutoCommitFetcher::new(db.clone()),
        db::DbCollectionProvider::new_arc(db.clone()),
    );
    let response = runner
        .execute(QueryRequest::new(format!(
            "query {{ Stamps(filter: {filter}, order: {{created_at: ASC}}) {{ created_at }} }}"
        )))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.as_ref().unwrap()["Stamps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["created_at"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn equality_on_rfc3339_text_uses_the_string_index() {
    let db = seeded_db().await;
    assert_eq!(
        stamps_matching(&db, r#"{created_at: {_eq: "2026-09-29T04:20:21Z"}}"#).await,
        vec![STAMPS[1]]
    );
}

#[tokio::test]
async fn a_range_of_rfc3339_text_uses_the_string_index() {
    let db = seeded_db().await;
    assert_eq!(
        stamps_matching(&db, r#"{created_at: {_ge: "2026-09-28T00:00:00Z"}}"#).await,
        vec![STAMPS[1], STAMPS[2]]
    );
    assert_eq!(
        stamps_matching(
            &db,
            r#"{owner: {_eq: "alice"}, created_at: {_ge: "2026-09-28T00:00:00Z", _lt: "2026-09-30T00:00:00Z"}}"#
        )
        .await,
        vec![STAMPS[1]]
    );
}

#[tokio::test]
async fn a_range_of_rfc3339_text_uses_the_datetime_index() {
    let db = seeded_db().await;
    assert_eq!(
        stamps_matching(&db, r#"{observed_at: {_ge: "2026-09-28T00:00:00Z"}}"#).await,
        vec![STAMPS[1], STAMPS[2]]
    );
    assert_eq!(
        stamps_matching(&db, r#"{observed_at: {_eq: "2026-09-29T04:20:21Z"}}"#).await,
        vec![STAMPS[1]]
    );
}

#[tokio::test]
async fn an_element_of_rfc3339_text_uses_the_datetime_array_index() {
    let db = seeded_db().await;
    assert_eq!(
        stamps_matching(&db, r#"{seen_at: {_any: {_eq: "2026-09-29T04:20:21Z"}}}"#).await,
        vec![STAMPS[1]]
    );
}
