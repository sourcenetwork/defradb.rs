//! A doc-sync or sync-branchable request is held until a peer subscribes to
//! its topic, and sent again to each peer that subscribes later (#2876).
//!
//! The race needs gossipsub to see a connected peer before that peer's
//! SUBSCRIBE arrives, which a testo scenario cannot pin, so these drive the
//! coordinator with a transport that refuses `publish_raw` until the test
//! delivers the subscription event.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use blockstore::DefraBlockstore;
use regolith::sync::Notify;
use storage::RegolithStore;

use super::access_tests::NoopTransport;
use super::held_request::HeldRequest;
use super::pubsub_services::{BRANCHABLE_SYNC_TOPIC, DOC_SYNC_TOPIC};
use super::SyncCoordinator;
use crate::rt::time::Instant;
use crate::sync::manager::SyncConfig;
use crate::transport::{PeerId, TransportEvent};

/// Short enough to keep the tests fast; each request waits all of it.
const WAIT: Duration = Duration::from_millis(300);

async fn coordinator(
    transport: NoopTransport,
) -> SyncCoordinator<DefraBlockstore<RegolithStore>, NoopTransport> {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (coordinator, _events) = SyncCoordinator::new(
        transport,
        blockstore,
        crate::network::test_network(),
        SyncConfig::default(),
    )
    .await
    .unwrap();
    coordinator.start_pubsub_services().await.unwrap();
    coordinator
}

async fn subscribe(
    coordinator: &SyncCoordinator<DefraBlockstore<RegolithStore>, NoopTransport>,
    topic: &str,
) {
    coordinator
        .handle_transport_event(TransportEvent::PeerSubscribed {
            peer_id: PeerId::new(libp2p::PeerId::random().to_string()),
            topic: topic.to_string(),
        })
        .await
        .unwrap();
}

#[test]
fn doc_sync_sent_before_the_topic_has_a_peer_waits_for_one() {
    workers::block_on(async {
        let transport = NoopTransport::new().with_libp2p_identity();
        let probe = transport.clone();
        let coordinator = coordinator(transport).await;

        let (synced, ()) = futures::join!(
            coordinator.pubsub_sync_documents(vec!["doc1".into()], Some(WAIT), Some(1)),
            async {
                // The request reached gossipsub while no peer had subscribed.
                probe.raw_publish_attempted.notified().await;
                probe.raw_topic_has_peer.store(true, Ordering::Release);
                subscribe(&coordinator, DOC_SYNC_TOPIC).await;
            }
        );

        synced.expect("a doc-sync request must wait for the topic's first peer");
        assert_eq!(probe.raw_published.load_clone(), vec![DOC_SYNC_TOPIC]);
    })
    .unwrap();
}

#[test]
fn branchable_sync_sent_before_the_topic_has_a_peer_waits_for_one() {
    workers::block_on(async {
        let transport = NoopTransport::new().with_libp2p_identity();
        let probe = transport.clone();
        let coordinator = coordinator(transport).await;

        let (synced, ()) = futures::join!(
            coordinator.pubsub_sync_branchable_collection("c1".into(), Some(WAIT), Some(1)),
            async {
                probe.raw_publish_attempted.notified().await;
                probe.raw_topic_has_peer.store(true, Ordering::Release);
                subscribe(&coordinator, BRANCHABLE_SYNC_TOPIC).await;
            }
        );

        synced.expect("a sync-branchable request must wait for the topic's first peer");
        assert_eq!(
            probe.raw_published.load_clone(),
            vec![BRANCHABLE_SYNC_TOPIC]
        );
    })
    .unwrap();
}

#[test]
fn doc_sync_is_sent_again_to_a_peer_that_subscribes_later() {
    workers::block_on(async {
        let transport = NoopTransport::new().with_libp2p_identity();
        transport.raw_topic_has_peer.store(true, Ordering::Release);
        let probe = transport.clone();
        let coordinator = coordinator(transport).await;

        let (synced, ()) = futures::join!(
            coordinator.pubsub_sync_documents(vec!["doc1".into()], Some(WAIT), Some(2)),
            async {
                probe.raw_publish_attempted.notified().await;
                subscribe(&coordinator, DOC_SYNC_TOPIC).await;
            }
        );

        synced.unwrap();
        assert_eq!(
            probe.raw_published.load_clone(),
            vec![DOC_SYNC_TOPIC, DOC_SYNC_TOPIC],
            "a peer that subscribes after the first send must get the request"
        );
    })
    .unwrap();
}

#[test]
fn doc_sync_no_peer_ever_receives_fails_at_the_deadline() {
    workers::block_on(async {
        let coordinator = coordinator(NoopTransport::new().with_libp2p_identity()).await;

        let error = coordinator
            .pubsub_sync_documents(vec!["doc1".into()], Some(WAIT), Some(1))
            .await
            .expect_err("a request no peer received must fail");

        assert!(error.to_string().contains("InsufficientPeers"), "{error}");
    })
    .unwrap();
}

#[test]
fn doc_sync_expecting_no_reply_is_still_sent() {
    workers::block_on(async {
        let transport = NoopTransport::new().with_libp2p_identity();
        transport.raw_topic_has_peer.store(true, Ordering::Release);
        let probe = transport.clone();
        let coordinator = coordinator(transport).await;

        let replies = coordinator
            .pubsub_sync_documents(vec!["doc1".into()], Some(WAIT), Some(0))
            .await
            .unwrap();

        assert!(replies.is_empty());
        assert_eq!(probe.raw_published.load_clone(), vec![DOC_SYNC_TOPIC]);
    })
    .unwrap();
}

#[test]
fn branchable_sync_expecting_no_reply_is_still_sent() {
    workers::block_on(async {
        let transport = NoopTransport::new().with_libp2p_identity();
        transport.raw_topic_has_peer.store(true, Ordering::Release);
        let probe = transport.clone();
        let coordinator = coordinator(transport).await;

        let replies = coordinator
            .pubsub_sync_branchable_collection("c1".into(), Some(WAIT), Some(0))
            .await
            .unwrap();

        assert!(replies.is_empty());
        assert_eq!(
            probe.raw_published.load_clone(),
            vec![BRANCHABLE_SYNC_TOPIC]
        );
    })
    .unwrap();
}

#[test]
fn a_peer_joining_after_the_deadline_is_not_sent_the_request() {
    workers::block_on(async {
        let transport = NoopTransport::new();
        transport.raw_topic_has_peer.store(true, Ordering::Release);
        let joined = Notify::new();
        let (_replies_open, mut responses) = crate::rt::mpsc::channel(1);
        let mut request = HeldRequest::new(
            &transport,
            DOC_SYNC_TOPIC,
            Vec::new(),
            &joined,
            &mut responses,
            Instant::now(),
        );
        // The deadline has passed and a join is waiting when the request runs.
        joined.notify_waiters();

        assert!(request.next().await.unwrap().is_none());
        assert_eq!(
            transport.raw_published.load_clone(),
            vec![DOC_SYNC_TOPIC],
            "only the first send may go out once the deadline has passed"
        );
    })
    .unwrap();
}

#[test]
fn doc_sync_ends_at_the_deadline_when_publish_stalls() {
    workers::block_on(async {
        let transport = NoopTransport::new().with_libp2p_identity();
        transport.raw_publish_stalls.store(true, Ordering::Release);
        let coordinator = coordinator(transport).await;

        let ended = crate::rt::time::timeout(
            WAIT * 4,
            coordinator.pubsub_sync_documents(vec!["doc1".into()], Some(WAIT), Some(1)),
        )
        .await
        .expect("a stalled publish must not hold the request past its deadline");

        ended.expect_err("a request whose publish never completed must fail");
    })
    .unwrap();
}
