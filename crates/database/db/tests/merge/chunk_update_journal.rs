//! A chunk's rows in the update journal show only to a reader of its parent.

use std::sync::Arc;

use acp::DocumentACP;
use blockstore::{Blockstore as _, DefraBlockstore};
use db::block::builder::BlockResult;
use db::collection::Collection;
use db::database::DB;
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use defra_core::signing::SigningConfig;
use document::{DocID, Document, NormalValue};
use identity::Did;
use query::access_hooks::ReadValidator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use serde_json::Value;
use storage::RegolithStore;

const SDL: &str = "type Article { body: String @chunk(maxChars: 64) }";
/// [`SDL`] with the parent under a policy; its chunk collection takes none.
const POLICY_SDL: &str = r#"type Article @policy(id: "chunk-policy", resource: "articles") {
    body: String @chunk(maxChars: 64)
}"#;
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
    pub(super) db: Arc<DB<RegolithStore>>,
    pub(super) blockstore: Arc<DefraBlockstore<RegolithStore>>,
    pub(super) handler: Handler,
    pub(super) acp: Arc<dyn DocumentACP>,
}

pub(super) async fn node() -> Node {
    node_with_policy(true).await
}

pub(super) async fn node_with_policy(policy: bool) -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let sdl = match policy {
        true => POLICY_SDL,
        false => SDL,
    };
    db.add_collection(&defra_core::RequestContext::ambient(), sdl)
        .await
        .unwrap();
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
    pub(super) fn chunks(&self) -> Collection {
        self.db.get_collection(CHUNKS).unwrap().unwrap()
    }

    pub(super) async fn run(&self, query: &str, identity: Option<Did>) -> Value {
        self.run_with(query, identity, None).await
    }

    pub(super) async fn run_with(
        &self,
        query: &str,
        identity: Option<Did>,
        validator: Option<Arc<dyn ReadValidator>>,
    ) -> Value {
        let mut runner = QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(self.db.clone()),
            DbCollectionProvider::new_arc(self.db.clone()),
            Arc::new(DbTransactionRegistry::new(self.db.clone())),
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(self.db.clone())))
        .with_acp(self.acp.clone());
        if let Some(validator) = validator {
            runner = runner.with_read_validator(validator);
        }
        let mut request = QueryRequest::new(query);
        request.identity = identity;
        let response = runner.execute(request).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        response.data.unwrap()
    }

    /// The error messages of a request that must fail.
    pub(super) async fn errors(&self, query: &str) -> Vec<String> {
        let runner = QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(self.db.clone()),
            DbCollectionProvider::new_arc(self.db.clone()),
            Arc::new(DbTransactionRegistry::new(self.db.clone())),
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(self.db.clone())))
        .with_acp(self.acp.clone());
        let mut request = QueryRequest::new(query);
        request.identity = Some(owner());
        let response = runner.execute(request).await;
        response
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect()
    }

    pub(super) async fn journal(&self, identity: Option<Did>) -> Vec<String> {
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

pub(super) async fn merge_outcome(
    from: &Node,
    to: &Node,
    cid: &cid::Cid,
    doc_id: &str,
) -> Result<MergeOutcome, MergeError> {
    merge_outcome_in(from, to, cid, doc_id, from.chunks().collection_id()).await
}

/// [`merge_outcome`] for a block of the collection `collection_id`.
pub(super) async fn merge_outcome_in(
    from: &Node,
    to: &Node,
    cid: &cid::Cid,
    doc_id: &str,
    collection_id: &str,
) -> Result<MergeOutcome, MergeError> {
    let bytes = from.blockstore.get(cid).await.unwrap().unwrap();
    let block = Block::from_dag_cbor(&bytes).unwrap();
    let mut copies: Vec<cid::Cid> = block.signature.into_iter().collect();
    for link in block.links.iter().flatten() {
        let data = from.blockstore.get(&link.link).await.unwrap().unwrap();
        copies.extend(Block::from_dag_cbor(&data).unwrap().signature);
        to.blockstore.put(&link.link, &data).await.unwrap();
    }
    for signature in copies {
        let data = from.blockstore.get(&signature).await.unwrap().unwrap();
        to.blockstore.put(&signature, &data).await.unwrap();
    }
    to.blockstore.put(cid, &bytes).await.unwrap();
    to.handler
        .handle_block(
            cid,
            &bytes,
            BlockMetadata::normal(
                doc_id,
                collection_id,
                "did:key:z6MkChunkAuthor",
                Some("peer"),
                false,
            ),
        )
        .await
}

/// Merges `blocks` (CID, DocID, collection ID) of `from` into `to` as one batch.
pub(super) async fn merge_batch_in(
    from: &Node,
    to: &Node,
    blocks: &[(cid::Cid, String, String)],
) -> Vec<Result<MergeOutcome, MergeError>> {
    let mut batch = Vec::new();
    for (cid, doc_id, collection_id) in blocks {
        let bytes = from.blockstore.get(cid).await.unwrap().unwrap();
        let block = Block::from_dag_cbor(&bytes).unwrap();
        for link in block.links.iter().flatten() {
            let data = from.blockstore.get(&link.link).await.unwrap().unwrap();
            to.blockstore.put(&link.link, &data).await.unwrap();
        }
        to.blockstore.put(cid, &bytes).await.unwrap();
        batch.push(defra_core::merge::MergeBlock {
            cid: *cid,
            block_data: bytes.to_vec().into(),
            doc_id: doc_id.clone(),
            collection_id: collection_id.clone(),
            creator: "did:key:z6MkChunkAuthor".to_string(),
            sender_peer: Some("peer".to_string()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        });
    }
    to.handler.handle_block_batch(&batch).await
}

/// Merges a block into `to`. The peer's own write may already hold the same chunk.
async fn merge(from: &Node, to: &Node, cid: &cid::Cid, doc_id: &str) {
    let outcome = merge_outcome(from, to, cid, doc_id).await.unwrap();
    let applied = match &outcome {
        MergeOutcome::Skipped { reason, .. } => MergeOutcome::skip_reason_is_applied(reason),
        other => other.is_merged(),
    };
    assert!(applied, "{outcome:?}");
}

/// A chunk of `body`'s first piece, created on `author` for `parent`.
pub(super) async fn create_chunk(
    author: &Node,
    parent: &str,
    body: &str,
    signing: Option<&SigningConfig>,
) -> (Document, u64, BlockResult) {
    create_chunk_named(author, parent, body, signing, None).await
}

/// The `sourceCID` of `peer`'s own chunk of `body`'s first piece, if it holds one.
async fn source_on(peer: &Node, parent: &str, body: &str) -> Option<String> {
    let articles = peer.db.get_collection("Article").unwrap()?;
    let config =
        db::chunk::chunk_config("Article", articles.schema().chunked_field("body").unwrap())
            .unwrap();
    let hash = db::chunk::split::split(body, &config).remove(0).hash;
    let query = format!(
        r#"{{ {CHUNKS}(filter: {{_sourceID: {{_eq: "{parent}"}}, hash: {{_eq: "{hash}"}}}}) {{ sourceCID }} }}"#
    );
    for reader in [owner(), stranger()] {
        let data = peer.run(&query, Some(reader)).await;
        if let Some(source) = data[CHUNKS][0]["sourceCID"].as_str() {
            return Some(source.to_string());
        }
    }
    None
}

/// [`create_chunk`] naming the commit `source`.
async fn create_chunk_named(
    author: &Node,
    parent: &str,
    body: &str,
    signing: Option<&SigningConfig>,
    source: Option<String>,
) -> (Document, u64, BlockResult) {
    let chunks = author.chunks();
    let articles = author.db.get_collection("Article").unwrap().unwrap();
    let config =
        db::chunk::chunk_config("Article", articles.schema().chunked_field("body").unwrap())
            .unwrap();
    let piece = db::chunk::split::split(body, &config).remove(0);
    let mut chunk = Document::new(chunks.schema_arc());
    chunk
        .set("_sourceID", NormalValue::String(parent.into()))
        .unwrap();
    chunk.set("text", NormalValue::String(piece.text)).unwrap();
    chunk.set("hash", NormalValue::String(piece.hash)).unwrap();
    if let Some(source) = source {
        chunk.set("sourceCID", NormalValue::String(source)).unwrap();
    }

    let txn = author.db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let short_id = author.db.next_doc_short_id().await.unwrap();
    let created = db::block::builder::write_document_blocks(
        &txn.blockstore().unwrap(),
        &txn.headstore().unwrap(),
        &chunk,
        chunks.schema(),
        db::block::builder::DocRef::new(chunks.resolved_root_id(), short_id),
        None,
        None,
        signing,
        None,
    )
    .await
    .unwrap();
    db::docid::map::set_doc_id_mapping(
        &systemstore,
        chunks.resolved_root_id(),
        short_id,
        &created.doc_id,
    )
    .await
    .unwrap();
    for cid in std::iter::once(&created.cid).chain(&created.field_cids) {
        db::docid::map::set_block_doc_id_mapping(&systemstore, &cid.to_string(), &created.doc_id)
            .await
            .unwrap();
    }
    chunk.set_id(DocID::from_string(&created.doc_id).unwrap());
    chunks
        .save_with_datastore(&txn.datastore().unwrap(), &chunk, short_id)
        .await
        .unwrap();
    drop(systemstore);
    txn.force_commit().await.unwrap();
    (chunk, short_id, created)
}

/// A chunk of `body`, created on `author` and merged into `peer`.
pub(super) async fn merge_chunk(
    author: &Node,
    peer: &Node,
    parent: &str,
    body: &str,
    signing: Option<&SigningConfig>,
) -> (String, cid::Cid) {
    // The peer's own write may hold this chunk, so the author's names the same commit.
    let source = source_on(peer, parent, body).await;
    let (_, _, created) = create_chunk_named(author, parent, body, signing, source).await;
    merge(author, peer, &created.cid, &created.doc_id).await;
    (created.doc_id, created.cid)
}

/// A chunk of `parent`'s body, created on `author` and then written again
/// unchanged, both merged into `peer`. The second write is one journal row.
pub(super) async fn merge_updated_chunk(author: &Node, peer: &Node, parent: &str) -> String {
    let chunks = author.chunks();
    let source = source_on(peer, parent, BODY).await;
    let (chunk, short_id, created) = create_chunk_named(author, parent, BODY, None, source).await;
    let chunk_id = created.doc_id.clone();
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
#[mantra_macros::req_test("defra.events.delivery.chunk")]
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
