//! A merged block's signature blocks: the document owns them and they read
//! as merged, including the signature of a field block the receiver's
//! schema skips (`defra.orphan_gc.signature_owner`,
//! `defra.orphan_gc.signature_merged`).

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use crypto::PrivateKey as _;
use db::database::{DbOptions, DB};
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{Block, DAGLink, Signature, SignatureHeader, SignatureType};
use defra_core::merge::{MergeBlock, MergeOutcome};
use document::{Document, NormalValue};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

use super::replicated::{logs, merge, stored};

/// `block` signed with `key`: its signature block stored in `blockstore`,
/// its own bytes and CID.
async fn signed(
    mut block: Block,
    key: &crypto::Ed25519PrivateKey,
    blockstore: &DefraBlockstore<RegolithStore>,
) -> (Cid, Vec<u8>, Cid) {
    let public_hex = hex::encode(key.public_key().raw());
    let signature = Signature::new(
        SignatureHeader::new(SignatureType::EdDSA, public_hex.as_bytes().to_vec()),
        key.sign(&block.to_dag_cbor().unwrap()).unwrap(),
    );
    let signature_cid = signature.generate_cid().unwrap();
    blockstore
        .put(&signature_cid, &signature.to_dag_cbor().unwrap())
        .await
        .unwrap();
    block.signature = Some(signature_cid);
    let bytes = block.to_dag_cbor().unwrap();
    (block.generate_cid().unwrap(), bytes, signature_cid)
}

#[test]
#[mantra_macros::req_test("defra.orphan_gc.signature_owner")]
#[mantra_macros::req_test("defra.orphan_gc.signature_merged")]
fn a_merged_document_owns_its_signature_blocks_and_they_read_as_merged() {
    workers::block_on(async {
        // The sender's Log has blockNumber; the receiver's, mid-upgrade, only address.
        let sender_schema = logs("col-log");
        let scratch = Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            false,
        ));
        let mut doc = Document::new(sender_schema.clone());
        doc.set("address", NormalValue::String("a".to_string()))
            .unwrap();
        doc.set("blockNumber", NormalValue::Int(7)).unwrap();
        let built = db::block::builder::build_blocks_from_document(
            &storage::Crdts::default(),
            &doc,
            &sender_schema,
            &scratch,
        )
        .await
        .unwrap();

        let receiver_schema = CollectionVersion::new(
            "Log",
            "v1",
            "col-log",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new(
                    "2",
                    "address",
                    FieldKind::string(),
                    schema::CType::LwwRegister,
                ),
            ],
        );
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc_with_options(store.clone(), DbOptions::new()).unwrap());
        db.create_collection(receiver_schema).await.unwrap();
        let received = Arc::new(DefraBlockstore::new(store, true));
        let handler = DbMergeHandler::new(db.clone(), received.clone());

        // Every block arrives signed, as from a signing node.
        let key = crypto::generate_ed25519().unwrap();
        let mut composite = Block::from_dag_cbor(&built.block).unwrap();
        let mut links = Vec::new();
        let mut signatures = Vec::new();
        for link in composite.links.take().unwrap() {
            let field =
                Block::from_dag_cbor(&scratch.get(&link.link).await.unwrap().unwrap()).unwrap();
            let (cid, bytes, signature) = signed(field, &key, &received).await;
            received.put(&cid, &bytes).await.unwrap();
            links.push(DAGLink {
                name: link.name,
                link: cid,
            });
            signatures.push(signature);
        }
        composite.links = Some(links);
        let (cid, bytes, signature) = signed(composite, &key, &received).await;
        // A DocID names its genesis composite: the signed one is what the
        // sender wrote and the receiver merges.
        let doc_id = db::block::builder::derive_doc_id(&cid);
        signatures.push(signature);
        received.put(&cid, &bytes).await.unwrap();
        for signature in &signatures {
            assert!(
                !received.is_merged(signature).await.unwrap(),
                "a received signature starts unmerged"
            );
        }

        let block = MergeBlock {
            cid,
            block_data: bytes::Bytes::from(bytes),
            doc_id: doc_id.clone(),
            collection_id: "col-log".to_string(),
            creator: "did:key:z6MkrSignatureTest".to_string(),
            sender_peer: Some("peer1".to_string()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        };
        assert_eq!(merge(&handler, &block).await, MergeOutcome::Merged);
        assert!(
            stored(&handler, "col-log", &doc_id).await,
            "stored under the sender's DocID"
        );

        let txn = db.new_txn(true).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        let document = vec![doc_id.clone()];
        for signature in &signatures {
            let owners =
                db::docid::map::get_doc_ids_for_block(&systemstore, &signature.to_string())
                    .await
                    .unwrap();
            assert_eq!(owners, document, "{signature} is owned by the document");
            assert!(
                received.is_merged(signature).await.unwrap(),
                "{signature} reads as merged"
            );
        }
    })
    .unwrap()
}
