use std::sync::{Arc, Weak};

use bytes::Bytes;
use cid::Cid;
use defra_core::block::Block;
use defra_core::thread_bounds::MaybeSendSync;
use storage::corekv::Store;

use crate::database::spawn::spawn_task;
use crate::merge::merge_handler::DbMergeHandler;

/// Told about each composite a local write commits, so composites deferred
/// awaiting what the write created are released.
///
/// Waiters are otherwise released only from the merge path, which a local
/// write bypasses: a composite awaiting a document this node then creates
/// itself would wait for a peer to replay it or for the replication retry
/// clock.
pub trait LocalCommitRelease: MaybeSendSync {
    /// `cid` and `block_data` are the composite the write committed.
    ///
    /// Called from the committing transaction's success callback, which still
    /// holds that document's write guard, so the re-drive this triggers must
    /// run off the caller's task.
    fn committed(&self, cid: Cid, block_data: Bytes);
}

/// [`LocalCommitRelease`] backed by the node's merge handler, which owns the
/// deferred index. Held weakly: the handler holds the database that holds this.
pub(crate) struct HandlerRelease<S: Store, B: blockstore::Blockstore> {
    handler: Weak<DbMergeHandler<S, B>>,
}

impl<S, B> LocalCommitRelease for HandlerRelease<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    fn committed(&self, cid: Cid, block_data: Bytes) {
        let Some(handler) = self.handler.upgrade() else {
            return;
        };
        if !handler.deferred.has_waiters() {
            return;
        }
        spawn_task(async move {
            let block = Block::from_dag_cbor(&block_data).ok();
            handler.release_merged_composite(&cid, block.as_ref()).await;
            handler.redrive_deferred().await;
        });
    }
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    /// Release deferred composites awaiting what this node's own writes
    /// create, not only what merges into it.
    pub fn install_local_commit_release(self: &Arc<Self>) {
        self.db().set_local_commit_release(Arc::new(HandlerRelease {
            handler: Arc::downgrade(self),
        }));
    }
}
