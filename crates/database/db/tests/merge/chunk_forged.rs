//! A peer cannot plant a chunk whose hash is not its text, or rewrite a chunk
//! that already exists.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::chunk::hash::text_hash;
use db::collection::Collection;
use db::database::DB;
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use storage::RegolithStore;

pub(super) const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;
const CHUNKS: &str = "Article__body_chunk";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

pub(super) struct Node {
    pub(super) db: Arc<DB<RegolithStore>>,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    handler: Handler,
}

/// Chunks that carry a vector; merges never call the model, so its URL is never used.
const EMBEDDED_SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
        body_v: [Float32!] @embedding(fields: ["body"], provider: "openai", model: "m", url: "http://127.0.0.1:9")
    }
"#;

pub(super) async fn node() -> Node {
    node_with(SDL, None).await
}

/// A node that publishes its events on `bus`.
pub(super) async fn node_with_bus(bus: Option<Arc<dyn events::Bus>>) -> Node {
    node_with(SDL, bus).await
}

async fn node_with(sdl: &str, bus: Option<Arc<dyn events::Bus>>) -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let mut db = DB::from_arc(store.clone()).unwrap();
    if let Some(bus) = bus {
        db.set_event_bus(bus);
    }
    let db = Arc::new(db);
    db.add_collection(&defra_core::RequestContext::ambient(), sdl)
        .await
        .unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    Node {
        db,
        blockstore,
        handler,
    }
}

pub(super) fn chunks_of(node: &Node) -> Collection {
    node.db.get_collection(CHUNKS).unwrap().expect("chunks")
}

fn chunk(collection: &Collection, parent: &DocID, text: &str, hash: &str) -> Document {
    let mut doc = Document::new(collection.schema_arc());
    doc.set("_sourceID", NormalValue::String(parent.to_string()))
        .unwrap();
    doc.set("text", NormalValue::String(text.into())).unwrap();
    doc.set("hash", NormalValue::String(hash.into())).unwrap();
    doc
}

async fn parent_article(node: &Node) -> DocID {
    parent_article_titled(node, "t").await
}

async fn parent_article_titled(node: &Node, title: &str) -> DocID {
    let collection = node.db.get_collection("Article").unwrap().unwrap();
    let mut doc = Document::new(collection.schema_arc());
    doc.set("title", NormalValue::String(title.into())).unwrap();
    doc.set("body", NormalValue::String("short".into()))
        .unwrap();
    db::AutoCommitMutator::new(Arc::clone(&node.db))
        .create("Article", doc)
        .await
        .unwrap()
        .doc_id
}

async fn copy_links(from: &Node, to: &Node, block: &Block) {
    for link in block.links.iter().flatten() {
        let data = from.blockstore.get(&link.link).await.unwrap().unwrap();
        to.blockstore.put(&link.link, &data).await.unwrap();
    }
}

pub(super) async fn merge(
    from: &Node,
    to: &Node,
    collection: &Collection,
    cid: &cid::Cid,
    doc_id: &str,
) -> Result<MergeOutcome, MergeError> {
    let bytes = from.blockstore.get(cid).await.unwrap().unwrap();
    copy_links(from, to, &Block::from_dag_cbor(&bytes).unwrap()).await;
    to.blockstore.put(cid, &bytes).await.unwrap();
    to.handler
        .handle_block(
            cid,
            &bytes,
            BlockMetadata::normal(
                doc_id,
                collection.collection_id(),
                "did:key:z6MkChunkForger",
                Some("peer"),
                false,
            ),
        )
        .await
}

async fn stored(node: &Node, collection: &Collection, doc_id: &DocID) -> Option<Document> {
    let txn = node.db.new_txn(true).await.unwrap();
    let doc = collection
        .get_by_doc_id(
            &txn.datastore().unwrap(),
            &txn.systemstore().unwrap(),
            doc_id,
        )
        .await
        .unwrap();
    txn.force_discard().unwrap();
    doc
}

pub(super) async fn body_of(node: &Node, parent: &DocID) -> Option<String> {
    let parents = node.db.get_collection("Article").unwrap().unwrap();
    stored(node, &parents, parent)
        .await
        .unwrap()
        .get("body")
        .and_then(NormalValue::as_str)
        .map(str::to_owned)
}

fn assert_rejected(outcome: &MergeOutcome, why: &str) {
    let MergeOutcome::Rejected { reason } = outcome else {
        panic!("expected a rejection, got {outcome:?}");
    };
    assert!(reason.contains(why), "{reason}");
}

/// Delivers the block again: a refusal for good is rejected again, never stored or retried.
async fn assert_refused_again(
    from: &Node,
    to: &Node,
    collection: &Collection,
    cid: &cid::Cid,
    doc_id: &str,
    why: &str,
) {
    let again = merge(from, to, collection, cid, doc_id).await.unwrap();
    assert_rejected(&again, why);
}

/// The CID of an update of a stored chunk to `doc`'s `fields`, written on `author`.
async fn write_update(
    author: &Node,
    collection: &Collection,
    short_id: u64,
    doc: &Document,
    fields: &[&str],
) -> cid::Cid {
    let modified: rapidhash::RapidHashSet<String> =
        fields.iter().copied().map(String::from).collect();
    let txn = author.db.new_txn(false).await.unwrap();
    let update = db::block::builder::write_document_blocks(
        &txn.blockstore().unwrap(),
        &txn.headstore().unwrap(),
        doc,
        collection.schema(),
        db::block::builder::DocRef::new(collection.resolved_root_id(), short_id),
        Some(&modified),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    db::docid::map::set_block_doc_id_mapping(
        &txn.systemstore().unwrap(),
        &update.cid.to_string(),
        &doc.id().unwrap().to_string(),
    )
    .await
    .unwrap();
    txn.force_commit().await.unwrap();
    update.cid
}

#[test]
fn a_chunk_with_the_wrong_hash_is_refused_for_good() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = parent_article(&peer).await;
        let before = body_of(&peer, &parent).await;

        let collection = chunks_of(&author);
        let forged = chunk(&collection, &parent, "garbage", &text_hash("real text"));
        let built = db::block::builder::build_blocks_from_document(
            &storage::Crdts::default(),
            &forged,
            collection.schema(),
            &author.blockstore,
        )
        .await
        .unwrap();

        let outcome = merge(&author, &peer, &collection, &built.cid, &built.doc_id)
            .await
            .unwrap();
        assert_rejected(&outcome, "hash does not match");
        assert_refused_again(
            &author,
            &peer,
            &collection,
            &built.cid,
            &built.doc_id,
            "hash does not match",
        )
        .await;

        let chunk_id = DocID::from_string(&built.doc_id).unwrap();
        assert!(stored(&peer, &chunks_of(&peer), &chunk_id).await.is_none());
        assert_eq!(body_of(&peer, &parent).await, before);
    })
    .unwrap()
}

#[test]
fn a_chunk_cannot_be_rewritten_after_it_merged() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = parent_article(&peer).await;
        let before = body_of(&peer, &parent).await;

        let collection = chunks_of(&author);
        let text = "original text";
        let mut genuine = chunk(&collection, &parent, text, &text_hash(text));
        let handler_collection = collection.clone();
        let (chunk_id, short_id, created) = super::merge_handler_tests::create_doc_locally(
            &author.handler,
            &handler_collection,
            &mut genuine,
        )
        .await;
        let chunk_id_str = chunk_id.to_string();

        let merged = merge(&author, &peer, &collection, &created.cid, &chunk_id_str)
            .await
            .expect("the genuine chunk merges");
        assert!(merged.is_merged(), "{merged:?}");
        let peer_chunks = chunks_of(&peer);
        let kept = stored(&peer, &peer_chunks, &chunk_id).await.unwrap();

        let mut rewrite = Document::new(collection.schema_arc());
        rewrite.set_id(chunk_id.clone());
        rewrite
            .set("text", NormalValue::String("replaced".into()))
            .unwrap();
        rewrite
            .set("hash", NormalValue::String(text_hash("replaced")))
            .unwrap();
        let update =
            write_update(&author, &collection, short_id, &rewrite, &["text", "hash"]).await;

        let outcome = merge(&author, &peer, &collection, &update, &chunk_id_str)
            .await
            .unwrap();
        assert_rejected(&outcome, "cannot change after it is created");
        assert_refused_again(
            &author,
            &peer,
            &collection,
            &update,
            &chunk_id_str,
            "cannot change after it is created",
        )
        .await;

        assert_eq!(stored(&peer, &peer_chunks, &chunk_id).await.unwrap(), kept);
        assert_eq!(body_of(&peer, &parent).await, before);
    })
    .unwrap()
}

#[test]
fn a_chunk_cannot_move_to_another_parent_after_it_merged() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = parent_article(&peer).await;
        let other = parent_article_titled(&peer, "other").await;

        let collection = chunks_of(&author);
        let text = "original text";
        let mut genuine = chunk(&collection, &parent, text, &text_hash(text));
        let (chunk_id, short_id, created) = super::merge_handler_tests::create_doc_locally(
            &author.handler,
            &collection,
            &mut genuine,
        )
        .await;
        let chunk_id_str = chunk_id.to_string();
        let merged = merge(&author, &peer, &collection, &created.cid, &chunk_id_str)
            .await
            .expect("the genuine chunk merges");
        assert!(merged.is_merged(), "{merged:?}");
        let peer_chunks = chunks_of(&peer);
        let kept = stored(&peer, &peer_chunks, &chunk_id).await.unwrap();

        let mut moved = chunk(&collection, &other, text, &text_hash(text));
        moved.set_id(chunk_id.clone());
        let update = write_update(&author, &collection, short_id, &moved, &["_sourceID"]).await;

        let outcome = merge(&author, &peer, &collection, &update, &chunk_id_str)
            .await
            .unwrap();
        assert_rejected(&outcome, "cannot change after it is created");
        assert_eq!(stored(&peer, &peer_chunks, &chunk_id).await.unwrap(), kept);
    })
    .unwrap()
}

#[test]
fn a_chunk_with_a_null_text_and_a_real_hash_is_refused() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = parent_article(&peer).await;

        let collection = chunks_of(&author);
        let mut forged = chunk(&collection, &parent, "short", &text_hash("short"));
        forged.set("text", NormalValue::Null).unwrap();
        let built = db::block::builder::build_blocks_from_document(
            &storage::Crdts::default(),
            &forged,
            collection.schema(),
            &author.blockstore,
        )
        .await
        .unwrap();

        let outcome = merge(&author, &peer, &collection, &built.cid, &built.doc_id)
            .await
            .unwrap();

        assert_rejected(&outcome, "must both be strings");
        let chunk_id = DocID::from_string(&built.doc_id).unwrap();
        assert!(stored(&peer, &chunks_of(&peer), &chunk_id).await.is_none());
    })
    .unwrap()
}

#[test]
fn a_chunk_vector_cannot_be_changed_after_it_merged() {
    workers::block_on(async {
        let author = node_with(EMBEDDED_SDL, None).await;
        let peer = node_with(EMBEDDED_SDL, None).await;
        // With no body the parent has no chunks of its own, so its create needs no model.
        let articles = peer.db.get_collection("Article").unwrap().unwrap();
        let mut untitled = Document::new(articles.schema_arc());
        untitled
            .set("title", NormalValue::String("t".into()))
            .unwrap();
        let parent = db::AutoCommitMutator::new(Arc::clone(&peer.db))
            .create("Article", untitled)
            .await
            .unwrap()
            .doc_id;
        let collection = chunks_of(&author);
        let text = "original text";
        let mut genuine = chunk(&collection, &parent, text, &text_hash(text));
        genuine
            .set("body_v", NormalValue::Float64Array(vec![1.0, 0.0]))
            .unwrap();
        let (chunk_id, short_id, created) = super::merge_handler_tests::create_doc_locally(
            &author.handler,
            &collection,
            &mut genuine,
        )
        .await;
        let chunk_id_str = chunk_id.to_string();
        let merged = merge(&author, &peer, &collection, &created.cid, &chunk_id_str)
            .await
            .expect("the genuine chunk merges");
        assert!(merged.is_merged(), "{merged:?}");
        let peer_chunks = chunks_of(&peer);
        let kept = stored(&peer, &peer_chunks, &chunk_id).await.unwrap();

        let mut revector = Document::new(collection.schema_arc());
        revector.set_id(chunk_id.clone());
        revector
            .set("body_v", NormalValue::Float64Array(vec![0.0, 1.0]))
            .unwrap();
        let update = write_update(&author, &collection, short_id, &revector, &["body_v"]).await;

        let outcome = merge(&author, &peer, &collection, &update, &chunk_id_str)
            .await
            .unwrap();
        assert_rejected(&outcome, "cannot change after it is created. Field: body_v");
        assert_eq!(stored(&peer, &peer_chunks, &chunk_id).await.unwrap(), kept);
    })
    .unwrap()
}
