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

use super::push_docs_replay::ConnectedPeerTransport;

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

        assert!(
            matches!(&status, BroadcastStatus::Failed(error) if error.contains("InsufficientPeers")),
            "{status:?}"
        );
        assert_eq!(transport.publishes(), 2, "one publish to each topic");
    })
    .unwrap()
}
