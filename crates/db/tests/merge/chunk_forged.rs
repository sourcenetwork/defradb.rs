//! A peer cannot plant a chunk whose hash is not its text, or rewrite a chunk
//! that already exists.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::chunk::list::text_hash;
use db::collection::Collection;
use db::database::DB;
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeErrorDisposition, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use storage::RegolithStore;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;
const CHUNKS: &str = "Article__body_chunk";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

struct Node {
    db: Arc<DB<RegolithStore>>,
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

async fn node() -> Node {
    node_with(SDL).await
}

async fn node_with(sdl: &str) -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collections_atomic(query::parse_sdl(sdl).unwrap())
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

fn chunks_of(node: &Node) -> Collection {
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
    let collection = node.db.get_collection("Article").unwrap().unwrap();
    let mut doc = Document::new(collection.schema_arc());
    doc.set("title", NormalValue::String("t".into())).unwrap();
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

async fn merge(
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

async fn list_of(node: &Node, parent: &DocID) -> Vec<String> {
    let parents = node.db.get_collection("Article").unwrap().unwrap();
    match stored(node, &parents, parent)
        .await
        .unwrap()
        .get("_body_chunks")
    {
        Some(NormalValue::StringArray(list)) => list.clone(),
        Some(NormalValue::NillableStringArray(Some(list))) => list.clone(),
        _ => Vec::new(),
    }
}

fn assert_rejected(outcome: &MergeOutcome, why: &str) {
    let MergeOutcome::Rejected { reason } = outcome else {
        panic!("expected a rejection, got {outcome:?}");
    };
    assert!(reason.contains(why), "{reason}");
}

fn assert_terminal(node: &Node) {
    let error = MergeError::ChunkRejected("a chunk".into());
    assert_eq!(
        node.handler.error_disposition(&error),
        MergeErrorDisposition::Terminal
    );
}

#[test]
fn a_chunk_with_the_wrong_hash_is_refused_for_good() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = parent_article(&peer).await;
        let before = list_of(&peer, &parent).await;

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
        assert_terminal(&peer);

        let chunk_id = DocID::from_string(&built.doc_id).unwrap();
        assert!(stored(&peer, &chunks_of(&peer), &chunk_id).await.is_none());
        assert_eq!(list_of(&peer, &parent).await, before);
    })
    .unwrap()
}

#[test]
fn a_chunk_cannot_be_rewritten_after_it_merged() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = parent_article(&peer).await;
        let before = list_of(&peer, &parent).await;

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
        let modified: rapidhash::RapidHashSet<String> =
            ["text", "hash"].into_iter().map(String::from).collect();
        let txn = author.db.new_txn(false).await.unwrap();
        let update = db::block::builder::write_document_blocks(
            &txn.blockstore().unwrap(),
            &txn.headstore().unwrap(),
            &rewrite,
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
            &chunk_id_str,
        )
        .await
        .unwrap();
        txn.force_commit().await.unwrap();

        let outcome = merge(&author, &peer, &collection, &update.cid, &chunk_id_str)
            .await
            .unwrap();
        assert_rejected(&outcome, "cannot change after it is created");
        assert_terminal(&peer);

        assert_eq!(stored(&peer, &peer_chunks, &chunk_id).await.unwrap(), kept);
        assert_eq!(list_of(&peer, &parent).await, before);
    })
    .unwrap()
}

#[test]
fn a_chunk_vector_cannot_be_changed_after_it_merged() {
    workers::block_on(async {
        let author = node_with(EMBEDDED_SDL).await;
        let peer = node_with(EMBEDDED_SDL).await;
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
        let modified: rapidhash::RapidHashSet<String> =
            ["body_v".to_string()].into_iter().collect();
        let txn = author.db.new_txn(false).await.unwrap();
        let update = db::block::builder::write_document_blocks(
            &txn.blockstore().unwrap(),
            &txn.headstore().unwrap(),
            &revector,
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
            &chunk_id_str,
        )
        .await
        .unwrap();
        txn.force_commit().await.unwrap();

        let outcome = merge(&author, &peer, &collection, &update.cid, &chunk_id_str)
            .await
            .unwrap();
        assert_rejected(&outcome, "cannot change after it is created");
        assert_eq!(stored(&peer, &peer_chunks, &chunk_id).await.unwrap(), kept);
    })
    .unwrap()
}
