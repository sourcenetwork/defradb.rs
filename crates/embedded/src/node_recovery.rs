#[cfg(feature = "libp2p")]
use p2p::topics::DefraTopic;

#[cfg(feature = "libp2p")]
pub(crate) async fn restore_libp2p_replicators<S: storage::corekv::Store + 'static>(
    handle: &p2p::P2PHostHandle,
    peerstore: &storage::stores::Peerstore<S>,
) {
    match peerstore.list_replicators().await {
        Ok(entries) => {
            for (peer_id_str, data) in entries {
                // Not a replicator: the slot libp2p kept its keypair in before
                // the shared peer key. It is left in place so the identity
                // stays recoverable, so decoding it here would warn on every
                // start about a record that is doing its job.
                if peer_id_str == crate::node_peer_key::LEGACY_LIBP2P_KEY_ID {
                    continue;
                }
                match p2p::ReplicatorInfo::from_bytes(&data) {
                    Ok(info) => {
                        if let Some(peer_id) = info.peer_id() {
                            if let Err(error) =
                                handle.create_replicator_info(peer_id, info.clone()).await
                            {
                                tracing::warn!(peer_id = %peer_id, error = %error, "failed to restore replicator");
                                continue;
                            }

                            for collection_id in &info.collections {
                                if info.is_filtered_for_collection(collection_id) {
                                    continue;
                                }
                                let topic = DefraTopic::collection(collection_id);
                                if let Err(error) = handle.subscribe(topic).await {
                                    tracing::warn!(collection_id = %collection_id, error = %error, "failed to restore collection topic");
                                }
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(peer_id = %peer_id_str, error = %error, "failed to decode replicator info");
                    }
                }
            }
        }
        Err(error) => tracing::warn!(error = %error, "failed to load replicators from storage"),
    }
}

#[cfg(feature = "libp2p")]
pub(crate) async fn restore_libp2p_documents<S: storage::corekv::Store + 'static>(
    handle: &p2p::P2PHostHandle,
    peerstore: &storage::stores::Peerstore<S>,
) -> rapidhash::RapidHashSet<String> {
    let mut restored = rapidhash::RapidHashSet::default();
    if let Ok(doc_ids) = peerstore.load_documents().await {
        for doc_id in &doc_ids {
            let _ = handle.subscribe(DefraTopic::document(doc_id)).await;
            restored.insert(doc_id.clone());
        }
    }
    restored
}
