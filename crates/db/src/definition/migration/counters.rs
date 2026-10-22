//! One-time move of counter accumulators into their document values.
//!
//! Before counters became parts of the document value, each counter also kept
//! a value of its own at `/data/{version}/{doc}/{field}`, and the document
//! value held a mirror of it. An increment-only counter took the larger of the
//! two on every touch (the P-counter max reconcile), because increments written
//! before #1021 reached only the mirror; a counter that allows decrements
//! trusted its accumulator. This migration applies that rule once per counter,
//! writes the result into the document value, and deletes the accumulator, so
//! the document value is the counter's only stored copy.
//!
//! It runs at open, in bounded transactions that each hold one page of keys,
//! and records its completion so a later open skips it with one point read.
//! A batch that does not commit leaves its accumulators in place, so a run that
//! stops partway resumes where it stopped; a counter is never moved twice.

use rapidhash::RapidHashMap;

use datastore::NamespaceView;
use document::stored::{StoredDocument, StoredField, ValueKind};
use document::{Document, NormalValue};
use storage::corekv::{IterOptions, Store};

use crate::collection::Collection;
use crate::database::DB;
use crate::error::{Error, Result};

const COUNTER_MIGRATION_MARKER: &[u8] = b"/migration/counter-values/v1";
const ACCUMULATORS: &[u8] = b"/data/";
/// Keys one batch may visit. Every `/data/` key counts, moved or not.
const BATCH_KEYS: usize = 512;
/// Document bytes one batch may rewrite.
const BATCH_BYTES: usize = 4 << 20;

/// What one batch decided for one accumulator.
enum Move {
    /// Write the document value and delete the accumulator.
    Write { doc_key: Vec<u8>, value: Vec<u8> },
    /// Nothing to write: the accumulator is stale, orphaned or agrees.
    Drop,
}

impl<S> DB<S>
where
    S: Store,
{
    /// Move every counter accumulator into its document value, once.
    pub(crate) async fn maybe_migrate_counter_accumulators(&self) -> Result<()> {
        {
            let txn = self.new_txn(true).await?;
            let done = txn
                .systemstore()?
                .has(COUNTER_MIGRATION_MARKER)
                .await
                .map_err(Error::Storage)?;
            txn.discard()?;
            if done {
                return Ok(());
            }
        }
        let mut after: Option<Vec<u8>> = None;
        let mut moved = 0usize;
        loop {
            let txn = self.new_txn(false).await?;
            let (finished, last, count) = {
                let datastore = txn.datastore()?;
                let systemstore = txn.systemstore()?;
                let batch = migrate_batch(&datastore, &systemstore, after.as_deref()).await?;
                if batch.finished {
                    systemstore
                        .set(COUNTER_MIGRATION_MARKER, &[])
                        .await
                        .map_err(Error::Storage)?;
                }
                (batch.finished, batch.last, batch.moved)
            };
            txn.commit().await?;
            moved += count;
            if finished {
                break;
            }
            after = last;
        }
        if moved > 0 {
            tracing::info!(
                counters = moved,
                "moved counter values into their documents"
            );
        }
        Ok(())
    }
}

struct Batch {
    finished: bool,
    last: Option<Vec<u8>>,
    moved: usize,
}

async fn migrate_batch(
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    after: Option<&[u8]>,
) -> Result<Batch> {
    let mut options = IterOptions::new()
        .with_prefix(ACCUMULATORS.to_vec())
        .with_keys_only(true);
    if let Some(after) = after {
        options = options.with_start([after, &[0]].concat());
    }
    let mut iter = datastore.iterator(options).await.map_err(Error::Storage)?;
    // Collected, then written once the iterator is closed: a write under an
    // open scan of the same range is not what the scan should see.
    let mut keys = Vec::new();
    let mut finished = true;
    while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
        keys.push(pair.key);
        if keys.len() == BATCH_KEYS {
            finished = false;
            break;
        }
    }
    iter.close().await.map_err(Error::Storage)?;

    let mut versions: RapidHashMap<String, Option<Collection>> = RapidHashMap::default();
    let mut moved = 0;
    let mut written = 0;
    let mut last = None;
    for key in keys {
        if written >= BATCH_BYTES {
            finished = false;
            break;
        }
        if let Some(step) = plan_move(datastore, systemstore, &mut versions, &key).await? {
            if let Move::Write { doc_key, value } = step {
                written += value.len();
                datastore
                    .set(&doc_key, &value)
                    .await
                    .map_err(Error::Storage)?;
            }
            datastore.delete(&key).await.map_err(Error::Storage)?;
            moved += 1;
        }
        last = Some(key);
    }
    Ok(Batch {
        finished,
        last,
        moved,
    })
}

/// The move for the accumulator at `key`, or `None` when the key is not a
/// counter accumulator (an LWW value or its metadata) and stays.
async fn plan_move(
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    versions: &mut RapidHashMap<String, Option<Collection>>,
    key: &[u8],
) -> Result<Option<Move>> {
    let Some((version, doc_id, field_name)) = parse(key) else {
        return Ok(None);
    };
    if !versions.contains_key(version) {
        let collection =
            crate::definition::loader::get_collection_by_version_id(systemstore, version)
                .await?
                .map(Collection::new);
        versions.insert(version.to_owned(), collection);
    }
    let Some(collection) = versions.get(version).and_then(Option::as_ref) else {
        return Ok(None);
    };
    let Some(field) = collection.schema().field_by_name(field_name) else {
        return Ok(None);
    };
    let Some(kind) = crdt::field::counter_kind(field) else {
        return Ok(None);
    };
    let accumulated = datastore
        .get(key)
        .await
        .map_err(Error::Storage)?
        .and_then(|bytes| decode(kind, &bytes));
    let Some(accumulated) = accumulated else {
        return Ok(Some(Move::Drop));
    };
    let Some(doc_ref) = crate::docid::map::get_doc_ref(systemstore, doc_id).await? else {
        return Ok(Some(Move::Drop));
    };
    let short_id = doc_ref.doc_short_id;
    // An accumulator of a version the document no longer stores was replaced
    // when the document moved on; the stored value already carries it.
    if collection
        .load_version(datastore, short_id)
        .await?
        .as_deref()
        != Some(version)
    {
        return Ok(Some(Move::Drop));
    }
    let doc_key = collection.doc_key(short_id);
    let Some(bytes) = datastore.get(&doc_key).await.map_err(Error::Storage)? else {
        return Ok(Some(Move::Drop));
    };
    let increment_only = !field.crdt_type.allows_decrement();
    let value = match StoredDocument::decode(&bytes) {
        Some(mut stored) => {
            let part = collection
                .part_ids(datastore)
                .await?
                .and_then(|ids| ids.get(field_name).copied())
                .ok_or_else(|| {
                    Error::InvalidDocument(format!("missing part ID for counter {field_name}"))
                })?;
            let current = stored.fields.get(&part).and_then(StoredField::materialize);
            let target = settle(increment_only, accumulated, current.as_ref());
            if current.as_ref() == Some(&target) {
                return Ok(Some(Move::Drop));
            }
            let priority = stored.fields.get(&part).map_or(1, |part| part.priority);
            stored.fields.insert(
                part,
                StoredField::counter(field_name.to_owned(), kind, priority, &target)?,
            );
            stored
                .encode()
                .ok_or_else(|| Error::InvalidDocument("invalid local document value".into()))?
        }
        None => {
            let mut doc = Document::from_storage_cbor(&bytes, collection.schema_arc())?;
            let target = settle(increment_only, accumulated, doc.get(field_name));
            if doc.get(field_name) == Some(&target) {
                return Ok(Some(Move::Drop));
            }
            doc.set_unchecked(field_name.to_owned(), target);
            doc.to_cbor()?
        }
    };
    Ok(Some(Move::Write { doc_key, value }))
}

/// The counter's value after the move. An increment-only counter takes the
/// larger of its accumulator and its document value, as the max reconcile did;
/// a counter that allows decrements trusts its accumulator.
fn settle(
    increment_only: bool,
    accumulated: NormalValue,
    stored: Option<&NormalValue>,
) -> NormalValue {
    match stored {
        Some(stored) if increment_only && greater(stored, &accumulated) => stored.clone(),
        _ => accumulated,
    }
}

fn greater(left: &NormalValue, right: &NormalValue) -> bool {
    if let (Some(left), Some(right)) = (left.as_int(), right.as_int()) {
        return left > right;
    }
    let float = |value: &NormalValue| {
        value
            .as_float64()
            .or_else(|| value.as_float32().map(f64::from))
    };
    matches!((float(left), float(right)), (Some(left), Some(right)) if left > right)
}

/// An accumulator's bytes as a value of `kind`, the way a document value
/// materializes that kind.
fn decode(kind: ValueKind, bytes: &[u8]) -> Option<NormalValue> {
    StoredField {
        name: String::from("counter"),
        kind,
        priority: 0,
        value: bytes.to_vec(),
    }
    .materialize()
}

/// `(version, doc ID, field)` of `/data/{version}/{doc}/{field}`; `None` for any
/// other key under `/data/`.
fn parse(key: &[u8]) -> Option<(&str, &str, &str)> {
    let rest = std::str::from_utf8(key.strip_prefix(ACCUMULATORS)?).ok()?;
    let mut parts = rest.split('/');
    let (version, doc_id, field) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none() && !version.is_empty() && !doc_id.is_empty() && !field.is_empty())
        .then_some((version, doc_id, field))
}
