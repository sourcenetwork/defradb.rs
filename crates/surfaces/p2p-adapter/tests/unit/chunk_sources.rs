//! A filtered parent's `@chunk` collections follow it in the stored record:
//! paired with it when loaded, and removed with it.

use std::sync::Arc;

use crate::retry::tests::FakeTransport;
use crate::{DbTransportDocPusher, TransportDocPusher};
use p2p::transport::PeerId;

const SDL: &str = "type Article { kind: String @immutable body: String @chunk(maxChars: 64) } type Note { text: String }";
const PEER: &str = "remote-peer";

struct Node {
    pusher: Arc<dyn TransportDocPusher>,
    article: String,
    chunks: String,
    note: String,
}

async fn node() -> Node {
    let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
    db.set_network(edge::testing::edge());
    db.add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .unwrap();
    let id_of = |name: &str| {
        db.get_collection(name)
            .unwrap()
            .unwrap()
            .collection_id()
            .to_string()
    };
    let (article, chunks, note) = (
        id_of("Article"),
        id_of("Article__body_chunk"),
        id_of("Note"),
    );
    let transport = FakeTransport::with_hint(PeerId::new(PEER.into()));
    let blockstore = Arc::new(blockstore::DefraBlockstore::new(db.store().clone(), true));
    let (coordinator, _events) = p2p::sync::SyncCoordinator::new(
        transport.clone(),
        blockstore,
        p2p::network::Network::on(edge::testing::edge()),
        p2p::sync::SyncConfig::default(),
    )
    .await
    .unwrap();
    let pusher = DbTransportDocPusher::new(db, transport, coordinator.head_hint_car_authority());
    Node {
        pusher: Arc::new(pusher),
        article,
        chunks,
        note,
    }
}

impl Node {
    /// A record that takes public articles with their chunks, and every note.
    fn record(&self) -> p2p::ReplicatorInfo {
        p2p::ReplicatorInfo::from_raw_with_filters(
            PEER.into(),
            vec![self.article.clone(), self.chunks.clone(), self.note.clone()],
            Vec::new(),
            [(
                self.article.clone(),
                p2p::ReplicationFilter::eq("kind", serde_json::json!("public")),
            )]
            .into(),
        )
    }

    async fn stored(&self) -> p2p::ReplicatorInfo {
        self.pusher
            .load_persisted_replicators()
            .await
            .unwrap()
            .unwrap()
            .remove(0)
    }
}

#[nuclei::test]
async fn a_record_is_paired_with_the_chunk_collections_of_its_filtered_parents() {
    let node = node().await;
    let mut info = node.record();

    crate::set_chunk_sources(Some(&node.pusher), &mut info);

    assert_eq!(
        info.chunk_sources,
        [(node.chunks.clone(), node.article.clone())].into()
    );
    assert!(info.is_filtered_for_collection(&node.chunks));
}

#[nuclei::test]
async fn removing_a_parent_by_id_removes_its_chunk_collection_too() {
    let node = node().await;

    let removed = crate::collections_to_remove(Some(&node.pusher), vec![node.article.clone()]);

    assert_eq!(removed, vec![node.article.clone(), node.chunks.clone()]);
}

#[nuclei::test]
async fn a_stored_record_drops_the_filter_and_chunk_pairs_of_removed_collections() {
    let node = node().await;
    let mut info = node.record();
    crate::set_chunk_sources(Some(&node.pusher), &mut info);
    node.pusher.persist_replicator_info(&info).await.unwrap();

    node.pusher
        .persist_replicator(PEER, std::slice::from_ref(&node.note))
        .await
        .unwrap();

    let stored = node.stored().await;
    assert_eq!(stored.collections, vec![node.note.clone()]);
    assert!(stored.filters.is_empty());
    assert!(stored.chunk_sources.is_empty());
}
