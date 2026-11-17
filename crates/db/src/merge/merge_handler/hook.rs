use async_trait::async_trait;
use defra_core::merge::{BlockMetadata, MergeOutcome};
use defra_core::thread_bounds::{MaybeSend, MaybeSendSync};
use schema::CollectionVersion;

use super::MergeError;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait CompositePostCommitAction: MaybeSend {
    async fn run(self: Box<Self>) -> Result<(), MergeError>;
}

/// One composite frame as judged on its own, independent of the block that
/// carried it to this node.
#[derive(Debug, Clone, Copy)]
pub struct CompositeFrame<'a> {
    pub is_genesis: bool,
    pub status: u8,
    /// DID verified from this composite's own signature; `None` when unsigned.
    pub signer: Option<&'a str>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait CompositeMergeHook: MaybeSendSync {
    fn supports_transactions(&self) -> bool {
        false
    }

    fn bind_transaction(
        &self,
        _view: datastore::NamespaceView,
    ) -> Option<std::sync::Arc<dyn CompositeMergeHook>> {
        None
    }

    /// Whether `on_protected_update` can refuse anything, so the merge path
    /// only pays for per-frame signature verification when it matters.
    fn guards_protected_updates(&self) -> bool {
        false
    }

    async fn on_protected_update(
        &self,
        _doc_id: &str,
        _collection: &CollectionVersion,
        _frame: CompositeFrame<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        Ok(None)
    }

    /// Whether a change to `collection` whose verified signer
    /// `on_protected_update` refuses is parked by its document and tried
    /// again when the document's relationships change, rather than
    /// rejected for good: the refusal depends on a remote provider's state,
    /// which a later grant changes.
    fn parks_refusals(&self, _collection: &CollectionVersion) -> bool {
        false
    }

    /// Whether frames of `collection` are planned by
    /// [`plan_replica`](Self::plan_replica): the node registers replicated
    /// documents of it with a remote provider.
    fn plans_replicas(&self, _collection: &CollectionVersion) -> bool {
        false
    }

    /// What the merge does about one frame of a collection
    /// [`plans_replicas`](Self::plans_replicas) names, before its
    /// transaction opens. `signer` is the DID verified from a genesis
    /// frame's own signature, and `None` for any other frame.
    async fn plan_replica(
        &self,
        _doc_id: &str,
        _collection: &CollectionVersion,
        _is_genesis: bool,
        _signer: Option<&str>,
    ) -> Result<crate::merge::replica::ReplicaStep, MergeError> {
        Ok(crate::merge::replica::ReplicaStep::Gate)
    }

    async fn on_protected_composite(
        &self,
        _doc_id: &str,
        _collection: &CollectionVersion,
        _metadata: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        Ok(None)
    }

    async fn on_encrypted_link(
        &self,
        _doc_id: &str,
        _collection: &CollectionVersion,
        _metadata: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        Ok(None)
    }

    fn post_commit_action(
        &self,
        _doc_id: &str,
        _collection: &CollectionVersion,
        _metadata: &BlockMetadata<'_>,
    ) -> Option<Box<dyn CompositePostCommitAction>> {
        None
    }
}
