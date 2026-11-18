//! Head sets: the tips of a document's or a branchable collection's DAG.
//!
//! A set is one key whose value regolith folds with the store's merge operator
//! (`storage::merge::head_set`). A writer never reads to decide and never
//! deletes: it merges one operand per set per transaction (an `add` for each
//! block it publishes, a `supersede` for each parent), so concurrent writers
//! of one set never conflict, and a reader is one unvalidated point read
//! (`get_parts(key, &[])`). A parent whose head has not arrived yet is held
//! ahead until its own `add` consumes it; the writer chooses between
//! `supersede` and `supersede_ahead` from what records the parent's
//! publication, read so that the parent's publication committing meanwhile
//! conflicts, so an ahead name is only ever written for a parent that is truly
//! unpublished. A document head's record is its priority index entry, and a
//! held parent is read by its part of the set ([`document::publish`]); a
//! collection block's is a publication guard key.
//!
//! Documents are in [`document`]; the collection set and its publication
//! guard are here.

use cid::Cid;
use datastore::NamespaceView;
use storage::corekv::{Error, IterOptions, Iterator, Key, Reader, Result};
use storage::keys::headstore::{HeadstoreColPublishedKey, HeadstoreColSetKey};
use storage::merge::head_set::{Set, Write};

pub mod document;

/// The head set stored under `key`: empty when the key is absent.
pub(crate) async fn read_set<R>(reader: &R, key: &[u8]) -> Result<Set>
where
    R: Reader + ?Sized,
{
    match reader.get_parts(key, &[]).await? {
        None => Ok(Set::default()),
        Some(bytes) => {
            Set::read(&bytes).ok_or_else(|| Error::Other("a stored head set is malformed".into()))
        }
    }
}

/// Merge `write` into the set under `key`; nothing when it changes nothing.
pub(crate) async fn merge_set(writer: &NamespaceView, key: &[u8], write: &Write) -> Result<()> {
    if write.is_empty() {
        return Ok(());
    }
    writer.merge(key, &write.operand()).await
}

/// The live tips of a collection and the highest priority among them.
#[derive(Debug, Default)]
pub struct CollectionHeads {
    /// The live tips, ascending by CID text.
    pub live: Vec<Cid>,
    /// The highest priority among the live tips, or 0 when there are none.
    pub max_priority: u64,
}

/// A collection's head set: one point read.
pub async fn collection_set<R>(reader: &R, collection_short_id: u32) -> Result<Set>
where
    R: Reader + ?Sized,
{
    read_set(
        reader,
        &HeadstoreColSetKey::new(collection_short_id).bytes(),
    )
    .await
}

impl CollectionHeads {
    /// The live tips of `set`.
    pub fn of(set: &Set) -> Self {
        let mut found = Self::default();
        for (name, priority) in set.live() {
            let Some(cid) = std::str::from_utf8(name)
                .ok()
                .and_then(|text| text.parse::<Cid>().ok())
            else {
                continue;
            };
            found.max_priority = found.max_priority.max(priority);
            found.live.push(cid);
        }
        found
    }
}

/// The collection's live heads.
pub async fn live_collection_heads<R>(
    reader: &R,
    collection_short_id: u32,
) -> Result<CollectionHeads>
where
    R: Reader + ?Sized,
{
    Ok(CollectionHeads::of(
        &collection_set(reader, collection_short_id).await?,
    ))
}

/// The short IDs of the collections that have a head set, ascending.
pub async fn collections_with_heads<R>(reader: &R) -> Result<Vec<u32>>
where
    R: Reader + ?Sized,
{
    let mut iter = reader
        .iterator(
            IterOptions::new()
                .with_prefix(HeadstoreColSetKey::family_prefix())
                .with_keys_only(true),
        )
        .await?;
    let mut ids = Vec::new();
    while let Some(pair) = iter.next().await? {
        ids.extend(HeadstoreColSetKey::parse(&pair.key).map(|key| key.collection_id));
    }
    iter.close().await?;
    Ok(ids)
}

/// Publish a collection block once, in the caller's transaction: it becomes a
/// head at `priority` and each of `parents` stops being one. `set` is the
/// collection's set as the caller read it. A parent live in `set` or with a
/// publication guard is superseded; any other has not arrived, and is held
/// ahead until it does. The guard reads are validated.
pub async fn publish_collection_block(
    writer: &NamespaceView,
    collection_short_id: u32,
    set: &Set,
    cid: Cid,
    priority: u64,
    parents: &[Cid],
) -> Result<()> {
    let guard = HeadstoreColPublishedKey::new(collection_short_id, cid).bytes();
    if writer.has(&guard).await? {
        return Ok(());
    }
    let mut write = Write::default();
    for parent in parents {
        let name = parent.to_string().into_bytes();
        let published = HeadstoreColPublishedKey::new(collection_short_id, *parent).bytes();
        if set.is_live(&name) || writer.has(&published).await? {
            write.supersede(&name);
        } else {
            write.supersede_ahead(&name);
        }
    }
    write.add(cid.to_string().as_bytes(), priority);
    writer.set(&guard, &[]).await?;
    merge_set(
        writer,
        &HeadstoreColSetKey::new(collection_short_id).bytes(),
        &write,
    )
    .await
}
