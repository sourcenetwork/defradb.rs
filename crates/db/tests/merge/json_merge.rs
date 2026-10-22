//! Two signing peers that make the same Json edit at once write blocks with
//! the same ops, parents and priority but different signatures, so different
//! CIDs. Each write's data carries its own nonce, so the two edits are two
//! operations, and a delete that saw only one of them leaves the other on
//! every node, whatever order the blocks arrive in. Two signers' equal
//! creates are two documents, since a DocID covers its first block's
//! signature.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use bytes::Bytes;
use cid::Cid;
use crypto::{Key, PrivateKey};
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use db::{AutoCommitMutator, LensedAutoCommitFetcher};
use defra_core::block::Block;
use defra_core::json_crdt::{JsonEdit, JsonOp};
use defra_core::merge::{BlockMetadata, MergeHandler};
use defra_core::signing::{set_signing_config, SigningConfig, SigningKeyType};
use document::{DocID, Document, FieldOp, NormalValue};
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use rapidhash::RapidHashSet;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

const COLLECTION_ID: &str = "col-json-notes";

struct Node {
    db: Arc<DB<RegolithStore>>,
    blocks: Arc<DefraBlockstore<RegolithStore>>,
    handler: DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
    signing: SigningConfig,
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
                FieldDescription::new("2", "meta", FieldKind::json(), CType::Json),
            ],
        ))
        .await
        .unwrap();
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(db.clone(), blocks.clone());
        let key = crypto::generate_ed25519().unwrap();
        let public_key = key.public_key();
        let signing = SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: key.raw_owned(),
            public_key_bytes: public_key.raw_owned(),
            public_key_hex: hex::encode(public_key.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        Self {
            db,
            blocks,
            handler,
            signing,
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

    async fn create(&self, meta: serde_json::Value) -> DocID {
        let mut doc = self.document();
        doc.set("meta", NormalValue::Json(meta)).unwrap();
        set_signing_config(Some(self.signing.clone()));
        let created = AutoCommitMutator::new(self.db.clone())
            .create("Notes", doc)
            .await;
        set_signing_config(None);
        created.unwrap().doc_id
    }

    /// Write `doc`'s `meta`, signed, returning the new composite block.
    async fn update(&self, doc: Document) -> (Cid, Bytes) {
        set_signing_config(Some(self.signing.clone()));
        let result = AutoCommitMutator::new(self.db.clone())
            .update(
                "Notes",
                doc,
                ["meta".to_owned()].into_iter().collect::<RapidHashSet<_>>(),
            )
            .await;
        set_signing_config(None);
        let result = result.unwrap();
        (result.commit_cid.unwrap(), result.commit_block.unwrap())
    }

    async fn heads(&self, doc_id: &DocID) -> Vec<(Cid, Bytes)> {
        db::merge::replication::load_document_head_blocks(&self.db, &doc_id.to_string())
            .await
            .unwrap()
    }

    /// Merge the composite `cid` from `from`, with the field blocks it links.
    async fn receive(&self, from: &Node, doc_id: &DocID, (cid, bytes): &(Cid, Bytes)) {
        let composite = Block::from_dag_cbor(bytes).unwrap();
        let mut copy = composite.signature.into_iter().collect::<Vec<_>>();
        for link in composite.links.iter().flatten() {
            let data = from.blocks.get(&link.link).await.unwrap().unwrap();
            copy.extend(Block::from_dag_cbor(&data).unwrap().signature);
            self.blocks.put(&link.link, &data).await.unwrap();
        }
        for signature in copy {
            let data = from.blocks.get(&signature).await.unwrap().unwrap();
            self.blocks.put(&signature, &data).await.unwrap();
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
                    "did:key:z6MkrJsonMerge",
                    Some("peer"),
                    false,
                ),
            )
            .await
            .unwrap();
        assert!(outcome.is_merged(), "got {outcome:?}");
    }

    async fn meta(&self) -> NormalValue {
        let docs = LensedAutoCommitFetcher::new(self.db.clone())
            .get_all("Notes")
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        docs[0].get("meta").cloned().unwrap_or(NormalValue::Null)
    }

    /// Every document's `meta`, ordered by its JSON text.
    async fn metas(&self) -> Vec<serde_json::Value> {
        let mut metas: Vec<serde_json::Value> = LensedAutoCommitFetcher::new(self.db.clone())
            .get_all("Notes")
            .await
            .unwrap()
            .iter()
            .map(|doc| match doc.get("meta") {
                Some(NormalValue::Json(value)) => value.clone(),
                other => panic!("meta is {other:?}"),
            })
            .collect();
        metas.sort_by_key(|value| value.to_string());
        metas
    }

    fn edit(&self, doc_id: &DocID) -> Document {
        let mut doc = self.document();
        doc.set_id(doc_id.clone());
        doc
    }
}

/// Alice and Bob both set `a.b = 1` on the same heads; Alice then deletes `a`
/// on top of her own edit only. Carol receives Alice's edit, Bob's, then the
/// delete; Erin receives Alice's edit, the delete, then Bob's. Bob's edit was
/// never seen by the delete, so both keep it.
#[test]
fn equal_concurrent_json_edits_from_two_signers_converge_in_any_order() {
    workers::block_on(async {
        let alice = Node::open().await;
        let bob = Node::open().await;
        let carol = Node::open().await;
        let erin = Node::open().await;
        let doc_id = alice.create(serde_json::json!({"x": 1})).await;
        let created = alice.heads(&doc_id).await;
        for node in [&bob, &carol, &erin] {
            for head in &created {
                node.receive(&alice, &doc_id, head).await;
            }
        }

        let set = |node: &Node| {
            let mut doc = node.edit(&doc_id);
            doc.set(
                "meta",
                NormalValue::Json(serde_json::json!({"x": 1, "a": {"b": 1}})),
            )
            .unwrap();
            doc
        };
        let from_alice = alice.update(set(&alice)).await;
        let from_bob = bob.update(set(&bob)).await;
        assert_ne!(from_alice.0, from_bob.0);

        let mut delete = alice.edit(&doc_id);
        delete
            .edit(
                "meta",
                FieldOp::Paths(vec![JsonOp {
                    path: vec!["a".into()],
                    edit: JsonEdit::Delete,
                }]),
            )
            .unwrap();
        let deleted = alice.update(delete).await;

        for block in [&from_alice, &from_bob, &deleted] {
            carol
                .receive(alice_or_bob(block, &from_bob, &alice, &bob), &doc_id, block)
                .await;
        }
        for block in [&from_alice, &deleted, &from_bob] {
            erin.receive(alice_or_bob(block, &from_bob, &alice, &bob), &doc_id, block)
                .await;
        }
        let expected = NormalValue::Json(serde_json::json!({"x": 1, "a": {"b": 1}}));
        assert_eq!(carol.meta().await, expected);
        assert_eq!(erin.meta().await, expected);
    })
    .unwrap()
}

/// Alice and Bob both create a document with `{"a": {"b": 1}}`. A DocID is
/// the CID of the document's first composite block, which covers its
/// signature and the field blocks it links, so the two creates are two
/// documents. Two field blocks with no parents and one op ID therefore never
/// meet in one frontier. Alice then deletes `a` in hers; Carol and Erin
/// receive the three blocks in different orders and agree on both documents.
#[test]
fn equal_creates_from_two_signers_are_two_documents() {
    workers::block_on(async {
        let alice = Node::open().await;
        let bob = Node::open().await;
        let carol = Node::open().await;
        let erin = Node::open().await;
        let meta = serde_json::json!({"a": {"b": 1}});
        let alice_doc = alice.create(meta.clone()).await;
        let bob_doc = bob.create(meta.clone()).await;
        assert_ne!(alice_doc, bob_doc);
        let from_alice = alice.heads(&alice_doc).await.remove(0);
        let from_bob = bob.heads(&bob_doc).await.remove(0);

        let mut delete = alice.edit(&alice_doc);
        delete
            .edit(
                "meta",
                FieldOp::Paths(vec![JsonOp {
                    path: vec!["a".into()],
                    edit: JsonEdit::Delete,
                }]),
            )
            .unwrap();
        let deleted = alice.update(delete).await;

        carol.receive(&alice, &alice_doc, &from_alice).await;
        carol.receive(&bob, &bob_doc, &from_bob).await;
        carol.receive(&alice, &alice_doc, &deleted).await;
        erin.receive(&alice, &alice_doc, &from_alice).await;
        erin.receive(&alice, &alice_doc, &deleted).await;
        erin.receive(&bob, &bob_doc, &from_bob).await;
        let expected = [meta, serde_json::json!({})];
        assert_eq!(carol.metas().await, expected);
        assert_eq!(erin.metas().await, expected);
    })
    .unwrap()
}

fn alice_or_bob<'node>(
    block: &(Cid, Bytes),
    from_bob: &(Cid, Bytes),
    alice: &'node Node,
    bob: &'node Node,
) -> &'node Node {
    if block.0 == from_bob.0 {
        bob
    } else {
        alice
    }
}
