use std::sync::Arc;
use std::time::Duration;

use blockstore::DefraBlockstore;
use db::block::builder::BlockResult;
use db::merge::broadcast_mutator::broadcast::broadcast_with_creator;
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta};
use futures::future::{select, Either};
use p2p::sync::{SyncConfig, SyncCoordinator};
use query::mutator::BroadcastStatus;
use storage::RegolithStore;

use super::push_docs_replay::{ConnectedPeerTransport, PublishFails};

/// A document head block, as a committed write announces it.
fn head() -> BlockResult {
    let block = Block::new_with_options(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "schema".to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![],
        None,
        None,
    );
    let bytes = block.to_dag_cbor().unwrap();
    BlockResult {
        cid: defra_core::block::generate_cid_from_bytes(&bytes).unwrap(),
        block: bytes.into(),
        doc_id: "doc".to_string(),
        field_cids: vec![],
        encryption_cids: vec![],
    }
}

/// A write committed while the only peer is connected but not yet subscribed
/// is published once and reaches no one, as Go's `SendUpdate`. It is not
/// published again while the peer stays connected, which would hand it to the
/// peer once its subscription lands (#2841).
#[test]
fn a_write_that_finds_no_subscriber_is_not_published_again() {
    workers::block_on(async {
        let transport = ConnectedPeerTransport::new("local", "connected-not-subscribed");
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let (sync, _events) = SyncCoordinator::new(
            transport.clone(),
            Arc::new(DefraBlockstore::new(store, true)),
            p2p::network::Network::on(edge::testing::edge()),
            SyncConfig::default(),
        )
        .await
        .unwrap();
        let block_result = head();

        let announce = std::pin::pin!(broadcast_with_creator(
            &sync,
            None,
            &block_result,
            "collection",
            "Users",
            None,
        ));
        let deadline = std::pin::pin!(workers::delay(Duration::from_secs(5)).unwrap());
        let Either::Left((status, _)) = select(announce, deadline).await else {
            panic!(
                "still publishing after 5 s, {} publishes in",
                transport.publishes()
            );
        };

        assert!(matches!(&status, BroadcastStatus::Success), "{status:?}");
        assert_eq!(transport.publishes(), 2, "one publish to each topic");
    })
    .unwrap()
}

/// Announces `block_result` over a coordinator on `publish_fails` and returns
/// the reported status, panicking if the announce does not finish in time.
fn announce(publish_fails: PublishFails) -> (BroadcastStatus, usize) {
    workers::block_on(async {
        let transport = ConnectedPeerTransport::new("local", "connected-not-subscribed")
            .with_publish_fails(publish_fails);
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let (sync, _events) = SyncCoordinator::new(
            transport.clone(),
            Arc::new(DefraBlockstore::new(store, true)),
            p2p::network::Network::on(edge::testing::edge()),
            SyncConfig::default(),
        )
        .await
        .unwrap();
        let block_result = head();

        let announce = std::pin::pin!(broadcast_with_creator(
            &sync,
            None,
            &block_result,
            "collection",
            "Users",
            None,
        ));
        let deadline = std::pin::pin!(workers::delay(Duration::from_secs(5)).unwrap());
        let Either::Left((status, _)) = select(announce, deadline).await else {
            panic!(
                "still publishing after 5 s, {} publishes in",
                transport.publishes()
            );
        };
        (status, transport.publishes())
    })
    .unwrap()
}

/// A document topic with no peer and a collection topic that has one is the
/// usual path until a peer subscribes to the document: the write is announced
/// on the collection topic and reported as a success, like the other
/// no-subscriber cases, not as a partial failure (#3388).
#[test]
fn a_write_whose_document_topic_alone_has_no_peer_is_a_success() {
    let (status, publishes) = announce(PublishFails::DocumentTopic);

    assert!(matches!(&status, BroadcastStatus::Success), "{status:?}");
    assert_eq!(publishes, 2, "one publish to each topic");
}

/// The inverse: the collection topic has no peer and the document topic does.
/// The write is announced on the document topic and is a success too.
#[test]
fn a_write_whose_collection_topic_alone_has_no_peer_is_a_success() {
    let (status, publishes) = announce(PublishFails::CollectionTopic);

    assert!(matches!(&status, BroadcastStatus::Success), "{status:?}");
    assert_eq!(publishes, 2, "one publish to each topic");
}

/// A publish that fails for another reason is still a failure.
#[test]
fn a_write_whose_publish_fails_is_reported_as_a_failure() {
    let (status, publishes) = announce(PublishFails::Transport);

    assert!(matches!(&status, BroadcastStatus::Failed(_)), "{status:?}");
    assert_eq!(publishes, 2, "one publish to each topic");
}
