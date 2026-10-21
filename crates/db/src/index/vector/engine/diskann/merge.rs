//! StreamingMerge (FreshDiskANN §5.3): folding pending inserts and tombstones
//! into the graph.

use std::collections::BTreeMap;
use std::ops::ControlFlow;

use rapidhash::{HashSetExt, RapidHashSet};

use super::codec::{self, GraphState, Record};
use super::graph::decode;
use super::params::{CONSOLIDATE_MIN_DELETED, MAX_PLAN_PENDING, MERGE_MIN_BATCH};
use super::DiskAnn;
use crate::index::error::{Error, Result};
use crate::index::vector::quantize::{ProductQuantizer, Quantizer};
use crate::index::vector::store::{NodeId, VectorNodeStore};

/// What one merge will do, read from a snapshot.
///
/// Taking it on one transaction and applying it on another is what lets the
/// merge avoid range reads: its own transaction only reads and writes the keys
/// the plan names, so inserts appending to the pending log meanwhile cannot
/// abort it. Whatever they add waits for the next plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergePlan {
    /// Pending inserts to link in, in log order, at most [`MAX_PLAN_PENDING`].
    pub pending: Vec<NodeId>,
    /// The newest log position `pending` was read through: what the merge
    /// truncates through, and no further (`proofs/tla/PendingCache.tla`).
    pub through: u64,
    /// Tombstones to consolidate. Empty unless enough accumulated to be
    /// worth the pass over every record that finding their in-edges takes.
    pub deleted: Vec<NodeId>,
    /// Graph nodes with an edge to something in `deleted`.
    pub affected: Vec<NodeId>,
}

impl MergePlan {
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.deleted.is_empty()
    }
}

impl<S: VectorNodeStore> DiskAnn<S> {
    /// Whether at least `wanted` entries of `kind` exist, counting no further.
    async fn aux_at_least(&self, kind: u8, wanted: usize) -> Result<bool> {
        Ok(self.aux_ids(kind, wanted).await?.len() >= wanted)
    }

    /// The first `limit` ids of `kind`, in key order.
    async fn aux_ids(&self, kind: u8, limit: usize) -> Result<Vec<NodeId>> {
        let mut ids = Vec::new();
        if limit == 0 {
            return Ok(ids);
        }
        let stop = "vector index: DISKANN aux scan stopped early";
        match self
            .store()
            .iterate_aux(kind, b"", |key, _| {
                ids.push(codec::id_from_key(key)?);
                if ids.len() < limit {
                    Ok(())
                } else {
                    Err(Error::Other(stop.into()))
                }
            })
            .await
        {
            Ok(()) => Ok(ids),
            Err(_) if ids.len() >= limit => Ok(ids),
            Err(err) => Err(err),
        }
    }

    /// Tombstones worth a consolidation pass: a tenth of the graph, and never
    /// fewer than [`CONSOLIDATE_MIN_DELETED`].
    fn consolidation_threshold(graph: &GraphState) -> usize {
        CONSOLIDATE_MIN_DELETED.max(graph.live / 10) as usize
    }

    pub(super) async fn merge_due(&self) -> Result<bool> {
        if self.store().pending_log_tail().await?.is_some() {
            return Ok(true);
        }
        let threshold = Self::consolidation_threshold(&self.graph_state().await?);
        self.aux_at_least(codec::DELETED, threshold).await
    }

    /// The first [`MAX_PLAN_PENDING`] distinct vectors the pending log
    /// inserts, oldest first, and the position of the last entry read.
    async fn plan_pending(&self, m: usize) -> Result<(Vec<NodeId>, u64)> {
        let mut pending = Vec::new();
        let mut seen = RapidHashSet::new();
        let mut through = 0;
        self.store()
            .iterate_pending_log(|position, entry| {
                if pending.len() >= MAX_PLAN_PENDING {
                    return Ok(ControlFlow::Break(()));
                }
                let (id, _) = codec::decode_log_entry(entry, m)?;
                if seen.insert(id) {
                    pending.push(id);
                }
                through = position;
                Ok(ControlFlow::Continue(()))
            })
            .await?;
        Ok((pending, through))
    }

    /// Reads what a merge should do. Empty when untrained or nothing is due.
    pub async fn merge_plan(&self) -> Result<MergePlan> {
        let Some(trained) = self.trained().await? else {
            return Ok(MergePlan::default());
        };
        let m = trained.m as usize;
        let (pending, through) = self.plan_pending(m).await?;
        let threshold = Self::consolidation_threshold(&self.graph_state().await?);
        if !self.aux_at_least(codec::DELETED, threshold).await? {
            return Ok(MergePlan {
                pending,
                through,
                ..MergePlan::default()
            });
        }

        let mut deleted = RapidHashSet::new();
        self.store()
            .iterate_aux(codec::DELETED, b"", |key, _| {
                deleted.insert(codec::id_from_key(key)?);
                Ok(())
            })
            .await?;
        let mut affected = Vec::new();
        self.store()
            .iterate_aux(codec::GRAPH, b"", |key, value| {
                let id = codec::id_from_key(key)?;
                if deleted.contains(&id) {
                    return Ok(());
                }
                let neighbors = codec::neighbor_ids(value, m, self.params.resident_codes)?;
                if neighbors.iter().any(|n| deleted.contains(n)) {
                    affected.push(id);
                }
                Ok(())
            })
            .await?;
        let mut deleted: Vec<NodeId> = deleted.into_iter().collect();
        deleted.sort();
        Ok(MergePlan {
            pending,
            through,
            deleted,
            affected,
        })
    }

    /// Applies `plan`: consolidates its tombstones, then links its pending
    /// inserts in slices, each slice's back-edges patched in before the next
    /// is linked.
    ///
    /// Reads and writes only the keys `plan` names, plus the records a walk
    /// expands and the graph state. An entry the plan names that changed since
    /// the snapshot is handled as it now is: a pending insert deleted meanwhile
    /// is skipped, a tombstone re-inserted meanwhile is kept, and a vector
    /// updated meanwhile is linked as it now is, its newer log entry linking
    /// it again in the next merge.
    pub async fn merge(&mut self, plan: MergePlan) -> Result<()> {
        let Some(trained) = self.trained().await? else {
            return Ok(());
        };
        if plan.is_empty() {
            return Ok(());
        }
        let pq = self.quantizer(&trained).await?;
        let mut graph = self.graph_state().await?;

        if !plan.deleted.is_empty() {
            self.consolidate(&pq, &mut graph, &plan.deleted, &plan.affected)
                .await?;
        }

        let slice = MERGE_MIN_BATCH.max(graph.live as usize / 10);
        for ids in plan.pending.chunks(slice) {
            self.merge_slice(&pq, &mut graph, ids).await?;
        }
        graph.generation += 1;
        self.put_graph_state(&graph).await?;
        self.truncate_pending_log(plan.through).await
    }

    /// Drops the log entries through `through`, the ones the plan linked:
    /// entries past it are still pending. A search at the new generation loads
    /// its copy by a scan, so it never needs the dropped ones; a search still
    /// at an older snapshot reads them there.
    pub(super) async fn truncate_pending_log(&mut self, through: u64) -> Result<()> {
        if let Some(tail) = self.store().pending_log_tail().await? {
            if tail <= through {
                self.store_mut().truncate_pending_log(tail, through).await?;
            }
        }
        Ok(())
    }

    /// The insert and patch phases for one slice of pending inserts.
    async fn merge_slice(
        &mut self,
        pq: &ProductQuantizer,
        graph: &mut GraphState,
        ids: &[NodeId],
    ) -> Result<()> {
        // Insert phase: link each node, buffering the back-edges it is owed
        // (the paper's Δ) rather than rewriting each neighbour per insert.
        let mut owed: BTreeMap<NodeId, Vec<(NodeId, Vec<u8>)>> = BTreeMap::new();
        for &id in ids {
            let Some(node) = self.store().get_node(id).await? else {
                continue;
            };
            if node.deleted {
                continue;
            }
            let mut code = vec![0u8; pq.code_len()];
            pq.encode(&node.vector, &mut code);
            for (neighbor, _) in self.link(pq, graph, id, &node.vector, &code).await? {
                owed.entry(neighbor).or_default().push((id, code.clone()));
            }
        }

        // Patch phase: each owed neighbour is read and written once.
        for (neighbor, edges) in owed {
            self.add_back_edges(pq, neighbor, edges).await?;
        }
        Ok(())
    }

    /// Delete phase (paper Algorithm 4): every node with an edge to a
    /// tombstone takes that tombstone's out-edges in its place, re-pruned, and
    /// the tombstones' records are dropped.
    ///
    /// A tombstone re-inserted since the plan was taken has left the delete
    /// list and is kept.
    async fn consolidate(
        &mut self,
        pq: &ProductQuantizer,
        graph: &mut GraphState,
        planned: &[NodeId],
        affected: &[NodeId],
    ) -> Result<()> {
        let m = pq.code_len();
        let mut deleted = RapidHashSet::with_capacity(planned.len());
        for &id in planned {
            if self
                .store()
                .get_aux(codec::DELETED, &codec::record_key(id))
                .await?
                .is_some()
            {
                deleted.insert(id);
            }
        }
        if deleted.is_empty() {
            return Ok(());
        }

        let mut fallback_entry = None;
        for &id in affected {
            if deleted.contains(&id) {
                continue;
            }
            let Some(record) = self.record(m, id).await? else {
                continue;
            };
            let mut edges = Vec::with_capacity(record.neighbors.len());
            for (neighbor, code) in record.neighbors {
                if !deleted.contains(&neighbor) {
                    edges.push((neighbor, code));
                    continue;
                }
                if let Some(gone) = self.record(m, neighbor).await? {
                    edges.extend(
                        gone.neighbors
                            .into_iter()
                            .filter(|(n, _)| *n != id && !deleted.contains(n)),
                    );
                }
            }
            let base = decode(pq, &record.code);
            let neighbors = self.prune(pq, &base, edges);
            self.put_record(
                id,
                &Record {
                    code: record.code,
                    neighbors,
                },
            )
            .await?;
            fallback_entry.get_or_insert(id);
        }

        if graph.entry.is_some_and(|entry| deleted.contains(&entry)) {
            graph.entry = match fallback_entry {
                Some(id) => Some(id),
                None => self.any_live_record(&deleted).await?,
            };
        }
        let mut removed = 0;
        for id in &deleted {
            let key = codec::record_key(*id);
            if self.store().get_aux(codec::GRAPH, &key).await?.is_some() {
                self.store_mut().delete_aux(codec::GRAPH, &key).await?;
                self.store_mut().delete_aux(codec::CODE, &key).await?;
                removed += 1;
            }
            self.store_mut().delete_aux(codec::DELETED, &key).await?;
        }
        graph.live = graph.live.saturating_sub(removed);
        Ok(())
    }

    async fn any_live_record(&self, deleted: &RapidHashSet<NodeId>) -> Result<Option<NodeId>> {
        let mut found = None;
        let stop = "vector index: any_live_record stopped early";
        match self
            .store()
            .iterate_aux(codec::GRAPH, b"", |key, _| {
                let id = codec::id_from_key(key)?;
                if deleted.contains(&id) {
                    return Ok(());
                }
                found = Some(id);
                Err(Error::Other(stop.into()))
            })
            .await
        {
            Ok(()) => Ok(None),
            Err(_) if found.is_some() => Ok(found),
            Err(err) => Err(err),
        }
    }
}
