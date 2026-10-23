//! Database merge handler for processing incoming P2P blocks.
//!
//! This module implements the `MergeHandler` trait from the P2P layer,
//! bridging incoming blocks to the CRDT system for document merging.

mod authorization;
mod batch;
mod block_reads;
mod collection;
pub mod composite;
mod composite_fields;
mod composite_heads;
pub mod composite_persist;
mod counter;
mod definition;
mod dispatch;
mod doc_identity;
mod encryption;
pub(crate) mod error;
mod frame_source;
pub mod hook;
mod lww;
mod merge_keys;
mod protected_update;
mod recovery;
pub mod se_merge;
mod signature;
pub(crate) use signature::verify_signature_data;

pub use error::MergeError;
pub(crate) use error::{CounterMergeResult, LwwMergeResult};

use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use rapidhash::{RapidHashMap, RapidHashSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use cid::Cid;
use crdt::traits::{Context, ReplicatedData, ValueReader};
use crdt::{Counter, CounterDelta, Lww, LwwDelta, NumericKind};
use datastore::NamespaceView;
use defra_core::block::{
    Block, CollectionDefinitionDeltaPayload, CrdtDelta, FieldDefinitionDeltaPayload,
    IMMUTABLE_LINK_NAME,
};
use defra_core::merge::{BlockMetadata, MergeErrorDisposition, MergeHandler, MergeOutcome};
use defra_core::types::DocId;
use document::{DocID, Document, NormalValue};
use events::{MergeCompleteData, Message, Update};
use schema::{
    self, CType, CollectionSource, CollectionVersion, FieldDescription, FieldKind, QuerySource,
    ScalarKind,
};
use storage::corekv::{Key, Store};
use storage::keys::systemstore::{CollectionKey, CollectionVersionKey};
use zeroize::Zeroizing;

use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use hook::CompositeMergeHook;

/// Default maximum parent-chain depth for merge operations.
///
/// Bounds the work and heap used while traversing a malicious or corrupt DAG,
/// while leaving room for long-lived documents with thousands of updates.
/// Valid depths are `0..DEFAULT_MAX_MERGE_DEPTH`.
pub const DEFAULT_MAX_MERGE_DEPTH: usize = 8192;

/// A lock-free set of block CIDs.
pub type CidSet = HopscotchMap<Cid, (), RandomState>;

/// Decryption keys fetched outside the engine-owned attempts of one merge,
/// shared by those attempts without a lock.
pub(crate) type RetryDecryptionKeys = Arc<HopscotchMap<Cid, Zeroizing<[u8; 32]>, RandomState>>;

/// An empty [`RetryDecryptionKeys`].
pub(crate) fn retry_decryption_keys() -> RetryDecryptionKeys {
    Arc::new(HopscotchMap::with_hasher(RandomState::default()))
}

/// The error for a merge attempt that asked again for a key it was given:
/// running it again would only ask again.
pub(crate) fn repeated_key_request(encryption_cid: &Cid) -> MergeError {
    MergeError::MergeFailed(format!(
        "merge requested decryption key {encryption_cid} again after it was provided"
    ))
}

/// An empty [`CidSet`].
pub fn cid_set() -> CidSet {
    CidSet::with_hasher(RandomState::default())
}

/// Database merge handler that processes incoming P2P blocks.
///
/// This handler decodes IPLD blocks, extracts CRDT deltas, and applies
/// them to the database using the appropriate CRDT type.
pub struct DbMergeHandler<S: Store, B: blockstore::Blockstore> {
    /// Reference to the database for creating transactions.
    pub(crate) db: Arc<DB<S>>,
    /// Reference to the blockstore for loading linked blocks.
    pub(crate) blockstore: Arc<B>,
    /// Shared parent-chain depth policy for identity and merge traversals.
    max_merge_depth: usize,
    /// Optional merge hook for policy-specific behavior around composite merges.
    composite_merge_hook: Arc<regolith::sync::OnceCell<Arc<dyn CompositeMergeHook>>>,
    /// Tracks composite CIDs that have already been merged, preventing
    /// duplicate processing from concurrent dual-broadcast paths (doc topic
    /// + collection topic). Matches Go's `loadComposites` dedup guard.
    pub(crate) merged_composites: Arc<CidSet>,
    /// Tracks collection CIDs that have already been merged, preventing
    /// replayed collection blocks from re-adding obsolete collection heads.
    pub(crate) merged_collections: Arc<CidSet>,
    /// Optional SE encryption key for generating search artifacts on replicated documents.
    /// When set, the merge handler generates SE artifacts after merging documents
    /// that belong to collections with encrypted indexes.
    se_enc_key: Arc<regolith::sync::OnceCell<Zeroizing<Vec<u8>>>>,
    /// Optional repusher for fanning SE artifacts to this node's replicators
    /// after a merge commits. Mirrors Go, where a merging node re-enters
    /// `SendUpdate` and runs the SE push handler (`internal/db/p2p/p2p.go:709`).
    #[cfg(not(target_arch = "wasm32"))]
    se_repusher: Arc<regolith::sync::OnceCell<Arc<dyn crate::merge::SeArtifactRepusher>>>,
    /// Optional KMS service. When set, `decrypt_block_data` routes DEK
    /// retrieval through the KMS (NAC/DAC-gated, cross-peer fetch) instead
    /// of reading the raw key directly from the Encryption block.
    kms: Arc<regolith::sync::OnceCell<Arc<dyn kms::KmsService>>>,
    /// Request-scoped DEKs prepared outside an engine-owned merge attempt.
    /// `None` means this handler is on the legacy non-retry path.
    retry_decryption_keys: Option<RetryDecryptionKeys>,
    retry_prefetch_requested: Option<Arc<AtomicBool>>,
    /// Keys fetched before this merge's own transaction (see
    /// `merge_keys`): consulted before any request to the KMS.
    prepared_keys: Option<merge_keys::PreparedMergeKeys>,
    /// Encryption CIDs with a background DEK prefetch currently in flight, so
    /// repeated deliveries of the same deferred field block
    /// (pushlog + gossip + retries) don't fan out duplicate cross-peer
    /// fetches.
    prefetched_dek_cids: Arc<CidSet>,
    /// Composites a merge validator deferred, by the CIDs they await.
    pub(crate) deferred: Arc<crate::merge::governance::DeferredMerges>,
    /// Composites a merge validator rejected. A reject rests on present bytes
    /// and never changes, and the block stays in the blockstore's unmerged
    /// set, so without this the governance sweep would re-judge every
    /// rejected composite each tick and, past its budget, never reach a
    /// deferred one. In memory: after a restart each is re-judged once.
    pub(crate) rejected_governed: Arc<CidSet>,
    /// Where composites merged by re-drive are reported, so they take the
    /// same post-merge path as a first-attempt merge.
    redriven_sink:
        Arc<regolith::sync::OnceCell<Arc<dyn crate::merge::governance::RedrivenMergeSink>>>,
}

impl<S: Store, B: blockstore::Blockstore> Clone for DbMergeHandler<S, B> {
    fn clone(&self) -> Self {
        Self {
            db: Arc::clone(&self.db),
            blockstore: Arc::clone(&self.blockstore),
            max_merge_depth: self.max_merge_depth,
            composite_merge_hook: Arc::clone(&self.composite_merge_hook),
            merged_composites: Arc::clone(&self.merged_composites),
            merged_collections: Arc::clone(&self.merged_collections),
            se_enc_key: Arc::clone(&self.se_enc_key),
            #[cfg(not(target_arch = "wasm32"))]
            se_repusher: Arc::clone(&self.se_repusher),
            kms: Arc::clone(&self.kms),
            retry_decryption_keys: self.retry_decryption_keys.clone(),
            retry_prefetch_requested: self.retry_prefetch_requested.clone(),
            prepared_keys: self.prepared_keys.clone(),
            prefetched_dek_cids: Arc::clone(&self.prefetched_dek_cids),
            deferred: Arc::clone(&self.deferred),
            rejected_governed: Arc::clone(&self.rejected_governed),
            redriven_sink: Arc::clone(&self.redriven_sink),
        }
    }
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(super) fn with_retry_decryption_context(
        &self,
        keys: RetryDecryptionKeys,
        prefetch_requested: Arc<AtomicBool>,
    ) -> Self {
        let mut handler = self.clone();
        handler.retry_decryption_keys = Some(keys);
        handler.retry_prefetch_requested = Some(prefetch_requested);
        handler
    }

    pub(super) fn retry_decryption_keys(&self) -> Option<&RetryDecryptionKeys> {
        self.retry_decryption_keys.as_ref()
    }

    pub(super) fn request_retry_dek_prefetch(&self) -> bool {
        if let Some(requested) = &self.retry_prefetch_requested {
            requested.store(true, std::sync::atomic::Ordering::Release);
            true
        } else {
            false
        }
    }

    pub(super) fn reset_retry_dek_prefetch_request(&self) {
        if let Some(requested) = &self.retry_prefetch_requested {
            requested.store(false, std::sync::atomic::Ordering::Release);
        }
    }

    /// The database handle this merge handler writes through.
    pub fn db(&self) -> &Arc<DB<S>> {
        &self.db
    }

    /// The active collection a block belongs to, by its collection version id,
    /// else by the id its metadata carries. Either id may name a version or
    /// the collection itself: the two are equal for a collection's first
    /// version and differ after a patch, and metadata recovered from a block
    /// carries the version. A version no longer active is looked up in the
    /// store for the collection id it belongs to. A lookup error is an
    /// error, never an absent collection, so the collection guard is never
    /// skipped on one.
    pub(crate) async fn block_collection(
        &self,
        collection_version_id: &str,
        fallback_collection_id: Option<&str>,
    ) -> std::result::Result<Option<Collection>, MergeError> {
        let ids = || std::iter::once(collection_version_id).chain(fallback_collection_id);
        for id in ids() {
            if let Some(collection) = self.db.get_collection_by_version_id(id)? {
                return Ok(Some(collection));
            }
            if let Some(collection) = self.db.find_collection_by_id(id)? {
                return Ok(Some(collection));
            }
        }
        for id in ids() {
            if let Some(stored) = self.db.get_collection_by_version_id_full(id).await? {
                // Prefer the cached entry, which carries index action state,
                // but fall back to the definition just read from the store. A
                // collection can be durably known and absent from the cache,
                // which is keyed by name and so holds one collection per name.
                return Ok(Some(
                    self.db
                        .find_collection_by_id(stored.collection_id())?
                        .unwrap_or(stored),
                ));
            }
        }
        Ok(None)
    }

    /// Collection-definition blocks already merged in this process.
    pub fn merged_collections(&self) -> &CidSet {
        &self.merged_collections
    }

    /// DEK block CIDs whose prefetch has already been spawned.
    pub fn prefetched_dek_cids(&self) -> &Arc<CidSet> {
        &self.prefetched_dek_cids
    }

    /// Create a new database merge handler.
    pub fn new(db: Arc<DB<S>>, blockstore: Arc<B>) -> Self {
        Self::new_with_max_merge_depth(db, blockstore, DEFAULT_MAX_MERGE_DEPTH)
    }

    /// Create a merge handler with an explicit parent-chain depth limit.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    pub fn new_with_max_merge_depth(
        db: Arc<DB<S>>,
        blockstore: Arc<B>,
        max_merge_depth: usize,
    ) -> Self {
        Self {
            db,
            blockstore,
            max_merge_depth,
            composite_merge_hook: Arc::new(regolith::sync::OnceCell::new()),
            merged_composites: Arc::new(cid_set()),
            merged_collections: Arc::new(cid_set()),
            se_enc_key: Arc::new(regolith::sync::OnceCell::new()),
            #[cfg(not(target_arch = "wasm32"))]
            se_repusher: Arc::new(regolith::sync::OnceCell::new()),
            kms: Arc::new(regolith::sync::OnceCell::new()),
            retry_decryption_keys: None,
            retry_prefetch_requested: None,
            prepared_keys: None,
            prefetched_dek_cids: Arc::new(cid_set()),
            deferred: Arc::new(Default::default()),
            rejected_governed: Arc::new(cid_set()),
            redriven_sink: Arc::new(regolith::sync::OnceCell::new()),
        }
    }

    /// Lower the deferred index's capacity, so a test can reach the
    /// at-capacity path without indexing
    /// [`crate::merge::governance::MAX_DEFERRED_COMPOSITES`] composites first.
    /// Nothing in production calls this.
    pub fn set_deferred_capacity(&self, capacity: usize) {
        self.deferred.set_capacity(capacity);
    }

    /// Report composites merged by re-drive to `sink`, which marks them merged
    /// and fans them out as the replication layer does a first-attempt merge.
    pub fn set_redriven_merge_sink(
        &self,
        sink: Arc<dyn crate::merge::governance::RedrivenMergeSink>,
    ) {
        let _ = self.redriven_sink.set(sink);
    }

    pub(crate) fn redriven_merge_sink(
        &self,
    ) -> Option<&Arc<dyn crate::merge::governance::RedrivenMergeSink>> {
        self.redriven_sink.get()
    }

    /// Enforce the same parent-chain depth policy for every merge traversal.
    pub fn ensure_merge_depth(
        &self,
        cid: &Cid,
        depth: usize,
    ) -> std::result::Result<(), MergeError> {
        if depth >= self.max_merge_depth {
            return Err(MergeError::depth_exceeded(cid, depth));
        }
        Ok(())
    }

    /// Set the composite merge hook after construction.
    pub fn set_composite_merge_hook(&self, hook: Arc<dyn CompositeMergeHook>) {
        let _ = self.composite_merge_hook.set(hook);
    }

    pub(crate) fn composite_merge_hook(&self) -> Option<&Arc<dyn CompositeMergeHook>> {
        self.composite_merge_hook.get()
    }

    /// Set the SE encryption key for generating artifacts on replicated documents.
    pub fn set_se_enc_key(&self, key: Vec<u8>) {
        let _ = self.se_enc_key.set(Zeroizing::new(key));
    }

    /// Get the SE encryption key, if configured.
    pub(crate) fn se_enc_key(&self) -> Option<&[u8]> {
        self.se_enc_key.get().map(|k| k.as_slice())
    }

    /// Set the SE artifact repusher used to fan artifacts to replicators after a merge commits.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_se_repusher(&self, repusher: Arc<dyn crate::merge::SeArtifactRepusher>) {
        let _ = self.se_repusher.set(repusher);
    }

    /// Post-commit action that regenerates this document's SE artifacts and pushes
    /// them to the collection's replicators. `None` when no repusher is wired or the
    /// collection has no encrypted indexes.
    pub(crate) fn se_post_commit_action(
        &self,
        doc_id: &str,
        collection: &CollectionVersion,
    ) -> Option<Box<dyn hook::CompositePostCommitAction>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if collection.encrypted_indexes.is_empty() {
                return None;
            }
            let repusher = self.se_repusher.get()?.clone();
            Some(Box::new(se_merge::SeRepushAction::new(
                repusher,
                collection.collection_id.clone(),
                doc_id.to_string(),
            )))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (doc_id, collection);
            None
        }
    }

    /// Set the KMS service. Routes `decrypt_block_data` through the KMS once set.
    pub fn set_kms(&self, kms: Arc<dyn kms::KmsService>) {
        let _ = self.kms.set(kms);
    }

    /// Get the KMS service, if configured.
    pub(crate) fn kms(&self) -> Option<Arc<dyn kms::KmsService>> {
        self.kms.get().cloned()
    }

    /// Get reference to blockstore.
    pub fn blockstore(&self) -> &Arc<B> {
        &self.blockstore
    }
}
