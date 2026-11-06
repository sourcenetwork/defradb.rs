//! A filtered replicator's replay and retry decide a `@chunk` chunk by its
//! parent's document and the parent's filter.

use std::sync::Arc;

use db::merge::{push_existing_docs, retry_doc, PushExistingDocsSeOptions};
use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use p2p::transport::PeerId;
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use storage::RegolithStore;

use super::push_docs_replay::ConnectedPeerTransport;

const SDL: &str = "type Article { kind: String body: String @chunk(maxChars: 64) }";
const CHUNKS: &str = "Article__body_chunk";

struct Node {
    db: Arc<DB<RegolithStore>>,
    peer: PeerId,
    filters: p2p::ReplicationFilters,
    chunks_id: String,
}

/// An article and the DocIDs of its chunks.
struct Written {
    id: String,
    chunks: Vec<String>,
}

// A node whose replicator takes only public articles, with their chunks.
async fn node() -> Node {
    let db = Arc::new(DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap());
    db.set_network(edge::testing::edge());
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .unwrap();
    let id_of = |name: &str| {
        db.get_collection(name)
            .unwrap()
            .unwrap()
            .collection_id()
            .to_string()
    };
    let (articles_id, chunks_id) = (id_of("Article"), id_of(CHUNKS));
    let filters = p2p::ReplicationFilters::from([(
        articles_id.clone(),
        p2p::ReplicationFilter::eq("kind", serde_json::json!("public")),
    )]);
    let peer = PeerId::new("receiver".into());
    let info = p2p::ReplicatorInfo::from_raw_with_filters(
        peer.as_str().to_owned(),
        vec![articles_id.clone(), chunks_id.clone()],
        vec![],
        filters.clone(),
    )
    .with_chunk_sources([(chunks_id.clone(), articles_id)]);
    db.peerstore()
        .create_replicator(peer.as_str(), &info.to_bytes().unwrap())
        .await
        .unwrap();
    Node {
        db,
        peer,
        filters,
        chunks_id,
    }
}

async fn write(node: &Node, kind: &str) -> Written {
    let schema = node
        .db
        .get_collection("Article")
        .unwrap()
        .unwrap()
        .schema_arc();
    let mut doc = Document::new(schema);
    doc.set("kind", NormalValue::String(kind.into())).unwrap();
    doc.set("body", NormalValue::String(format!("The {kind} body.")))
        .unwrap();
    let created = AutoCommitMutator::new(node.db.clone())
        .create("Article", doc)
        .await
        .unwrap();
    Written {
        id: created.doc_id.to_string(),
        chunks: created
            .derived
            .iter()
            .map(|chunk| chunk.doc_id.to_string())
            .collect(),
    }
}

fn sent(received: &regolith::sync::BoundedReceiver<p2p::PushLogRequest>) -> RapidHashSet<String> {
    let mut docs = RapidHashSet::default();
    while let Some(request) = received.try_recv() {
        docs.insert(request.doc_id);
    }
    docs
}

async fn retry(node: &Node, chunk: &str) -> RapidHashSet<String> {
    let generation = node
        .db
        .peerstore()
        .observe_push_head_with_generation(node.peer.as_str(), chunk, &node.chunks_id)
        .await
        .unwrap()
        .expect("the replicator owns the chunk's marker");
    let (requests, received) = regolith::sync::bounded(16);
    let transport =
        ConnectedPeerTransport::new("sender", node.peer.as_str()).with_replies(true, requests);
    retry_doc(
        &transport,
        &node.db,
        None,
        &node.peer,
        chunk,
        &node.chunks_id,
        generation,
        &node.filters,
        &p2p::EqOnlyFilterMatcher,
        &p2p::sync::HeadHintCarAuthority::default(),
    )
    .await
    .unwrap();
    sent(&received)
}

#[test]
fn the_replay_sends_only_the_chunks_of_matching_parents() {
    workers::block_on(async {
        let node = node().await;
        let public = write(&node, "public").await;
        write(&node, "secret").await;
        let (requests, received) = regolith::sync::bounded(16);
        let transport =
            ConnectedPeerTransport::new("sender", node.peer.as_str()).with_replies(true, requests);

        push_existing_docs(
            &transport,
            &node.db,
            None,
            &node.peer,
            &["Article".into(), CHUNKS.into()],
            &node.filters,
            PushExistingDocsSeOptions::default(),
            &p2p::EqOnlyFilterMatcher,
            &p2p::sync::HeadHintCarAuthority::default(),
        )
        .await
        .unwrap();

        let mut expected: RapidHashSet<String> = public.chunks.into_iter().collect();
        expected.insert(public.id);
        assert_eq!(sent(&received), expected);
    })
    .unwrap()
}

#[test]
fn a_retry_sends_the_chunk_of_a_matching_parent() {
    workers::block_on(async {
        let node = node().await;
        let public = write(&node, "public").await;
        let chunk = public.chunks[0].clone();

        assert_eq!(retry(&node, &chunk).await, RapidHashSet::from_iter([chunk]));
    })
    .unwrap()
}

#[test]
fn a_retry_of_a_chunk_of_an_unmatched_parent_clears_its_marker_unsent() {
    workers::block_on(async {
        let node = node().await;
        let secret = write(&node, "secret").await;

        assert!(retry(&node, &secret.chunks[0]).await.is_empty());
        assert!(node
            .db
            .peerstore()
            .get_retry_documents(node.peer.as_str())
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}
