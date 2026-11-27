//! Changes parked because a remote ACP provider refused their signer
//! (`proofs/tla/ParkedMerge.tla`).
//!
//! A replicated change whose verified signer the provider refuses is not
//! rejected for good: the refusal is a fact about the provider's state, which
//! a later grant changes. The change is deferred under its document
//! ([`WaitKey::AcpObject`]), as governance defers a composite, and so is a
//! later change whose history holds it. It is tried again, through the normal
//! merge path and a fresh decision, when this node changes that document's
//! relationships, and on the bounded sweep, which covers a grant made
//! elsewhere. The index holds at most `MAX_WAITERS_PER_DEPENDENCY` changes
//! per document and `MAX_DEFERRED_COMPOSITES` per node; a change past either
//! bound, or parked before a restart, is not indexed, not even under the
//! refused composite, and is tried again on the replication layer's own
//! retry clock, as every retryable skip is
//! ([`DeferredMerges::park`](crate::merge::governance::DeferredMerges::park)).

use std::sync::{Arc, Weak};

use defra_core::thread_bounds::MaybeSendSync;
use storage::corekv::Store;

use crate::database::spawn::spawn_task;
#[cfg(doc)]
use crate::merge::governance::DeferredMerges;
use crate::merge::governance::{WaitKey, SWEEP_BUDGET};
use crate::merge::merge_handler::DbMergeHandler;

/// Releases what is parked under an object when this node changes the
/// object's relationships. Held weakly: the handler holds the provider that
/// holds this.
struct ParkedRelease<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    handler: Weak<DbMergeHandler<S, B>>,
}

impl<S, B> acp::ObjectChangeListener for ParkedRelease<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    fn changed(&self, policy_id: &str, resource_name: &str, object_id: &str) {
        let Some(handler) = self.handler.upgrade() else {
            return;
        };
        if !handler.deferred.has_waiters() {
            return;
        }
        handler
            .deferred
            .release([WaitKey::acp_object(policy_id, resource_name, object_id)]);
        if !handler.deferred.has_ready() {
            return;
        }
        let pool = handler.db.pool().cloned();
        spawn_task(pool.as_ref(), "db::parked_acp_redrive", async move {
            handler.redrive_deferred().await;
        });
    }
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    /// Re-drive changes parked under an object when this node changes its
    /// relationships on the installed provider.
    pub fn watch_parked_acp(self: &Arc<Self>) {
        if let Some(acp) = self.db.document_acp() {
            acp.watch_object_changes(Arc::new(ParkedRelease {
                handler: Arc::downgrade(self),
            }));
        }
    }

    /// One bounded pass: release what is parked under up to
    /// [`SWEEP_BUDGET`] objects and re-drive it. Returns how many objects
    /// were released.
    pub async fn sweep_parked_acp(&self) -> usize {
        let released = self.deferred.release_parked(SWEEP_BUDGET);
        if released > 0 {
            self.redrive_deferred().await;
        }
        released
    }
}
