//! Bringing an iroh node's persisted replicators and document subscriptions
//! back after a restart.

use std::sync::Arc;

use p2p::P2PTransport;
use rapidhash::{HashSetExt, RapidHashSet};

/// Restore persisted replicators into the coordinator and resubscribe persisted
/// document topics, returning the document ids that were resubscribed.
pub async fn restore_iroh_p2p_state<S, B>(
    store: Arc<S>,
    transport: &p2p::iroh::IrohTransport,
    coordinator: &Arc<p2p::sync::IrohSyncCoordinator<B>>,
) -> RapidHashSet<String>
where
    S: storage::corekv::Store + 'static,
    B: blockstore::Blockstore + 'static,
{
    let peerstore = storage::stores::Peerstore::new(store);

    match peerstore.list_replicators().await {
        Ok(entries) => {
            for (peer_id_str, data) in entries {
                let replicator = match p2p::ReplicatorInfo::from_bytes(&data) {
                    Ok(replicator) => replicator,
                    Err(error) => {
                        tracing::warn!(
                            peer_id = %peer_id_str,
                            error = %error,
                            "failed to decode persisted P2P replicator"
                        );
                        continue;
                    }
                };
                let peer_id = p2p::transport::PeerId::new(replicator.peer_id_str().to_string());
                // Restore the complete durable record. Reconstructing it from
                // collection IDs silently discarded filters and addresses,
                // so an idempotent post-restart AddReplicator looked like a
                // filter change and launched a full existing-document replay.
                // This matches Go's loadAndPublishReplicators behavior: the
                // persisted replicator is the startup source of truth.
                if let Err(error) = coordinator
                    .create_replicator_info(&peer_id, replicator, false)
                    .await
                {
                    tracing::warn!(                        peer_id = %peer_id,
                        error = %error,
                        "failed to restore persisted P2P replicator"
                    );
                }
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to load persisted P2P replicators")
        }
    }

    let mut restored_doc_ids = RapidHashSet::new();
    match peerstore.load_documents().await {
        Ok(doc_ids) => {
            for doc_id in doc_ids {
                if let Err(error) = transport
                    .subscribe(p2p::topics::DefraTopic::document(&doc_id))
                    .await
                {
                    tracing::warn!(                        doc_id = %doc_id,
                        error = %error,
                        "failed to restore P2P document subscription"
                    );
                }
                restored_doc_ids.insert(doc_id);
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to load persisted P2P document subscriptions");
        }
    }

    restored_doc_ids
}
