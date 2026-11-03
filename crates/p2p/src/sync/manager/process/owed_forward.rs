//! Merged roots whose forward to this node's replicators failed.
//!
//! Such a root stays unmarked, so its upstream pending record survives and
//! the receiver clock re-drives it. In this process the database answers that
//! re-drive with its "already merged" skip rather than a merge, so this set is
//! what tells the re-drive to forward before it discharges the record. After a
//! restart the set is empty and the database merges the root again.

use blockstore::Blockstore;
use cid::Cid;

use super::SyncManager;

impl<B: Blockstore + 'static> SyncManager<B> {
    /// Remember that `cid` merged here but its forward did not become durable.
    pub(crate) fn owe_forward(&self, cid: Cid) {
        if self.owed_forwards.len() >= self.persisted_pending_capacity() {
            tracing::warn!(
                cid = %cid,
                "Owed-forward set is full; a re-drive in this process will not forward this root"
            );
            return;
        }
        self.owed_forwards.insert(cid, ());
    }

    /// Whether `cid` merged here and still owes its forward.
    pub(crate) fn forward_owed(&self, cid: &Cid) -> bool {
        self.owed_forwards.contains_key(cid)
    }

    /// The forward of `cid` is durable.
    pub(crate) fn forward_settled(&self, cid: &Cid) {
        self.owed_forwards.remove(cid);
    }
}
