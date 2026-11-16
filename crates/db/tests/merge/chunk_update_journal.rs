//! A chunk's rows in the update journal show only to a reader of its parent.

use std::sync::Arc;

use acp::DocumentACP;
use blockstore::{Blockstore as _, DefraBlockstore};
use db::collection::Collection;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler};
use document::{Document, NormalValue};
use identity::Did;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use schema::PolicyDescription;
use serde_json::Value;
use storage::RegolithStore;

const SDL: &str = "type Article { body: String @chunk(maxChars: 64) }";
pub(super) const CHUNKS: &str = "Article__body_chunk";
pub(super) const BODY: &str = "A short story.";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

pub(super) fn owner() -> Did {
    Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap()
}

pub(super) fn stranger() -> Did {
    Did::new("did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi").unwrap()
}

pub(super) struct Node {
    db: Arc<DB<RegolithStore>>,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    handler: Handler,
    acp: Arc<dyn DocumentACP>,
}

pub(super) async fn node() -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let mut collections = query::parse_sdl(SDL).unwrap();
    for collection in &mut collections {
        if collection.chunk_of.is_none() {
            collection.policy = Some(PolicyDescription::new("chunk-policy", "articles"));
        }
    }
    db.create_collections_atomic(collections).await.unwrap();
    let acp: Arc<dyn DocumentACP> = Arc::new(acp::ZanzibarDocumentACP::new(Arc::new(
        acp::PersistentZanzibarStore::from_store(store.clone()),
    )));
    db.set_document_acp(acp.clone());
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    Node {
        db,
        blockstore,
        handler,
        acp,
    }
}

impl Node {
    fn chunks(&self) -> Collection {
        self.db.get_collection(CHUNKS).unwrap().unwrap()
    }

    pub(super) async fn run(&self, query: &str, identity: Option<Did>) -> Value {
        let runner = QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(self.db.clone()),
            DbCollectionProvider::new_arc(self.db.clone()),
            Arc::new(DbTransactionRegistry::new(self.db.clone())),
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(self.db.clone())))
        .with_acp(self.acp.clone());
        let mut request = QueryRequest::new(query);
        request.identity = identity;
        let response = runner.execute(request).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        response.data.unwrap()
    }

    async fn journal(&self, identity: Option<Did>) -> Vec<String> {
        let data = self
            .run(
                &format!(
                    r#"{{ _documentUpdates(collection: "{CHUNKS}") {{ entries {{ docID }} }} }}"#
                ),
                identity,
            )
            .await;
        data["_documentUpdates"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["docID"].as_str().unwrap().to_string())
            .collect()
    }
}

async fn merge(from: &Node, to: &Node, cid: &cid::Cid, doc_id: &str) {
    let bytes = from.blockstore.get(cid).await.unwrap().unwrap();
    for link in Block::from_dag_cbor(&bytes).unwrap().links.iter().flatten() {
        let data = from.blockstore.get(&link.link).await.unwrap().unwrap();
        to.blockstore.put(&link.link, &data).await.unwrap();
    }
    to.blockstore.put(cid, &bytes).await.unwrap();
    let outcome = to
        .handler
        .handle_block(
            cid,
            &bytes,
            BlockMetadata::normal(
                doc_id,
                from.chunks().collection_id(),
                "did:key:z6MkChunkAuthor",
                Some("peer"),
                false,
            ),
        )
        .await
        .unwrap();
    assert!(outcome.is_merged(), "{outcome:?}");
}

/// A chunk of `parent`'s body, created on `author` and then written again
/// unchanged, both merged into `peer`. The second write is one journal row.
pub(super) async fn merge_updated_chunk(author: &Node, peer: &Node, parent: &str) -> String {
    let chunks = author.chunks();
    let articles = author.db.get_collection("Article").unwrap().unwrap();
    let config =
        db::chunk::chunk_config("Article", articles.schema().chunked_field("body").unwrap())
            .unwrap();
    let piece = db::chunk::split::split(BODY, &config).remove(0);
    let mut chunk = Document::new(chunks.schema_arc());
    chunk
        .set("_sourceID", NormalValue::String(parent.into()))
        .unwrap();
    chunk.set("text", NormalValue::String(piece.text)).unwrap();
    chunk.set("hash", NormalValue::String(piece.hash)).unwrap();

    let (chunk_id, short_id, created) =
        super::merge_handler_tests::create_doc_locally(&author.handler, &chunks, &mut chunk).await;
    let chunk_id = chunk_id.to_string();
    merge(author, peer, &created.cid, &chunk_id).await;

    let modified: rapidhash::RapidHashSet<String> = ["text".to_string()].into_iter().collect();
    let txn = author.db.new_txn(false).await.unwrap();
    let rewritten = db::block::builder::write_document_blocks(
        &txn.blockstore().unwrap(),
        &txn.headstore().unwrap(),
        &chunk,
        chunks.schema(),
        db::block::builder::DocRef::new(chunks.resolved_root_id(), short_id),
        Some(&modified),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    db::docid::map::set_block_doc_id_mapping(
        &txn.systemstore().unwrap(),
        &rewritten.cid.to_string(),
        &chunk_id,
    )
    .await
    .unwrap();
    txn.force_commit().await.unwrap();
    merge(author, peer, &rewritten.cid, &chunk_id).await;
    chunk_id
}

#[test]
fn a_chunks_updates_show_only_to_a_reader_of_its_parent() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let created = peer
            .run(
                &format!(r#"mutation {{ add_Article(input: {{body: "{BODY}"}}) {{ _docID }} }}"#),
                Some(owner()),
            )
            .await;
        let parent = created["add_Article"][0]["_docID"].as_str().unwrap();

        let chunk_id = merge_updated_chunk(&author, &peer, parent).await;

        assert_eq!(peer.journal(Some(owner())).await, [chunk_id]);
        for reader in [None, Some(stranger())] {
            assert!(peer.journal(reader).await.is_empty());
        }
    })
    .unwrap()
}
