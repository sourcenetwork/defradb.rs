//! A store written with the old DateTime index encoding is rebuilt on open.
//!
//! The encoding changed from one varint of nanoseconds to a marker plus
//! separate seconds and nanos varints. Nothing rebuilt those entries before:
//! the existing reindex path is driven by schema migration, not by a storage
//! format change. An upgraded store therefore answered range queries with
//! new-format bounds against old-format entries, so rows went missing, deletes
//! orphaned their entries, and unique indexes stopped rejecting duplicates.

use db::definition::migration::format::{CURRENT_INDEX_FORMAT, INDEX_FORMAT_KEY};
use storage::RegolithStore;

/// The marker byte every encoded time key starts with, per `encoding::mod`.
const TIME_MARKER: u8 = 8;

/// The encoding this branch replaces: a single varint of nanoseconds.
fn old_format_key(unix_nanos: i64) -> Vec<u8> {
    storage::encoding::encode_varint_ascending(vec![TIME_MARKER], unix_nanos)
}

/// The two encodings genuinely differ, which is what makes a rebuild necessary.
/// Without this the rest of the test could pass for the wrong reason.
#[test]
fn the_old_and_new_encodings_differ() {
    let value = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap();

    let new = storage::field_value::encode_field_value(
        Vec::new(),
        &document::NormalValue::Time(value),
        false,
    )
    .expect("the new encoding must succeed");
    let old = old_format_key(value.timestamp_nanos_opt().expect("in range"));

    assert_ne!(new, old, "a rebuild is only needed if the bytes differ");
}

/// Opening a store with no format stamp rebuilds and then stamps it, so the
/// second open finds the store current instead of rebuilding again.
#[test]
fn an_unstamped_store_is_stamped_on_open() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();

        let db = db::DB::open(store.clone()).await.expect("first open");
        assert_eq!(
            db.stored_index_format().await.expect("read the stamp"),
            Some(CURRENT_INDEX_FORMAT),
            "opening must stamp the format it wrote"
        );
        drop(db);

        let db = db::DB::open(store).await.expect("second open");
        assert_eq!(
            db.stored_index_format().await.expect("read the stamp"),
            Some(CURRENT_INDEX_FORMAT),
            "the stamp must survive a reopen"
        );
    })
    .unwrap()
}

/// A fresh store carries no documents, so the rebuild costs nothing and the
/// stamp still lands.
#[test]
fn a_fresh_store_opens_clean() {
    workers::block_on(async {
        let db = db::DB::open(RegolithStore::in_memory().unwrap())
            .await
            .expect("open");
        assert_eq!(
            db.stored_index_format().await.unwrap(),
            Some(CURRENT_INDEX_FORMAT)
        );
    })
    .unwrap()
}

/// The concrete failure a rebuild prevents: an entry written with the old
/// encoding falls outside the bounds a range query builds with the new one, so
/// the row is invisible rather than merely slow to find.
#[test]
fn an_old_format_entry_falls_outside_new_format_range_bounds() {
    let key_for = |rfc3339: &str| {
        let value = chrono::DateTime::parse_from_rfc3339(rfc3339).unwrap();
        let new = storage::field_value::encode_field_value(
            Vec::new(),
            &document::NormalValue::Time(value),
            false,
        )
        .expect("encodes");
        let old = old_format_key(value.timestamp_nanos_opt().expect("in range"));
        (new, old)
    };

    let (lower, _) = key_for("2024-01-01T00:00:00Z");
    let (upper, _) = key_for("2024-12-31T23:59:59Z");
    let (target_new, target_old) = key_for("2024-06-15T12:00:00Z");

    assert!(
        lower <= target_new && target_new <= upper,
        "the new-format entry is inside the window, as it must be"
    );
    assert!(
        target_old < lower || upper < target_old,
        "the old-format entry for the same instant is outside the window, so a \
         scan over these bounds never returns it"
    );
}

/// A store stamped before the index keyspace tag and descending JSON paths is
/// rebuilt to the current format in one pass.
#[test]
fn a_format_three_descending_json_index_is_rebuilt_on_open() {
    rebuilds_a_descending_json_index_stamped(3);
}

/// A store stamped by the index keyspace tag is rebuilt for descending JSON paths.
#[test]
fn a_format_four_descending_json_index_is_rebuilt_on_open() {
    rebuilds_a_descending_json_index_stamped(4);
}

/// A descending JSON index stamped `stored` must be rebuilt before a path range
/// query uses current bounds. Removing the entries models keys the new scan
/// cannot read.
fn rebuilds_a_descending_json_index_stamped(stored: u32) {
    use std::sync::Arc;

    use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
    use document::Document;
    use query::mutator::DocMutator;
    use query::{QueryExecutor, QueryRequest, QueryRunner};
    use storage::corekv::{IterOptions, Iterator as _};
    use storage::keys::IndexDataStoreKey;

    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = Arc::new(DB::open(store.clone()).await.unwrap());
        db.add_collection(
            &defra_core::RequestContext::ambient(),
            "type User { name: String, custom: JSON @index(direction: DESC) }",
        )
        .await
        .unwrap();
        let collection = db.require_collection("User").unwrap();
        let schema = collection.schema_arc();
        let document =
            Document::from_json_str(r#"{"name":"John","custom":{"height":180}}"#, schema.clone())
                .unwrap();
        AutoCommitMutator::new(db.clone())
            .create("User", document)
            .await
            .unwrap();
        let index_id = schema.indexes[0].id;
        let prefix = IndexDataStoreKey::index_prefix(collection.resolved_root_id(), index_id);
        let txn = db.new_txn(false).await.unwrap();
        let datastore = txn.datastore().unwrap();
        let mut iter = datastore
            .iterator(IterOptions::new().with_prefix(prefix).with_keys_only(true))
            .await
            .unwrap();
        let mut keys = Vec::new();
        while let Some(pair) = iter.next().await.unwrap() {
            keys.push(pair.key);
        }
        iter.close().await.unwrap();
        drop(iter);
        assert!(!keys.is_empty(), "the document has index entries");
        for key in keys {
            datastore.delete(&key).await.unwrap();
        }
        drop(datastore);
        txn.systemstore()
            .unwrap()
            .set(INDEX_FORMAT_KEY, &stored.to_be_bytes())
            .await
            .unwrap();
        txn.commit().await.unwrap();
        drop(db);

        let reopened = Arc::new(DB::open(store).await.unwrap());
        let schema = reopened
            .require_collection("User")
            .unwrap()
            .schema()
            .clone();
        let runner = QueryRunner::new(LensedAutoCommitFetcher::new(reopened.clone()), vec![schema]);
        let result = runner
            .execute(QueryRequest::new(
                "query { User(filter: {custom: {height: {_gt: 170}}}) { name } }",
            ))
            .await;
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(
            result.data.unwrap(),
            serde_json::json!({"User": [{"name": "John"}]})
        );
        assert_eq!(CURRENT_INDEX_FORMAT, 5);
        assert_eq!(reopened.stored_index_format().await.unwrap(), Some(5));
    })
    .unwrap();
}
