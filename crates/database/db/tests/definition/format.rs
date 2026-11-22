//! A store written by an older index encoding is refused on open.
//!
//! The encoding changed from one varint of nanoseconds to a marker plus
//! separate seconds and nanos varints. Nothing rebuilt those entries before:
//! the existing reindex path is driven by schema migration, not by a storage
//! format change. An upgraded store therefore answered range queries with
//! new-format bounds against old-format entries, so rows went missing, deletes
//! orphaned their entries, and unique indexes stopped rejecting duplicates.

use db::definition::migration::format::CURRENT_INDEX_FORMAT;
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

fn prefixed_index_format_key() -> Vec<u8> {
    [
        b"s".as_slice(),
        db::definition::migration::format::INDEX_FORMAT_KEY,
    ]
    .concat()
}

/// Rebuilds an accepted older encoding and stamps it current, rather than
/// leaving its entries in a layout this build cannot read directly.
#[test]
fn a_rebuildable_older_format_is_stamped_after_rebuild() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = db::DB::open(store.clone()).await.expect("fresh open");
        db.create_collection(simple_schema()).await.expect("schema");
        let txn = db.new_txn(false).await.expect("txn");
        txn.systemstore()
            .expect("systemstore")
            .set(&prefixed_index_format_key(), &3u32.to_be_bytes())
            .await
            .expect("stamp the older format");
        txn.commit().await.expect("commit");
        drop(db);

        let db = db::DB::open(store).await.expect("format 3 rebuilds");
        assert_eq!(
            db.stored_index_format().await.expect("read the stamp"),
            Some(CURRENT_INDEX_FORMAT),
            "the rebuilt store must report the layout this build wrote"
        );
    })
    .unwrap()
}

/// A store with collections but no stamp predates the format marker and is
/// rebuilt, rather than refused.
#[test]
fn an_unstamped_store_with_collections_is_rebuilt() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = db::DB::open(store.clone()).await.expect("fresh open");
        db.create_collection(simple_schema()).await.expect("schema");
        let txn = db.new_txn(false).await.expect("txn");
        txn.systemstore()
            .expect("systemstore")
            .delete(&prefixed_index_format_key())
            .await
            .expect("remove the stamp");
        txn.commit().await.expect("commit");
        drop(db);

        let db = db::DB::open(store).await.expect("old store rebuilds");
        assert_eq!(
            db.stored_index_format().await.expect("read the stamp"),
            Some(CURRENT_INDEX_FORMAT)
        );
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
