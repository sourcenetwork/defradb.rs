//! Pending DAG tracking for Bitswap synchronization.

use rapidhash::{RapidHashMap, RapidHashSet};
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use cid::Cid;
use kovan::Atom;

use crate::sync::head_scope::{HeadVersion, Scope, ScopeDecision};
use crate::ExplicitReplayAuthorization;

/// Time-to-live for a pending DAG entry.
///
/// Entries older than this are evicted during insertion to prevent
/// indefinitely stale DAGs from accumulating.
pub const PENDING_DAG_TTL: Duration = Duration::from_secs(300);

/// Metadata for a pending DAG sync waiting for Bitswap to complete.
#[derive(Debug, Clone)]
pub struct PendingDag {
    /// Document ID from the original PushLog message
    pub doc_id: String,
    /// Collection ID from the original PushLog message
    pub collection_id: String,
    /// Monotonic priority decoded from the announced composite/collection
    /// head. `None` disables scope retirement because subsumption is unknown.
    pub head_priority: Option<u64>,
    /// Creator from the original PushLog message
    pub creator: String,
    /// CIDs still missing (gets smaller as blocks arrive via Bitswap)
    #[allow(dead_code)] // Used for tracking Bitswap progress
    pub missing: RapidHashSet<Cid>,
    /// The peer that originally provided this DAG (e.g. DocSync reply sender).
    /// Always included in the Bitswap provider list during retries so the
    /// blocks can be fetched even if the peer isn't in connected_peers().
    pub source_peer: Option<String>,
    /// Authenticated same-root announcers retained as recovery alternates.
    /// These never replace `source_peer` or reset the receiver retry clock.
    pub alternate_providers: Vec<String>,
    /// True when the DAG originated from an explicit replicator push.
    pub is_explicit_replicator: bool,
    /// Capability-based explicit replay authorization carried by two-stream pushes.
    pub explicit_replay_authorization: Option<ExplicitReplayAuthorization>,
    /// Whether a success reply can rely on this entry for eventual recovery.
    pub is_recovery_registered: bool,
    /// When this entry was inserted (for TTL eviction).
    pub inserted_at: Instant,
    /// How many times `retry_pending_dag` has been invoked for this root.
    ///
    /// Surfaced in diagnostic logs so the single aggregated WARN on terminal
    /// failure can carry an attempt count without scraping intermediate noise.
    pub attempts: u32,
    /// Number of Bitswap/CAR fetch rounds that exhausted providers without
    /// yielding a complete DAG for this root.
    pub fetch_failures: u32,
    /// Most recent provider-exhaustion error for this root, if any.
    pub last_fetch_error: Option<String>,
    /// Earliest instant a fetch dispatch may be claimed for this root.
    /// Every dispatch site must win `try_claim_pending_dag_dispatch`,
    /// which advances this by `retry_backoff(dispatches)`, the receiver's
    /// re-arm pacing (#1112 inbound half, #1116 stage 2).
    pub next_retry_at: n0_future::time::Instant,
    /// Fetch dispatches claimed for this root (drives the backoff rung).
    pub dispatches: u32,
    /// Committed merge work yielded with a locally ready DAG. Never persisted:
    /// restart/TTL recovery must validate local availability again.
    pub merge_continuation: bool,
    /// Transient local contention; discarded with this bounded pending root.
    pub storage_blocker: Option<Cid>,
}

/// Pending roots and the reverse index used to route arriving blocks only to
/// roots that are waiting for them.
///
/// Values sit behind `Arc` so a copy of the registry shares every untouched
/// entry and a write pays for the entries it changes.
#[derive(Debug, Default, Clone)]
pub(super) struct PendingDagRegistry {
    roots: RapidHashMap<Cid, Arc<PendingDag>>,
    waiters: RapidHashMap<Cid, Arc<RapidHashSet<Cid>>>,
    roots_by_source: RapidHashMap<Arc<str>, usize>,
    current_by_source_scope: RapidHashMap<Arc<Scope>, HeadVersion>,
}

impl Deref for PendingDagRegistry {
    type Target = RapidHashMap<Cid, Arc<PendingDag>>;

    fn deref(&self) -> &Self::Target {
        &self.roots
    }
}

impl PendingDagRegistry {
    /// The current head of `scope` among the pending roots.
    fn current(&self, scope: &Scope) -> Option<HeadVersion> {
        self.current_by_source_scope.get(scope).copied()
    }

    pub(super) fn scope_head_is_refresh_or_newer(
        &self,
        root_cid: Cid,
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> bool {
        Scope::of(root_cid, source_peer, collection_id, doc_id, head_priority).is_some_and(
            |(scope, version)| {
                ScopeDecision::refreshes_or_supersedes(version, self.current(&scope))
            },
        )
    }

    /// True when a newer head for the same sender scope is the current
    /// pending root, so this root is subsumed by an obligation that is
    /// already registered.
    pub(super) fn scope_head_is_covered_by_current(
        &self,
        root_cid: Cid,
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> bool {
        Scope::of(root_cid, source_peer, collection_id, doc_id, head_priority)
            .is_some_and(|(scope, version)| ScopeDecision::covered(version, self.current(&scope)))
    }

    fn scope_of(root_cid: Cid, dag: &PendingDag) -> Option<(Scope, HeadVersion)> {
        Scope::of(
            root_cid,
            dag.source_peer.as_deref(),
            &dag.collection_id,
            &dag.doc_id,
            dag.head_priority,
        )
    }

    pub(super) fn scope_head_decision(&self, root_cid: Cid, dag: &PendingDag) -> ScopeDecision {
        match Self::scope_of(root_cid, dag) {
            None => ScopeDecision::Independent,
            Some((scope, version)) => ScopeDecision::of(version, self.current(&scope)),
        }
    }

    /// Whether any entry has outlived `PENDING_DAG_TTL` at `now`.
    pub(super) fn has_expired(&self, now: Instant) -> bool {
        self.roots
            .values()
            .any(|dag| now.duration_since(dag.inserted_at) >= PENDING_DAG_TTL)
    }

    pub(super) fn snapshot(&self, root_cid: &Cid) -> Option<PendingDag> {
        self.roots.get(root_cid).map(|dag| PendingDag::clone(dag))
    }

    pub(super) fn get_mut(&mut self, root_cid: &Cid) -> Option<&mut PendingDag> {
        self.roots.get_mut(root_cid).map(Arc::make_mut)
    }

    pub(super) fn insert(&mut self, root_cid: Cid, dag: PendingDag) -> Option<PendingDag> {
        let previous = self.remove(&root_cid);
        for missing_cid in &dag.missing {
            Arc::make_mut(self.waiters.entry(*missing_cid).or_default()).insert(root_cid);
        }
        if let Some(source_peer) = &dag.source_peer {
            match self.roots_by_source.get_mut(source_peer.as_str()) {
                Some(count) => *count += 1,
                None => {
                    self.roots_by_source
                        .insert(Arc::from(source_peer.as_str()), 1);
                }
            }
        }
        if let Some((scope, version)) = Self::scope_of(root_cid, &dag) {
            self.current_by_source_scope
                .insert(Arc::new(scope), version);
        }
        self.roots.insert(root_cid, Arc::new(dag));
        previous
    }

    pub(super) fn remove(&mut self, root_cid: &Cid) -> Option<PendingDag> {
        let dag = self.roots.remove(root_cid)?;
        for missing_cid in &dag.missing {
            self.remove_waiter(missing_cid, root_cid);
        }
        if let Some(source_peer) = &dag.source_peer {
            let remove_source = self
                .roots_by_source
                .get_mut(source_peer.as_str())
                .is_some_and(|count| {
                    debug_assert!(*count > 0);
                    *count = count.saturating_sub(1);
                    *count == 0
                });
            if remove_source {
                self.roots_by_source.remove(source_peer.as_str());
            }
        }
        if let Some((scope, _)) = Self::scope_of(*root_cid, &dag) {
            if self
                .current(&scope)
                .is_some_and(|current| current.cid == *root_cid)
            {
                self.current_by_source_scope.remove(&scope);
            }
        }
        Some(Arc::try_unwrap(dag).unwrap_or_else(|shared| PendingDag::clone(&shared)))
    }

    pub(super) fn source_count(&self, source_peer: &str) -> usize {
        self.roots_by_source
            .get(source_peer)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn replace_missing(&mut self, root_cid: &Cid, missing: RapidHashSet<Cid>) -> bool {
        let Some(previous) = self.roots.get(root_cid).map(|dag| dag.missing.clone()) else {
            return false;
        };

        for missing_cid in previous.difference(&missing) {
            self.remove_waiter(missing_cid, root_cid);
        }
        for missing_cid in missing.difference(&previous) {
            Arc::make_mut(self.waiters.entry(*missing_cid).or_default()).insert(*root_cid);
        }
        if let Some(dag) = self.roots.get_mut(root_cid) {
            Arc::make_mut(dag).missing = missing;
        }
        true
    }

    pub(super) fn waiting_roots(&self, missing_cid: &Cid) -> Vec<Cid> {
        self.waiters
            .get(missing_cid)
            .map(|roots| roots.iter().copied().collect())
            .unwrap_or_default()
    }

    pub(super) fn has_waiters(&self, missing_cid: &Cid) -> bool {
        self.waiters.contains_key(missing_cid)
    }

    pub(super) fn advance_waiters(
        &mut self,
        received_cid: &Cid,
        newly_missing: &[Cid],
    ) -> Vec<Cid> {
        let waiting_roots: Vec<Cid> = self
            .waiters
            .remove(received_cid)
            .map(|roots| roots.iter().copied().collect())
            .unwrap_or_default();
        let mut emptied = Vec::new();

        for root_cid in waiting_roots {
            let Some(dag) = self.roots.get_mut(&root_cid).map(Arc::make_mut) else {
                continue;
            };
            if !dag.missing.remove(received_cid) {
                continue;
            }
            for missing_cid in newly_missing {
                if dag.missing.insert(*missing_cid) {
                    Arc::make_mut(self.waiters.entry(*missing_cid).or_default()).insert(root_cid);
                }
            }
            if dag.missing.is_empty() {
                emptied.push(root_cid);
            }
        }

        emptied
    }

    fn remove_waiter(&mut self, missing_cid: &Cid, root_cid: &Cid) {
        let remove_entry = self.waiters.get_mut(missing_cid).is_some_and(|roots| {
            let roots = Arc::make_mut(roots);
            roots.remove(root_cid);
            roots.is_empty()
        });
        if remove_entry {
            self.waiters.remove(missing_cid);
        }
    }
}

/// The registry shared by every sync path, replaced as one unit on each
/// write: admission reads the capacity, the per-source quota and the scope
/// head before inserting, and an insert or removal touches all four indexes.
#[derive(Debug, Default)]
pub(super) struct SharedPendingDags(Atom<PendingDagRegistry>);

impl SharedPendingDags {
    pub(super) fn read<R>(&self, f: impl FnOnce(&PendingDagRegistry) -> R) -> R {
        self.0.peek(f)
    }

    /// Apply `f` to a copy of the registry and publish it. `f` may run more
    /// than once under contention, so it must stay pure.
    pub(super) fn update<R>(&self, mut f: impl FnMut(&mut PendingDagRegistry) -> R) -> R {
        loop {
            let current = self.0.load();
            let mut next = PendingDagRegistry::clone(&current);
            let result = f(&mut next);
            if self.0.compare_and_swap(&current, next).is_ok() {
                return result;
            }
        }
    }
}

/// Base delay before the first scheduled re-dispatch of a pending root.
pub const PENDING_RETRY_BASE: Duration = Duration::from_secs(2);
/// Local committed work is immediately eligible; the bounded scheduler owns fairness.
pub const PENDING_MERGE_CONTINUATION_DELAY: Duration = Duration::ZERO;
/// Ceiling for the per-root dispatch backoff.
pub const PENDING_RETRY_CAP: Duration = Duration::from_secs(60);

/// Seconds from the first fetch dispatch of a pending root to the fifth, when
/// every dispatch in between fails to complete the DAG.
///
/// A pushed block whose DAG is incomplete is acked as success once it is
/// registered pending, so the sender stops retrying and the receiver owns
/// recovery on this ladder alone. Anything that waits on convergence has to
/// allow at least this long, or it reports a fault where the pacing is simply
/// still running; `a_root_that_keeps_failing_is_not_retried_for_a_minute`
/// pins the arithmetic.
pub const PENDING_RECOVERY_WORST_CASE_SECS: u64 = 60;

/// Capped exponential backoff for pending-DAG fetch dispatches: 2s, 4s,
/// 8s, 16s, 32s, then 60s forever.
pub fn retry_backoff(dispatches: u32) -> Duration {
    let shifted = PENDING_RETRY_BASE.saturating_mul(1u32 << dispatches.min(5));
    shifted.min(PENDING_RETRY_CAP)
}
