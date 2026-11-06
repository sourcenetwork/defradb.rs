//! A filtered replicator's replay and retry decide a `@chunk` chunk by its
//! parent's document and the parent's filter.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::chunk::list::text_hash;
use db::merge::merge_handler::DbMergeHandler;
use db::merge::{push_existing_docs, retry_doc, PushExistingDocsSeOptions};
use db::{AutoCommitMutator, DB};
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{Document, NormalValue};
use p2p::transport::PeerId;
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use storage::RegolithStore;

use super::push_docs_replay::ConnectedPeerTransport;

const SDL: &str = "type Article { kind: String body: String @chunk(maxChars: 64) }";
const CHUNKS: &str = "Article__body_chunk";
const PUBLIC: &str = "public";
const SECRET: &str = "secret";
const CHUNK_TEXT: &str = "A chunk from another node.";
/// A DocID no article on this node has.
const ABSENT_PARENT: &str = "bae-00000000-0000-0000-0000-000000000000";
/// More than every request a test sends.
const REQUEST_CAPACITY: usize = 16;

struct Node {
    db: Arc<DB<RegolithStore>>,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
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
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
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
        p2p::ReplicationFilter::eq("kind", serde_json::json!(PUBLIC)),
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
        blockstore: Arc::new(DefraBlockstore::new(store, false)),
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

/// Merges a chunk that came from another node, naming `parent` as its source
/// when there is one, and returns its DocID.
async fn merge_foreign_chunk(node: &Node, parent: Option<&str>) -> String {
    let chunks = node.db.get_collection(CHUNKS).unwrap().unwrap();
    let mut doc = Document::new(chunks.schema_arc());
    if let Some(parent) = parent {
        doc.set("_sourceID", NormalValue::String(parent.into()))
            .unwrap();
    }
    doc.set("text", NormalValue::String(CHUNK_TEXT.into()))
        .unwrap();
    doc.set("hash", NormalValue::String(text_hash(CHUNK_TEXT)))
        .unwrap();
    let author = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        false,
    ));
    let built = db::block::builder::build_blocks_from_document(
        &storage::Crdts::default(),
        &doc,
        chunks.schema(),
        &author,
    )
    .await
    .unwrap();
    let bytes = author.get(&built.cid).await.unwrap().unwrap();
    for link in Block::from_dag_cbor(&bytes).unwrap().links.iter().flatten() {
        let data = author.get(&link.link).await.unwrap().unwrap();
        node.blockstore.put(&link.link, &data).await.unwrap();
    }
    node.blockstore.put(&built.cid, &bytes).await.unwrap();
    let outcome = DbMergeHandler::new(node.db.clone(), node.blockstore.clone())
        .handle_block(
            &built.cid,
            &bytes,
            BlockMetadata::normal(
                &built.doc_id,
                chunks.collection_id(),
                "did:key:z6MkChunkAuthor",
                Some("author"),
                false,
            ),
        )
        .await
        .unwrap();
    assert_eq!(outcome, MergeOutcome::Merged);
    built.doc_id
}

fn transport(
    node: &Node,
) -> (
    ConnectedPeerTransport,
    regolith::sync::BoundedReceiver<p2p::PushLogRequest>,
) {
    let (requests, received) = regolith::sync::bounded(REQUEST_CAPACITY);
    let transport =
        ConnectedPeerTransport::new("sender", node.peer.as_str()).with_replies(true, requests);
    (transport, received)
}

fn sent(received: &regolith::sync::BoundedReceiver<p2p::PushLogRequest>) -> RapidHashSet<String> {
    let mut docs = RapidHashSet::default();
    while let Some(request) = received.try_recv() {
        docs.insert(request.doc_id);
    }
    docs
}

async fn replay(node: &Node) -> RapidHashSet<String> {
    let (transport, received) = transport(node);
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
    sent(&received)
}

/// Retries `chunk`'s marker and returns what it sent, or its error.
async fn retry(node: &Node, chunk: &str) -> Result<RapidHashSet<String>, String> {
    let generation = node
        .db
        .peerstore()
        .observe_push_head_with_generation(node.peer.as_str(), chunk, &node.chunks_id)
        .await
        .unwrap()
        .expect("the replicator owns the chunk's marker");
    let (transport, received) = transport(node);
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
    .await?;
    Ok(sent(&received))
}

async fn markers(node: &Node) -> Vec<String> {
    node.db
        .peerstore()
        .get_retry_documents(node.peer.as_str())
        .await
        .unwrap()
        .into_iter()
        .map(|marker| marker.doc_id)
        .collect()
}

#[test]
fn the_replay_sends_only_the_chunks_of_matching_parents() {
    workers::block_on(async {
        let node = node().await;
        let public = write(&node, PUBLIC).await;
        write(&node, SECRET).await;

        let mut expected: RapidHashSet<String> = public.chunks.into_iter().collect();
        expected.insert(public.id);
        assert_eq!(replay(&node).await, expected);
    })
    .unwrap()
}

#[test]
fn a_retry_sends_the_chunk_of_a_matching_parent() {
    workers::block_on(async {
        let node = node().await;
        let chunk = write(&node, PUBLIC).await.chunks[0].clone();

        assert_eq!(
            retry(&node, &chunk).await.unwrap(),
            RapidHashSet::from_iter([chunk])
        );
    })
    .unwrap()
}

#[test]
fn a_retry_of_a_chunk_of_an_unmatched_parent_clears_its_marker_unsent() {
    workers::block_on(async {
        let node = node().await;
        let chunk = write(&node, SECRET).await.chunks[0].clone();

        assert!(retry(&node, &chunk).await.unwrap().is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_retry_of_a_chunk_without_a_source_clears_its_marker_unsent() {
    workers::block_on(async {
        let node = node().await;
        let chunk = merge_foreign_chunk(&node, None).await;

        assert!(retry(&node, &chunk).await.unwrap().is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_retry_of_a_chunk_whose_parent_has_not_arrived_keeps_its_marker() {
    workers::block_on(async {
        let node = node().await;
        let chunk = merge_foreign_chunk(&node, Some(ABSENT_PARENT)).await;

        let error = retry(&node, &chunk).await.unwrap_err();
        assert!(error.contains("waits for its parent"), "{error}");
        assert_eq!(markers(&node).await, vec![chunk]);
    })
    .unwrap()
}

#[test]
fn the_replay_leaves_a_chunk_whose_parent_has_not_arrived_owed() {
    workers::block_on(async {
        let node = node().await;
        let chunk = merge_foreign_chunk(&node, Some(ABSENT_PARENT)).await;

        assert!(replay(&node).await.is_empty());
        assert_eq!(markers(&node).await, vec![chunk]);
    })
    .unwrap()
}
