//! Test-only witness for a refused incomplete ancestor of an accepted head.
use super::*;
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};

/// A synthetic field-dependent gate: before the ancestor's field is available
/// it allows the announcement, but it refuses that ancestor when the fetched
/// CAR supplies the field. It allows the later head. This exercises the real
/// gate contract and event path; it is not the DB filter implementation.
struct RefuseAncestorWhenFieldAvailable {
    ancestor: Vec<u8>,
    field: Cid,
    allowed_ancestor: AtomicUsize,
    refused_ancestor: AtomicUsize,
    allowed_descendant: AtomicUsize,
}

#[async_trait::async_trait]
impl crate::sync::receive_gate::ReceiveGate for RefuseAncestorWhenFieldAvailable {
    async fn allows(
        &self,
        _collection_id: &str,
        _doc_id: &str,
        head: &[u8],
        blocks: &dyn crate::sync::receive_gate::GateBlocks,
    ) -> bool {
        if head == self.ancestor.as_slice() {
            if blocks.block(&self.field).await.is_some() {
                self.refused_ancestor.fetch_add(1, Ordering::SeqCst);
                false
            } else {
                self.allowed_ancestor.fetch_add(1, Ordering::SeqCst);
                true
            }
        } else {
            self.allowed_descendant.fetch_add(1, Ordering::SeqCst);
            true
        }
    }
}

#[nuclei::test(threads = 0)]
async fn authorized_push_of_descendant_skips_refused_ancestor_missing_child() {
    let sender = random_peer_id();
    let registry = Arc::new(ReplicatorRegistry::new());
    registry.add_replicator("collection1", sender.as_str());
    let (coordinator, mut events) = create_test_coordinator(
        AccessMode::Controlled,
        registry,
        Arc::new(PeerStateTracker::new()),
    );
    assert!(coordinator
        .check_pushlog_access_str(sender.as_str(), "collection1")
        .await
        .is_ok());
    assert!(coordinator
        .check_pushlog_access_str(random_peer_id().as_str(), "collection1")
        .await
        .is_err());

    let ((ancestor_cid, ancestor), (missing_cid, missing)) = gated_document();
    let gate = Arc::new(RefuseAncestorWhenFieldAvailable {
        ancestor: ancestor.clone(),
        field: missing_cid,
        allowed_ancestor: AtomicUsize::new(0),
        refused_ancestor: AtomicUsize::new(0),
        allowed_descendant: AtomicUsize::new(0),
    });
    coordinator.set_receive_gate(gate.clone());
    let transport = coordinator.runtime.transport.clone();
    let request = |cid: Cid, data: Vec<u8>| {
        PushLogRequest::new(
            "doc".to_string(),
            bytes::Bytes::from(cid.to_bytes()),
            "collection1".to_string(),
            "creator".to_string(),
            bytes::Bytes::from(data),
        )
    };

    // The authorized remote-announcement handler accepts and stores the head
    // while the gate cannot yet read its field. No direct blockstore writes.
    coordinator
        .handle_transport_event(TransportEvent::PushLogRequest {
            peer_id: sender.clone(),
            request: request(ancestor_cid, ancestor.clone()),
            token: 0,
        })
        .await
        .unwrap();
    assert_eq!(transport.pushlog_replies().pop().unwrap().err_message, None);
    assert!(coordinator
        .manager()
        .pending_dag_snapshot(&ancestor_cid)
        .is_some());
    assert!(!coordinator
        .manager()
        .blockstore()
        .has(&missing_cid)
        .await
        .unwrap());

    // The same gate can now see the missing field in the CAR and refuses it;
    // the CAR handler discharges the preexisting head without importing it.
    let car = crate::sync::car::encode_car(
        &[ancestor_cid],
        &[
            (&ancestor_cid, ancestor.as_slice()),
            (&missing_cid, missing.as_slice()),
        ],
    )
    .unwrap();
    coordinator
        .handle_transport_event(TransportEvent::CarFetchResponse {
            query_id: None,
            peer_id: sender.clone(),
            root_cid: ancestor_cid,
            car_data: car,
        })
        .await
        .unwrap();
    assert!(coordinator
        .manager()
        .blockstore()
        .is_merged(&ancestor_cid)
        .await
        .unwrap());
    assert!(!coordinator
        .manager()
        .blockstore()
        .has(&missing_cid)
        .await
        .unwrap());
    assert!(coordinator
        .manager()
        .pending_dag_snapshot(&ancestor_cid)
        .is_none());

    // The same authorized peer first sends the later head's own field through
    // the normal legacy-dependency path; the real DB gate accepts non-heads.
    let later_field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "blockNumber".to_string(),
            priority: 2,
            collection_version_id: "v1".to_string(),
            data: vec![0x18, 101],
        }),
        vec![],
        vec![],
    );
    let later_field_cid = later_field.generate_cid().unwrap();
    coordinator
        .handle_transport_event(TransportEvent::PushLogRequest {
            peer_id: sender.clone(),
            request: request(later_field_cid, later_field.to_dag_cbor().unwrap()),
            token: 0,
        })
        .await
        .unwrap();
    assert_eq!(transport.pushlog_replies().pop().unwrap().err_message, None);
    assert!(coordinator
        .manager()
        .blockstore()
        .has(&later_field_cid)
        .await
        .unwrap());

    // A later valid composite references its newer field and the refused
    // ancestor as a prior head. The same authorized handler accepts it.
    let later = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "v1".to_string(),
            priority: 2,
            status: 1,
        }),
        vec![ancestor_cid],
        vec![DAGLink::new("blockNumber", later_field_cid)],
    );
    let later_cid = later.generate_cid().unwrap();
    coordinator
        .handle_transport_event(TransportEvent::PushLogRequest {
            peer_id: sender,
            request: request(later_cid, later.to_dag_cbor().unwrap()),
            token: 0,
        })
        .await
        .unwrap();
    assert_eq!(transport.pushlog_replies().pop().unwrap().err_message, None);
    assert!(coordinator
        .manager()
        .pending_dag_snapshot(&later_cid)
        .is_some());
    assert_eq!(gate.allowed_ancestor.load(Ordering::SeqCst), 1);
    assert_eq!(gate.refused_ancestor.load(Ordering::SeqCst), 1);
    assert_eq!(gate.allowed_descendant.load(Ordering::SeqCst), 2);

    // Test coordinators do not run the production retry clock; invoke its
    // claim/dispatch boundary using the existing helper, then inspect the
    // actual event rather than claiming an applied or visible document.
    dispatch_complete_pending(&coordinator).await;
    let event = recv_block_received(&mut events).await;
    assert!(matches!(event, SyncEvent::DagReady { root_cid, .. } if root_cid == later_cid));
    assert!(!coordinator
        .manager()
        .blockstore()
        .has(&missing_cid)
        .await
        .unwrap());
}
