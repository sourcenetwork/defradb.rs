use crate::{P2PError, P2PErrorExt as _, P2PResult};

pub async fn set_persisted_replicator_status<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &str,
    status: p2p::ReplicatorStatus,
) -> P2PResult<bool> {
    let Some(current_bytes) = peerstore
        .get_replicator(peer_id)
        .await
        .map_err(|error| P2PError::persistence(format!("failed to load replicator: {error}")))?
    else {
        return Ok(false);
    };

    let mut info = p2p::ReplicatorInfo::from_bytes(&current_bytes)
        .map_err(|error| P2PError::internal(format!("failed to decode replicator: {error}")))?;
    if !info.set_status_if_changed_now(status) {
        return Ok(false);
    }

    let bytes = info.to_bytes().map_err(|error| {
        P2PError::internal(format!("failed to encode replicator status: {error}"))
    })?;
    let Some(incarnation) = peerstore
        .get_replicator_incarnation(peer_id)
        .await
        .map_err(|error| {
            P2PError::persistence(format!("failed to load peer incarnation: {error}"))
        })?
    else {
        return Ok(false);
    };
    peerstore
        .set_replicator_if_incarnation(peer_id, incarnation, &current_bytes, &bytes)
        .await
        .map_err(|error| P2PError::persistence(format!("failed to persist replicator: {error}")))
}

/// Set status only while the failure's exact scope generation still owns the
/// peer. A late completion must not mark a recreated peer inactive or alter
/// the state after a newer head replaced the retry marker.
pub async fn set_persisted_replicator_status_for_scope_generation<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &str,
    doc_id: &str,
    collection_id: &str,
    generation: u64,
    status: p2p::ReplicatorStatus,
) -> P2PResult<bool> {
    let Some(current_bytes) = peerstore
        .get_replicator(peer_id)
        .await
        .map_err(|error| P2PError::persistence(format!("failed to load replicator: {error}")))?
    else {
        return Ok(false);
    };

    let mut info = p2p::ReplicatorInfo::from_bytes(&current_bytes)
        .map_err(|error| P2PError::internal(format!("failed to decode replicator: {error}")))?;
    if !info.set_status_if_changed_now(status) {
        return Ok(false);
    }

    let updated_bytes = info.to_bytes().map_err(|error| {
        P2PError::internal(format!("failed to encode replicator status: {error}"))
    })?;
    peerstore
        .set_replicator_if_marker_generation(
            peer_id,
            doc_id,
            collection_id,
            generation,
            &current_bytes,
            &updated_bytes,
        )
        .await
        .map_err(|error| P2PError::persistence(format!("failed to persist replicator: {error}")))
}

/// Update retry-pass status only if both the peer incarnation and bytes still
/// match the observation used to build the update.
pub async fn set_persisted_replicator_status_for_incarnation<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &str,
    expected_incarnation: u64,
    status: p2p::ReplicatorStatus,
) -> P2PResult<bool> {
    set_persisted_replicator_status_for_incarnation_with_marker_state(
        peerstore,
        peer_id,
        expected_incarnation,
        status,
        false,
    )
    .await
}

pub async fn set_persisted_replicator_status_for_incarnation_with_marker_state<
    S: storage::corekv::Store,
>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &str,
    expected_incarnation: u64,
    status: p2p::ReplicatorStatus,
    require_no_markers: bool,
) -> P2PResult<bool> {
    let Some(current_bytes) = peerstore
        .get_replicator(peer_id)
        .await
        .map_err(|error| P2PError::persistence(format!("failed to load replicator: {error}")))?
    else {
        return Ok(false);
    };

    let mut info = p2p::ReplicatorInfo::from_bytes(&current_bytes)
        .map_err(|error| P2PError::internal(format!("failed to decode replicator: {error}")))?;
    if !info.set_status_if_changed_now(status) {
        return Ok(false);
    }

    let updated_bytes = info.to_bytes().map_err(|error| {
        P2PError::internal(format!("failed to encode replicator status: {error}"))
    })?;
    peerstore
        .set_replicator_if_incarnation_with_marker_state(
            peer_id,
            expected_incarnation,
            &current_bytes,
            &updated_bytes,
            require_no_markers,
        )
        .await
        .map_err(|error| P2PError::persistence(format!("failed to persist replicator: {error}")))
}

pub async fn load_persisted_replicators<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
) -> P2PResult<Vec<p2p::ReplicatorInfo>> {
    let entries = peerstore
        .list_replicators()
        .await
        .map_err(|error| P2PError::persistence(format!("failed to list replicators: {error}")))?;

    let mut replicators = Vec::with_capacity(entries.len());
    for (peer_id, bytes) in entries {
        match p2p::ReplicatorInfo::from_bytes(&bytes) {
            Ok(info) => replicators.push(info),
            Err(error) => {
                tracing::warn!(
                    peer_id = %peer_id,
                    error = %error,
                    "skipping invalid persisted replicator"
                );
            }
        }
    }
    Ok(replicators)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::RegolithStore;

    use super::*;

    #[nuclei::test]
    async fn status_update_persists_first_transition_only() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store);
        let info = p2p::ReplicatorInfo::new("peer-1", vec!["users".to_string()]).unwrap();
        peerstore
            .create_replicator(info.peer_id_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();
        let incarnation = peerstore
            .get_replicator_incarnation(info.peer_id_str())
            .await
            .unwrap()
            .unwrap();

        assert!(set_persisted_replicator_status(
            &peerstore,
            info.peer_id_str(),
            p2p::ReplicatorStatus::Inactive,
        )
        .await
        .unwrap());
        let inactive = p2p::ReplicatorInfo::from_bytes(
            &peerstore
                .get_replicator(info.peer_id_str())
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(inactive.status, p2p::ReplicatorStatus::Inactive);
        assert_eq!(
            peerstore
                .get_replicator_incarnation(info.peer_id_str())
                .await
                .unwrap(),
            Some(incarnation),
            "status changes must not create a new peer incarnation"
        );

        assert!(!set_persisted_replicator_status(
            &peerstore,
            info.peer_id_str(),
            p2p::ReplicatorStatus::Inactive,
        )
        .await
        .unwrap());
        let repeated = p2p::ReplicatorInfo::from_bytes(
            &peerstore
                .get_replicator(info.peer_id_str())
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(repeated.last_status_change, inactive.last_status_change);

        assert!(set_persisted_replicator_status(
            &peerstore,
            info.peer_id_str(),
            p2p::ReplicatorStatus::Active,
        )
        .await
        .unwrap());
    }

    #[nuclei::test]
    async fn scope_generation_fences_late_replicator_status_updates() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store);
        let info = p2p::ReplicatorInfo::new("peer-a", vec!["collection-a".to_string()]).unwrap();
        peerstore
            .create_replicator(info.peer_id_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();
        let old_generation = peerstore
            .observe_push_head_with_generation("peer-a", "doc-a", "collection-a")
            .await
            .unwrap()
            .unwrap();

        assert!(set_persisted_replicator_status_for_scope_generation(
            &peerstore,
            "peer-a",
            "doc-a",
            "collection-a",
            old_generation,
            p2p::ReplicatorStatus::Inactive,
        )
        .await
        .unwrap());
        let inactive_bytes = peerstore.get_replicator("peer-a").await.unwrap().unwrap();

        let new_generation = peerstore
            .observe_push_head_with_generation("peer-a", "doc-a", "collection-a")
            .await
            .unwrap()
            .unwrap();
        assert!(new_generation > old_generation);
        assert!(!set_persisted_replicator_status_for_scope_generation(
            &peerstore,
            "peer-a",
            "doc-a",
            "collection-a",
            old_generation,
            p2p::ReplicatorStatus::Active,
        )
        .await
        .unwrap());
        assert_eq!(
            peerstore.get_replicator("peer-a").await.unwrap().unwrap(),
            inactive_bytes,
            "a late status update must leave the peer state unchanged"
        );
    }

    #[nuclei::test]
    async fn retry_status_requires_same_incarnation_and_no_pending_markers() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store);
        let info = p2p::ReplicatorInfo::new("peer-fenced", vec!["users".to_string()]).unwrap();
        peerstore
            .create_replicator(info.peer_id_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();
        let incarnation = peerstore
            .get_replicator_incarnation(info.peer_id_str())
            .await
            .unwrap()
            .unwrap();
        peerstore
            .observe_push_head_with_generation(info.peer_id_str(), "doc", "users")
            .await
            .unwrap()
            .unwrap();

        assert!(
            !set_persisted_replicator_status_for_incarnation_with_marker_state(
                &peerstore,
                info.peer_id_str(),
                incarnation,
                p2p::ReplicatorStatus::Active,
                true,
            )
            .await
            .unwrap()
        );

        peerstore
            .delete_replicator(info.peer_id_str())
            .await
            .unwrap();
        peerstore
            .create_replicator(info.peer_id_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();
        assert!(!set_persisted_replicator_status_for_incarnation(
            &peerstore,
            info.peer_id_str(),
            incarnation,
            p2p::ReplicatorStatus::Inactive,
        )
        .await
        .unwrap());
        let current = p2p::ReplicatorInfo::from_bytes(
            &peerstore
                .get_replicator(info.peer_id_str())
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(current.status, p2p::ReplicatorStatus::Active);
    }
}
