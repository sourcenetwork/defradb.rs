//! Opening a store upgrades older secondary-index encodings.

use db::definition::migration::format::{CURRENT_INDEX_FORMAT, INDEX_FORMAT_KEY};
use schema::{CollectionVersion, FieldDescription, FieldKind};
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

/// Opening a new store with no format stamp stamps it, so the second open
/// finds the store current.
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

fn simple_schema() -> CollectionVersion {
    CollectionVersion::new(
        "Tallies",
        "tallies-v1",
        "tallies-collection",
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
}

/// Older and unstamped stores are rebuilt and stamped on open.
#[test]
fn older_index_formats_are_upgraded() {
    workers::block_on(async {
        for format in [None, Some(1u32), Some(2), Some(3)] {
            let store = RegolithStore::in_memory().unwrap();
            let db = db::DB::open(store.clone()).await.unwrap();
            db.create_collection(simple_schema()).await.unwrap();
            let txn = db.new_txn(false).await.unwrap();
            let systemstore = txn.systemstore().unwrap();
            match format {
                Some(format) => systemstore
                    .set(
                        db::definition::migration::format::INDEX_FORMAT_KEY,
                        &format.to_be_bytes(),
                    )
                    .await
                    .unwrap(),
                None => systemstore
                    .delete(db::definition::migration::format::INDEX_FORMAT_KEY)
                    .await
                    .unwrap(),
            }
            drop(systemstore);
            txn.commit().await.unwrap();
            drop(db);
            let db = db::DB::open(store).await.unwrap();
            assert_eq!(
                db.stored_index_format().await.unwrap(),
                Some(CURRENT_INDEX_FORMAT)
            );
            assert!(db.require_collection("Tallies").is_ok());
        }
    })
    .unwrap()
}

/// A future format cannot be interpreted by this build.
#[test]
fn future_index_format_is_refused() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = db::DB::open(store.clone()).await.unwrap();
        let txn = db.new_txn(false).await.unwrap();
        txn.systemstore().unwrap().set(db::definition::migration::format::INDEX_FORMAT_KEY, &(CURRENT_INDEX_FORMAT + 1).to_be_bytes()).await.unwrap();
        txn.commit().await.unwrap();
        drop(db);
        assert!(matches!(db::DB::open(store).await, Err(db::Error::UnsupportedIndexFormat { stored: Some(format), .. }) if format == CURRENT_INDEX_FORMAT + 1));
    }).unwrap()
}

/// A rebuild never treats a legacy key under /i/ as a new index entry.
#[test]
fn upgrade_rebuilds_entries_without_deleting_legacy_keys() {
    use db::AutoCommitMutator;
    use document::{Document, NormalValue};
    use query::DocMutator;
    use schema::IndexDescription;
    use std::sync::Arc;
    use storage::corekv::{IterOptions, Iterator as _};
    use storage::keys::IndexDataStoreKey;

    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = Arc::new(db::DB::open(store.clone()).await.unwrap());
        let schema =
            simple_schema().with_index(IndexDescription::new("counts").with_field("count", false));
        db.create_collection(schema.clone()).await.unwrap();
        let collection = db.require_collection("Tallies").unwrap();
        let prefix = IndexDataStoreKey::index_prefix(
            collection.resolved_root_id(),
            collection.schema().indexes[0].id,
        );
        let mut doc = Document::new(schema);
        doc.set("count", NormalValue::Int(42)).unwrap();
        AutoCommitMutator::new(db.clone())
            .create("Tallies", doc)
            .await
            .unwrap();
        let txn = db.new_txn(false).await.unwrap();
        let data = txn.datastore().unwrap();
        let mut iter = data
            .iterator(IterOptions::new().with_prefix(prefix.clone()))
            .await
            .unwrap();
        let entry = iter.next().await.unwrap().unwrap();
        iter.close().await.unwrap();
        drop(iter);
        data.delete(&entry.key).await.unwrap();
        data.set(b"/i/pk/legacy", b"preserve").await.unwrap();
        txn.systemstore()
            .unwrap()
            .set(
                db::definition::migration::format::INDEX_FORMAT_KEY,
                &3u32.to_be_bytes(),
            )
            .await
            .unwrap();
        let system = txn.systemstore().unwrap();
        system.set(b"/reindex/owed/tallies-collection", br#"{"token":17,"migration_destination":null}"#).await.unwrap();
        system.set(b"/reindex/progress/tallies-collection", br#"{"token":17,"stage":{"Walk":{"fence":null,"last":18446744073709551615}},"deferred":[]}"#).await.unwrap();
        drop(system);
        drop(data);
        txn.commit().await.unwrap();
        drop(db);
        let db = db::DB::open(store).await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let data = txn.datastore().unwrap();
        assert_eq!(
            data.get(b"/i/pk/legacy").await.unwrap().unwrap().as_ref(),
            b"preserve"
        );
        let mut iter = data
            .iterator(IterOptions::new().with_prefix(prefix))
            .await
            .unwrap();
        assert!(iter.next().await.unwrap().is_some());
        assert!(iter.next().await.unwrap().is_none());
        iter.close().await.unwrap();
    })
    .unwrap()
}

/// A collection created through a constructor that skips the open sequence is
/// stamped in its creation transaction, so a later open does not mistake it
/// for a legacy unstamped store.
#[test]
fn a_collection_created_without_open_is_reopenable() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = db::DB::new(store.clone()).expect("construct");
        db.create_collection(simple_schema()).await.expect("schema");
        drop(db);

        let db = db::DB::open(store).await.expect("reopen");
        assert_eq!(
            db.stored_index_format().await.expect("read the stamp"),
            Some(CURRENT_INDEX_FORMAT)
        );
        assert!(db.require_collection("Tallies").is_ok());
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
    rebuilds_a_descending_json_index_stamped(3, false);
}

/// A full reindex a format-3 build left part-way through its walk is started
/// over, not resumed past the documents whose keys it never rewrote.
#[test]
fn a_format_three_walk_left_part_way_is_restarted_on_open() {
    rebuilds_a_descending_json_index_stamped(3, true);
}

/// A store stamped by the index keyspace tag is rebuilt for descending JSON paths.
#[test]
fn a_format_four_descending_json_index_is_rebuilt_on_open() {
    rebuilds_a_descending_json_index_stamped(4, false);
}

/// A descending JSON index stamped `stored` must be rebuilt before a path range
/// query uses current bounds. Removing the entries models keys the new scan
/// cannot read. With `stale_walk`, an older build's reindex is owed and its
/// walk cursor already sits past the document.
fn rebuilds_a_descending_json_index_stamped(stored: u32, stale_walk: bool) {
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
        let systemstore = txn.systemstore().unwrap();
        systemstore
            .set(INDEX_FORMAT_KEY, &stored.to_be_bytes())
            .await
            .unwrap();
        if stale_walk {
            let collection_id = collection.collection_id();
            systemstore
                .set(
                    format!("/reindex/owed/{collection_id}").as_bytes(),
                    br#"{"token":7,"migration_destination":null}"#,
                )
                .await
                .unwrap();
            systemstore
                .set(
                    format!("/reindex/progress/{collection_id}").as_bytes(),
                    br#"{"token":7,"stage":{"Walk":{"fence":1000000,"last":1000000}},"deferred":[]}"#,
                )
                .await
                .unwrap();
        }
        drop(systemstore);
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
