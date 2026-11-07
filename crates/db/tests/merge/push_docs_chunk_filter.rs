//! A filtered replicator's replay and retry send a `@chunk` chunk only together
//! with its parent, decided by the parent's document and filter.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::chunk::list::text_hash;
use db::merge::merge_handler::DbMergeHandler;
use db::merge::{push_existing_docs, retry_doc, PushExistingDocsSeOptions};
use db::{AutoCommitMutator, DB};
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{Document, NormalValue};
use p2p::replicator::ReplicationFilterMatcher;
use p2p::transport::PeerId;
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use storage::RegolithStore;

use super::chunk_filter_support::{public_only_replicator, CHUNKS, PEER, PUBLIC, SDL, SECRET};
use super::push_docs_replay::ConnectedPeerTransport;

const CHUNK_TEXT: &str = "A chunk from another node.";
/// A DocID no article on this node has.
const ABSENT_PARENT: &str = "bae-00000000-0000-0000-0000-000000000000";
/// More than every request a test sends.
const REQUEST_CAPACITY: usize = 32;
/// Paragraphs of 64 characters or less, so each is a chunk of its own.
const PARAGRAPHS: [&str; 3] = ["First paragraph.", "Second paragraph.", "Third paragraph."];
/// Two articles, each checked once as itself and once for all its chunks.
const FILTER_CHECKS_FOR_TWO_ARTICLES: usize = 4;

struct Node {
    db: Arc<DB<RegolithStore>>,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    peer: PeerId,
    filters: p2p::ReplicationFilters,
    chunks_id: String,
}

/// An article, its chunks, and their heads.
struct Written {
    id: String,
    commit: cid::Cid,
    chunks: Vec<String>,
    chunk_commits: Vec<cid::Cid>,
}

impl Written {
    /// The article with its first chunk.
    fn with_first_chunk(&self) -> RapidHashSet<String> {
        RapidHashSet::from_iter([self.id.clone(), self.chunks[0].clone()])
    }

    fn with_chunks(&self) -> RapidHashSet<String> {
        let mut docs: RapidHashSet<String> = self.chunks.iter().cloned().collect();
        docs.insert(self.id.clone());
        docs
    }
}

/// A node whose replicator takes only public articles, or every article when
/// `filtered` is false, as on a node that only authors documents.
async fn node_with(filtered: bool) -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.set_network(edge::testing::edge());
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .unwrap();
    let info = public_only_replicator(&db);
    let chunks_id = db
        .get_collection(CHUNKS)
        .unwrap()
        .unwrap()
        .collection_id()
        .to_string();
    db.peerstore()
        .create_replicator(PEER, &info.to_bytes().unwrap())
        .await
        .unwrap();
    Node {
        db,
        blockstore: Arc::new(DefraBlockstore::new(store, false)),
        peer: PeerId::new(PEER.into()),
        filters: if filtered {
            info.filters
        } else {
            p2p::ReplicationFilters::new()
        },
        chunks_id,
    }
}

async fn node() -> Node {
    node_with(true).await
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
    let body = PARAGRAPHS
        .iter()
        .map(|paragraph| format!("{kind}: {paragraph}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    doc.set("body", NormalValue::String(body)).unwrap();
    let created = AutoCommitMutator::new(node.db.clone())
        .create("Article", doc)
        .await
        .unwrap();
    Written {
        id: created.doc_id.to_string(),
        commit: created.commit_cid.expect("the create's commit"),
        chunks: created
            .derived
            .iter()
            .map(|chunk| chunk.doc_id.to_string())
            .collect(),
        chunk_commits: created
            .derived
            .iter()
            .map(|chunk| chunk.commit_cid)
            .collect(),
    }
}

/// Merges, as from another node, the document whose head is `cid`.
async fn merge_from(from: &Node, to: &Node, collection: &str, cid: &cid::Cid, doc_id: &str) {
    let collection = to.db.get_collection(collection).unwrap().unwrap();
    let bytes = from.blockstore.get(cid).await.unwrap().unwrap();
    for link in Block::from_dag_cbor(&bytes).unwrap().links.iter().flatten() {
        let data = from.blockstore.get(&link.link).await.unwrap().unwrap();
        to.blockstore.put(&link.link, &data).await.unwrap();
    }
    to.blockstore.put(cid, &bytes).await.unwrap();
    let outcome = DbMergeHandler::new(to.db.clone(), to.blockstore.clone())
        .handle_block(
            cid,
            &bytes,
            BlockMetadata::normal(
                doc_id,
                collection.collection_id(),
                "did:key:z6MkChunkAuthor",
                Some("author"),
                false,
            ),
        )
        .await
        .unwrap();
    assert_eq!(outcome, MergeOutcome::Merged);
}

/// An article of `kind` written on another node whose first chunk reaches
/// `node` before the article does.
async fn chunk_then_parent(node: &Node, kind: &str) -> Written {
    let author = node_with(false).await;
    let written = write(&author, kind).await;
    merge_from(
        &author,
        node,
        CHUNKS,
        &written.chunk_commits[0],
        &written.chunks[0],
    )
    .await;
    merge_from(&author, node, "Article", &written.commit, &written.id).await;
    written
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
    let author = node_with(false).await;
    let built = db::block::builder::build_blocks_from_document(
        &storage::Crdts::default(),
        &doc,
        chunks.schema(),
        &author.blockstore,
    )
    .await
    .unwrap();
    merge_from(&author, node, CHUNKS, &built.cid, &built.doc_id).await;
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

async fn replay_with(node: &Node, matcher: &dyn ReplicationFilterMatcher) -> RapidHashSet<String> {
    let (transport, received) = transport(node);
    push_existing_docs(
        &transport,
        &node.db,
        None,
        &node.peer,
        &["Article".into(), CHUNKS.into()],
        &node.filters,
        PushExistingDocsSeOptions::default(),
        matcher,
        &p2p::sync::HeadHintCarAuthority::default(),
    )
    .await
    .unwrap();
    sent(&received)
}

async fn replay(node: &Node) -> RapidHashSet<String> {
    replay_with(node, &p2p::EqOnlyFilterMatcher).await
}

/// Retries `chunk`'s marker and returns the DocIDs it sent.
async fn retry(node: &Node, chunk: &str) -> RapidHashSet<String> {
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
    .await
    .unwrap();
    sent(&received)
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

/// A matcher that counts the filter checks it runs.
#[derive(Default)]
struct Counting(AtomicUsize);

impl ReplicationFilterMatcher for Counting {
    fn matches(
        &self,
        collection_id: &str,
        filter: &p2p::ReplicationFilter,
        document: &serde_json::Value,
    ) -> bool {
        self.0.fetch_add(1, Ordering::SeqCst);
        p2p::EqOnlyFilterMatcher.matches(collection_id, filter, document)
    }
}

#[test]
fn the_replay_sends_only_the_matching_parents_with_their_chunks() {
    workers::block_on(async {
        let node = node().await;
        let public = write(&node, PUBLIC).await;
        write(&node, SECRET).await;

        assert_eq!(replay(&node).await, public.with_chunks());
    })
    .unwrap()
}

#[test]
fn the_replay_checks_each_parent_once_however_many_chunks_it_has() {
    workers::block_on(async {
        let node = node().await;
        write(&node, PUBLIC).await;
        write(&node, SECRET).await;
        let matcher = Counting::default();

        replay_with(&node, &matcher).await;

        assert_eq!(
            matcher.0.load(Ordering::SeqCst),
            FILTER_CHECKS_FOR_TWO_ARTICLES
        );
    })
    .unwrap()
}

#[test]
fn a_retry_sends_a_chunk_together_with_its_matching_parent() {
    workers::block_on(async {
        let node = node().await;
        let public = write(&node, PUBLIC).await;

        assert_eq!(
            retry(&node, &public.chunks[0]).await,
            public.with_first_chunk()
        );
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_retry_of_a_chunk_of_an_unmatched_parent_clears_its_marker_unsent() {
    workers::block_on(async {
        let node = node().await;
        let chunk = write(&node, SECRET).await.chunks[0].clone();

        assert!(retry(&node, &chunk).await.is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_retry_of_a_chunk_without_a_source_clears_its_marker_unsent() {
    workers::block_on(async {
        let node = node().await;
        let chunk = merge_foreign_chunk(&node, None).await;

        assert!(retry(&node, &chunk).await.is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_retry_of_a_chunk_whose_parent_has_not_arrived_clears_its_marker_unsent() {
    workers::block_on(async {
        let node = node().await;
        let chunk = merge_foreign_chunk(&node, Some(ABSENT_PARENT)).await;

        assert!(retry(&node, &chunk).await.is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn the_replay_skips_a_chunk_whose_parent_has_not_arrived_without_a_marker() {
    workers::block_on(async {
        let node = node().await;
        merge_foreign_chunk(&node, Some(ABSENT_PARENT)).await;

        assert!(replay(&node).await.is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_chunk_that_arrived_before_its_matching_parent_is_retried_with_it() {
    workers::block_on(async {
        let node = node().await;
        let public = chunk_then_parent(&node, PUBLIC).await;

        assert_eq!(
            retry(&node, &public.chunks[0]).await,
            public.with_first_chunk()
        );
    })
    .unwrap()
}

#[test]
fn a_chunk_that_arrived_before_an_unmatched_parent_is_not_sent() {
    workers::block_on(async {
        let node = node().await;
        let secret = chunk_then_parent(&node, SECRET).await;

        assert!(retry(&node, &secret.chunks[0]).await.is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_chunk_whose_parent_document_is_gone_is_not_sent() {
    workers::block_on(async {
        let node = node().await;
        let public = write(&node, PUBLIC).await;
        let articles = node.db.get_collection("Article").unwrap().unwrap();
        let txn = node.db.new_txn(false).await.unwrap();
        let parent = db::docid::map::get_doc_ref(&txn.systemstore().unwrap(), &public.id)
            .await
            .unwrap()
            .unwrap();
        txn.datastore()
            .unwrap()
            .delete(&storage::keys::doc_key(
                articles.collection_id(),
                parent.doc_short_id,
            ))
            .await
            .unwrap();
        txn.commit().await.unwrap();

        assert!(retry(&node, &public.chunks[0]).await.is_empty());
        assert!(markers(&node).await.is_empty());
    })
    .unwrap()
}
