//! Deferred maintenance of a vector index (`maintenance: DEFERRED`, Go's
//! sourcenetwork/defradb#5326).
//!
//! A write to a deferred index leaves the graph alone: it puts one pending
//! entry for its node, a blind write that conflicts with no other writer, and
//! asks for a drain once it commits. The drain applies a batch of entries to
//! the graph and deletes them in one transaction, reading each entry it
//! deletes, so an entry rewritten meanwhile makes the drain run again with the
//! newer one. A search scores the pending entries next to the graph's hits, so
//! a committed write is found right away (`proofs/tla/DeferredVector.tla`).

mod backlog;
mod drain;
pub mod entry;
mod search;

pub use backlog::{Backlog, Backlogs, PENDING_LIMIT};
pub(crate) use drain::{drain_vector_turn, VectorTurn};
pub use entry::{PendingEntry, PendingOp, PENDING};
pub(crate) use search::merge_pending;

use std::ops::ControlFlow;

use crate::index::error::{Error, Result};
use crate::index::vector::store::{NodeId, VectorNodeStore};

/// Records `entry` as its node's pending write, replacing any earlier one.
/// Blind: it reads nothing, so it conflicts with no other write.
pub async fn put_pending<S>(store: &mut S, entry: &PendingEntry) -> Result<()>
where
    S: VectorNodeStore,
{
    store
        .put_aux(PENDING, &entry::node_key(entry.id), &entry.encode())
        .await
}

/// Drops the node's pending write, if any, without reading it.
pub async fn delete_pending<S>(store: &mut S, id: NodeId) -> Result<()>
where
    S: VectorNodeStore,
{
    store.delete_aux(PENDING, &entry::node_key(id)).await
}

/// The node's pending write, read so a later rewrite of it fails this
/// transaction's commit.
pub async fn get_pending<S>(store: &S, id: NodeId) -> Result<Option<PendingEntry>>
where
    S: VectorNodeStore,
{
    store
        .get_aux(PENDING, &entry::node_key(id))
        .await?
        .map(|bytes| entry::decode_value(id, &bytes))
        .transpose()
}

/// Up to `limit` pending writes in node order, read by a scan that validates
/// nothing.
pub async fn list_pending<S>(store: &S, limit: usize) -> Result<Vec<PendingEntry>>
where
    S: VectorNodeStore,
{
    let mut entries = Vec::new();
    visit_pending(store, |key, value| {
        if entries.len() >= limit {
            return Ok(ControlFlow::Break(()));
        }
        let id = entry::node_from_key(key)?;
        entries.push(entry::decode_value(id, value)?);
        Ok(ControlFlow::Continue(()))
    })
    .await?;
    Ok(entries)
}

/// How many pending writes the index holds.
pub async fn count_pending<S>(store: &S) -> Result<u64>
where
    S: VectorNodeStore,
{
    let mut count = 0;
    visit_pending(store, |_, _| {
        count += 1;
        Ok(ControlFlow::Continue(()))
    })
    .await?;
    Ok(count)
}

/// The store's aux scan has no early stop, so a break ends it with a marker
/// error that is swallowed here.
async fn visit_pending<S, F>(store: &S, mut visit: F) -> Result<()>
where
    S: VectorNodeStore,
    F: FnMut(&[u8], &[u8]) -> Result<ControlFlow<()>> + defra_core::thread_bounds::MaybeSend,
{
    const STOP: &str = "vector index: pending scan stopped early";
    let mut stopped = false;
    let scanned = store
        .iterate_aux(PENDING, b"", |key, value| match visit(key, value)? {
            ControlFlow::Continue(()) => Ok(()),
            ControlFlow::Break(()) => {
                stopped = true;
                Err(Error::Other(STOP.into()))
            }
        })
        .await;
    match scanned {
        Err(_) if stopped => Ok(()),
        other => other,
    }
}
