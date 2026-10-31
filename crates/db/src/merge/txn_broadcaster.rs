//! `TxnBroadcaster` implementation backed by the `SyncCoordinator`.
//!
//! Used by `DbTransactionRegistry::with_broadcaster` so that committed
//! transactional writes get pushed to replicators and gossipsub topics —
//! mirroring what `BroadcastMutator` already does for the single-mutation
//! auto-commit path.

use std::sync::Arc;

use crate::block::builder::BlockResult;
use crate::event::emission::{BroadcastDone, TxnBroadcastEvent, TxnBroadcaster};
use blockstore::Blockstore;
use p2p::sync::SyncCoordinator;
use p2p::transport::P2PTransport;
use regolith::sync::{Atom, Event};

use crate::merge::broadcast_mutator::broadcast::{
    broadcast_with_retry_with_creator, log_broadcast_failure, spawn_gossip_or_report,
};

/// `TxnBroadcaster` that fans committed transactional writes out to peers via
/// `SyncCoordinator::push_to_replicators` and gossipsub.
///
/// Each submitted write becomes its own task on the coordinator's runtime,
/// which starts only once the task of the write submitted before it
/// finished, so durable delivery markers are registered in commit-receipt
/// order without the committing transaction waiting for earlier ones.
pub struct SyncTxnBroadcaster<B, T>
where
    B: Blockstore + defra_core::thread_bounds::MaybeSendSync + 'static,
    T: P2PTransport + 'static,
{
    sync: Arc<SyncCoordinator<B, T>>,
    /// Where a push that found no peers is reported.
    bus: Option<Arc<dyn events::Bus>>,
    /// Set when the last submitted write's task finished.
    last: Atom<Arc<Event>>,
}

impl<B, T> SyncTxnBroadcaster<B, T>
where
    B: Blockstore + defra_core::thread_bounds::MaybeSendSync + 'static,
    T: P2PTransport + 'static,
{
    pub fn new(sync: Arc<SyncCoordinator<B, T>>, bus: Option<Arc<dyn events::Bus>>) -> Self {
        let finished = Event::new();
        finished.set();
        Self {
            sync,
            bus,
            last: Atom::new(Arc::new(finished)),
        }
    }
}

/// Sets its event when dropped: a task that ends, panics or is never run
/// lets the next one start.
struct Finished(Arc<Event>);

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.set();
    }
}

impl<B, T> TxnBroadcaster for SyncTxnBroadcaster<B, T>
where
    B: Blockstore + defra_core::thread_bounds::MaybeSendSync + 'static,
    T: P2PTransport + 'static,
{
    fn submit_update(&self, event: TxnBroadcastEvent, done: BroadcastDone) {
        let mine = Arc::new(Event::new());
        let previous = Arc::clone(&*self.last.swap(Arc::clone(&mine)));
        let finished = Finished(mine);
        let sync = Arc::clone(&self.sync);
        let bus = self.bus.clone();
        // A task skipped at shutdown is dropped here with `done` and
        // `finished`, so neither its transaction nor the next write waits.
        self.sync
            .spawn_background_task("ordered_transaction_broadcast", async move {
                previous.wait().await;
                broadcast(&sync, bus.as_ref(), event).await;
                drop(done);
                drop(finished);
            });
    }
}

async fn broadcast<B, T>(
    sync: &Arc<SyncCoordinator<B, T>>,
    bus: Option<&Arc<dyn events::Bus>>,
    event: TxnBroadcastEvent,
) where
    B: Blockstore + defra_core::thread_bounds::MaybeSendSync + 'static,
    T: P2PTransport + 'static,
{
    let TxnBroadcastEvent {
        collection_name,
        collection_id,
        doc_id,
        doc_cid,
        doc_block,
        document_json,
        collection_block,
        creator_did,
    } = event;

    let creator_ref = creator_did.as_deref();
    if let Some(document_json) = document_json.as_ref() {
        if let Err(error) = sync
            .push_document_to_replicators_with_creator(
                &doc_cid,
                &doc_block,
                &doc_id,
                &collection_id,
                document_json,
                creator_ref,
            )
            .await
        {
            tracing::error!(%error, %doc_id, %collection_id, "Committed transaction has an undurable P2P document marker");
        }
    } else if let Err(error) = sync
        .push_to_replicators_with_creator(
            &doc_cid,
            &doc_block,
            &doc_id,
            &collection_id,
            creator_ref,
        )
        .await
    {
        tracing::error!(%error, %doc_id, %collection_id, "Committed transaction has an undurable P2P document marker");
    }
    if let Some((col_cid, col_block)) = collection_block.as_ref() {
        if let Err(error) = sync
            .push_to_replicators_with_creator(col_cid, col_block, "", &collection_id, creator_ref)
            .await
        {
            tracing::error!(%error, %collection_id, "Committed transaction has an undurable P2P collection marker");
        }
    }

    let gossip = Arc::clone(sync);
    let report_bus = bus;
    let bus = bus.cloned();
    let roots = std::iter::once((
        doc_cid,
        doc_id.clone(),
        collection_id.clone(),
        doc_block.len(),
    ))
    .chain(collection_block.as_ref().map(|(col_cid, col_block)| {
        (
            *col_cid,
            String::new(),
            collection_id.clone(),
            col_block.len(),
        )
    }))
    .collect();

    // The transaction's broadcast is done once its scope markers are
    // durable and its gossip holds a broadcast slot. Gossip still runs under
    // the node lifecycle without holding the transaction open.
    spawn_gossip_or_report(
        sync,
        report_bus,
        "broadcast_transaction_update",
        roots,
        async move {
            let creator_ref = creator_did.as_deref();

            let doc_block_result = BlockResult {
                cid: doc_cid,
                block: doc_block,
                doc_id: doc_id.clone(),
                field_cids: vec![],
                encryption_cids: vec![],
            };
            log_broadcast_failure(
                &broadcast_with_retry_with_creator(
                    &gossip,
                    bus.as_ref(),
                    &doc_id,
                    &doc_block_result,
                    &collection_id,
                    &collection_name,
                    creator_ref,
                )
                .await,
            );

            if let Some((col_cid, col_block)) = collection_block {
                let col_block_result = BlockResult {
                    cid: col_cid,
                    block: col_block,
                    doc_id: String::new(),
                    field_cids: vec![],
                    encryption_cids: vec![],
                };
                log_broadcast_failure(
                    &broadcast_with_retry_with_creator(
                        &gossip,
                        bus.as_ref(),
                        &doc_id,
                        &col_block_result,
                        &collection_id,
                        &collection_name,
                        creator_ref,
                    )
                    .await,
                );
            }
        },
    )
    .await;
}
