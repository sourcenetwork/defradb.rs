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
use defra_core::TextOp;
use document::{DocID, Document, FieldOp, NormalValue};
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
                FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
                FieldDescription::new("2", "body", FieldKind::string(), CType::Text),
                FieldDescription::new("3", "title", FieldKind::string(), CType::LwwRegister),
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

    fn document(&self) -> Document {
        Document::new(
            self.db
                .get_collection("Notes")
                .unwrap()
                .unwrap()
                .schema_arc(),
        )
    }

    async fn create(&self, body: &str) -> DocID {
        let mut doc = self.document();
        doc.set("body", NormalValue::String(body.into())).unwrap();
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

fn edit(node: &Node, doc_id: &DocID) -> Document {
    let mut doc = node.document();
    doc.set_id(doc_id.clone());
    doc
}

fn ins(pos: u64, text: &str) -> TextOp {
    TextOp::Insert {
        pos,
        text: text.into(),
    }
}

fn del(pos: u64, len: u64) -> TextOp {
    TextOp::Delete { pos, len }
}

fn text(value: &str) -> NormalValue {
    NormalValue::String(value.into())
}

/// Both nodes hold "hello world". Alice sets the whole value, which is diffed
/// into an insert; Bob appends with an explicit insert. Each then receives
/// the other's block: one through a replay, since its parents are no longer
/// the receiver's heads.
#[test]
fn concurrent_edits_of_one_text_keep_both_on_every_node() {
    workers::block_on(async {
        let alice = Node::open().await;
        let bob = Node::open().await;
        let doc_id = alice.create("hello world").await;
        for head in alice.heads(&doc_id).await {
            bob.receive(&alice, &doc_id, &head).await;
        }
        assert_eq!(bob.body().await, text("hello world"));

        let mut whole = edit(&alice, &doc_id);
        whole.set("body", text("hello brave world")).unwrap();
        let from_alice = alice.update(whole, &["body"]).await;

        let mut ops = edit(&bob, &doc_id);
        ops.edit("body", FieldOp::Edit(vec![ins(11, "!")])).unwrap();
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
    })
    .unwrap()
}

/// Two inserts at the same spot keep each run whole, in the same order on
/// both nodes, and a delete concurrent with an insert inside its range
/// keeps the insert.
#[test]
fn concurrent_runs_and_deletes_converge() {
    workers::block_on(async {
        let alice = Node::open().await;
        let bob = Node::open().await;
        let doc_id = alice.create("abcdef").await;
        for head in alice.heads(&doc_id).await {
            bob.receive(&alice, &doc_id, &head).await;
        }

        let mut run = edit(&alice, &doc_id);
        run.edit("body", FieldOp::Edit(vec![ins(3, "XYZ")]))
            .unwrap();
        let from_alice = alice.update(run, &["body"]).await;

        let mut cut = edit(&bob, &doc_id);
        cut.edit("body", FieldOp::Edit(vec![del(1, 4), ins(1, "123")]))
            .unwrap();
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
    })
    .unwrap()
}

/// An update of another field leaves the text alone, and a text update
/// beside it touches only the text.
#[test]
fn a_text_update_beside_another_field_keeps_both() {
    workers::block_on(async {
        let alice = Node::open().await;
        let bob = Node::open().await;
        let doc_id = alice.create("draft").await;
        for head in alice.heads(&doc_id).await {
            bob.receive(&alice, &doc_id, &head).await;
        }

        let mut title = edit(&alice, &doc_id);
        title.set("title", text("Notes")).unwrap();
        let from_alice = alice.update(title, &["title"]).await;

        let mut body = edit(&bob, &doc_id);
        body.edit("body", FieldOp::Edit(vec![del(0, 5), ins(0, "final")]))
            .unwrap();
        let from_bob = bob.update(body, &["body"]).await;

        bob.receive(&alice, &doc_id, &from_alice).await;
        alice.receive(&bob, &doc_id, &from_bob).await;
        for node in [&alice, &bob] {
            assert_eq!(node.body().await, text("final"));
        }
    })
    .unwrap()
}

/// An update to null reads null on every node, as a create with null does,
/// and its block restates the text in the smallest form, CBOR null. An edit
/// concurrent with it keeps its characters; two concurrent nulls stay null;
/// an edit after a null is text again.
#[test]
fn a_text_updated_to_null_reads_null_on_every_node() {
    workers::block_on(async {
        let alice = Node::open().await;
        let bob = Node::open().await;
        let doc_id = alice.create("hello").await;
        for head in alice.heads(&doc_id).await {
            bob.receive(&alice, &doc_id, &head).await;
        }

        let mut cleared = edit(&alice, &doc_id);
        cleared.set("body", NormalValue::Null).unwrap();
        let from_alice = alice.update(cleared, &["body"]).await;
        let field_block = Block::from_dag_cbor(&from_alice.1).unwrap().links.unwrap()[0].link;
        let field_block =
            Block::from_dag_cbor(&alice.blocks.get(&field_block).await.unwrap().unwrap()).unwrap();
        assert_eq!(field_block.delta.field().unwrap().data, [0xf6]);
        bob.receive(&alice, &doc_id, &from_alice).await;
        assert_eq!(alice.body().await, NormalValue::Null);
        assert_eq!(bob.body().await, NormalValue::Null);

        let mut again = edit(&alice, &doc_id);
        again.set("body", text("hi")).unwrap();
        let from_alice = alice.update(again, &["body"]).await;
        bob.receive(&alice, &doc_id, &from_alice).await;
        assert_eq!(bob.body().await, text("hi"));

        let mut cleared = edit(&alice, &doc_id);
        cleared.set("body", NormalValue::Null).unwrap();
        let from_alice = alice.update(cleared, &["body"]).await;
        let mut typed = edit(&bob, &doc_id);
        typed
            .edit("body", FieldOp::Edit(vec![ins(2, "!")]))
            .unwrap();
        let from_bob = bob.update(typed, &["body"]).await;
        alice.receive(&bob, &doc_id, &from_bob).await;
        bob.receive(&alice, &doc_id, &from_alice).await;
        assert_eq!(alice.body().await, text("!"));
        assert_eq!(bob.body().await, text("!"));

        let mut cleared = edit(&alice, &doc_id);
        cleared.set("body", NormalValue::Null).unwrap();
        let from_alice = alice.update(cleared, &["body"]).await;
        // Bob types before he clears, so his clear has other heads than
        // Alice's and the two clears are different blocks.
        let mut typed = edit(&bob, &doc_id);
        typed.set("body", text("?")).unwrap();
        let typed_by_bob = bob.update(typed, &["body"]).await;
        let mut cleared = edit(&bob, &doc_id);
        cleared.set("body", NormalValue::Null).unwrap();
        let from_bob = bob.update(cleared, &["body"]).await;
        assert_ne!(from_alice.0, from_bob.0);
        alice.receive(&bob, &doc_id, &typed_by_bob).await;
        alice.receive(&bob, &doc_id, &from_bob).await;
        bob.receive(&alice, &doc_id, &from_alice).await;
        assert_eq!(alice.body().await, NormalValue::Null);
        assert_eq!(bob.body().await, NormalValue::Null);
    })
    .unwrap()
}
