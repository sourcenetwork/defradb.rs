//! The store's database-wide transaction hooks: how each transaction ended.
//!
//! regolith runs exactly one of `on_commit` and `on_abort` for every
//! transaction of the database, whatever ends it. These count each outcome
//! as `defradb.storage.events` by kind: a commit, or an abort by its reason.
//! A dropped transaction is not counted: every retry attempt that opens its
//! scoped IO drops a stand-in transaction, so that count would say nothing.

use regolith::{AbortReason, CommitInfo, TransactionHooks};
use telemetry::StorageEvent;

pub(super) struct StoreHooks;

impl TransactionHooks for StoreHooks {
    fn on_commit(&self, _info: &CommitInfo) {
        telemetry::record_storage_event(StorageEvent::Commit, 0);
    }

    fn on_abort(&self, reason: &AbortReason<'_>) {
        let event = match reason {
            AbortReason::Dropped => return,
            AbortReason::Rollback => StorageEvent::AbortRollback,
            AbortReason::Conflict(_) => StorageEvent::AbortConflict,
            AbortReason::Closed => StorageEvent::AbortClosed,
            AbortReason::CallbackPanicked => StorageEvent::AbortCallbackPanicked,
            _ => StorageEvent::AbortError,
        };
        telemetry::record_storage_event(event, 0);
    }
}

#[cfg(test)]
mod hooks_tests {
    use crate::backends::RegolithStore;
    use crate::corekv::{Store, Writer};
    use telemetry::StorageEvent;

    /// Every transaction's one outcome reaches the store's hooks.
    #[test]
    fn commits_rollbacks_and_conflicts_are_counted_by_kind() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let count = telemetry::storage_event_count;
            let (commits, rollbacks, conflicts) = (
                count(StorageEvent::Commit),
                count(StorageEvent::AbortRollback),
                count(StorageEvent::AbortConflict),
            );
            let mut committed = store.new_txn(false).await.unwrap();
            committed.set(b"key", b"one").await.unwrap();
            committed.commit().await.unwrap();
            assert!(count(StorageEvent::Commit) > commits);

            store.new_txn(false).await.unwrap().discard();
            assert!(count(StorageEvent::AbortRollback) > rollbacks);

            let mut loser = store.new_txn(false).await.unwrap();
            assert!(loser.get(b"key").await.unwrap().is_some());
            loser.set(b"key", b"loser").await.unwrap();
            let mut winner = store.new_txn(false).await.unwrap();
            winner.set(b"key", b"winner").await.unwrap();
            winner.commit().await.unwrap();
            assert!(loser.commit().await.is_err());
            assert!(count(StorageEvent::AbortConflict) > conflicts);
        })
        .unwrap()
    }
}
