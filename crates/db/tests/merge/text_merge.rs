//! Text fields merge positional edits: two nodes that edit one text at once
//! and swap their blocks, in either order, end at the same text with both
//! edits kept.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use bytes::Bytes;
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use db::{AutoCommitMutator, LensedAutoCommitFetcher};
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler};
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use rapidhash::RapidHashSet;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

const COLLECTION_ID: &str = "col-notes";

struct Node {
    db: Arc<DB<RegolithStore>>,
    blocks: Arc<DefraBlockstore<RegolithStore>>,
    handler: DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
}

impl Node {
    async fn open() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(CollectionVersion::new(
            "Notes",
            "v1",
            COLLECTION_ID,
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "body", FieldKind::string()).with_crdt_type(CType::Text),
                FieldDescription::new("3", "title", FieldKind::string()),
            ],
        ))
        .await
        .unwrap();
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(db.clone(), blocks.clone());
        Self {
            db,
            blocks,
            handler,
        }
    }

    async fn create(&self, body: &str) -> DocID {
        let mut doc = Document::new();
        doc.set("body", NormalValue::String(body.into()));
        AutoCommitMutator::new(self.db.clone())
            .create("Notes", doc)
            .await
            .unwrap()
            .doc_id
    }

    /// Write `doc`'s changed fields, returning the new composite block.
    async fn update(&self, doc: Document, fields: &[&str]) -> (Cid, Bytes) {
        let result = AutoCommitMutator::new(self.db.clone())
            .update(
                "Notes",
                doc,
                fields
                    .iter()
                    .map(|f| (*f).to_owned())
                    .collect::<RapidHashSet<_>>(),
            )
            .await
            .unwrap();
        (result.commit_cid.unwrap(), result.commit_block.unwrap())
    }

    async fn heads(&self, doc_id: &DocID) -> Vec<(Cid, Bytes)> {
        db::merge::replication::load_document_head_blocks(&self.db, &doc_id.to_string())
            .await
            .unwrap()
    }

    /// Merge the composite `cid` from `from`, with the field blocks it links,
    /// as a peer's delivery.
    async fn receive(&self, from: &Node, doc_id: &DocID, (cid, bytes): &(Cid, Bytes)) {
        for link in Block::from_dag_cbor(bytes).unwrap().links.iter().flatten() {
            let data = from.blocks.get(&link.link).await.unwrap().unwrap();
            self.blocks.put(&link.link, &data).await.unwrap();
        }
        self.blocks.put(cid, bytes).await.unwrap();
        let outcome = self
            .handler
            .handle_block(
                cid,
                bytes,
                BlockMetadata::normal(
                    &doc_id.to_string(),
                    COLLECTION_ID,
                    "did:key:z6MkrTextMerge",
                    Some("peer"),
                    false,
                ),
            )
            .await
            .unwrap();
        assert!(outcome.is_merged(), "got {outcome:?}");
    }

    async fn body(&self) -> NormalValue {
        let docs = LensedAutoCommitFetcher::new(self.db.clone())
            .get_all("Notes")
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        docs[0].get("body").cloned().unwrap_or(NormalValue::Null)
    }
}

fn edit(doc_id: &DocID) -> Document {
    let mut doc = Document::new();
    doc.set_id(doc_id.clone());
    doc
}

fn text(value: &str) -> NormalValue {
    NormalValue::String(value.into())
}

/// Both nodes hold "hello world". Alice sets the whole value, which is diffed
/// into an insert; Bob appends with an explicit insert. Each then receives
/// the other's block: one through a replay, since its parents are no longer
/// the receiver's heads.
#[tokio::test]
async fn concurrent_edits_of_one_text_keep_both_on_every_node() {
    let alice = Node::open().await;
    let bob = Node::open().await;
    let doc_id = alice.create("hello world").await;
    for head in alice.heads(&doc_id).await {
        bob.receive(&alice, &doc_id, &head).await;
    }
    assert_eq!(bob.body().await, text("hello world"));

    let mut whole = edit(&doc_id);
    whole.set("body", text("hello brave world"));
    let from_alice = alice.update(whole, &["body"]).await;

    let mut ops = edit(&doc_id);
    ops.insert_text("body", 11, "!");
    let from_bob = bob.update(ops, &["body"]).await;
    assert_eq!(bob.body().await, text("hello world!"));

    alice.receive(&bob, &doc_id, &from_bob).await;
    bob.receive(&alice, &doc_id, &from_alice).await;
    assert_eq!(alice.body().await, text("hello brave world!"));
    assert_eq!(bob.body().await, text("hello brave world!"));

    // A block that arrives again is already merged and changes nothing.
    let again = alice
        .handler
        .handle_block(
            &from_bob.0,
            &from_bob.1,
            BlockMetadata::normal(
                &doc_id.to_string(),
                COLLECTION_ID,
                "did:key:z6MkrTextMerge",
                Some("peer"),
                false,
            ),
        )
        .await
        .unwrap();
    assert!(!again.is_merged(), "got {again:?}");
    assert_eq!(alice.body().await, text("hello brave world!"));
}

/// Two inserts at the same spot keep each run whole, in the same order on
/// both nodes, and a delete concurrent with an insert inside its range
/// keeps the insert.
#[tokio::test]
async fn concurrent_runs_and_deletes_converge() {
    let alice = Node::open().await;
    let bob = Node::open().await;
    let doc_id = alice.create("abcdef").await;
    for head in alice.heads(&doc_id).await {
        bob.receive(&alice, &doc_id, &head).await;
    }

    let mut run = edit(&doc_id);
    run.insert_text("body", 3, "XYZ");
    let from_alice = alice.update(run, &["body"]).await;

    let mut cut = edit(&doc_id);
    cut.delete_text("body", 1, 4);
    cut.insert_text("body", 1, "123");
    let from_bob = bob.update(cut, &["body"]).await;

    bob.receive(&alice, &doc_id, &from_alice).await;
    alice.receive(&bob, &doc_id, &from_bob).await;
    let merged = alice.body().await;
    assert_eq!(bob.body().await, merged);
    let merged = merged.as_str().unwrap().to_owned();
    assert!(merged.contains("XYZ"), "{merged}");
    assert!(merged.contains("123"), "{merged}");
    assert!(merged.starts_with('a') && merged.ends_with('f'), "{merged}");
    assert_eq!(merged.len(), "a".len() + 3 + 3 + "f".len(), "{merged}");
}

/// An update of another field leaves the text alone, and a text update
/// beside it touches only the text.
#[tokio::test]
async fn a_text_update_beside_another_field_keeps_both() {
    let alice = Node::open().await;
    let bob = Node::open().await;
    let doc_id = alice.create("draft").await;
    for head in alice.heads(&doc_id).await {
        bob.receive(&alice, &doc_id, &head).await;
    }

    let mut title = edit(&doc_id);
    title.set("title", text("Notes"));
    let from_alice = alice.update(title, &["title"]).await;

    let mut body = edit(&doc_id);
    body.replace_text("body", 0, 5, "final");
    let from_bob = bob.update(body, &["body"]).await;

    bob.receive(&alice, &doc_id, &from_alice).await;
    alice.receive(&bob, &doc_id, &from_bob).await;
    for node in [&alice, &bob] {
        assert_eq!(node.body().await, text("final"));
    }
}
