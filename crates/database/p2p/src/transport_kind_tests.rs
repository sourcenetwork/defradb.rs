use crate::transport::TransportEvent;
use crate::QueryId;
use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

#[test]
fn transport_block_sync_events_use_transport_neutral_labels() {
    let cid = Cid::new_v1(0x55, Code::Sha2_256.digest(b"block"));
    let events = [
        TransportEvent::<()>::BlockSyncProgress {
            query_id: QueryId(1),
            missing_count: 2,
        },
        TransportEvent::<()>::BlockSyncComplete {
            query_id: QueryId(1),
            success: true,
            cause: None,
            error: None,
            provider_busy: false,
        },
        TransportEvent::<()>::BlockSyncBlockReceived {
            query_id: QueryId(1),
            cid,
            data: b"block".to_vec(),
        },
    ];

    assert_eq!(
        events.iter().map(TransportEvent::kind).collect::<Vec<_>>(),
        [
            "block_sync_progress",
            "block_sync_complete",
            "block_sync_block_received",
        ]
    );
}
