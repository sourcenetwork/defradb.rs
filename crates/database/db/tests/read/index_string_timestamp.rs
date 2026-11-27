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
use std::sync::Arc;
use storage::RegolithStore;

const STAMPS: [&str; 3] = [
    "2026-09-27T00:00:00Z",
    "2026-09-29T04:20:21Z",
    "2026-09-30T12:00:00Z",
];

async fn seeded_db() -> Arc<DB<RegolithStore>> {
    let sdl = r#"type Stamps {
        owner: String @index(name: "Stamps_owner_ASC")
        created_at: String @index(name: "Stamps_created_at_ASC")
        observed_at: DateTime @index(name: "Stamps_observed_at_ASC")
        seen_at: [DateTime!] @index(name: "Stamps_seen_at_ASC")
    }"#;
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let version = db
        .add_collection(&defra_core::RequestContext::ambient(), sdl)
        .await
        .unwrap()
        .remove(0);
    let docs = STAMPS
        .iter()
        .map(|stamp| {
            let mut doc = Document::new(version.clone());
            doc.set("owner", NormalValue::String("alice".to_string()))
                .unwrap();
            doc.set("created_at", NormalValue::String((*stamp).to_string()))
                .unwrap();
            let time = chrono::DateTime::parse_from_rfc3339(stamp).unwrap();
            doc.set("observed_at", NormalValue::Time(time)).unwrap();
            doc.set("seen_at", NormalValue::TimeArray(vec![time]))
                .unwrap();
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

#[test]
fn equality_on_rfc3339_text_uses_the_string_index() {
    workers::block_on(async {
        let db = seeded_db().await;
        assert_eq!(
            stamps_matching(&db, r#"{created_at: {_eq: "2026-09-29T04:20:21Z"}}"#).await,
            vec![STAMPS[1]]
        );
    })
    .unwrap()
}

#[test]
fn a_range_of_rfc3339_text_uses_the_string_index() {
    workers::block_on(async {
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
})
    .unwrap()
}

#[test]
fn a_range_of_rfc3339_text_uses_the_datetime_index() {
    workers::block_on(async {
        let db = seeded_db().await;
        assert_eq!(
            stamps_matching(&db, r#"{observed_at: {_ge: "2026-09-28T00:00:00Z"}}"#).await,
            vec![STAMPS[1], STAMPS[2]]
        );
        assert_eq!(
            stamps_matching(&db, r#"{observed_at: {_eq: "2026-09-29T04:20:21Z"}}"#).await,
            vec![STAMPS[1]]
        );
    })
    .unwrap()
}

#[test]
fn an_element_of_rfc3339_text_uses_the_datetime_array_index() {
    workers::block_on(async {
        let db = seeded_db().await;
        assert_eq!(
            stamps_matching(&db, r#"{seen_at: {_any: {_eq: "2026-09-29T04:20:21Z"}}}"#).await,
            vec![STAMPS[1]]
        );
    })
    .unwrap()
}
