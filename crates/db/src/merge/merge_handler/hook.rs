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
