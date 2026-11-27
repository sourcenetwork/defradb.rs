//! Changes parked because a remote ACP provider refused their signer
//! (`proofs/tla/ParkedMerge.tla`).
//!
//! A replicated change whose verified signer the provider refuses is not
//! rejected for good: the refusal is a fact about the provider's state, which
//! a later grant changes. The change is deferred under its document
//! ([`WaitKey::AcpObject`]), as governance defers a composite, and so is a
//! later change whose history holds it. It is tried again, through the normal
//! merge path and a fresh decision, when this node changes that document's
//! relationships, when the home of a policy another deployment owns reaches
//! a new verified state (every parked object of that policy, one re-drive of
//! it at a time), and on the bounded sweep, which covers a grant made
//! elsewhere. When such a policy's route is removed, its parked changes leave
//! the index. The index holds at most `MAX_WAITERS_PER_DEPENDENCY` changes
//! per document and `MAX_DEFERRED_COMPOSITES` per node; a change past either
//! bound, or parked before a restart, is not indexed, not even under the
//! refused composite, and is tried again on the replication layer's own
//! retry clock, as every retryable skip is
//! ([`DeferredMerges::park`](crate::merge::governance::DeferredMerges::park)).

use std::sync::{Arc, Weak};

use defra_core::thread_bounds::MaybeSendSync;
use storage::corekv::Store;

use crate::database::spawn::spawn_task;
use crate::merge::governance::{WaitKey, SWEEP_BUDGET};
use crate::merge::merge_handler::policy_redrives::PolicyRedrives;
use crate::merge::merge_handler::DbMergeHandler;

/// Releases what is parked under an object when this node changes the
/// object's relationships, or when the home of a policy another deployment
/// owns reaches a new state. Held weakly: the handler holds the provider
/// that holds this.
struct ParkedRelease<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    handler: Weak<DbMergeHandler<S, B>>,
    redrives: PolicyRedrives,
}

impl<S, B> acp::ObjectChangeListener for ParkedRelease<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    fn changed(&self, policy_id: &str, resource_name: &str, object_id: &str) {
        let Some(handler) = self.waiting() else {
            return;
        };
        handler
            .deferred
            .release([WaitKey::acp_object(policy_id, resource_name, object_id)]);
        if handler.deferred.has_ready() {
            Self::redrive(handler);
        }
    }

    /// The home of a policy another deployment owns has a new verified
    /// state: every change parked under one of its objects is asked again,
    /// by one re-drive of the policy at a time ([`PolicyRedrives`]).
    fn policy_changed(&self, policy_id: &str) {
        let Some(handler) = self.waiting() else {
            return;
        };
        let Some(mut turn) = self.redrives.begin(policy_id) else {
            return;
        };
        let policy = policy_id.to_owned();
        let pool = handler.db.pool().cloned();
        spawn_task(pool.as_ref(), "db::parked_acp_policy_redrive", async move {
            loop {
                turn.released(handler.deferred.release_policy(&policy));
                if handler.deferred.has_ready() {
                    handler.redrive_deferred().await;
                }
                if !turn.again() {
                    return;
                }
            }
        });
    }

    /// The policy has no home whose state could let its parked changes in
    /// any more: they leave the index, unmerged, for the replication
    /// layer's retry clock, as a change past the index's bound does, and
    /// are not asked again of the node's own deployment now.
    fn policy_removed(&self, policy_id: &str) {
        let Some(handler) = self.handler.upgrade() else {
            return;
        };
        let forgotten = handler.deferred.forget_policy(policy_id);
        if forgotten > 0 {
            tracing::warn!(
                policy = %policy_id,
                forgotten,
                "the policy is no longer decided by another deployment's home: \
                 its parked changes are left to the replication layer's retry clock"
            );
        }
    }

    fn released_under(&self, policy_id: &str) -> usize {
        self.redrives.released(policy_id)
    }

    fn waiting_under(&self, policy_id: &str) -> usize {
        self.handler.upgrade().map_or(0, |handler| {
            handler.deferred.objects_parked_under(policy_id)
        })
    }
}

impl<S, B> ParkedRelease<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    /// The handler, when anything waits in its index.
    fn waiting(&self) -> Option<Arc<DbMergeHandler<S, B>>> {
        self.handler
            .upgrade()
            .filter(|handler| handler.deferred.has_waiters())
    }

    /// Re-drive what was released.
    fn redrive(handler: Arc<DbMergeHandler<S, B>>) {
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
                redrives: PolicyRedrives::default(),
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
