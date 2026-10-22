//! A Text field's stored value is the replay of its blocks across schema
//! versions: a peer's block merges into a document stored at an older
//! version, and a lens never stores a text no block made, since a peer's
//! edit counts its positions in the text its blocks make.

use super::*;
use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler};
use defra_core::TextOp;
use document::{FieldOp, NormalValue};
use serde_json::json;

const SDL: &str = "type Users { name: String verified: String @crdt(type: text) }";

struct Node {
    db: Arc<DB<RegolithStore>>,
    blocks: Arc<DefraBlockstore<RegolithStore>>,
    handler: DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
}

impl Node {
    async fn open(lens: bool) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut raw_db = DB::from_arc(store.clone()).unwrap();
        if lens {
            raw_db.set_lens_store(Arc::new(SetVerifiedStore {
                verified_value: Some(json!("LENSED")),
                ..Default::default()
            }));
        }
        raw_db
            .create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();
        let db = Arc::new(raw_db);
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(db.clone(), blocks.clone());
        Self {
            db,
            blocks,
            handler,
        }
    }

    fn document(&self) -> document::Document {
        document::Document::new(
            self.db
                .get_collection("Users")
                .unwrap()
                .unwrap()
                .schema_arc(),
        )
    }

    async fn heads(&self, doc_id: &document::DocID) -> Vec<(Cid, Bytes)> {
        db::merge::replication::load_document_head_blocks(&self.db, &doc_id.to_string())
            .await
            .unwrap()
    }

    async fn receive(&self, from: &Node, doc_id: &document::DocID, (cid, bytes): &(Cid, Bytes)) {
        for link in Block::from_dag_cbor(bytes).unwrap().links.iter().flatten() {
            let data = from.blocks.get(&link.link).await.unwrap().unwrap();
            self.blocks.put(&link.link, &data).await.unwrap();
        }
        self.blocks.put(cid, bytes).await.unwrap();
        let collection_id = self
            .db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .collection_id()
            .to_owned();
        let outcome = self
            .handler
            .handle_block(
                cid,
                bytes,
                BlockMetadata::normal(
                    &doc_id.to_string(),
                    &collection_id,
                    "did:key:z6MkrTextLens",
                    Some("peer"),
                    false,
                ),
            )
            .await
            .unwrap();
        assert!(outcome.is_merged(), "got {outcome:?}");
    }

    /// Append " world" to the field's "hello", returning the write's block.
    async fn append(&self, doc_id: &document::DocID) -> (Cid, Bytes) {
        let mut append = self.document();
        append.set_id(doc_id.clone());
        append
            .edit(
                "verified",
                FieldOp::Edit(vec![TextOp::Insert {
                    pos: 5,
                    text: " world".into(),
                }]),
            )
            .unwrap();
        let result = AutoCommitMutator::new(self.db.clone())
            .update(
                "Users",
                append,
                RapidHashSet::from_iter(["verified".to_string()]),
            )
            .await
            .unwrap();
        (result.commit_cid.unwrap(), result.commit_block.unwrap())
    }

    /// The field's stored text, as the document value holds it.
    async fn stored_text(&self, doc_id: &document::DocID) -> Option<NormalValue> {
        let collection = self.db.get_collection("Users").unwrap().unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        let short_id = collection
            .resolve_doc_short_id(&systemstore, doc_id)
            .await
            .unwrap()
            .unwrap();
        let doc = collection
            .get_with_datastore(&datastore, short_id, doc_id)
            .await
            .unwrap()
            .unwrap();
        drop(datastore);
        drop(systemstore);
        txn.discard().unwrap();
        doc.get("verified").cloned()
    }
}

/// Alice and Bob hold "hello". Alice adds a schema version, which leaves the
/// document stored at the old one; Bob appends " world". Alice merges Bob's
/// block into the document as it is stored.
#[test]
fn a_text_block_merges_into_a_document_stored_at_an_older_version() {
    workers::block_on(async {
        let alice = Node::open(false).await;
        let bob = Node::open(false).await;
        let mut doc = alice.document();
        doc.set("verified", NormalValue::String("hello".into()))
            .unwrap();
        let doc_id = AutoCommitMutator::new(alice.db.clone())
            .create("Users", doc)
            .await
            .unwrap()
            .doc_id;
        for head in alice.heads(&doc_id).await {
            bob.receive(&alice, &doc_id, &head).await;
        }
        add_placeholder_version(&alice.db, "note").await;

        let block = bob.append(&doc_id).await;
        alice.receive(&bob, &doc_id, &block).await;

        assert_eq!(
            alice.stored_text(&doc_id).await,
            Some(NormalValue::String("hello world".into()))
        );
    })
    .unwrap()
}

/// Alice and Bob hold "hello". Alice adds a version whose lens rewrites the
/// text and materializes her documents; Bob appends " world" on top of the
/// one block both hold. On Alice, Bob's block lands on her heads, so it takes
/// the fast path over her stored text, which has to be the text its blocks
/// make: the merge must neither fail nor apply Bob's edit to the lensed text.
/// Alice's next write moves the document to the new version and carries the
/// lens change as edits in its block.
#[test]
fn a_text_edit_on_the_fast_path_lands_on_the_text_its_blocks_make() {
    workers::block_on(async {
        let alice = Node::open(true).await;
        let bob = Node::open(false).await;
        let v1 = alice
            .db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_owned();
        let mut doc = alice.document();
        doc.set("verified", NormalValue::String("hello".into()))
            .unwrap();
        let doc_id = AutoCommitMutator::new(alice.db.clone())
            .create("Users", doc)
            .await
            .unwrap()
            .doc_id;
        for head in alice.heads(&doc_id).await {
            bob.receive(&alice, &doc_id, &head).await;
        }

        let v2 = add_placeholder_version(&alice.db, "note").await;
        alice
            .db
            .set_migration(
                LensConfig::new(
                    &v1,
                    &v2,
                    LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
                ),
                None,
            )
            .await
            .unwrap();
        alice.db.materialize_collection("Users").await.unwrap();

        let block = bob.append(&doc_id).await;
        alice.receive(&bob, &doc_id, &block).await;

        assert_eq!(
            bob.stored_text(&doc_id).await,
            Some(NormalValue::String("hello world".into()))
        );
        assert_eq!(
            alice.stored_text(&doc_id).await,
            Some(NormalValue::String("hello world".into())),
            "the stored text is the text the field's blocks make"
        );

        let mut rename = alice.document();
        rename.set_id(doc_id.clone());
        rename
            .set("name", NormalValue::String("Alice".into()))
            .unwrap();
        let moved = AutoCommitMutator::new(alice.db.clone())
            .update(
                "Users",
                rename,
                RapidHashSet::from_iter(["name".to_string()]),
            )
            .await
            .unwrap();
        let links = Block::from_dag_cbor(&moved.commit_block.unwrap())
            .unwrap()
            .links
            .unwrap_or_default();
        assert!(
            links.iter().any(|link| link.name == "verified"),
            "the write that moves the document carries the lens change as a block"
        );
        assert_eq!(
            alice.stored_text(&doc_id).await,
            Some(NormalValue::String("LENSED".into()))
        );
    })
    .unwrap()
}
