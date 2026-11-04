//! Pending DAG registration and retry logic.

use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;
use web_time::Instant;

use cid::Cid;

use blockstore::Blockstore;

use crate::error::{Error, Result};
use crate::sync::arbitration::AttemptSample;
use crate::sync::manager::events::SyncEvent;
use crate::sync::manager::links::{extract_ipld_links, find_all_missing_links};
use crate::sync::manager::pending::{
    retry_backoff, PendingDag, PendingDagRegistry, PendingOrigin, SharedPendingDags,
    PENDING_ROTATE_AFTER_DISPATCHES,
};
use crate::sync::pending_store::{
    PersistedPendingDag, PersistedQuarantinedDag, PersistedView, Registration,
};

use super::SyncManager;

struct PendingResyncInFlight<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for PendingResyncInFlight<'_> {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[derive(Clone)]
pub(crate) struct PendingDagLease {
    root_cid: Cid,
    pending: Arc<SharedPendingDags>,
    persisted: Arc<PersistedView>,
    max_pending_dags: usize,
    max_per_peer: usize,
}

impl std::fmt::Debug for PendingDagLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingDagLease")
            .field("root_cid", &self.root_cid)
            .finish_non_exhaustive()
    }
}

impl PendingDagLease {
    pub(crate) fn is_current(&self) -> bool {
        self.pending
            .read(|pending| pending.contains_key(&self.root_cid))
    }

    /// Count an attempt that ended with the DAG still incomplete, returning
    /// how many such attempts the root has made, or `None` once the root has
    /// left the map.
    pub(crate) fn record_incomplete_attempt(&self) -> Option<u32> {
        self.pending.update(|pending| {
            let dag = pending.get_mut(&self.root_cid)?;
            dag.incomplete_attempts = dag.incomplete_attempts.saturating_add(1);
            Some(dag.incomplete_attempts)
        })
    }

    /// The fetch attempt ended and gave its slot back: the root is due again
    /// one backoff from now, so roots that waited during the attempt go first.
    /// An attempt `preempted` at its quantum while still making progress is
    /// due again at once instead: behind every root already due, with no
    /// backoff, since it did not fail.
    ///
    /// A root with a durable record that has had
    /// `PENDING_ROTATE_AFTER_DISPATCHES` dispatches without merging instead
    /// gives its in-memory entry up when it holds a contended place (the map
    /// is full, or its source peer is at its quota) while a durable root has
    /// no entry. Its record stays, and the resync sweep, which resumes after
    /// the place it admitted at last and gives a peer's places to that peer's
    /// roots in turn, brings it back once the others had a turn: durable
    /// roots past the cap take turns instead of the first ones in key order
    /// holding the map for good.
    ///
    /// `sample` feeds the root's estimates and nothing else: the re-arm is the
    /// same whatever it says.
    pub(crate) fn finish_attempt(&self, preempted: bool, sample: AttemptSample) {
        let now = n0_future::time::Instant::now();
        let durable = self.persisted.contains(&self.root_cid);
        let rotates = durable
            && self.pending.read(|pending| {
                pending.get(&self.root_cid).is_some_and(|dag| {
                    dag.dispatches >= PENDING_ROTATE_AFTER_DISPATCHES
                        && (pending.len() >= self.max_pending_dags
                            || dag.source_peer.as_deref().is_some_and(|peer| {
                                pending.source_count(peer) >= self.max_per_peer
                            }))
                }) && self.persisted.len()
                    > pending
                        .keys()
                        .filter(|root| self.persisted.contains(root))
                        .count()
            });
        self.pending.update(|pending| {
            if rotates {
                pending.remove(&self.root_cid);
            } else if let Some(dag) = pending.get_mut(&self.root_cid) {
                dag.next_retry_at = if preempted {
                    now
                } else {
                    dag.next_retry_at.max(now + retry_backoff(dag.dispatches))
                };
                dag.estimates.observe(sample);
            }
        });
        if rotates {
            tracing::debug!(
                root_cid = %self.root_cid,
                "Durable pending root gave its in-memory slot to a waiting one; its record stays"
            );
        }
    }
}

/// One merge turn of a root. Dropping it ends a local continuation the turn
/// did not continue.
#[must_use = "a local continuation ends when its turn drops"]
pub(crate) struct MergeTurn {
    root_cid: Cid,
    local: Option<Arc<SharedPendingDags>>,
}

impl Drop for MergeTurn {
    fn drop(&mut self) {
        let Some(pending) = self.local.take() else {
            return;
        };
        let root = self.root_cid;
        let ended = |registry: &PendingDagRegistry| {
            registry.get(&root).is_some_and(|dag| {
                dag.origin == PendingOrigin::LocalContinuation && !dag.merge_continuation
            })
        };
        if pending.read(ended) {
            pending.update(|registry| {
                if ended(registry) {
                    registry.remove(&root);
                }
            });
        }
    }
}

/// Match the outbound backlog's fairness policy: one peer may occupy at most
/// one quarter of the global pending-DAG capacity.
const PENDING_DAG_PEER_CAPACITY_DIVISOR: usize = 4;

/// Outcome of admitting a pending-DAG registration into the in-memory map.
///
/// The rejection variants carry the limit that tripped so the caller's nack
/// reports the number the operator will see in logs (global vs per-peer).
pub(super) enum PendingDagAdmission {
    Admitted {
        superseded: Box<Option<(Cid, PendingDag)>>,
    },
    GlobalCapacity,
    PeerQuota {
        max_per_peer: usize,
    },
}

#[derive(Debug, Clone)]
pub struct PendingDagFetchFailure {
    pub doc_id: String,
    pub collection_id: String,
    pub source_peer: Option<String>,
    pub missing_count: usize,
    pub fetch_failures: u32,
}

fn evict_expired_pending_dags(
    pending: &mut crate::sync::manager::pending::PendingDagRegistry,
    now: Instant,
) -> Vec<(Cid, PendingDag)> {
    let expired: Vec<_> = pending
        .iter()
        .filter(|(_, dag)| dag.is_expired(now))
        .map(|(cid, dag)| (*cid, PendingDag::clone(dag)))
        .collect();

    for (cid, _) in &expired {
        pending.remove(cid);
    }

    expired
}

impl<B: Blockstore + 'static> SyncManager<B> {
    pub(crate) fn pending_dag_lease(&self, root_cid: Cid) -> PendingDagLease {
        PendingDagLease {
            root_cid,
            pending: Arc::clone(&self.pending_dags),
            persisted: Arc::clone(&self.persisted),
            max_pending_dags: self.max_pending_dags,
            max_per_peer: self.max_pending_dags_per_peer(),
        }
    }
    /// Get the pending DAGs count (for testing/monitoring).
    pub fn pending_dag_count(&self) -> usize {
        self.pending_dags.read(|pending| pending.len())
    }

    pub(super) fn max_pending_dags_per_peer(&self) -> usize {
        (self.max_pending_dags / PENDING_DAG_PEER_CAPACITY_DIVISOR).max(1)
    }

    /// A peer's share of the durable cap, as `max_pending_dags_per_peer` is
    /// its share of the map: the durable registrations it may hold at once.
    pub fn max_persisted_pending_per_peer(&self) -> usize {
        (self.persisted_pending_capacity() / PENDING_DAG_PEER_CAPACITY_DIVISOR).max(1)
    }

    /// Whether registering this head durably keeps its peer within its
    /// durable share: the head is its scope's durable current head or links
    /// it as a parent, taking over its record, or the peer holds fewer records
    /// than its share. The count is the view's, so registrations of one peer
    /// that run at once can pass it together; the store's registration counts
    /// them in its own transaction and refuses those past the share.
    pub(super) fn durable_share_admits(
        &self,
        root_cid: Cid,
        parents: &[Cid],
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> bool {
        self.persisted.refreshes_or_supersedes(
            root_cid,
            parents,
            source_peer,
            collection_id,
            doc_id,
            head_priority,
        ) || self.persisted.source_count(source_peer) < self.max_persisted_pending_per_peer()
    }

    /// Whether a head the map has no place for can be acknowledged on a
    /// durable record alone: a store is installed, has a free slot or the
    /// head takes over its scope's, and the head's peer is within its share.
    pub(super) fn admits_durable_only(
        &self,
        root_cid: Cid,
        parents: &[Cid],
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> bool {
        self.pending_store().is_some()
            && (self.persisted.len() < self.persisted_pending_capacity()
                || self.persisted.refreshes_or_supersedes(
                    root_cid,
                    parents,
                    source_peer,
                    collection_id,
                    doc_id,
                    head_priority,
                ))
            && self.durable_share_admits(
                root_cid,
                parents,
                source_peer,
                collection_id,
                doc_id,
                head_priority,
            )
    }

    /// Whether this side owns the root's recovery for a sender: its pushed or
    /// restored entry is registered, or a durable record holds it while it
    /// has no entry (it gave its entry up past the cap, or the node
    /// restarted). Either backs a success reply. A local continuation does
    /// not: it has no record and ends with its merge turn.
    pub(super) fn is_pending_dag_owned(&self, root_cid: &Cid) -> bool {
        self.persisted.contains(root_cid)
            || self.pending_dags.read(|pending| {
                pending.get(root_cid).is_some_and(|dag| {
                    dag.is_recovery_registered && dag.origin != PendingOrigin::LocalContinuation
                })
            })
    }

    pub(super) fn mark_pending_dag_recovery_registered(
        &self,
        root_cid: &Cid,
        inserted_at: Instant,
    ) {
        let registered = self.pending_dags.update(|pending| {
            pending
                .get_mut(root_cid)
                .filter(|dag| dag.inserted_at == inserted_at)
                .map(|dag| dag.is_recovery_registered = true)
                .is_some()
        });
        if registered {
            self.pending_dag_ready.notify_one();
        }
    }

    /// Wake the existing dispatch owner without creating another fetch owner.
    pub(crate) async fn pending_dag_ready(&self) {
        let registered = std::pin::pin!(self.pending_dag_ready.notified());
        let released = std::pin::pin!(self.process_queue.released());
        futures::future::select(registered, released).await;
    }

    /// Remember contention without retaining a transport task or CAR buffer.
    pub(crate) fn defer_pending_dag_for_storage(&self, root: &Cid, blocker: Cid) {
        let deferred = self.pending_dags.update(|pending| {
            pending
                .get_mut(root)
                .map(|dag| dag.storage_blocker = Some(blocker))
                .is_some()
        });
        if deferred {
            // Also wake if the owner released before registration.
            self.pending_dag_ready.notify_one();
        }
    }

    /// Milliseconds until the earliest due pending-DAG retry, including a
    /// complete DAG awaiting a terminal merge outcome. `None` means no live
    /// receiver obligation is registered.
    pub fn next_pending_retry_in_ms(&self) -> Option<u64> {
        let now = n0_future::time::Instant::now();
        self.pending_dags.read(|pending| {
            pending
                .values()
                .map(|dag| dag.next_retry_at.saturating_duration_since(now).as_millis() as u64)
                .min()
        })
    }

    /// Drop every entry past its TTL, counting each in the diagnostics.
    fn evict_expired(&self) {
        let now = Instant::now();
        if !self.pending_dags.read(|pending| pending.has_expired(now)) {
            return;
        }
        let expired = self
            .pending_dags
            .update(|pending| evict_expired_pending_dags(pending, now).len());
        for _ in 0..expired {
            self.diagnostics.record_pending_dag_expired();
        }
    }

    /// Return whether a PushLog can proceed without decoding its block. At
    /// capacity, blocks awaited by existing roots must still enter so those
    /// roots can complete and free their slots.
    pub(super) fn can_process_pushlog(&self, cid: &Cid) -> bool {
        self.evict_expired();
        self.pending_dags.read(|pending| {
            pending.len() < self.max_pending_dags
                || pending.contains_key(cid)
                || pending.has_waiters(cid)
        })
    }

    /// Get CIDs of all pending DAGs.
    pub fn pending_dag_cids(&self) -> Vec<Cid> {
        self.pending_dags
            .read(|pending| pending.keys().copied().collect())
    }

    /// Pending DAGs worth re-driving when `peer` (re)connects: entries the
    /// peer originally provided plus entries whose fetches exhausted their
    /// providers (covers restored registrations whose providers were not yet
    /// reconnected at restore time, #1099).
    pub fn pending_dags_needing_redrive(&self, peer: &str) -> Vec<(Cid, PendingDag)> {
        self.pending_dags.read(|pending| {
            pending
                .iter()
                .filter(|(_, dag)| {
                    !dag.missing.is_empty()
                        && (dag.source_peer.as_deref() == Some(peer) || dag.fetch_failures > 0)
                })
                .map(|(cid, dag)| (*cid, PendingDag::clone(dag)))
                .collect()
        })
    }

    /// Pending DAGs this peer announced or is an alternate provider for.
    pub fn pending_dags_from(&self, peer: &str) -> Vec<(Cid, PendingDag)> {
        self.pending_dags.read(|pending| {
            pending
                .iter()
                .filter(|(_, dag)| {
                    dag.source_peer.as_deref() == Some(peer)
                        || dag.alternate_providers.iter().any(|alt| alt == peer)
                })
                .map(|(cid, dag)| (*cid, PendingDag::clone(dag)))
                .collect()
        })
    }

    /// Get missing CIDs for a pending DAG.
    pub fn pending_dag_missing(&self, root_cid: &Cid) -> Vec<Cid> {
        self.pending_dags.read(|pending| {
            pending
                .get(root_cid)
                .map(|dag| dag.missing.iter().copied().collect())
                .unwrap_or_default()
        })
    }

    /// Snapshot a pending DAG entry for dispatch (e.g. after a claimed
    /// post-fetch retry, #1116 stage 2).
    pub fn pending_dag_snapshot(&self, root_cid: &Cid) -> Option<PendingDag> {
        self.pending_dags.read(|pending| pending.snapshot(root_cid))
    }

    /// How many times `retry_pending_dag` has been called for this root.
    ///
    /// Returns 0 if no entry exists (either never registered or already resolved).
    pub fn pending_dag_attempts(&self, root_cid: &Cid) -> u32 {
        self.pending_dags
            .read(|pending| pending.get(root_cid).map_or(0, |dag| dag.attempts))
    }

    /// Get the source peer for a pending DAG (the peer that originally provided it).
    pub fn pending_dag_source_peer(&self, root_cid: &Cid) -> Option<String> {
        self.pending_dags.read(|pending| {
            pending
                .get(root_cid)
                .and_then(|dag| dag.source_peer.clone())
        })
    }

    /// Record a provider-exhaustion failure for a pending DAG and return a
    /// snapshot suitable for rate-limited logging.
    pub fn record_pending_dag_fetch_failure(
        &self,
        root_cid: &Cid,
        error: &str,
    ) -> Option<PendingDagFetchFailure> {
        self.pending_dags.update(|pending| {
            let dag = pending.get_mut(root_cid)?;
            dag.fetch_failures = dag.fetch_failures.saturating_add(1);
            dag.last_fetch_error = Some(error.to_string());
            Some(PendingDagFetchFailure {
                doc_id: dag.doc_id.clone(),
                collection_id: dag.collection_id.clone(),
                source_peer: dag.source_peer.clone(),
                missing_count: dag.missing.len(),
                fetch_failures: dag.fetch_failures,
            })
        })
    }

    /// Clear the remembered fetch-failure state for a pending DAG.
    pub fn clear_pending_dag_fetch_failures(&self, root_cid: &Cid) {
        self.pending_dags.update(|pending| {
            if let Some(dag) = pending.get_mut(root_cid) {
                dag.fetch_failures = 0;
                dag.last_fetch_error = None;
            }
        });
    }

    /// A block just arrived (PushLog store or Bitswap fetch). Update every
    /// pending root waiting on it: drop it from `missing`, add the block's
    /// own absent links to the frontier, and run the full verification walk
    /// (`retry_pending_dag`) ONLY for roots whose frontier emptied. The full
    /// walk per arriving block was O(DAG) per root per block, the #1112
    /// inbound storm; the clock's periodic retry self-heals any frontier
    /// drift this incremental pass might accumulate.
    ///
    /// This covers the explicit replay path where a composite can be registered
    /// as pending before its linked field blocks arrive via later PushLog
    /// requests rather than Bitswap.
    pub async fn retry_pending_dags_waiting_on(&self, cid: &Cid) -> Result<Vec<Cid>> {
        if !self.pending_dags.read(|pending| pending.has_waiters(cid)) {
            return Ok(Vec::new());
        }

        // The arriving block's own links that are still absent become the
        // new frontier for every waiting root. Computed once per block.
        // Uses `extract_ipld_links` (not `defra_core::collect_block_links`
        // directly) so the `encryption` link stays excluded here too, Go
        // never serves that block over Bitswap (#976), so treating it as
        // "absent" would strand the frontier on a CID that never arrives.
        let mut absent_links: Vec<Cid> = Vec::new();
        if let Ok(Some(data)) = self.blockstore.get(cid).await {
            for link in extract_ipld_links(&data).unwrap_or_default() {
                if !matches!(self.blockstore.has(&link).await, Ok(true)) {
                    absent_links.push(link);
                }
            }
        }

        let emptied = self
            .pending_dags
            .update(|pending| pending.advance_waiters(cid, &absent_links));

        let mut completed = Vec::new();
        for root_cid in emptied {
            if self.retry_pending_dag(&root_cid).await? {
                completed.push(root_cid);
            }
        }

        Ok(completed)
    }

    /// Insert a pending DAG entry, enforcing TTL eviction and capacity limits.
    ///
    /// Expired entries (see `PendingDag::is_expired`) are removed before checking
    /// the capacity. If the map is still at `max_pending_dags` after eviction the
    /// new entry is rejected so callers can return a backpressure nack (#1088
    /// W1) instead of acking a discarded registration. A source peer may hold
    /// at most one quarter of the global map so one noisy pusher cannot starve
    /// every other peer (#1088 W2).
    /// An acknowledged entry never expires: the ack ended the sender's retry
    /// marker, so it stays until merge, quarantine or a scope head that links
    /// it, and counts against the cap meanwhile.
    /// Admit a registration into the in-memory map, reporting `true` on
    /// success. Callers that need to distinguish the failing limit (to nack
    /// with the right number) use [`Self::try_insert_pending_dag`].
    #[cfg(test)]
    pub(super) fn insert_pending_dag(&self, root_cid: Cid, dag: PendingDag) -> bool {
        matches!(
            self.try_insert_pending_dag(root_cid, &[], dag),
            PendingDagAdmission::Admitted { .. }
        )
    }

    /// `parents` are the heads `root_cid` links: it retires its scope's
    /// current head only when that head is one of them.
    pub(super) fn try_insert_pending_dag(
        &self,
        root_cid: Cid,
        parents: &[Cid],
        dag: PendingDag,
    ) -> PendingDagAdmission {
        use crate::sync::head_scope::ScopeDecision;

        let max_per_peer = self.max_pending_dags_per_peer();
        let registered = dag.is_recovery_registered;
        let (admission, depth) = self.pending_dags.update(|pending| {
            evict_expired_pending_dags(pending, Instant::now());

            let scope_decision = pending.scope_head_decision(root_cid, parents, &dag);
            let superseded_root = match scope_decision {
                ScopeDecision::Supersedes(root) => Some(root),
                _ => None,
            };

            if pending.len() >= self.max_pending_dags
                && !pending.contains_key(&root_cid)
                && superseded_root.is_none()
            {
                return (PendingDagAdmission::GlobalCapacity, None);
            }

            if let Some(source_peer) = dag.source_peer.as_deref() {
                let existing_source = pending
                    .get(&root_cid)
                    .and_then(|existing| existing.source_peer.as_deref());
                if existing_source != Some(source_peer)
                    && pending.source_count(source_peer) >= max_per_peer
                    && superseded_root.is_none()
                {
                    return (PendingDagAdmission::PeerQuota { max_per_peer }, None);
                }
            }

            let superseded = superseded_root
                .and_then(|root| pending.remove(&root).map(|previous| (root, previous)));
            pending.insert(root_cid, dag.clone());
            (
                PendingDagAdmission::Admitted {
                    superseded: Box::new(superseded),
                },
                Some(pending.len()),
            )
        });
        if let Some(depth) = depth {
            self.diagnostics.observe_pending_dag_depth(depth);
            if registered {
                self.pending_dag_ready.notify_one();
            }
        }
        admission
    }

    fn update_pending_dag_missing_if_current(
        &self,
        root_cid: &Cid,
        inserted_at: Instant,
        missing: RapidHashSet<Cid>,
    ) -> bool {
        self.pending_dags.update(|pending| {
            if pending
                .get(root_cid)
                .is_none_or(|dag| dag.inserted_at != inserted_at)
            {
                return false;
            }
            pending.replace_missing(root_cid, missing.clone())
        })
    }

    /// Remove a pending DAG entry once another fetch path has completed it.
    pub fn clear_pending_dag(&self, root_cid: &Cid) -> bool {
        if !self
            .pending_dags
            .read(|pending| pending.contains_key(root_cid))
        {
            return false;
        }
        self.pending_dags
            .update(|pending| pending.remove(root_cid).is_some())
    }

    /// Claim the right to dispatch receiver work for this root. Returns false
    /// when the entry is missing or not yet due. Complete roots remain
    /// eligible because this same clock re-drives a transiently failed merge.
    /// A successful claim advances the clock; registration, completion, and
    /// peer connect may only make the root due.
    pub fn try_claim_pending_dag_dispatch(
        &self,
        root_cid: &Cid,
        now: n0_future::time::Instant,
    ) -> bool {
        let eligible = self.pending_dags.read(|pending| {
            pending.get(root_cid).is_some_and(|dag| {
                dag.is_recovery_registered
                    && dag.storage_blocker.is_none()
                    && now >= dag.next_retry_at
            })
        });
        let claimed = eligible
            && self.pending_dags.update(|pending| {
                let Some(dag) = pending.get_mut(root_cid) else {
                    return false;
                };
                if !dag.is_recovery_registered
                    || dag.storage_blocker.is_some()
                    || now < dag.next_retry_at
                {
                    return false;
                }
                if dag.merge_continuation {
                    // A committed yield makes the root ready again; errors or lost claims keep retry pacing.
                    dag.next_retry_at = now + super::super::pending::PENDING_RETRY_BASE;
                } else {
                    dag.dispatches = dag.dispatches.saturating_add(1);
                    dag.next_retry_at = now + super::super::pending::retry_backoff(dag.dispatches);
                }
                true
            });
        if claimed {
            self.diagnostics.record_pending_dag_retry_dispatched();
        } else {
            self.diagnostics.record_pending_dag_retry_suppressed();
        }
        claimed
    }

    /// Only a committed work-budget yield opts into local continuation.
    /// ACP skips and errors leave the normal fetch retry policy intact.
    #[cfg(test)]
    pub(crate) fn schedule_pending_merge_continuation(&self, root_cid: &Cid) {
        self.continue_merge(root_cid, None);
    }

    pub(crate) fn continue_merge(
        &self,
        root_cid: &Cid,
        metadata: Option<&crate::sync::merge::BlockMetadata<'_>>,
    ) -> bool {
        let next = n0_future::time::Instant::now()
            + super::super::pending::PENDING_MERGE_CONTINUATION_DELAY;
        let scheduled = self.pending_dags.update(|pending| {
            if !pending.contains_key(root_cid) {
                let Some(metadata) = metadata else {
                    return false;
                };
                if pending.len() >= self.max_pending_dags
                    || metadata.sender_peer.is_some_and(|peer| {
                        pending.source_count(peer) >= self.max_pending_dags_per_peer()
                    })
                {
                    return false;
                }
                // Pull sync has no PushLog registration. Its first committed yield
                // joins the same bounded local scheduler without changing push metadata.
                pending.insert(
                    *root_cid,
                    PendingDag {
                        doc_id: metadata.doc_id.unwrap_or_default().to_string(),
                        collection_id: metadata.collection_id.unwrap_or_default().to_string(),
                        head_priority: None,
                        creator: metadata.creator.unwrap_or_default().to_string(),
                        missing: RapidHashSet::new(),
                        source_peer: metadata.sender_peer.map(str::to_string),
                        alternate_providers: Vec::new(),
                        is_explicit_replicator: metadata.is_explicit_replicator,
                        explicit_replay_authorization: metadata
                            .explicit_replay_authorization
                            .clone(),
                        is_recovery_registered: true,
                        inserted_at: Instant::now(),
                        attempts: 0,
                        fetch_failures: 0,
                        last_fetch_error: None,
                        next_retry_at: next,
                        dispatches: 0,
                        incomplete_attempts: 0,
                        merge_continuation: true,
                        storage_blocker: None,
                        origin: PendingOrigin::LocalContinuation,
                        estimates: crate::sync::arbitration::RootEstimates::default(),
                    },
                );
            }
            let Some(dag) = pending.get_mut(root_cid) else {
                return false;
            };
            dag.merge_continuation = true;
            dag.next_retry_at = next;
            dag.inserted_at = Instant::now();
            pending.replace_missing(root_cid, RapidHashSet::new());
            true
        });
        if scheduled {
            self.pending_dag_ready.notify_one();
        }
        scheduled
    }

    /// A merge turn of `root_cid` starts: consume its scheduled continuation.
    /// The returned turn ends a local continuation when it drops, unless the
    /// turn yielded again through `continue_merge`. Nobody else owns such an
    /// entry, so a turn that merged, skipped or failed leaves nothing behind
    /// to count against `max_pending_dags` or to be re-driven; the next pull
    /// or push of the head starts the merge again.
    pub(crate) fn consume_pending_merge_continuation(&self, root_cid: &Cid) -> MergeTurn {
        let entry = self.pending_dags.read(|pending| {
            pending.get(root_cid).map(|dag| {
                (
                    dag.merge_continuation,
                    dag.origin == PendingOrigin::LocalContinuation,
                )
            })
        });
        if entry.is_some_and(|(scheduled, _)| scheduled) {
            self.pending_dags.update(|pending| {
                if let Some(dag) = pending.get_mut(root_cid) {
                    dag.merge_continuation = false;
                }
            });
        }
        MergeTurn {
            root_cid: *root_cid,
            local: entry
                .is_some_and(|(_, local)| local)
                .then(|| Arc::clone(&self.pending_dags)),
        }
    }

    /// Make a root promptly due (e.g. a provider just connected) without
    /// resetting its backoff rung. The retry clock performs the dispatch.
    pub fn expedite_pending_dag_retry(&self, root_cid: &Cid) {
        let now = n0_future::time::Instant::now();
        let registered = self.pending_dags.update(|pending| {
            pending.get_mut(root_cid).map(|dag| {
                dag.next_retry_at = dag.next_retry_at.min(now);
                dag.is_recovery_registered
            })
        });
        if registered == Some(true) {
            self.pending_dag_ready.notify_one();
        }
    }

    /// Return due receiver obligations in oldest-due order without claiming
    /// them. The coordinator reserves bounded ownership before advancing a
    /// root's retry rung, so neither fetches nor merge re-drive can accumulate
    /// behind the limiter.
    pub(crate) fn due_pending_dag_retries(
        &self,
        now: n0_future::time::Instant,
    ) -> Vec<(Cid, PendingDag)> {
        self.evict_expired();
        let released: Vec<Cid> = self.pending_dags.read(|pending| {
            pending
                .iter()
                .filter_map(|(root, dag)| {
                    dag.storage_blocker
                        .filter(|cid| !self.process_queue.is_active(cid))
                        .map(|_| *root)
                })
                .collect()
        });
        if !released.is_empty() {
            self.pending_dags.update(|pending| {
                for root in &released {
                    if let Some(dag) = pending.get_mut(root) {
                        dag.storage_blocker = None;
                        dag.next_retry_at = dag.next_retry_at.min(now);
                    }
                }
            });
        }
        let mut due: Vec<_> = self.pending_dags.read(|pending| {
            pending
                .iter()
                .filter(|(_, dag)| {
                    dag.is_recovery_registered
                        && dag.storage_blocker.is_none()
                        && now >= dag.next_retry_at
                })
                .map(|(cid, dag)| (*cid, PendingDag::clone(dag)))
                .collect()
        });
        due.sort_unstable_by(|(left_cid, left), (right_cid, right)| {
            left.next_retry_at
                .cmp(&right.next_retry_at)
                .then_with(|| left_cid.to_bytes().cmp(&right_cid.to_bytes()))
        });
        self.observe_dispatch_pass(now, &due);
        due
    }

    /// Test seam that claims every currently due root. Production dispatch
    /// claims roots individually only after reserving bounded task ownership.
    #[cfg(test)]
    pub fn claim_due_pending_dag_retries(
        &self,
        now: n0_future::time::Instant,
    ) -> Vec<(Cid, PendingDag)> {
        self.due_pending_dag_retries(now)
            .into_iter()
            .filter(|(cid, _)| self.try_claim_pending_dag_dispatch(cid, now))
            .collect()
    }

    /// Process a pending DAG after Bitswap blocks have been received.
    ///
    /// This is called when BitswapComplete is received, indicating all requested
    /// blocks have arrived. We re-check the DAG for any remaining missing links
    /// (recursively, at all depths) and process it if complete.
    ///
    /// Transport handlers call this in their lane slot, so a complete DAG
    /// never waits for room in the merge loop's channel: while it is full the
    /// root is made due at once for the receiver clock, and this returns
    /// `false`.
    pub async fn retry_pending_dag(&self, root_cid: &Cid) -> Result<bool> {
        enum PendingDagRetryEntry {
            Current(PendingDag),
            Expired(PendingDag),
        }

        // Record the attempt (and capture the incremented value) in the same
        // update so concurrent retries observe monotonic attempt counts.
        let now = Instant::now();
        let pending_info = self.pending_dags.update(|pending| {
            let expired = evict_expired_pending_dags(pending, now);
            if let Some((_, dag)) = expired.into_iter().find(|(cid, _)| cid == root_cid) {
                Some(PendingDagRetryEntry::Expired(dag))
            } else {
                pending.get_mut(root_cid).map(|dag| {
                    dag.attempts = dag.attempts.saturating_add(1);
                    PendingDagRetryEntry::Current(dag.clone())
                })
            }
        });

        let Some(info) = pending_info else {
            tracing::debug!(
                root_cid = %root_cid,
                "No pending DAG found for retry (already resolved or never registered)"
            );
            return Ok(false);
        };

        let info = match info {
            PendingDagRetryEntry::Current(info) => info,
            PendingDagRetryEntry::Expired(info) => {
                self.diagnostics.record_pending_dag_expired();
                tracing::warn!(
                    root_cid = %root_cid,
                    doc_id = %info.doc_id,
                    collection_id = %info.collection_id,
                    source_peer = ?info.source_peer,
                    missing_count = info.missing.len(),
                    attempts = info.attempts,
                    fetch_failures = info.fetch_failures,
                    last_fetch_error = ?info.last_fetch_error,
                    age_secs = info.inserted_at.elapsed().as_secs(),
                    "Pending DAG expired (TTL exceeded), dropping"
                );
                return Ok(false);
            }
        };

        self.diagnostics.record_missing_link_retry();

        // Load the root block from blockstore
        let block_data = match self.blockstore.get(root_cid).await {
            Ok(Some(data)) => data,
            Ok(None) => {
                tracing::error!(
                    root_cid = %root_cid,
                    "Root block not found in blockstore during retry"
                );
                return Err(Error::BlockstoreError("Root block not found".to_string()));
            }
            Err(e) => {
                tracing::error!(
                    root_cid = %root_cid,
                    error = %e,
                    "Failed to load root block from blockstore"
                );
                return Err(Error::from_blockstore(e));
            }
        };

        // Recursively check ALL missing links at every depth of the DAG.
        // This is critical for multi-level DAGs like Collection → Composite → LWW
        // where a single-level check would declare the DAG "ready" prematurely.
        let missing = match find_all_missing_links(self.blockstore.as_ref(), &block_data).await {
            Ok(missing) => missing,
            Err(e) => {
                tracing::error!(
                    root_cid = %root_cid,
                    error = %e,
                    "Failed to re-check missing links for pending DAG"
                );
                return Err(e);
            }
        };

        tracing::debug!(
            root_cid = %root_cid,
            doc_id = %info.doc_id,
            missing_count = missing.len(),
            "Retrying pending DAG"
        );

        if !missing.is_empty() {
            tracing::debug!(
                root_cid = %root_cid,
                missing_count = missing.len(),
                "Still missing blocks for DAG"
            );
            // Update the pending info with new missing CIDs (preserve original inserted_at).
            if !self.update_pending_dag_missing_if_current(
                root_cid,
                info.inserted_at,
                missing.into_iter().collect(),
            ) {
                tracing::debug!(
                    root_cid = %root_cid,
                    "Pending DAG changed before retry update; skipping stale retry result"
                );
            }
            return Ok(false);
        }

        // Keep the live registration until merge/mark or quarantine reaches a
        // durable terminal outcome. The receiver clock can then re-drive a
        // transient merge failure without waiting for restart reconciliation.
        if !self.update_pending_dag_missing_if_current(
            root_cid,
            info.inserted_at,
            RapidHashSet::new(),
        ) {
            tracing::debug!(
                root_cid = %root_cid,
                "Pending DAG changed before ready event; skipping stale completion"
            );
            return Ok(false);
        }
        tracing::info!(
            root_cid = %root_cid,
            doc_id = %info.doc_id,
            collection_id = %info.collection_id,
            attempts = info.attempts,
            pending_duration_ms = info.inserted_at.elapsed().as_millis() as u64,
            "DAG complete, emitting DagReady"
        );

        match self.event_tx.try_send(SyncEvent::DagReady {
            root_cid: *root_cid,
            doc_id: info.doc_id.clone(),
            collection_id: info.collection_id.clone(),
            creator: info.creator.clone(),
            sender_peer: info.source_peer.clone(),
            is_explicit_replicator: info.is_explicit_replicator,
            explicit_replay_authorization: info.explicit_replay_authorization.clone(),
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::debug!(
                    root_cid = %root_cid,
                    "Merge loop channel full; the receiver clock dispatches the complete DAG"
                );
                self.expedite_pending_dag_retry(root_cid);
                return Ok(false);
            }
            Err(TrySendError::Closed(_)) => {
                tracing::error!(
                    root_cid = %root_cid,
                    "Failed to emit DagReady for completed DAG"
                );
                return Err(Error::ChannelSend);
            }
        }

        self.diagnostics.record_pending_dag_resolved();
        Ok(true)
    }

    /// Reconcile the in-memory pending map against the durable registrations
    /// (#1099): drop records whose roots merged, and re-register + re-drive
    /// every unmerged record with no live in-memory entry. Runs at startup
    /// (restore), periodically and as the single-flight sweep behind peer
    /// connects, so a registration whose in-memory entry gave its slot up
    /// (`PendingDagLease::finish_attempt`) is recovered without a restart.
    /// Roots that no longer fit under `max_pending_dags` or their peer's quota
    /// keep their record for the next sweep, which resumes after the place
    /// this one admitted at last and gives each peer's places to its roots in
    /// turn after the one it admitted last. Returns the count re-driven.
    pub async fn resync_persisted_pending_dags(&self) -> usize {
        let Some(store) = self.pending_store() else {
            return 0;
        };
        // Cheap steady-state exit: nothing to reconcile while every durable
        // root still has a live, unexpired in-memory entry. An expired entry
        // must not mask its record, eviction is lazy, and the record is the
        // only remaining owner of the recovery obligation. Every Nth sweep is
        // forced past this exit so an orphan record (present in the store but
        // absent from the accounting set after a rare reserve/put race) is
        // still rediscovered.
        let forced = self
            .pending_resync_tick
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .is_multiple_of(super::PENDING_RESYNC_FORCED_TICK);
        if !forced {
            let roots: Vec<Cid> = self.persisted.roots();
            let now = Instant::now();
            let all_live = !roots.is_empty()
                && self.pending_dags.read(|pending| {
                    roots
                        .iter()
                        .all(|root| pending.get(root).is_some_and(|dag| !dag.is_expired(now)))
                });
            if all_live {
                return 0;
            }
        }
        if self
            .pending_resync_in_flight
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return 0;
        }
        let _in_flight = PendingResyncInFlight(&self.pending_resync_in_flight);
        self.resync_persisted_pending_dags_inner(store).await
    }

    async fn resync_persisted_pending_dags_inner(
        &self,
        store: Arc<dyn crate::sync::pending_store::PendingDagStorage>,
    ) -> usize {
        // The sweep's transaction rebuilds the view of what is durable from
        // the records it read, in commit-receipt order, so it neither undoes
        // a later registration nor revives a later removal. A record that
        // comes or goes during the sweep makes it run again; when a busy
        // store keeps it from committing, the records of one snapshot are
        // still re-driven and the view is rebuilt by a later sweep.
        let records = match store.sweep(&self.persisted).await {
            Ok(records) => records,
            Err(error) => {
                tracing::debug!(error = %error, "Pending DAG sweep did not commit; re-driving a snapshot");
                match store.load_all().await {
                    Ok(records) => records,
                    Err(error) => {
                        tracing::warn!(error = %error, "Failed to load persisted pending DAG records");
                        return 0;
                    }
                }
            }
        };
        if records.is_empty() {
            return 0;
        }
        let mut peer_cursors = self.pending_resync_peer_cursors.load_clone();
        let records = self.records_after_resync_cursor(records, &mut peer_cursors);

        let mut restored = 0usize;
        let mut waiting = 0usize;
        let mut last_admitted = None;
        for (place, root_cid, mut record) in records {
            let now = Instant::now();
            let live = self
                .pending_dags
                .read(|pending| pending.get(&root_cid).map(|dag| !dag.is_expired(now)));
            match live {
                Some(true) => continue,
                Some(false) => {
                    // Expired in-memory entry: evict it here so the
                    // record below re-registers with a fresh TTL and a
                    // recomputed missing set.
                    self.pending_dags.update(|pending| {
                        if pending
                            .get(&root_cid)
                            .is_some_and(|dag| dag.is_expired(now))
                        {
                            pending.remove(&root_cid);
                        }
                    });
                }
                None => {}
            }
            if matches!(self.is_merged(&root_cid).await, Ok(true)) {
                self.remove_persisted_pending(&root_cid).await;
                continue;
            }

            // A quarantined root's live record is a leftover, not a
            // recovery obligation. Steady state never reaches this branch
            // (quarantine_pending_dag's own delete already removed the live
            // record before this sweep ran); it only fires for the crash
            // window between that write and delete (steps (2)->(3)). Either
            // way the quarantine record is authoritative, so clean up the
            // leftover instead of re-registering a root that will only be
            // rejected again.
            if matches!(store.is_quarantined(&root_cid).await, Ok(true)) {
                self.remove_persisted_pending(&root_cid).await;
                continue;
            }

            // A root the map has no place for keeps its record for a later
            // sweep without its block being read or its DAG walked: past the
            // cap such roots are routine (heads acknowledged on their durable
            // record alone). Only a root whose scope has an in-memory head it
            // may replace is read, for the heads it links.
            if !self.restored_root_may_fit(root_cid, &record) {
                waiting += 1;
                continue;
            }

            // The root block names the heads it links, which decide whether it
            // takes over its scope's in-memory head.
            let data = match self.blockstore.get(&root_cid).await {
                Ok(Some(data)) => Some(data),
                _ => None,
            };
            let block = data
                .as_deref()
                .and_then(|data| defra_core::Block::from_dag_cbor(data).ok());
            let decoded_head_priority = block.as_ref().map(|block| block.delta.priority());
            let parents = block.and_then(|block| block.heads).unwrap_or_default();

            // A root that does not link that head waits too, its DAG unwalked.
            if !self.restored_root_fits(root_cid, &parents, &record) {
                waiting += 1;
                continue;
            }

            let missing = match &data {
                Some(data) => find_all_missing_links(self.blockstore.as_ref(), data)
                    .await
                    .unwrap_or_else(|_| vec![root_cid]),
                None => vec![root_cid],
            };
            let head_priority = decoded_head_priority.or(record.head_priority);

            // One-way upgrade for records written before scope-current
            // coalescing: once the root block is locally decodable, persist
            // its monotonic priority so restart/TTL admission remains able to
            // retire obsolete heads without retaining payload/CID ledgers.
            if record.head_priority.is_none() && head_priority.is_some() {
                record.head_priority = head_priority;
                let upgrade = Registration {
                    root: root_cid,
                    parents: &[],
                    record: &record,
                    capacity: self.persisted_pending_capacity(),
                    peer_capacity: self.max_persisted_pending_per_peer(),
                };
                if let Err(error) = store.register(upgrade, &self.persisted).await {
                    tracing::warn!(
                        root_cid = %root_cid,
                        error = %error,
                        "Failed to upgrade legacy pending DAG scope metadata; obligation remains independently replayable"
                    );
                }
            }

            let dag = PendingDag {
                doc_id: record.doc_id.clone(),
                collection_id: record.collection_id.clone(),
                head_priority,
                creator: record.creator.clone(),
                missing: missing.iter().copied().collect(),
                source_peer: record.source_peer.clone(),
                alternate_providers: record
                    .alternate_providers
                    .iter()
                    .take(crate::sync::pending_store::MAX_PENDING_DAG_ALTERNATE_PROVIDERS)
                    .cloned()
                    .collect(),
                is_explicit_replicator: record.is_explicit_replicator,
                explicit_replay_authorization: record
                    .explicit_replay_authorization
                    .clone()
                    .map(Into::into),
                is_recovery_registered: true,
                inserted_at: Instant::now(),
                attempts: 0,
                fetch_failures: 0,
                last_fetch_error: None,
                next_retry_at: n0_future::time::Instant::now(),
                dispatches: 0,
                incomplete_attempts: 0,
                merge_continuation: false,
                storage_blocker: None,
                origin: PendingOrigin::Restored,
                estimates: crate::sync::arbitration::RootEstimates::default(),
            };

            let superseded = match self.try_insert_pending_dag(root_cid, &parents, dag.clone()) {
                PendingDagAdmission::Admitted { superseded } => *superseded,
                PendingDagAdmission::GlobalCapacity | PendingDagAdmission::PeerQuota { .. } => {
                    tracing::warn!(
                        root_cid = %root_cid,
                        doc_id = %record.doc_id,
                        max = self.max_pending_dags,
                        max_per_peer = self.max_pending_dags_per_peer(),
                        source_peer = ?record.source_peer,
                        "Pending DAGs at capacity during resync; record kept for the next sweep"
                    );
                    continue;
                }
            };
            if let Some((old_root, _)) = superseded {
                // Both records survived the restart, so deleting the older
                // one after the newer one is live cannot create an ownership
                // gap. A failed idempotent delete remains visible and the
                // forced resync sweep retries it.
                self.remove_persisted_pending(&old_root).await;
            }

            if missing.is_empty() {
                if let Err(error) = self.retry_pending_dag(&root_cid).await {
                    tracing::warn!(
                        root_cid = %root_cid,
                        error = %error,
                        "Failed to resolve complete restored pending DAG"
                    );
                }
            } else {
                // Restored incomplete roots stay immediately due. The same
                // receiver clock that owns live registrations performs the
                // fetch claim; restart is not an alternate dispatch path.
                tracing::debug!(
                    root_cid = %root_cid,
                    missing_count = missing.len(),
                    "Restored pending DAG left due for receiver clock"
                );
            }
            restored += 1;
            last_admitted = Some(place);
            if let Some(peer) = &record.source_peer {
                peer_cursors.insert(peer.clone(), root_cid);
            }
        }
        if last_admitted.is_some() {
            self.pending_resync_cursor.store(last_admitted);
        }
        self.pending_resync_peer_cursors.store(peer_cursors);

        if waiting > 0 {
            tracing::debug!(
                waiting,
                max = self.max_pending_dags,
                "Durable pending DAG roots wait for an in-memory entry"
            );
        }
        if restored > 0 {
            tracing::info!(restored, "restored persisted pending DAG registrations");
        }
        restored
    }

    /// Whether the map could take a durable root that has no entry, before its
    /// block is read: `restored_root_fits` with no parents, or the root's
    /// scope has an in-memory head, which the root replaces if it links it.
    fn restored_root_may_fit(&self, root_cid: Cid, record: &PersistedPendingDag) -> bool {
        self.restored_root_fits(root_cid, &[], record)
            || self.pending_dags.read(|pending| {
                pending.has_scope_head(
                    root_cid,
                    record.source_peer.as_deref(),
                    &record.collection_id,
                    &record.doc_id,
                    record.head_priority,
                )
            })
    }

    /// Whether the map could take a durable root that has no entry: it has a
    /// place and the root's peer is within its quota, or the root links the
    /// in-memory head of its scope as a parent and replaces it. A record
    /// written without a priority may replace one only once its block is
    /// read, so it is let through.
    fn restored_root_fits(
        &self,
        root_cid: Cid,
        parents: &[Cid],
        record: &PersistedPendingDag,
    ) -> bool {
        record.head_priority.is_none()
            || self.pending_dags.read(|pending| {
                (pending.len() < self.max_pending_dags
                    && record.source_peer.as_deref().is_none_or(|peer| {
                        pending.source_count(peer) < self.max_pending_dags_per_peer()
                    }))
                    || pending.scope_head_is_refresh_or_newer(
                        root_cid,
                        parents,
                        record.source_peer.as_deref(),
                        &record.collection_id,
                        &record.doc_id,
                        record.head_priority,
                    )
            })
    }

    /// The records in the order the sweep offers them places, each after the
    /// key of the place it takes. The places run in key order (the store's),
    /// starting after the previous sweep's cursor and wrapping around, and a
    /// source peer's places go to that peer's roots in key order after the
    /// one in `peer_cursors`. Peers left with no record leave `peer_cursors`.
    fn records_after_resync_cursor(
        &self,
        mut records: Vec<(Cid, PersistedPendingDag)>,
        peer_cursors: &mut RapidHashMap<String, Cid>,
    ) -> Vec<(Cid, Cid, PersistedPendingDag)> {
        records.sort_by_cached_key(|(root, _)| root.to_string());
        if let Some(cursor) = self.pending_resync_cursor.load_clone() {
            let cursor = cursor.to_string();
            let start = records.partition_point(|(root, _)| root.to_string() <= cursor);
            records.rotate_left(start);
        }
        let mut peer_places: RapidHashMap<&str, Vec<(usize, String)>> = RapidHashMap::default();
        for (index, (root, record)) in records.iter().enumerate() {
            if let Some(peer) = record.source_peer.as_deref() {
                peer_places
                    .entry(peer)
                    .or_default()
                    .push((index, root.to_string()));
            }
        }
        peer_cursors.retain(|peer, _| peer_places.contains_key(peer.as_str()));
        let mut order: Vec<usize> = (0..records.len()).collect();
        for (peer, places) in &peer_places {
            let Some(cursor) = peer_cursors.get(*peer).map(Cid::to_string) else {
                continue;
            };
            let mut turns: Vec<_> = places.iter().collect();
            turns.sort_unstable_by(|(_, a), (_, b)| a.cmp(b));
            let start = turns.partition_point(|(_, key)| *key <= cursor);
            turns.rotate_left(start);
            for ((place, _), (turn, _)) in places.iter().zip(turns) {
                if let Some(slot) = order.get_mut(*place) {
                    *slot = *turn;
                }
            }
        }
        let places: Vec<Cid> = records.iter().map(|(root, _)| *root).collect();
        let mut records: Vec<_> = records.into_iter().map(Some).collect();
        places
            .into_iter()
            .zip(order)
            .filter_map(|(place, index)| {
                let (root, record) = records.get_mut(index)?.take()?;
                Some((place, root, record))
            })
            .collect()
    }

    /// Quarantine a terminally-rejected pending-DAG root (#1128).
    ///
    /// `MergeOutcome::Rejected` means the merge failed on the block's
    /// *content* (e.g. a unique-index violation) rather than transiently:
    /// replaying it will fail identically every time. Unlike a successful
    /// merge, the block is deliberately left unmerged, a remote re-push can
    /// still retry (sender-paced) and re-quarantine on repeat rejection,
    /// but local re-drive (retry clock, resync sweep) must stop.
    ///
    /// The quarantine record and the deletion of the live `/p2p/pending_dag/`
    /// record commit together. A store written by an earlier build, which
    /// wrote them one after the other, may hold both after a crash; the
    /// resync sweep consults `is_quarantined` before re-registering such a
    /// leftover, so it is deleted instead of re-driving a merge that is
    /// known to fail forever.
    pub async fn quarantine_pending_dag(&self, root_cid: &Cid, reason: &str) {
        let entry = self.build_quarantine_entry(root_cid, reason).await;

        // The quarantine record, the deletion of the live record and its
        // slot, and the gauge's count move in one transaction; a repeat
        // rejection of a root already quarantined does not count again.
        match self.pending_store() {
            Some(store) => {
                if let Err(error) = store.quarantine(root_cid, &entry, &self.persisted).await {
                    tracing::error!(
                        root_cid = %root_cid,
                        error = %error,
                        "Failed to write quarantine record; leaving the live pending-DAG registration in place for retry"
                    );
                    return;
                }
            }
            None => self.persisted.count_quarantined(),
        }

        self.pending_dags.update(|pending| {
            pending.remove(root_cid);
        });
        self.diagnostics.record_pending_dag_terminal_quarantined();

        tracing::warn!(
            root_cid = %root_cid,
            doc_id = %entry.record.doc_id,
            collection_id = %entry.record.collection_id,
            reason = %reason,
            "Pending DAG quarantined: merge deterministically rejected, will not be re-driven locally"
        );
    }

    /// Build the quarantine record for `root_cid`, preferring the most
    /// complete provenance available so quarantine never fails for lack of
    /// it: the live durable record if one exists, else the in-memory
    /// `PendingDag` entry, else an empty-provenance record as a last resort.
    async fn build_quarantine_entry(
        &self,
        root_cid: &Cid,
        reason: &str,
    ) -> PersistedQuarantinedDag {
        let record = match self.load_live_durable_record(root_cid).await {
            Some(record) => record,
            None => self
                .pending_dags
                .read(|pending| {
                    pending.get(root_cid).map(|dag| PersistedPendingDag {
                        doc_id: dag.doc_id.clone(),
                        collection_id: dag.collection_id.clone(),
                        head_priority: dag.head_priority,
                        creator: dag.creator.clone(),
                        source_peer: dag.source_peer.clone(),
                        alternate_providers: dag.alternate_providers.clone(),
                        is_explicit_replicator: dag.is_explicit_replicator,
                        explicit_replay_authorization: dag
                            .explicit_replay_authorization
                            .as_ref()
                            .map(Into::into),
                    })
                })
                .unwrap_or_else(|| PersistedPendingDag {
                    doc_id: String::new(),
                    collection_id: String::new(),
                    head_priority: None,
                    creator: String::new(),
                    source_peer: None,
                    alternate_providers: Vec::new(),
                    is_explicit_replicator: false,
                    explicit_replay_authorization: None,
                }),
        };

        PersistedQuarantinedDag {
            record,
            reason: reason.to_string(),
            quarantined_at_unix_secs: PersistedQuarantinedDag::now_unix_secs(),
        }
    }

    /// Look up the live durable record for `root_cid`, if any. The store has
    /// no by-CID lookup (only `load_all`/prefix scan), so this pays an O(n)
    /// scan of the live keyspace, acceptable because quarantine only fires
    /// on a deterministic content rejection, not the hot merge path.
    async fn load_live_durable_record(&self, root_cid: &Cid) -> Option<PersistedPendingDag> {
        if !self.persisted.contains(root_cid) {
            return None;
        }
        let store = self.pending_store()?;
        match store.load_all().await {
            Ok(records) => records
                .into_iter()
                .find(|(cid, _)| cid == root_cid)
                .map(|(_, record)| record),
            Err(error) => {
                tracing::warn!(
                    root_cid = %root_cid,
                    error = %error,
                    "Failed to load durable pending DAG records while building quarantine entry"
                );
                None
            }
        }
    }
}

#[cfg(test)]
#[path = "pending_dag_tests.rs"]
mod tests;
