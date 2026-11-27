//! One-time move of head sets from the marker layout to one merged value per
//! set.
//!
//! Before head sets were merged values, a document's heads were keys
//! (`/d/{doc}/{field}/{cid}`) and a parent was superseded by a marker a child
//! wrote (`/ds/{doc}/{field}/{parent}/{child}`); a collection's were `/c/` and
//! `/cs/`. This reads that layout and writes the same live heads as head-set
//! merges (`block::heads`), then deletes the old keys, in five phases of
//! bounded batches: document markers, collection markers, document heads,
//! collection heads, document publication guards. Markers go first so that
//! every head still stored when the head phases run is live.
//!
//! A marker whose parent head is stored supersedes it: the head goes, and a
//! publication guard records that it was published. A marker whose parent
//! head is gone is dropped when the parent was published (its guard, or for a
//! collection a merged block, says so) and held ahead otherwise, to be
//! consumed when its parent arrives. A document head's publication is
//! recorded by its priority index entry (`block::heads::document::publish`),
//! which every publication writes and the priority index backfill fills in,
//! so the document guards (`/r/`), this run's and any an earlier one left, go
//! last. Each batch is one transaction that writes the new set values and
//! deletes the old keys it read, so a run that stops partway resumes where the
//! deletes ended, and a second run finds nothing. A stamp makes later opens
//! one point read; a store stamped by the run before the guards phase existed
//! runs again, and only that phase finds keys.

use std::collections::BTreeMap;

use cid::Cid;
use datastore::NamespaceView;
use storage::corekv::{IterOptions, Key, KvPair, Store};
use storage::keys::doc_id_index::decode_doc_short_id_prefix;
use storage::keys::headstore::{HeadstoreColPublishedKey, HeadstoreColSetKey, HeadstoreDocKey};
use storage::keys::ToMergeIndexKey;
use storage::merge::head_set::Write;

use crate::block::builder::decode_priority_varint;
use crate::block::heads::document;
use crate::database::DB;
use crate::error::{Error, Result};

const MIGRATION_MARKER: &[u8] = b"/migration/head-sets/v2";
/// The stamp of the run before the guards phase, removed with the new stamp.
const EARLIER_MARKER: &[u8] = b"/migration/head-sets/v1";
/// Keys one batch may read.
const BATCH_KEYS: usize = 512;

/// The legacy keys a phase reads, and what it does with each.
#[derive(Clone, Copy)]
enum Phase {
    DocumentMarkers,
    CollectionMarkers,
    DocumentHeads,
    CollectionHeads,
    DocumentGuards,
}

impl Phase {
    const ALL: [Self; 5] = [
        Self::DocumentMarkers,
        Self::CollectionMarkers,
        Self::DocumentHeads,
        Self::CollectionHeads,
        Self::DocumentGuards,
    ];

    fn prefix(self) -> &'static [u8] {
        match self {
            Self::DocumentMarkers => b"/ds/",
            Self::CollectionMarkers => b"/cs/",
            Self::DocumentHeads => b"/d/",
            Self::CollectionHeads => b"/c/",
            Self::DocumentGuards => b"/r/",
        }
    }
}

impl<S: Store> DB<S> {
    /// Move every head set to its merged value, once.
    pub(crate) async fn maybe_migrate_head_sets(&self) -> Result<()> {
        {
            let txn = self.new_txn(true).await?;
            let done = txn
                .systemstore()?
                .has(MIGRATION_MARKER)
                .await
                .map_err(Error::Storage)?;
            txn.discard()?;
            if done {
                return Ok(());
            }
        }
        let mut moved = 0usize;
        for phase in Phase::ALL {
            let mut after: Option<Vec<u8>> = None;
            loop {
                let txn = self.new_txn(false).await?;
                let batch = {
                    let headstore = txn.headstore()?;
                    let blockstore = txn.blockstore()?;
                    migrate_batch(phase, &headstore, &blockstore, after.as_deref()).await?
                };
                txn.commit().await?;
                moved += batch.moved;
                if batch.finished {
                    break;
                }
                after = batch.last;
            }
        }
        let txn = self.new_txn(false).await?;
        let systemstore = txn.systemstore()?;
        systemstore
            .set(MIGRATION_MARKER, &[])
            .await
            .map_err(Error::Storage)?;
        // A fresh store never had the earlier stamp: no tombstone for it.
        if systemstore
            .has(EARLIER_MARKER)
            .await
            .map_err(Error::Storage)?
        {
            systemstore
                .delete(EARLIER_MARKER)
                .await
                .map_err(Error::Storage)?;
        }
        drop(systemstore);
        txn.commit().await?;
        if moved > 0 {
            tracing::info!(keys = moved, "moved head sets to merged values");
        }
        Ok(())
    }
}

struct Batch {
    finished: bool,
    last: Option<Vec<u8>>,
    moved: usize,
}

/// What a batch writes: the new set merges, by set.
#[derive(Default)]
struct Writes {
    documents: BTreeMap<u64, Write>,
    collections: BTreeMap<u32, Write>,
}

/// The collection short ID and the `/`-separated parts after it that a `/cs/`
/// or `/c/` key holds.
fn collection_parts(key: &[u8], prefix: &[u8]) -> Option<(u32, Vec<Cid>)> {
    let mut parts = std::str::from_utf8(key.strip_prefix(prefix)?)
        .ok()?
        .split('/');
    let collection = parts.next()?.parse().ok()?;
    let cids = parts.map(|part| part.parse().ok()).collect::<Option<_>>()?;
    Some((collection, cids))
}

/// A marker `/ds/{doc}/{field}/{parent}/{child}`: its document, the parent
/// head it names, and the parent's name in the set.
fn document_marker(key: &[u8]) -> Option<HeadstoreDocKey> {
    let (rest, doc_short_id) = decode_doc_short_id_prefix(key.strip_prefix(b"/ds/")?).ok()?;
    let text = std::str::from_utf8(rest.strip_prefix(b"/")?).ok()?;
    let (head, _child) = text.rsplit_once('/')?;
    let (field, parent) = head.rsplit_once('/')?;
    Some(HeadstoreDocKey::new(
        doc_short_id,
        field,
        parent.parse().ok()?,
    ))
}

/// Whether the key was moved. A stored parent head is superseded: it goes,
/// and its guard records that it was published. A parent head that is gone
/// was published when `published` says so, and is held ahead otherwise.
async fn supersede_parent(
    headstore: &NamespaceView,
    head: &[u8],
    guard: &[u8],
    published: impl std::future::Future<Output = Result<bool>>,
) -> Result<bool> {
    if headstore.has(head).await.map_err(Error::Storage)? {
        headstore.delete(head).await.map_err(Error::Storage)?;
        headstore.set(guard, &[]).await.map_err(Error::Storage)?;
        return Ok(true);
    }
    if headstore.has(guard).await.map_err(Error::Storage)? {
        return Ok(true);
    }
    if published.await? {
        headstore.set(guard, &[]).await.map_err(Error::Storage)?;
        return Ok(true);
    }
    Ok(false)
}

async fn move_key(
    phase: Phase,
    headstore: &NamespaceView,
    blockstore: &NamespaceView,
    pair: &KvPair,
    writes: &mut Writes,
) -> Result<bool> {
    match phase {
        Phase::DocumentMarkers => {
            let Some(parent) = document_marker(&pair.key) else {
                return Ok(false);
            };
            let name = document::name(&parent.field_id, &parent.cid);
            let guard = document::publication_key(&parent);
            if !supersede_parent(headstore, &parent.bytes(), &guard, async { Ok(false) }).await? {
                writes
                    .documents
                    .entry(parent.doc_short_id)
                    .or_default()
                    .supersede_ahead(&name);
            }
            Ok(true)
        }
        Phase::CollectionMarkers => {
            let Some((collection, cids)) = collection_parts(&pair.key, b"/cs/") else {
                return Ok(false);
            };
            let [parent, _child] = cids[..] else {
                return Ok(false);
            };
            let guard = HeadstoreColPublishedKey::new(collection, parent).bytes();
            let head = format!("/c/{collection}/{parent}").into_bytes();
            // A collection block is published once it is stored and no longer
            // waits to be merged.
            let merged = async {
                Ok(blockstore
                    .has(&parent.to_bytes())
                    .await
                    .map_err(Error::Storage)?
                    && !blockstore
                        .has(&ToMergeIndexKey::new(parent).bytes())
                        .await
                        .map_err(Error::Storage)?)
            };
            if !supersede_parent(headstore, &head, &guard, merged).await? {
                writes
                    .collections
                    .entry(collection)
                    .or_default()
                    .supersede_ahead(parent.to_string().as_bytes());
            }
            Ok(true)
        }
        Phase::DocumentHeads => {
            let Some(head) = HeadstoreDocKey::parse(&pair.key) else {
                return Ok(false);
            };
            writes.documents.entry(head.doc_short_id).or_default().add(
                &document::name(&head.field_id, &head.cid),
                decode_priority_varint(&pair.value),
            );
            Ok(true)
        }
        Phase::CollectionHeads => {
            let Some((collection, cids)) = collection_parts(&pair.key, b"/c/") else {
                return Ok(false);
            };
            let [cid] = cids[..] else {
                return Ok(false);
            };
            writes.collections.entry(collection).or_default().add(
                cid.to_string().as_bytes(),
                decode_priority_varint(&pair.value),
            );
            headstore
                .set(&HeadstoreColPublishedKey::new(collection, cid).bytes(), &[])
                .await
                .map_err(Error::Storage)?;
            Ok(true)
        }
        // Every guard goes: the priority index entry records the publication.
        Phase::DocumentGuards => Ok(true),
    }
}

async fn migrate_batch(
    phase: Phase,
    headstore: &NamespaceView,
    blockstore: &NamespaceView,
    after: Option<&[u8]>,
) -> Result<Batch> {
    let mut options = IterOptions::new().with_prefix(phase.prefix().to_vec());
    if let Some(after) = after {
        options = options.with_start([after, &[0]].concat());
    }
    let mut iter = headstore.iterator(options).await.map_err(Error::Storage)?;
    // Collected, then written once the iterator is closed.
    let mut pairs = Vec::new();
    let mut finished = true;
    while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
        pairs.push(pair);
        if pairs.len() == BATCH_KEYS {
            finished = false;
            break;
        }
    }
    iter.close().await.map_err(Error::Storage)?;

    let mut writes = Writes::default();
    let mut moved = 0;
    for pair in &pairs {
        if move_key(phase, headstore, blockstore, pair, &mut writes).await? {
            headstore.delete(&pair.key).await.map_err(Error::Storage)?;
            moved += 1;
        } else {
            tracing::warn!(phase_prefix = ?phase.prefix(), "a head key of an unknown shape stays where it is");
        }
    }
    for (doc_short_id, write) in &writes.documents {
        document::commit(headstore, *doc_short_id, write)
            .await
            .map_err(Error::Storage)?;
    }
    for (collection, write) in &writes.collections {
        let key = HeadstoreColSetKey::new(*collection).bytes();
        crate::block::heads::merge_set(headstore, &key, write)
            .await
            .map_err(Error::Storage)?;
    }
    Ok(Batch {
        finished,
        last: pairs.last().map(|pair| pair.key.clone()),
        moved,
    })
}
