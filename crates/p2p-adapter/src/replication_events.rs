//! What a replication loop tells the event bus about the blocks it handled.

use p2p::sync::ReplicationResult;

/// Publish merge completion and quarantine, so subscribers learn that a
/// replicated write landed or was deterministically refused.
pub fn publish_replication_result(
    event_bus: &dyn events::Bus,
    local_peer: &str,
    result: &ReplicationResult,
) {
    match result {
        ReplicationResult::Merged {
            cid,
            doc_id,
            collection_id,
        }
        | ReplicationResult::MergedButBroadcastFailed {
            cid,
            doc_id,
            collection_id,
            ..
        } => {
            event_bus.publish(events::Message::merge_complete(events::MergeCompleteData {
                doc_id: doc_id.clone(),
                subject_doc_id: None,
                cid: *cid,
                collection_id: collection_id.clone(),
                by_peer: local_peer.to_string(),
            }));
            if !doc_id.is_empty() {
                event_bus.publish(events::Message::se_artifact_received(
                    events::SEArtifactReceivedData {
                        doc_id: doc_id.clone(),
                    },
                ));
            }
        }
        ReplicationResult::Failed { cid, error } => {
            tracing::error!(cid = %cid, error = %error, "block merge failed");
        }
        ReplicationResult::Skipped {
            cid,
            doc_id,
            collection_id,
            reason,
            terminal,
        } => {
            let document_terminal = !doc_id.is_empty()
                && matches!(
                    reason.as_str(),
                    "already applied" | "nonce already applied" | "already merged"
                );
            let collection_terminal =
                doc_id.is_empty() && reason == "no linked composites needed merging";
            if *terminal && (document_terminal || collection_terminal) {
                event_bus.publish(events::Message::merge_complete(events::MergeCompleteData {
                    doc_id: doc_id.clone(),
                    subject_doc_id: None,
                    cid: *cid,
                    collection_id: collection_id.clone(),
                    by_peer: local_peer.to_string(),
                }));
            }
            tracing::debug!(cid = %cid, reason = %reason, "replication loop skipped block");
        }
        ReplicationResult::Quarantined {
            cid,
            doc_id,
            collection_id,
            reason,
        } => {
            tracing::warn!(
                cid = %cid,
                doc_id = %doc_id,
                collection_id = %collection_id,
                reason = %reason,
                "block quarantined: merge deterministically rejected, will not be re-driven locally"
            );
            event_bus.publish(events::Message::pending_dag_quarantined(
                events::PendingDagQuarantinedData {
                    cid: *cid,
                    doc_id: doc_id.clone(),
                    collection_id: collection_id.clone(),
                    reason: reason.clone(),
                },
            ));
        }
        _ => {}
    }
}
