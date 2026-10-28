//! A PushLog for a CID another request is already processing joins that
//! processing instead of being turned away.
//!
//! The duplicate waits for the owner to let the CID go, then looks again: a
//! block merged or registered for recovery by then is acked, and a block still
//! unsafe (the owner's attempt failed) is processed by the duplicate under the
//! CID's guard. The whole join is bounded by [`PUSHLOG_JOIN_DEADLINE`], after
//! which the duplicate answers [`Error::PushLogInFlight`], the retry-later
//! nack. Modeled by `proofs/tla/PushLogJoin.tla` (success implies durable, one
//! processor per CID, every join ends) and `proofs/lean/SingleFlight/Join.lean`.

use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;

use crate::error::{Error, Result};
use crate::sync::queue::ProcessGuard;

use super::SyncManager;

/// How long a duplicate PushLog waits for the CID's owner. The wait holds a
/// transport admission slot, so it stays well under the second a duplicate may
/// hold one; processing a block takes milliseconds.
pub(super) const PUSHLOG_JOIN_DEADLINE: Duration = Duration::from_millis(500);

impl<B> SyncManager<B>
where
    B: Blockstore + 'static,
{
    /// Join the processing another request owns for `cid`: `None` when the
    /// block needs nothing more (the duplicate acks), the CID's guard when it
    /// is free and the block is still unsafe (the duplicate processes its own
    /// copy), [`Error::PushLogInFlight`] when the deadline passes first.
    pub(super) async fn join_in_flight(
        &self,
        cid: &Cid,
        sender_peer: Option<&str>,
        explicit_replay: bool,
    ) -> Result<Option<ProcessGuard>> {
        self.diagnostics.record_single_flight_suppressed();
        tracing::debug!(
            cid = %cid,
            sender_peer = ?sender_peer,
            explicit_replay,
            "Joining the processing of a PushLog for the same CID"
        );
        let join = async {
            loop {
                if self.needs_nothing_more(cid).await? {
                    return Ok(None);
                }
                match self.process_queue.try_acquire(cid) {
                    Ok(guard) => return Ok(Some(guard)),
                    Err(owner) => owner.released().await,
                }
            }
        };
        match workers::timeout(PUSHLOG_JOIN_DEADLINE, join).await {
            Ok(joined) => joined,
            Err(reason) => {
                tracing::debug!(cid = %cid, %reason, "Stopped waiting for the CID's owner");
                Err(Error::PushLogInFlight {
                    cid: cid.to_string(),
                })
            }
        }
    }

    /// Whether the block needs nothing more from a duplicate: its DAG is
    /// registered for recovery, or it is merged.
    async fn needs_nothing_more(&self, cid: &Cid) -> Result<bool> {
        if self.is_pending_dag_recovery_registered(cid) {
            return Ok(true);
        }
        let merged = self
            .blockstore
            .is_merged(cid)
            .await
            .map_err(Error::from_blockstore)?;
        if merged {
            self.diagnostics.record_already_merged_fast_path();
        }
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blockstore::DefraBlockstore;
    use storage::RegolithStore;
    use web_time::Instant;

    use super::*;
    use crate::sync::manager::process::pushlog::tests::{
        create_composite_block, create_lww_block, make_broadcast, BlockingPendingDagStore,
    };
    use crate::sync::{PeerStateTracker, SyncConfig};

    type Manager = SyncManager<DefraBlockstore<RegolithStore>>;

    fn manager(store: &Arc<RegolithStore>) -> Arc<Manager> {
        let blockstore = Arc::new(DefraBlockstore::new(Arc::clone(store), true));
        let (manager, _events) = SyncManager::new(
            blockstore,
            Arc::new(PeerStateTracker::new()),
            SyncConfig::default(),
        );
        Arc::new(manager)
    }

    fn head() -> (Cid, Arc<crate::message::PushLogBroadcast>) {
        let (field_cid, _) = create_lww_block("name");
        let (cid, block) = create_composite_block("doc123", "name", field_cid);
        (
            cid,
            Arc::new(make_broadcast("doc123", cid, block, "collection1")),
        )
    }

    /// Waits, under a deadline with bounded backoff, until `count` PushLogs
    /// joined an owner.
    async fn joined(manager: &Manager, count: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut backoff = Duration::from_millis(1);
        while manager.diagnostics().snapshot().single_flight_suppressed < count {
            assert!(
                Instant::now() < deadline,
                "the duplicate never joined the owner"
            );
            crate::rt::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_millis(20));
        }
    }

    /// Two PushLogs of one CID at once: the second waits for the first,
    /// finds the block registered, and acks without processing it again.
    #[nuclei::test]
    async fn a_duplicate_waits_for_the_owner_and_acks_the_block_it_registered() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let manager = manager(&store);
        let pending_store = Arc::new(BlockingPendingDagStore::new(store));
        manager
            .install_pending_dag_store(pending_store.clone())
            .await;
        let (cid, message) = head();

        let owner = crate::rt::spawn({
            let (manager, message) = (Arc::clone(&manager), Arc::clone(&message));
            async move {
                manager
                    .process_pushlog(&message, Some("peer-1"), false, None)
                    .await
            }
        });
        pending_store.register_entered.notified().await;
        let duplicate = crate::rt::spawn({
            let (manager, message) = (Arc::clone(&manager), Arc::clone(&message));
            async move {
                manager
                    .process_pushlog_from_dag_provider(&message, Some("peer-2"), false, None)
                    .await
            }
        });
        joined(&manager, 1).await;
        pending_store.register_release.notify_one();

        owner
            .join()
            .await
            .unwrap()
            .expect("the owner registers the block");
        duplicate
            .join()
            .await
            .unwrap()
            .expect("the duplicate acks once the owner registered the block");
        let pending = manager
            .pending_dag_snapshot(&cid)
            .expect("the block stays registered");
        assert_eq!(pending.source_peer.as_deref(), Some("peer-1"));
        assert!(
            pending.alternate_providers.is_empty(),
            "a direct provider that processed its copy would be recorded as an alternate"
        );
        assert_eq!(manager.diagnostics().snapshot().pending_dag_registered, 1);
        assert_eq!(manager.process_queue.active_count(), 0);
    }

    /// The owner lets the CID go without leaving the block merged or
    /// registered, as a failed attempt does: the waiting duplicate takes the
    /// guard and processes its own copy.
    #[nuclei::test]
    async fn a_duplicate_processes_its_own_copy_when_the_owner_failed() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let manager = manager(&store);
        let (cid, message) = head();
        let owner = manager
            .process_queue
            .try_acquire_nowait(&cid)
            .expect("an owner whose attempt fails");

        let duplicate = crate::rt::spawn({
            let (manager, message) = (Arc::clone(&manager), Arc::clone(&message));
            async move {
                manager
                    .process_pushlog(&message, Some("peer-2"), false, None)
                    .await
            }
        });
        joined(&manager, 1).await;
        drop(owner);

        duplicate
            .join()
            .await
            .unwrap()
            .expect("the duplicate processes its own copy");
        let pending = manager
            .pending_dag_snapshot(&cid)
            .expect("the duplicate registered the block");
        assert_eq!(pending.source_peer.as_deref(), Some("peer-2"));
        assert_eq!(manager.process_queue.active_count(), 0);
    }

    /// An owner that never lets go: the duplicate waits out its deadline,
    /// then answers the retry-later nack, owning nothing.
    #[nuclei::test]
    async fn a_duplicate_falls_back_to_in_flight_at_its_deadline() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let manager = manager(&store);
        let (cid, message) = head();
        let _owner = manager
            .process_queue
            .try_acquire_nowait(&cid)
            .expect("an owner that never lets go");

        let started = Instant::now();
        let result = crate::rt::time::timeout(
            Duration::from_secs(5),
            manager.process_pushlog(&message, Some("peer-2"), false, None),
        )
        .await
        .expect("the join is bounded");

        assert!(matches!(
            result,
            Err(Error::PushLogInFlight { cid: ref busy }) if busy == &cid.to_string()
        ));
        assert!(
            started.elapsed() >= PUSHLOG_JOIN_DEADLINE,
            "the duplicate waited for the owner before falling back"
        );
        assert_eq!(manager.process_queue.active_count(), 1);
        assert!(manager.pending_dag_snapshot(&cid).is_none());
    }
}
