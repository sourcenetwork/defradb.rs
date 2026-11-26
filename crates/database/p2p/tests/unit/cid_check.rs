//! Every P2P arrival path refuses a block whose bytes do not hash to the CID
//! it arrives under, and stores none of it (#3292). Each test hands one path
//! such a block and fails if that path's `verify_block_cid` call is removed.
//! The gossip deferral path has its test in `gossip_deferral_tests.rs`.

use defra_core::block::{Block, CrdtDelta};

use super::*;

/// `bytes` re-encoded with a different priority (a composite) or value (a
/// field), so they still parse but no longer hash to their CID.
fn tampered(bytes: &[u8]) -> Vec<u8> {
    let mut block = Block::from_dag_cbor(bytes).unwrap();
    match &mut block.delta {
        CrdtDelta::Composite(payload) => payload.priority += 1,
        CrdtDelta::Lww(payload) => payload.data = vec![0x18, 101],
        other => panic!("no tamper for {other:?}"),
    }
    let bytes = block.to_dag_cbor().unwrap();
    assert!(blockstore::verify_block_cid(&block.generate_cid().unwrap(), &bytes).is_ok());
    bytes
}

fn open_coordinator() -> SyncCoordinator<TestBlockstore, NoopTransport> {
    let (coordinator, _events) = create_test_coordinator(
        AccessMode::Open,
        Arc::new(ReplicatorRegistry::new()),
        Arc::new(PeerStateTracker::new()),
    );
    coordinator
}

fn push_of(cid: Cid, block: Vec<u8>) -> TransportEvent<usize> {
    TransportEvent::TwoStreamRequest {
        peer_id: random_peer_id(),
        request: PushLogRequest::new(
            "doc".to_string(),
            bytes::Bytes::from(cid.to_bytes()),
            "col-log".to_string(),
            "creator".to_string(),
            bytes::Bytes::from(block),
        ),
        token: None,
        is_explicit_replicator: true,
        explicit_replay_authorization: None,
    }
}

async fn holds(coordinator: &SyncCoordinator<TestBlockstore, NoopTransport>, cid: &Cid) -> bool {
    coordinator.manager().blockstore().has(cid).await.unwrap()
}

/// A replicator's push whose head does not match its CID is refused, and
/// neither the head nor a fetch for its links is kept
/// (`pushlog.rs` `process_block_inner`).
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.p2p.cid_check")]
async fn a_pushed_head_not_matching_its_cid_is_refused() {
    let coordinator = open_coordinator();
    let ((head_cid, head), _) = gated_document();

    let result = coordinator
        .handle_transport_event(push_of(head_cid, tampered(&head)))
        .await;

    assert!(
        matches!(result, Err(Error::BlockCidMismatch { .. })),
        "{result:?}"
    );
    assert!(!holds(&coordinator, &head_cid).await);
    assert!(coordinator
        .manager()
        .pending_dag_snapshot(&head_cid)
        .is_none());
}

/// The receive gate does not judge a head that does not match its CID: the
/// push goes on to the ingest, which refuses it with the CID error, rather
/// than being answered as a handled skip (`receive_gate.rs` `gate_arrival`).
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.p2p.cid_check")]
async fn the_receive_gate_leaves_a_head_not_matching_its_cid_to_the_ingest() {
    let coordinator = open_coordinator();
    coordinator.set_receive_gate(Arc::new(RefuseAll));
    let ((head_cid, head), _) = gated_document();

    let result = coordinator
        .handle_transport_event(push_of(head_cid, tampered(&head)))
        .await;

    assert!(
        matches!(result, Err(Error::BlockCidMismatch { .. })),
        "{result:?}"
    );
    assert!(!holds(&coordinator, &head_cid).await);
}

/// A block fetched by block sync that does not match its CID is refused and
/// not stored (`block_sync.rs` `store_sync_block`).
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.p2p.cid_check")]
async fn a_synced_block_not_matching_its_cid_is_refused() {
    let coordinator = open_coordinator();
    let (_, (field_cid, field)) = gated_document();

    let result = coordinator
        .handle_transport_event(TransportEvent::BlockSyncBlockReceived {
            query_id: QueryId(1),
            cid: field_cid,
            data: tampered(&field),
        })
        .await;

    assert!(
        matches!(result, Err(Error::BlockCidMismatch { .. })),
        "{result:?}"
    );
    assert!(!holds(&coordinator, &field_cid).await);
}

/// A fetched CAR with one block that does not match its CID is refused
/// whole: none of its blocks is stored and the document stays unmerged,
/// waiting on an honest provider (`car.rs` `handle_car_fetch_response`).
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.p2p.cid_check")]
async fn a_fetched_car_with_a_block_not_matching_its_cid_is_refused() {
    let coordinator = open_coordinator();
    let ((head_cid, head), (field_cid, field)) = gated_document();
    coordinator
        .handle_transport_event(push_of(head_cid, head.clone()))
        .await
        .unwrap();
    assert!(coordinator
        .manager()
        .pending_dag_snapshot(&head_cid)
        .is_some());

    let forged = tampered(&field);
    let car = crate::sync::car::encode_car(
        &[head_cid],
        &[
            (&head_cid, head.as_slice()),
            (&field_cid, forged.as_slice()),
        ],
    )
    .unwrap();
    let result = coordinator
        .handle_transport_event(TransportEvent::CarFetchResponse {
            query_id: None,
            peer_id: random_peer_id(),
            root_cid: head_cid,
            car_data: car,
        })
        .await;

    assert!(
        matches!(result, Err(Error::BlockCidMismatch { .. })),
        "{result:?}"
    );
    assert!(!holds(&coordinator, &field_cid).await);
    assert!(!coordinator
        .manager()
        .blockstore()
        .is_merged(&head_cid)
        .await
        .unwrap());
    assert!(coordinator
        .manager()
        .pending_dag_snapshot(&head_cid)
        .is_some());
}
