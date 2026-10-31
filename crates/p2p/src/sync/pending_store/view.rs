//! What this node holds durably, as the commits that changed it say.
//!
//! The view keeps the roots with a durable record and, for each sender
//! scope, its durable current head, so admission and the resync sweep decide
//! without reading the store. Nothing writes it but a commit of the store:
//! each transition registers one of the callbacks below on its transaction,
//! and the store dispatches them in commit-receipt order. Two transitions of
//! one root therefore land in the order they committed, and a sweep, which
//! rebuilds the view from the records it read, lands between the commits
//! before it and those after it.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use cid::Cid;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashSet;
use storage::corekv::TxnCallback;

use super::PersistedPendingDag;
use crate::sync::head_scope::{HeadVersion, Scope, ScopeDecision};

/// The roots with a durable record and the durable current head of each
/// sender scope, with the bytes the records take per source peer and in all.
pub struct PersistedView {
    roots: HopscotchMap<Cid, Footprint, RandomState>,
    source_bytes: HopscotchMap<String, Arc<AtomicU64>, RandomState>,
    total_bytes: AtomicU64,
    scope_heads: HopscotchMap<Scope, HeadVersion, RandomState>,
    quarantined: AtomicUsize,
}

/// What one durable record takes, charged to its source peer.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Footprint {
    source: Option<String>,
    bytes: u64,
}

impl Footprint {
    fn of(root: &Cid, record: &PersistedPendingDag) -> Self {
        Self {
            source: record.source_peer.clone(),
            bytes: super::store::footprint(root, record),
        }
    }
}

impl Default for PersistedView {
    fn default() -> Self {
        Self {
            roots: HopscotchMap::with_hasher(RandomState::default()),
            source_bytes: HopscotchMap::with_hasher(RandomState::default()),
            total_bytes: AtomicU64::new(0),
            scope_heads: HopscotchMap::with_hasher(RandomState::default()),
            quarantined: AtomicUsize::new(0),
        }
    }
}

impl PersistedView {
    /// Whether a root has a durable record.
    pub fn contains(&self, root: &Cid) -> bool {
        self.roots.contains_key(root)
    }

    /// How many roots have a durable record.
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// Whether no root has a durable record.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// The roots with a durable record.
    pub fn roots(&self) -> Vec<Cid> {
        self.roots.keys().collect()
    }

    /// Bytes the durable records of `source_peer` take.
    pub fn source_bytes(&self, source_peer: &str) -> u64 {
        self.source_bytes
            .get(source_peer)
            .map_or(0, |bytes| bytes.load(Ordering::Relaxed))
    }

    /// Bytes every durable record takes.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }

    fn insert_root(&self, root: Cid, footprint: Footprint) {
        self.charge(&footprint, true);
        if let Some(previous) = self.roots.insert(root, footprint) {
            self.charge(&previous, false);
        }
    }

    fn remove_root(&self, root: &Cid) {
        if let Some(previous) = self.roots.remove(root) {
            self.charge(&previous, false);
        }
    }

    fn charge(&self, footprint: &Footprint, add: bool) {
        let apply = |counter: &AtomicU64| {
            if add {
                counter.fetch_add(footprint.bytes, Ordering::Relaxed);
            } else {
                counter.fetch_sub(footprint.bytes, Ordering::Relaxed);
            }
        };
        apply(&self.total_bytes);
        if let Some(source) = &footprint.source {
            let counter = match self.source_bytes.get(source.as_str()) {
                Some(counter) => counter,
                None => self
                    .source_bytes
                    .get_or_insert(source.clone(), Arc::new(AtomicU64::new(0))),
            };
            apply(&counter);
        }
    }

    /// How many roots are quarantined.
    pub fn quarantined(&self) -> usize {
        self.quarantined.load(Ordering::Relaxed)
    }

    pub(crate) fn set_quarantined(&self, count: usize) {
        self.quarantined.store(count, Ordering::Relaxed);
    }

    /// Counts one more quarantined root.
    pub(crate) fn count_quarantined(&self) {
        self.quarantined.fetch_add(1, Ordering::Relaxed);
    }

    /// What a head is to the durable current head of its scope.
    pub(crate) fn scope_decision(
        &self,
        root: Cid,
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> ScopeDecision {
        match Scope::of(root, source_peer, collection_id, doc_id, head_priority) {
            None => ScopeDecision::Independent,
            Some((scope, version)) => ScopeDecision::of(version, self.scope_heads.get(&scope)),
        }
    }

    /// Whether the head is its scope's durable current head, or newer.
    pub(crate) fn refreshes_or_supersedes(
        &self,
        root: Cid,
        source_peer: Option<&str>,
        collection_id: &str,
        doc_id: &str,
        head_priority: Option<u64>,
    ) -> bool {
        Scope::of(root, source_peer, collection_id, doc_id, head_priority).is_some_and(
            |(scope, version)| {
                ScopeDecision::refreshes_or_supersedes(version, self.scope_heads.get(&scope))
            },
        )
    }

    /// The commit of a registration of `root`, which retired `retired`.
    pub(super) fn registered(
        self: &Arc<Self>,
        root: Cid,
        record: &PersistedPendingDag,
        retired: Option<Cid>,
    ) -> TxnCallback {
        let view = Arc::clone(self);
        let head = scope_head(root, record);
        let footprint = Footprint::of(&root, record);
        Box::new(move || {
            // The retired root shares the scope, whose head this one takes.
            if let Some(retired) = retired {
                view.remove_root(&retired);
            }
            view.insert_root(root, footprint.clone());
            if let Some((scope, version)) = head.clone() {
                view.remember(scope, version);
            }
        })
    }

    /// The commit of a transition that took away `root` and its `record`, if
    /// it had one; a quarantine that is new to the store counts in the gauge.
    pub(super) fn removed(
        self: &Arc<Self>,
        root: Cid,
        record: Option<&PersistedPendingDag>,
        newly_quarantined: bool,
    ) -> TxnCallback {
        let view = Arc::clone(self);
        let scope = record
            .and_then(|record| scope_head(root, record))
            .map(|(scope, _)| scope);
        Box::new(move || {
            view.remove_root(&root);
            if let Some(scope) = &scope {
                if view
                    .scope_heads
                    .get(scope)
                    .is_some_and(|head| head.cid == root)
                {
                    view.scope_heads.remove(scope);
                }
            }
            if newly_quarantined {
                view.count_quarantined();
            }
        })
    }

    /// The commit of a sweep that read `records`: the view becomes them.
    pub(super) fn swept(self: &Arc<Self>, records: &[(Cid, PersistedPendingDag)]) -> TxnCallback {
        let view = Arc::clone(self);
        let roots: RapidHashSet<Cid> = records.iter().map(|(root, _)| *root).collect();
        let footprints: Vec<(Cid, Footprint)> = records
            .iter()
            .map(|(root, record)| (*root, Footprint::of(root, record)))
            .collect();
        let heads: Vec<(Scope, HeadVersion)> = records
            .iter()
            .filter_map(|(root, record)| scope_head(*root, record))
            .collect();
        Box::new(move || {
            let stale_roots: Vec<Cid> = view
                .roots
                .keys()
                .filter(|root| !roots.contains(root))
                .collect();
            for root in stale_roots {
                view.remove_root(&root);
            }
            for (root, footprint) in &footprints {
                view.insert_root(*root, footprint.clone());
            }
            let stale_scopes: Vec<Scope> = view
                .scope_heads
                .iter()
                .filter(|(_, head)| !roots.contains(&head.cid))
                .map(|(scope, _)| scope)
                .collect();
            for scope in stale_scopes {
                view.scope_heads.remove(&scope);
            }
            for (scope, version) in &heads {
                view.remember(scope.clone(), *version);
            }
        })
    }

    /// Keeps `version` as its scope's head unless a newer one is kept.
    fn remember(&self, scope: Scope, version: HeadVersion) {
        if self
            .scope_heads
            .get(&scope)
            .is_none_or(|current| version >= current)
        {
            self.scope_heads.insert(scope, version);
        }
    }
}

/// The scope and version of a record's head, if it belongs to a scope.
pub(super) fn scope_head(root: Cid, record: &PersistedPendingDag) -> Option<(Scope, HeadVersion)> {
    Scope::of(
        root,
        record.source_peer.as_deref(),
        &record.collection_id,
        &record.doc_id,
        record.head_priority,
    )
}
