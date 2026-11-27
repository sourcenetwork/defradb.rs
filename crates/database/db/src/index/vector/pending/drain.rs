//! Applying a deferred vector index's pending writes to its graph, as Go's
//! `vectorMaintenanceWriter.drainIndex` does.
//!
//! A batch is planned on a snapshot and applied on its own transaction, which
//! reads each planned entry before deleting it. A writer that replaces an
//! entry after that read fails the batch's commit, and the retry applies the
//! newer entry, so no write is deleted unapplied. A writer that commits first
//! is simply read.

use std::sync::Arc;

use storage::corekv::{RetryAttemptError, RetryAttemptValue, Store};

use super::backlog::FirstCountClaim;
use super::{count_pending, list_pending};
use crate::collection::loader::load_collection_from_systemstore;
use crate::database::DB;
use crate::index::deferred::IndexRef;
use crate::index::error::{Error, Result};
use crate::index::vector::index::{VectorIndex, LIVE_EPOCH};
use crate::index::vector::store::{KvNodeStore, NodeId};

/// Entries one transaction applies. Go measured no difference from 16 to 256.
const BATCH: usize = 32;

/// What one turn of a deferred vector index did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VectorTurn {
    /// Pending writes applied to the graph.
    pub applied: usize,
    /// Whether the turn stopped with writes still pending.
    pub more: bool,
}

/// Applies up to `plans` batches of the pending writes of the vector index
/// `target` of the collection `name`.
pub(crate) async fn drain_vector_turn<S>(
    db: &DB<S>,
    name: &str,
    target: IndexRef,
    plans: usize,
) -> Result<VectorTurn>
where
    S: Store + 'static,
{
    let backlog = db.deferred_drains.backlogs().get(target.0, target.1);
    let mut turn = VectorTurn::default();
    // The count lives in memory, so a node that opens on a backlog adds it.
    match backlog.first_count() {
        FirstCountClaim::Claimed(first_count) => {
            if let Snapshot::Count(stored) = snapshot(db, target, None).await? {
                backlog.add(stored);
                first_count.complete();
            }
        }
        FirstCountClaim::Initializing => return Ok(turn),
        FirstCountClaim::Ready => {}
    }
    for _ in 0..plans {
        // Read before the plan: writes after it may be missing from the plan,
        // so they must stay counted.
        let seen = backlog.pending();
        let Snapshot::Planned(planned) = snapshot(db, target, Some(BATCH)).await? else {
            return Ok(turn);
        };
        if planned.is_empty() {
            backlog.sub(seen);
            return Ok(turn);
        }
        let Some(applied) = apply(db, name, target, planned.clone()).await? else {
            db.deferred_drains.backlogs().forget(target.0, target.1);
            return Ok(turn);
        };
        turn.applied += applied;
        if planned.len() < BATCH {
            backlog.sub(seen);
            return Ok(turn);
        }
        backlog.sub(applied as u64);
    }
    turn.more = true;
    Ok(turn)
}

/// What a fresh snapshot holds pending: every write when `limit` is `None`
/// (only counted), or the first `limit` in node order.
async fn snapshot<S>(db: &DB<S>, target: IndexRef, limit: Option<usize>) -> Result<Snapshot>
where
    S: Store + 'static,
{
    let snapshot = db
        .new_txn(true)
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    let read = async {
        let mut view = snapshot
            .datastore()
            .map_err(|e| Error::Other(e.to_string()))?;
        let store = KvNodeStore::new(&mut view, target.0, target.1, LIVE_EPOCH);
        match limit {
            None => Ok(Snapshot::Count(count_pending(&store).await?)),
            Some(limit) => Ok(Snapshot::Planned(
                list_pending(&store, limit)
                    .await?
                    .into_iter()
                    .map(|entry| entry.id)
                    .collect(),
            )),
        }
    };
    let read = read.await;
    let _ = snapshot.discard();
    read
}

enum Snapshot {
    Count(u64),
    Planned(Vec<NodeId>),
}

/// Applies and deletes the planned entries still pending, or `None` when the
/// index is gone or no longer deferred. Read inside the transaction, so a drop
/// commits first or makes this run again and find it gone.
async fn apply<S>(
    db: &DB<S>,
    name: &str,
    target: IndexRef,
    planned: Vec<NodeId>,
) -> Result<Option<usize>>
where
    S: Store + 'static,
{
    let (name, planned) = (name.to_string(), Arc::new(planned));
    let codes = Arc::clone(&db.vector_codes);
    let applied: Option<usize> = datastore::transact(
        db.store().as_ref(),
        db.options().retry_options(),
        move |txn| {
            let (name, planned, codes) = (name.clone(), Arc::clone(&planned), Arc::clone(&codes));
            Box::pin(async move {
                let applied = apply_attempt(txn, &name, target, &planned, &codes)
                    .await
                    .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
                Ok(Box::new(applied) as RetryAttemptValue)
            })
        },
    )
    .await
    .map_err(|error| match error.downcast_abort::<Error>() {
        Ok(error) => error,
        Err(error) => Error::Storage(error.into_storage()),
    })?;
    Ok(applied)
}

async fn apply_attempt(
    txn: &mut datastore::BasicTxn,
    name: &str,
    target: IndexRef,
    planned: &[NodeId],
    codes: &crate::index::vector::engine::diskann::CodeCaches,
) -> Result<Option<usize>> {
    let mut datastore = txn.datastore();
    let systemstore = txn.systemstore();
    let collection = load_collection_from_systemstore(&systemstore, name)
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    let desc = collection.as_ref().and_then(|collection| {
        collection
            .write_indexes()
            .iter()
            .find(|index| index.id == target.1)
            .cloned()
    });
    let Some(desc) = desc.filter(|desc| desc.vector().is_some_and(|v| v.maintenance.is_deferred()))
    else {
        return Ok(None);
    };
    let mut index = VectorIndex::try_new(target.0, desc)?;
    index.share_quantizer(codes);

    Ok(Some(index.apply_planned(&mut datastore, planned).await?))
}
